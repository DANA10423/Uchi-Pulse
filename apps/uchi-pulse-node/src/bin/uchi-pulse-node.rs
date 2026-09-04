#![cfg_attr(all(feature = "firmware", target_os = "none"), no_std)]
#![cfg_attr(all(feature = "firmware", target_os = "none"), no_main)]

#[cfg(not(all(feature = "firmware", target_os = "none")))]
fn main() {
    println!("uchi-pulse-node: build for a Pico target with --features pico-w or pico-2-w");
}

#[cfg(all(feature = "firmware", target_os = "none"))]
mod firmware {
    use cyw43::{JoinOptions, aligned_bytes};
    use cyw43_pio::{DEFAULT_CLOCK_DIVIDER, PioSpi};
    use defmt::{info, warn};
    use defmt_rtt as _;
    use embassy_executor::Spawner;
    use embassy_net::udp::{PacketMetadata, UdpSocket};
    use embassy_net::{IpEndpoint, Ipv4Address, StackResources};
    use embassy_rp::clocks::RoscRng;
    use embassy_rp::flash::{Blocking, Flash};
    use embassy_rp::gpio::{Input, Output, Pull};
    use embassy_rp::peripherals::{DMA_CH0, DMA_CH1, FLASH, PIO0, USB};
    use embassy_rp::pio::{InterruptHandler, Pio};
    use embassy_rp::{bind_interrupts, dma};
    use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
    use embassy_sync::channel::Channel;
    use embassy_time::{Duration, Instant, Timer, with_deadline};
    use embassy_usb::class::cdc_acm::{CdcAcmClass, State as CdcState};
    use embassy_usb::{Builder, UsbDevice};
    use panic_probe as _;
    use static_cell::StaticCell;

    use uchi_pulse_node::cdc::{
        CdcAction, CdcLineParser, MAX_CDC_LINE_SIZE, NodeCdcHandler as CdcCommandHandler,
    };
    use uchi_pulse_node::config::{DEFAULT_CONFIG, PersistedNodeConfig};
    use uchi_pulse_node::input::{InputController, TriggeredAction};
    use uchi_pulse_node::storage::{CONFIG_STORAGE_SIZE, ConfigManager, ConfigStorage};
    use uchi_pulse_node::udp::{NodeUdpProtocol, PendingEvent, RetryPolicy};

    bind_interrupts!(struct Irqs {
        PIO0_IRQ_0 => InterruptHandler<PIO0>;
        DMA_IRQ_0 => dma::InterruptHandler<DMA_CH0>, dma::InterruptHandler<DMA_CH1>;
        USBCTRL_IRQ => embassy_rp::usb::InterruptHandler<USB>;
    });

    const FLASH_SIZE: usize = 2 * 1024 * 1024;
    const CONFIG_STORAGE_OFFSET: u32 = (FLASH_SIZE - CONFIG_STORAGE_SIZE) as u32;
    type NodeFlash = Flash<'static, FLASH, Blocking, FLASH_SIZE>;
    type FirmwareCdcHandler = CdcCommandHandler<FlashConfigStorage>;

    struct FlashConfigStorage {
        flash: NodeFlash,
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    enum FlashStorageError {
        Flash(embassy_rp::flash::Error),
        TooLarge,
    }

    impl FlashConfigStorage {
        fn new(flash: NodeFlash) -> Self {
            Self { flash }
        }
    }

    impl ConfigStorage for FlashConfigStorage {
        type Error = FlashStorageError;

        fn read(&mut self, destination: &mut [u8]) -> Result<usize, Self::Error> {
            self.flash
                .blocking_read(CONFIG_STORAGE_OFFSET, destination)
                .map_err(FlashStorageError::Flash)?;
            Ok(destination
                .iter()
                .position(|byte| *byte == 0xff)
                .unwrap_or(destination.len()))
        }

        fn write(&mut self, data: &[u8]) -> Result<(), Self::Error> {
            if data.len() > CONFIG_STORAGE_SIZE {
                return Err(FlashStorageError::TooLarge);
            }
            let end = CONFIG_STORAGE_OFFSET + CONFIG_STORAGE_SIZE as u32;
            self.flash
                .blocking_erase(CONFIG_STORAGE_OFFSET, end)
                .map_err(FlashStorageError::Flash)?;
            self.flash
                .blocking_write(CONFIG_STORAGE_OFFSET, data)
                .map_err(FlashStorageError::Flash)
        }

        fn clear(&mut self) -> Result<(), Self::Error> {
            let end = CONFIG_STORAGE_OFFSET + CONFIG_STORAGE_SIZE as u32;
            self.flash
                .blocking_erase(CONFIG_STORAGE_OFFSET, end)
                .map_err(FlashStorageError::Flash)
        }
    }

