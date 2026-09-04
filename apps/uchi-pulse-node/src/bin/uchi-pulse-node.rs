#![cfg_attr(all(feature = "firmware", target_os = "none"), no_std)]
#![cfg_attr(all(feature = "firmware", target_os = "none"), no_main)]

#[cfg(not(all(feature = "firmware", target_os = "none")))]
fn main() {
    println!("uchi-pulse-node: build for a Pico target with --features pico-w or pico-2-w");
}

#[cfg(all(feature = "firmware", target_os = "none"))]
mod firmware {
    use core::fmt::Write as _;

    use cyw43::{JoinOptions, aligned_bytes};
    use cyw43_pio::{DEFAULT_CLOCK_DIVIDER, PioSpi};
    use defmt::{info, warn};
    use defmt_rtt as _;
    use embassy_executor::Spawner;
    use embassy_net::udp::{PacketMetadata, UdpSocket};
    use embassy_net::{IpEndpoint, Ipv4Address, Ipv4Cidr, StackResources, StaticConfigV4};
    use embassy_rp::clocks::RoscRng;
    use embassy_rp::flash::{Blocking, Flash};
    use embassy_rp::gpio::{Input, Output, Pull};
    use embassy_rp::peripherals::{DMA_CH0, DMA_CH1, FLASH, PIO0, USB};
    use embassy_rp::pio::{InterruptHandler, Pio};
    use embassy_rp::{bind_interrupts, dma};
    use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
    use embassy_sync::channel::Channel;
    use embassy_sync::mutex::Mutex;
    use embassy_time::{Duration, Instant, Timer, with_deadline, with_timeout};
    use embassy_usb::class::cdc_acm::{CdcAcmClass, State as CdcState};
    use embassy_usb::{Builder, UsbDevice};
    use panic_probe as _;
    use static_cell::StaticCell;

    use uchi_pulse_node::cdc::{
        CdcAction, CdcLineParser, MAX_CDC_LINE_SIZE, NodeCdcHandler as CdcCommandHandler,
    };
    use uchi_pulse_node::config::{
        DEFAULT_CONFIG, Ipv4Text, NetworkMode, PersistedNodeConfig, parse_ipv4,
    };
    use uchi_pulse_node::input::{InputController, TriggeredAction};
    use uchi_pulse_node::storage::{CONFIG_STORAGE_SIZE, ConfigManager, ConfigStorage};
    use uchi_pulse_node::udp::{NodeUdpProtocol, PendingEvent, RetryPolicy, is_hello_request};

    bind_interrupts!(struct Irqs {
        PIO0_IRQ_0 => InterruptHandler<PIO0>;
        DMA_IRQ_0 => dma::InterruptHandler<DMA_CH0>, dma::InterruptHandler<DMA_CH1>;
        USBCTRL_IRQ => embassy_rp::usb::InterruptHandler<USB>;
    });

    const FLASH_SIZE: usize = 2 * 1024 * 1024;
    const CONFIG_STORAGE_OFFSET: u32 = (FLASH_SIZE - CONFIG_STORAGE_SIZE) as u32;
    const AVAILABLE_GPIO_COUNT: usize = 26;
    const CONFIG_RECORD_MAGIC: u32 = 0x5543_4647;
    const CONFIG_RECORD_HEADER_SIZE: usize = 8;
    type NodeFlash = Flash<'static, FLASH, Blocking, FLASH_SIZE>;
    type FirmwareCdcHandler = CdcCommandHandler<FlashConfigStorage>;
    type HubEndpoint = Mutex<CriticalSectionRawMutex, Option<IpEndpoint>>;

    struct FlashConfigStorage {
        flash: NodeFlash,
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    enum FlashStorageError {
        Flash(embassy_rp::flash::Error),
        TooLarge,
        VerifyFailed,
    }

    impl FlashConfigStorage {
        fn new(flash: NodeFlash) -> Self {
            Self { flash }
        }
    }

    impl ConfigStorage for FlashConfigStorage {
        type Error = FlashStorageError;

