use std::env;
use std::io;
use std::net::UdpSocket;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use uchi_pulse_hub::{
    DEFAULT_BIND_ADDR, DEFAULT_DB_PATH, DEFAULT_HELLO_REQUEST_ADDR, DEFAULT_OFFLINE_TIMEOUT_SEC,
    action::ActionEngine,
    cdc::spawn_server,
    db::Database,
    udp::{ActionExecutionStatus, HubUdpProcessor, PacketOutcome, encode_hello_request},
};

struct HubConfig {
    bind_addr: String,
    db_path: String,
    hello_request_addr: String,
    offline_timeout: Duration,
    cdc_device: Option<String>,
}

impl HubConfig {
    fn from_args() -> Result<Self, String> {
        let mut bind_addr =
            env::var("UCHI_PULSE_BIND").unwrap_or_else(|_| DEFAULT_BIND_ADDR.into());
        let mut db_path = env::var("UCHI_PULSE_DB").unwrap_or_else(|_| DEFAULT_DB_PATH.into());
        let mut hello_request_addr = env::var("UCHI_PULSE_HELLO_REQUEST_ADDR")
            .unwrap_or_else(|_| DEFAULT_HELLO_REQUEST_ADDR.into());
        let mut offline_timeout = env::var("UCHI_PULSE_OFFLINE_TIMEOUT_SEC")
            .ok()
            .map(|value| {
                value
                    .parse::<u64>()
                    .map_err(|_| "UCHI_PULSE_OFFLINE_TIMEOUT_SEC must be an integer")
            })
            .transpose()?
            .map(Duration::from_secs)
            .unwrap_or_else(|| Duration::from_secs(DEFAULT_OFFLINE_TIMEOUT_SEC));
        let mut cdc_device = env::var("UCHI_PULSE_CDC_DEVICE").ok();

        let mut args = env::args().skip(1);
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--bind" => {
                    bind_addr = args.next().ok_or("--bind requires an address")?;
                }
                "--db" => {
                    db_path = args.next().ok_or("--db requires a path")?;
                }
                "--hello-request-addr" => {
                    hello_request_addr = args
                        .next()
                        .ok_or("--hello-request-addr requires an address")?;
                }
                "--offline-timeout-sec" => {
                    let value = args
                        .next()
                        .ok_or("--offline-timeout-sec requires seconds")?;
                    offline_timeout = Duration::from_secs(
                        value
                            .parse()
                            .map_err(|_| "--offline-timeout-sec must be an integer")?,
                    );
                }
                "--cdc-device" => {
                    cdc_device = Some(args.next().ok_or("--cdc-device requires a path")?);
                }
                "--help" | "-h" => {
                    println!(
                        "Usage: uchi-pulse-hub [--bind ADDR] [--db PATH] [--hello-request-addr ADDR] [--offline-timeout-sec SECONDS] [--cdc-device PATH]"
                    );
                    println!(
                        "Defaults: bind={DEFAULT_BIND_ADDR}, db={DEFAULT_DB_PATH}, hello-request={DEFAULT_HELLO_REQUEST_ADDR}, offline-timeout={} seconds",
                        DEFAULT_OFFLINE_TIMEOUT_SEC
                    );
                    std::process::exit(0);
                }
                unknown => return Err(format!("unknown argument: {unknown}")),
            }
        }

        Ok(Self {
            bind_addr,
            db_path,
            hello_request_addr,
            offline_timeout,
            cdc_device,
        })
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = HubConfig::from_args()
        .map_err(|message| io::Error::new(io::ErrorKind::InvalidInput, message))?;
    let database = Database::open(&config.db_path)?;
    let _cdc_thread = config
        .cdc_device
        .as_ref()
        .map(|path| spawn_server(path.clone(), config.db_path.clone()));
    let socket = UdpSocket::bind(&config.bind_addr)?;
    socket.set_broadcast(true)?;
    socket.set_read_timeout(Some(Duration::from_secs(1)))?;
    println!("Uchi Pulse Hub listening on {}", config.bind_addr);

    let mut processor = HubUdpProcessor::from_database(
        database.clone(),
        Instant::now(),
        config.offline_timeout,
        ActionEngine::new(database),
    )?;
    let mut hello_request = [0u8; 64];
    let hello_request_len = encode_hello_request(&mut hello_request).map_err(|error| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("HELLO_REQUEST encoding failed: {error:?}"),
        )
    })?;
    socket.send_to(
        &hello_request[..hello_request_len],
        &config.hello_request_addr,
    )?;

    let mut buffer = [0u8; 2048];
    loop {
        match socket.recv_from(&mut buffer) {
            Ok((length, _source)) => {
                let now = Instant::now();
                match processor.process_datagram(&buffer[..length], now, &unix_timestamp()) {
                    Ok(PacketOutcome::EventAccepted {
                        device_id,
                        event_id,
                        duplicate,
                        action_status,
                        ack,
                    }) => {
                        let mut ack_buffer = [0u8; 256];
                        let ack_length =
                            uchi_pulse_hub::common_protocol::codec::encode(&ack, &mut ack_buffer)
                                .map_err(|error| {
                                io::Error::new(
                                    io::ErrorKind::InvalidData,
                                    format!("ACK encoding failed: {error:?}"),
                                )
                            })?;
                        socket.send_to(&ack_buffer[..ack_length], _source)?;
                        if !duplicate {
                            match action_status {
                                ActionExecutionStatus::Applied => {
                                    println!("EVENT from {device_id} event_id={event_id}");
                                }
                                ActionExecutionStatus::Failed(error) => {
                                    eprintln!(
                                        "Action failed after EVENT acceptance: device_id={device_id} event_id={event_id}: {error}"
                                    );
                                }
                                ActionExecutionStatus::SkippedDuplicate => {}
                            }
                        }
                    }
                    Ok(
                        PacketOutcome::Ignored
                        | PacketOutcome::HelloAccepted { .. }
                        | PacketOutcome::HeartbeatAccepted { .. },
                    ) => {}
                    Err(error @ uchi_pulse_hub::udp::UdpProcessingError::Database(_)) => {
                        eprintln!("discarding UDP packet: {error}");
                    }
                    Err(_) => {}
                }
                processor.mark_offline(now);
            }
            Err(error)
                if error.kind() == io::ErrorKind::TimedOut
                    || error.kind() == io::ErrorKind::WouldBlock =>
            {
                processor.mark_offline(Instant::now());
            }
            Err(error) => return Err(error.into()),
        }
    }
}

fn unix_timestamp() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        .to_string()
}