    static ACTION_EVENTS: Channel<CriticalSectionRawMutex, TriggeredAction, 8> = Channel::new();

    static EVENT_RX_METADATA: StaticCell<[PacketMetadata; 4]> = StaticCell::new();
    static EVENT_RX_BUFFER: StaticCell<[u8; 512]> = StaticCell::new();
    static EVENT_TX_METADATA: StaticCell<[PacketMetadata; 4]> = StaticCell::new();
    static EVENT_TX_BUFFER: StaticCell<[u8; 512]> = StaticCell::new();
    static RUNTIME_CONFIG: StaticCell<PersistedNodeConfig> = StaticCell::new();
    static USB_CONFIG_DESCRIPTOR: StaticCell<[u8; 256]> = StaticCell::new();
    static USB_BOS_DESCRIPTOR: StaticCell<[u8; 256]> = StaticCell::new();
    static USB_MSOS_DESCRIPTOR: StaticCell<[u8; 256]> = StaticCell::new();
    static USB_CONTROL_BUFFER: StaticCell<[u8; 64]> = StaticCell::new();
    static CDC_STATE: StaticCell<CdcState<'static>> = StaticCell::new();

    #[embassy_executor::task]
    async fn cyw43_task(
        runner: cyw43::Runner<'static, cyw43::SpiBus<Output<'static>, PioSpi<'static, PIO0, 0>>>,
    ) -> ! {
        runner.run().await
    }

    #[embassy_executor::task]
    async fn net_task(mut runner: embassy_net::Runner<'static, cyw43::NetDriver<'static>>) -> ! {
        runner.run().await
    }

    #[embassy_executor::task]
    async fn input_task(inputs: InputSet, config: &'static PersistedNodeConfig) -> ! {
        let mut controller = match InputController::new(config.input_config()) {
            Ok(controller) => controller,
            Err(_) => loop {
                Timer::after_secs(1).await;
            },
        };
        let initial_raw_states = [
            (2, inputs.one.is_high()),
            (3, inputs.two.is_high()),
            (4, inputs.three.is_high()),
        ];
        for &(gpio, raw) in &initial_raw_states {
            if controller.has_gpio(gpio) {
                let _ = controller.initialize(gpio, raw, 0);
            }
        }

        let mut now_ms = 0_u64;
        loop {
            let raw_states = [
                (2, inputs.one.is_high()),
                (3, inputs.two.is_high()),
                (4, inputs.three.is_high()),
            ];
            for &(gpio, raw) in &raw_states {
                if !controller.has_gpio(gpio) {
                    continue;
                }
                let result = match controller.update(gpio, raw, now_ms) {
                    Ok(result) => result,
                    Err(_) => continue,
                };
                for action in result.actions {
                    ACTION_EVENTS.send(action).await;
                }
            }
            Timer::after_millis(10).await;
            now_ms = now_ms.saturating_add(10);
        }
    }

    #[embassy_executor::task]
    async fn event_sender_task(
        mut socket: UdpSocket<'static>,
        hub: IpEndpoint,
        boot_id: u64,
        config: &'static PersistedNodeConfig,
    ) -> ! {
        let mut protocol = NodeUdpProtocol::new(config.device_id.as_str(), boot_id)
            .expect("invalid device ID configuration");
        let mut tx = [0; 512];
        let mut rx = [0; 512];

        loop {
            let action = ACTION_EVENTS.receive().await;
            let event = match protocol.event_from_action(action) {
                Ok(event) => event,
                Err(_) => {
                    warn!("unable to allocate EVENT ID");
                    continue;
                }
            };
            let used = match protocol.encode_message(event.message(), &mut tx) {
                Ok(used) => used,
                Err(_) => {
                    warn!("EVENT JSON buffer too small");
                    continue;
                }
            };

            let mut delivered = false;
            let mut retry_policy = RetryPolicy::new(config.event_retry_count);
            while let Some(attempt) = retry_policy.next_attempt() {
                if socket.send_to(&tx[..used], hub).await.is_err() {
                    warn!("EVENT send failed");
                    continue;
                }

                let deadline = Instant::now() + Duration::from_millis(config.ack_timeout_ms as u64);
                if wait_for_ack(&mut socket, &protocol, &event, &mut rx, deadline).await {
                    info!("EVENT delivered (attempt {})", attempt);
                    delivered = true;
                    break;
                }
            }
            if !delivered {
                warn!("EVENT delivery failed after retries");
            }
        }
    }