        fn read(&mut self, destination: &mut [u8]) -> Result<usize, Self::Error> {
            let mut record = [0xff_u8; CONFIG_STORAGE_SIZE];
            self.flash
                .blocking_read(CONFIG_STORAGE_OFFSET, &mut record)
                .map_err(FlashStorageError::Flash)?;
            if record.len() < CONFIG_RECORD_HEADER_SIZE
                || u32::from_le_bytes(record[..4].try_into().unwrap()) != CONFIG_RECORD_MAGIC
            {
                return Ok(0);
            }
            let length = u16::from_le_bytes(record[4..6].try_into().unwrap()) as usize;
            if length == 0
                || length > CONFIG_STORAGE_SIZE - CONFIG_RECORD_HEADER_SIZE
                || length > destination.len()
            {
                return Ok(0);
            }
            destination[..length].copy_from_slice(
                &record[CONFIG_RECORD_HEADER_SIZE..CONFIG_RECORD_HEADER_SIZE + length],
            );
            Ok(length)
        }

        fn write(&mut self, data: &[u8]) -> Result<(), Self::Error> {
            if data.is_empty() || data.len() > CONFIG_STORAGE_SIZE - CONFIG_RECORD_HEADER_SIZE {
                return Err(FlashStorageError::TooLarge);
            }
            let mut record = [0xff_u8; CONFIG_STORAGE_SIZE];
            record[..4].copy_from_slice(&CONFIG_RECORD_MAGIC.to_le_bytes());
            record[4..6].copy_from_slice(&(data.len() as u16).to_le_bytes());
            record[CONFIG_RECORD_HEADER_SIZE..CONFIG_RECORD_HEADER_SIZE + data.len()]
                .copy_from_slice(data);
            let end = CONFIG_STORAGE_OFFSET + CONFIG_STORAGE_SIZE as u32;
            self.flash
                .blocking_erase(CONFIG_STORAGE_OFFSET, end)
                .map_err(FlashStorageError::Flash)?;
            self.flash
                .blocking_write(
                    CONFIG_STORAGE_OFFSET,
                    &record[..CONFIG_RECORD_HEADER_SIZE + data.len()],
                )
                .map_err(FlashStorageError::Flash)?;

            let mut verify = [0_u8; CONFIG_RECORD_HEADER_SIZE];
            self.flash
                .blocking_read(CONFIG_STORAGE_OFFSET, &mut verify)
                .map_err(FlashStorageError::Flash)?;
            if verify != record[..CONFIG_RECORD_HEADER_SIZE] {
                return Err(FlashStorageError::VerifyFailed);
            }
            Ok(())
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
    static CONTROL_RX_METADATA: StaticCell<[PacketMetadata; 4]> = StaticCell::new();
    static CONTROL_RX_BUFFER: StaticCell<[u8; 512]> = StaticCell::new();
    static CONTROL_TX_METADATA: StaticCell<[PacketMetadata; 4]> = StaticCell::new();
    static CONTROL_TX_BUFFER: StaticCell<[u8; 512]> = StaticCell::new();
    static RUNTIME_CONFIG: StaticCell<PersistedNodeConfig> = StaticCell::new();
    static USB_CONFIG_DESCRIPTOR: StaticCell<[u8; 256]> = StaticCell::new();
    static USB_BOS_DESCRIPTOR: StaticCell<[u8; 256]> = StaticCell::new();
    static USB_MSOS_DESCRIPTOR: StaticCell<[u8; 256]> = StaticCell::new();
    static USB_CONTROL_BUFFER: StaticCell<[u8; 64]> = StaticCell::new();
    static CDC_STATE: StaticCell<CdcState<'static>> = StaticCell::new();
    static HUB_ENDPOINT: StaticCell<HubEndpoint> = StaticCell::new();

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

    struct GpioInputPin {
        gpio: u8,
        input: Input<'static>,
    }

    struct InputSet {
        pins: [GpioInputPin; AVAILABLE_GPIO_COUNT],
    }

    #[embassy_executor::task]
    async fn input_task(inputs: InputSet, config: &'static PersistedNodeConfig) -> ! {
        let mut controller = match InputController::new(config.input_config()) {
            Ok(controller) => controller,
            Err(_) => loop {
                Timer::after_secs(1).await;
            },
        };
        for pin in &inputs.pins {
            if controller.has_gpio(pin.gpio) {
                let _ = controller.initialize(pin.gpio, pin.input.is_high(), 0);
            }
        }

        let mut now_ms = 0_u64;
        loop {
            for pin in &inputs.pins {
                if !controller.has_gpio(pin.gpio) {
                    continue;
                }
                let result = match controller.update(pin.gpio, pin.input.is_high(), now_ms) {
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
        hub_endpoint: &'static HubEndpoint,
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
                let Some(hub) = *hub_endpoint.lock().await else {
                    warn!("EVENT dropped because parent has not been discovered");
                    break;
                };
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

    #[embassy_executor::task]
    async fn discovery_and_heartbeat_task(
        socket: UdpSocket<'static>,
        hub_endpoint: &'static HubEndpoint,
        boot_id: u64,
        config: &'static PersistedNodeConfig,
    ) -> ! {
        let protocol = NodeUdpProtocol::new(config.device_id.as_str(), boot_id)
            .expect("invalid device ID configuration");
        let mut rx = [0; 128];
        let mut tx = [0; 256];
        let mut last_heartbeat = Instant::now();

        loop {
            if let Ok(Ok((length, source))) =
                with_timeout(Duration::from_secs(1), socket.recv_from(&mut rx)).await
                && is_hello_request(&rx[..length])
            {
                *hub_endpoint.lock().await = Some(source.endpoint);
                let hello = protocol.hello();
                if let Ok(hello_length) = protocol.encode_message(&hello, &mut tx) {
                    let _ = socket.send_to(&tx[..hello_length], source.endpoint).await;
                }
            }

            let now = Instant::now();
            if now.saturating_duration_since(last_heartbeat)
                >= Duration::from_secs(config.heartbeat_interval_sec.into())
            {
                if let Some(hub) = *hub_endpoint.lock().await {
                    let heartbeat = protocol.heartbeat();
                    if let Ok(heartbeat_length) = protocol.encode_message(&heartbeat, &mut tx) {
                        let _ = socket.send_to(&tx[..heartbeat_length], hub).await;
                    }
                }
                last_heartbeat = now;
            }
        }
    }

    fn network_config(config: &PersistedNodeConfig) -> embassy_net::Config {
        match config.network.mode {
            NetworkMode::Dhcp => embassy_net::Config::dhcpv4(Default::default()),
            NetworkMode::Static => {
                let static_ipv4 = config
                    .network
                    .static_ipv4
                    .as_ref()
                    .expect("validated static network configuration is missing");
                let ip_address = parse_ipv4(static_ipv4.ip_address.as_str())
                    .expect("validated static IP address is invalid");
                let gateway = parse_ipv4(static_ipv4.gateway.as_str())
                    .expect("validated static gateway is invalid");
                let dns =
                    parse_ipv4(static_ipv4.dns.as_str()).expect("validated static DNS is invalid");
                let mut dns_servers = heapless::Vec::new();
                dns_servers
                    .push(Ipv4Address::new(dns[0], dns[1], dns[2], dns[3]))
                    .unwrap();
                embassy_net::Config::ipv4_static(StaticConfigV4 {
                    address: Ipv4Cidr::new(
                        Ipv4Address::new(
                            ip_address[0],
                            ip_address[1],
                            ip_address[2],
                            ip_address[3],
                        ),
                        static_ipv4.prefix_length,
                    ),
                    gateway: Some(Ipv4Address::new(
                        gateway[0], gateway[1], gateway[2], gateway[3],
                    )),
                    dns_servers,
                })
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
        stack: embassy_net::Stack<'static>,
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
                    let result = match handler.handle_line_with_status(
                        line.as_slice(),
                        &mut response,
                        current_ip_address(&stack),
                    ) {
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

    fn current_ip_address(stack: &embassy_net::Stack<'static>) -> Option<Ipv4Text> {
        let config = stack.config_v4()?;
        let ip_address = config.address.address();
        let mut text = Ipv4Text::new();
        write!(&mut text, "{ip_address}").ok()?;
        Some(text)
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
        let config = network_config(runtime_config);
        let (stack, runner) = embassy_net::new(
            net_device,
            config,
            STACK.init(StackResources::new()),
            rng.next_u64(),
        );
        spawner.spawn(net_task(runner).unwrap());
        spawner.spawn(
            cdc_task(
                cdc_class,
                cdc_handler,
                embassy_rp::watchdog::Watchdog::new(p.WATCHDOG),
                stack,
            )
            .unwrap(),
        );

        while let Err(err) = control
            .join(
                runtime_config.wifi.ssid.as_str(),
                JoinOptions::new(runtime_config.wifi.password.as_bytes()),
            )
            .await
        {
            warn!("Wi-Fi join failed: {:?}", err);
            Timer::after_secs(2).await;
        }
        stack.wait_config_up().await;
        info!("Wi-Fi ready");

        let inputs = InputSet {
            pins: [
                GpioInputPin {
                    gpio: 0,
                    input: Input::new(p.PIN_0, Pull::Up),
                },
                GpioInputPin {
                    gpio: 1,
                    input: Input::new(p.PIN_1, Pull::Up),
                },
                GpioInputPin {
                    gpio: 2,
                    input: Input::new(p.PIN_2, Pull::Up),
                },
                GpioInputPin {
                    gpio: 3,
                    input: Input::new(p.PIN_3, Pull::Up),
                },
                GpioInputPin {
                    gpio: 4,
                    input: Input::new(p.PIN_4, Pull::Up),
                },
                GpioInputPin {
                    gpio: 5,
                    input: Input::new(p.PIN_5, Pull::Up),
                },
                GpioInputPin {
                    gpio: 6,
                    input: Input::new(p.PIN_6, Pull::Up),
                },
                GpioInputPin {
                    gpio: 7,
                    input: Input::new(p.PIN_7, Pull::Up),
                },
                GpioInputPin {
                    gpio: 8,
                    input: Input::new(p.PIN_8, Pull::Up),
                },
                GpioInputPin {
                    gpio: 9,
                    input: Input::new(p.PIN_9, Pull::Up),
                },
                GpioInputPin {
                    gpio: 10,
                    input: Input::new(p.PIN_10, Pull::Up),
                },
                GpioInputPin {
                    gpio: 11,
                    input: Input::new(p.PIN_11, Pull::Up),
                },
                GpioInputPin {
                    gpio: 12,
                    input: Input::new(p.PIN_12, Pull::Up),
                },
                GpioInputPin {
                    gpio: 13,
                    input: Input::new(p.PIN_13, Pull::Up),
                },
                GpioInputPin {
                    gpio: 14,
                    input: Input::new(p.PIN_14, Pull::Up),
                },
                GpioInputPin {
                    gpio: 15,
                    input: Input::new(p.PIN_15, Pull::Up),
                },
                GpioInputPin {
                    gpio: 16,
                    input: Input::new(p.PIN_16, Pull::Up),
                },
                GpioInputPin {
                    gpio: 17,
                    input: Input::new(p.PIN_17, Pull::Up),
                },
                GpioInputPin {
                    gpio: 18,
                    input: Input::new(p.PIN_18, Pull::Up),
                },
                GpioInputPin {
                    gpio: 19,
                    input: Input::new(p.PIN_19, Pull::Up),
                },
                GpioInputPin {
                    gpio: 20,
                    input: Input::new(p.PIN_20, Pull::Up),
                },
                GpioInputPin {
                    gpio: 21,
                    input: Input::new(p.PIN_21, Pull::Up),
                },
                GpioInputPin {
                    gpio: 22,
                    input: Input::new(p.PIN_22, Pull::Up),
                },
                GpioInputPin {
                    gpio: 26,
                    input: Input::new(p.PIN_26, Pull::Up),
                },
                GpioInputPin {
                    gpio: 27,
                    input: Input::new(p.PIN_27, Pull::Up),
                },
                GpioInputPin {
                    gpio: 28,
                    input: Input::new(p.PIN_28, Pull::Up),
                },
            ],
        };
        spawner.spawn(input_task(inputs, runtime_config).unwrap());

        let mut control_socket = UdpSocket::new(
            stack,
            CONTROL_RX_METADATA.init([PacketMetadata::EMPTY; 4]),
            CONTROL_RX_BUFFER.init([0; 512]),
            CONTROL_TX_METADATA.init([PacketMetadata::EMPTY; 4]),
            CONTROL_TX_BUFFER.init([0; 512]),
        );
        control_socket.bind(DEFAULT_CONFIG.local_port).unwrap();
        let hub_endpoint = HUB_ENDPOINT.init(Mutex::new(None));

        let event_socket = UdpSocket::new(
            stack,
            EVENT_RX_METADATA.init([PacketMetadata::EMPTY; 4]),
            EVENT_RX_BUFFER.init([0; 512]),
            EVENT_TX_METADATA.init([PacketMetadata::EMPTY; 4]),
            EVENT_TX_BUFFER.init([0; 512]),
        );
        let mut event_socket = event_socket;
        event_socket.bind(0).unwrap();
        spawner
            .spawn(event_sender_task(event_socket, hub_endpoint, boot_id, runtime_config).unwrap());
        spawner.spawn(
            discovery_and_heartbeat_task(control_socket, hub_endpoint, boot_id, runtime_config)
                .unwrap(),
        );

        loop {
            Timer::after_secs(3600).await;
        }
    }
}
