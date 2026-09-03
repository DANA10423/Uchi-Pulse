#![cfg_attr(all(feature = "firmware", target_os = "none"), no_std)]
#![cfg_attr(all(feature = "firmware", target_os = "none"), no_main)]

#[cfg(not(all(feature = "firmware", target_os = "none")))]
fn main() {
    println!("uchi-pulse-node: build for a Pico target with --features pico-w or pico-2-w");
}

#[cfg(all(feature = "firmware", target_os = "none"))]
mod firmware {
    use portable_atomic::{AtomicU32, Ordering};

    use cyw43::{JoinOptions, aligned_bytes};
    use cyw43_pio::{DEFAULT_CLOCK_DIVIDER, PioSpi};
    use defmt::{error, info, warn};
    use defmt_rtt as _;
    use embassy_executor::Spawner;
    use embassy_net::udp::{PacketMetadata, UdpSocket};
    use embassy_net::{IpEndpoint, Ipv4Address, StackResources};
    use embassy_rp::clocks::RoscRng;
    use embassy_rp::gpio::{Input, Output, Pull};
    use embassy_rp::peripherals::{DMA_CH0, DMA_CH1, PIO0};
    use embassy_rp::pio::{InterruptHandler, Pio};
    use embassy_rp::{bind_interrupts, dma};
    use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
    use embassy_sync::channel::Channel;
    use embassy_time::{Duration, Timer, with_timeout};
    use panic_probe as _;
    use static_cell::StaticCell;

    use uchi_pulse_node::config::{DEFAULT_CONFIG, InputFunction};
    use uchi_pulse_node::protocol::{self, EventType, MessageId};

    bind_interrupts!(struct Irqs {
        PIO0_IRQ_0 => InterruptHandler<PIO0>;
        DMA_IRQ_0 => dma::InterruptHandler<DMA_CH0>, dma::InterruptHandler<DMA_CH1>;
    });

    static NEXT_MESSAGE_ID: AtomicU32 = AtomicU32::new(1);
    static EVENTS: Channel<CriticalSectionRawMutex, OutboundEvent, 8> = Channel::new();

    #[derive(Clone, Copy)]
    struct OutboundEvent {
        channel: u8,
        event_type: EventType,
        function: InputFunction,
    }

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
    async fn input_task(inputs: InputSet) -> ! {
        let mut states = [DebounceState::new(); 3];
        loop {
            let raw_states = [
                inputs.one.is_high(),
                inputs.two.is_high(),
                inputs.three.is_high(),
            ];
            for (index, &raw) in raw_states.iter().enumerate() {
                let binding = DEFAULT_CONFIG.inputs[index];
                let active = if binding.active_high { raw } else { !raw };
                if states[index].update(active, binding.debounce_ms) {
                    EVENTS
                        .send(OutboundEvent {
                            channel: binding.channel,
                            event_type: binding.event_type,
                            function: binding.function,
                        })
                        .await;
                }
            }
            Timer::after_millis(10).await;
        }
    }

    #[derive(Clone, Copy)]
    struct DebounceState {
        stable: bool,
        last: bool,
        ticks: u16,
    }

    impl DebounceState {
        const fn new() -> Self {
            Self {
                stable: false,
                last: false,
                ticks: 0,
            }
        }

        fn update(&mut self, active: bool, debounce_ms: u16) -> bool {
            if active != self.last {
                self.last = active;
                self.ticks = 0;
                return false;
            }

            self.ticks = self.ticks.saturating_add(10);
            if self.ticks < debounce_ms || self.stable == active {
                return false;
            }
            self.stable = active;
            active
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
        let mut rng = RoscRng;

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
        spawner.spawn(input_task(inputs).unwrap());

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

        let mut hello = [0; 256];
        let hello_len = protocol::encode_hello(
            &mut hello,
            DEFAULT_CONFIG.device_id,
            next_message_id(),
            DEFAULT_CONFIG.name,
            DEFAULT_CONFIG.firmware_version,
        )
        .unwrap();
        let _ = socket.send_to(&hello[..hello_len], hub).await;

        let mut next_heartbeat_ms = DEFAULT_CONFIG.heartbeat_interval_sec * 1_000;
        let mut tx = [0; 256];
        let mut rx = [0; 256];
        loop {
            if let Ok(event) = EVENTS.try_receive() {
                let id = next_message_id();
                let len = protocol::encode_event(
                    &mut tx,
                    DEFAULT_CONFIG.device_id,
                    id,
                    event.event_type,
                    event.channel,
                    event_value(event.function),
                )
                .unwrap_or_else(|_| {
                    error!("EVENT JSON buffer too small");
                    0
                });
                if len > 0 {
                    send_event_with_retry(&mut socket, hub, &tx[..len], id, &mut rx).await;
                }
            }

            if next_heartbeat_ms <= 10 {
                let id = next_message_id();
                let len =
                    protocol::encode_heartbeat(&mut tx, DEFAULT_CONFIG.device_id, id).unwrap();
                let _ = socket.send_to(&tx[..len], hub).await;
                next_heartbeat_ms = DEFAULT_CONFIG.heartbeat_interval_sec * 1_000;
            } else {
                next_heartbeat_ms -= 10;
            }
            Timer::after_millis(10).await;
        }
    }

    async fn send_event_with_retry<'a>(
        socket: &mut UdpSocket<'a>,
        hub: IpEndpoint,
        payload: &[u8],
        message_id: MessageId,
        rx: &mut [u8],
    ) {
        for attempt in 0..=DEFAULT_CONFIG.event_retry_count {
            if socket.send_to(payload, hub).await.is_err() {
                warn!("EVENT send failed");
            }
            match with_timeout(
                Duration::from_millis(DEFAULT_CONFIG.ack_timeout_ms as u64),
                socket.recv_from(rx),
            )
            .await
            {
                Ok(Ok((len, _)))
                    if protocol::decode_ack(&rx[..len])
                        .is_some_and(|ack| ack.message_id == message_id) =>
                {
                    info!("EVENT ACK {} (attempt {})", message_id, attempt + 1);
                    return;
                }
                _ => {}
            }
        }
        warn!("EVENT {} delivery failed after retries", message_id);
    }

    fn next_message_id() -> MessageId {
        NEXT_MESSAGE_ID.fetch_add(1, Ordering::Relaxed)
    }

    fn event_value(function: InputFunction) -> i32 {
        match function {
            InputFunction::MealReady => 1,
            InputFunction::Call => 1,
            InputFunction::Busy => 1,
            InputFunction::BusyClear => 0,
            InputFunction::EntryRequest => 1,
            InputFunction::EntryOk => 1,
            InputFunction::EntryLater => 1,
            InputFunction::EntryNg => 0,
            InputFunction::MailDetected => 1,
            InputFunction::MailCleared => 0,
        }
    }
}