    async fn wait_for_ack(
        socket: &mut UdpSocket<'static>,
        protocol: &NodeUdpProtocol,
        event: &PendingEvent,
        rx: &mut [u8],
        deadline: Instant,
    ) -> bool {
        loop {
            let received = with_deadline(deadline, socket.recv_from(rx)).await;
            match received {
                Ok(Ok((len, _))) if protocol.ack_matches(event, &rx[..len]) => return true,
                Ok(Ok(_)) | Ok(Err(_)) => {
                    if Instant::now() >= deadline {
                        return false;
                    }
                }
                Err(_) => return false,
            }
        }
    }

    #[embassy_executor::task]
    async fn usb_device_task(
        mut usb: UsbDevice<'static, embassy_rp::usb::Driver<'static, USB>>,
    ) -> ! {
        usb.run().await
    }

    #[embassy_executor::task]
    async fn cdc_task(
        mut class: CdcAcmClass<'static, embassy_rp::usb::Driver<'static, USB>>,
        mut handler: FirmwareCdcHandler,
        mut watchdog: embassy_rp::watchdog::Watchdog,
    ) -> ! {
        let mut parser = CdcLineParser::new();
        let mut packet = [0_u8; 64];
        let mut response = [0_u8; MAX_CDC_LINE_SIZE];
        loop {
            class.wait_connection().await;
            loop {
                let length = match class.read_packet(&mut packet).await {
                    Ok(length) => length,
                    Err(_) => {
                        parser = CdcLineParser::new();
                        break;
                    }
                };
                let _ = parser.feed(&packet[..length]);
                while let Some(line) = parser.pop_line() {
                    let result = match handler.handle_line(line.as_slice(), &mut response) {
                        Ok(result) => result,
                        Err(_) => continue,
                    };
                    let mut offset = 0;
                    let max_packet_size = class.max_packet_size() as usize;
                    while offset < result.response_len {
                        let end = core::cmp::min(offset + max_packet_size, result.response_len);
                        if class.write_packet(&response[offset..end]).await.is_err() {
                            break;
                        }
                        offset = end;
                    }
                    if result.response_len % max_packet_size == 0 {
                        let _ = class.write_packet(&[]).await;
                    }
                    if result.action == CdcAction::Reboot {
                        watchdog.trigger_reset();
                        loop {
                            core::hint::spin_loop();
                        }
                    }
                }
            }
        }
    }

    struct InputSet {
        one: Input<'static>,
        two: Input<'static>,
        three: Input<'static>,
    }

    #[embassy_executor::main(
        executor = "embassy_rp::executor::Executor",
        entry = "cortex_m_rt::entry"
    )]
    async fn main(spawner: Spawner) {
        info!("Uchi Pulse Node {}", DEFAULT_CONFIG.device_id);
        let p = embassy_rp::init(Default::default());
        let defaults = PersistedNodeConfig::from_node_config(&DEFAULT_CONFIG)
            .expect("invalid built-in node configuration");
        let config_manager = ConfigManager::new(
            FlashConfigStorage::new(Flash::new_blocking(p.FLASH)),
            defaults,
        );
        let cdc_handler = CdcCommandHandler::new(config_manager);
        let runtime_config = RUNTIME_CONFIG.init(cdc_handler.config().clone());
        let usb_driver = embassy_rp::usb::Driver::new(p.USB, Irqs);
        let mut usb_config = embassy_usb::Config::new(0x1209, 0x5543);
        usb_config.manufacturer = Some("Uchi-Pulse");
        usb_config.product = Some("Uchi-Pulse Node");
        usb_config.serial_number = Some(DEFAULT_CONFIG.device_id);
        let mut usb_builder = Builder::new(
            usb_driver,
            usb_config,
            USB_CONFIG_DESCRIPTOR.init([0; 256]),
            USB_BOS_DESCRIPTOR.init([0; 256]),
            USB_MSOS_DESCRIPTOR.init([0; 256]),
            USB_CONTROL_BUFFER.init([0; 64]),
        );
        let cdc_class = CdcAcmClass::new(&mut usb_builder, CDC_STATE.init(CdcState::new()), 64);
        let usb_device = usb_builder.build();
        spawner.spawn(usb_device_task(usb_device).unwrap());
        spawner.spawn(
            cdc_task(
                cdc_class,
                cdc_handler,
                embassy_rp::watchdog::Watchdog::new(p.WATCHDOG),
            )
            .unwrap(),
        );
        let mut rng = RoscRng;
        let boot_id = rng.next_u64();

        let firmware = aligned_bytes!("../../firmware/43439A0.bin");
        let clm = aligned_bytes!("../../firmware/43439A0_clm.bin");
        #[cfg(feature = "pico-w")]
        let nvram = aligned_bytes!("../../firmware/nvram_rp2040.bin");
        #[cfg(feature = "pico-2-w")]
        let nvram = aligned_bytes!("../../firmware/nvram_rp235x.bin");

        let pwr = embassy_rp::gpio::Output::new(p.PIN_23, embassy_rp::gpio::Level::Low);
        let cs = embassy_rp::gpio::Output::new(p.PIN_25, embassy_rp::gpio::Level::High);
        let mut pio = Pio::new(p.PIO0, Irqs);
        let spi = PioSpi::new(
            &mut pio.common,
            pio.sm0,
            DEFAULT_CLOCK_DIVIDER,
            pio.irq0,
            cs,
            p.PIN_24,
            p.PIN_29,
            dma::Channel::new(p.DMA_CH0, Irqs),
        );

        static WIFI_STATE: StaticCell<cyw43::State> = StaticCell::new();
        let (net_device, mut control, wifi_runner) = cyw43::new(
            WIFI_STATE.init(cyw43::State::new()),
            pwr,
            spi,
            firmware,
            nvram,
        )
        .await;
        spawner.spawn(cyw43_task(wifi_runner).unwrap());
        control.init(clm).await;

        static STACK: StaticCell<StackResources<3>> = StaticCell::new();
        let config = embassy_net::Config::dhcpv4(Default::default());
        let (stack, runner) = embassy_net::new(
            net_device,
            config,
            STACK.init(StackResources::new()),
            rng.next_u64(),
        );
        spawner.spawn(net_task(runner).unwrap());

        while let Err(err) = control
            .join(
                DEFAULT_CONFIG.wifi_ssid,
                JoinOptions::new(DEFAULT_CONFIG.wifi_password.as_bytes()),
            )
            .await
        {
            warn!("Wi-Fi join failed: {:?}", err);
            Timer::after_secs(2).await;
        }
        stack.wait_config_up().await;
        info!("Wi-Fi ready");

        let inputs = InputSet {
            one: Input::new(p.PIN_2, Pull::Up),
            two: Input::new(p.PIN_3, Pull::Up),
            three: Input::new(p.PIN_4, Pull::Up),
        };
        spawner.spawn(input_task(inputs, runtime_config).unwrap());

        let mut rx_buffer = [0; 512];
        let mut tx_buffer = [0; 512];
        let mut rx_metadata = [PacketMetadata::EMPTY; 4];
        let mut tx_metadata = [PacketMetadata::EMPTY; 4];
        let mut socket = UdpSocket::new(
            stack,
            &mut rx_metadata,
            &mut rx_buffer,
            &mut tx_metadata,
            &mut tx_buffer,
        );
        socket.bind(DEFAULT_CONFIG.local_port).unwrap();
        let hub = IpEndpoint::new(
            Ipv4Address::new(
                DEFAULT_CONFIG.hub_ipv4[0],
                DEFAULT_CONFIG.hub_ipv4[1],
                DEFAULT_CONFIG.hub_ipv4[2],
                DEFAULT_CONFIG.hub_ipv4[3],
            )
            .into(),
            DEFAULT_CONFIG.hub_port,
        );

        let event_socket = UdpSocket::new(
            stack,
            EVENT_RX_METADATA.init([PacketMetadata::EMPTY; 4]),
            EVENT_RX_BUFFER.init([0; 512]),
            EVENT_TX_METADATA.init([PacketMetadata::EMPTY; 4]),
            EVENT_TX_BUFFER.init([0; 512]),
        );
        let mut event_socket = event_socket;
        event_socket.bind(0).unwrap();
        spawner.spawn(event_sender_task(event_socket, hub, boot_id, runtime_config).unwrap());

        let protocol = NodeUdpProtocol::new(runtime_config.device_id.as_str(), boot_id)
            .expect("invalid device ID configuration");
        let mut tx = [0; 512];
        let hello = protocol.hello();
        let hello_len = protocol
            .encode_message(&hello, &mut tx)
            .expect("HELLO JSON buffer too small");
        let _ = socket.send_to(&tx[..hello_len], hub).await;

        loop {
            Timer::after_secs(runtime_config.heartbeat_interval_sec.into()).await;
            let heartbeat = protocol.heartbeat();
            let heartbeat_len = protocol
                .encode_message(&heartbeat, &mut tx)
                .expect("HEARTBEAT JSON buffer too small");
            let _ = socket.send_to(&tx[..heartbeat_len], hub).await;
        }
    }
}
