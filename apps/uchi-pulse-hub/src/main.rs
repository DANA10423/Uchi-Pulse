use std::env;
use std::io;
use std::net::UdpSocket;
use std::time::{Duration, Instant};

use uchi_pulse_hub::{
    DEFAULT_BIND_ADDR, DEFAULT_OFFLINE_TIMEOUT, HubState, MessageType, NodeStatus, encode_ack,
};

struct HubConfig {
    bind_addr: String,
    offline_timeout: Duration,
}

impl HubConfig {
    fn from_args() -> Result<Self, String> {
        let mut bind_addr =
            env::var("UCHI_PULSE_BIND").unwrap_or_else(|_| DEFAULT_BIND_ADDR.into());
        let mut offline_timeout = env::var("UCHI_PULSE_OFFLINE_TIMEOUT_SEC")
            .ok()
            .map(|value| {
                value
                    .parse::<u64>()
                    .map_err(|_| "UCHI_PULSE_OFFLINE_TIMEOUT_SEC must be an integer")
            })
            .transpose()?
            .map(Duration::from_secs)
            .unwrap_or(DEFAULT_OFFLINE_TIMEOUT);

        let mut args = env::args().skip(1);
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--bind" => {
                    bind_addr = args.next().ok_or("--bind requires an address")?;
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
                "--help" | "-h" => {
                    println!("Usage: uchi-pulse-hub [--bind ADDR] [--offline-timeout-sec SECONDS]");
                    println!(
                        "Defaults: {DEFAULT_BIND_ADDR}, {} seconds",
                        DEFAULT_OFFLINE_TIMEOUT.as_secs()
                    );
                    std::process::exit(0);
                }
                unknown => return Err(format!("unknown argument: {unknown}")),
            }
        }

        Ok(Self {
            bind_addr,
            offline_timeout,
        })
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = HubConfig::from_args()
        .map_err(|message| io::Error::new(io::ErrorKind::InvalidInput, message))?;
    let socket = UdpSocket::bind(&config.bind_addr)?;
    socket.set_read_timeout(Some(Duration::from_secs(1)))?;
    println!("Uchi Pulse Hub listening on {}", config.bind_addr);

    let mut state = HubState::new(config.offline_timeout);
    let mut buffer = [0u8; 2048];
    loop {
        match socket.recv_from(&mut buffer) {
            Ok((length, source)) => {
                let now = Instant::now();
                match state.handle_datagram(&buffer[..length], source, now) {
                    Ok(result) => {
                        if result.message_type == MessageType::Event {
                            socket.send_to(&encode_ack(result.message_id), source)?;
                            if result.duplicate {
                                println!(
                                    "duplicate EVENT from {} message_id={} (ACK resent)",
                                    result.device_id, result.message_id
                                );
                            } else {
                                println!(
                                    "EVENT from {} message_id={}",
                                    result.device_id, result.message_id
                                );
                            }
                        } else {
                            println!(
                                "{} from {}",
                                format_message_type(result.message_type),
                                result.device_id
                            );
                        }
                    }
                    Err(error) => eprintln!("discarding packet from {source}: {error}"),
                }
                state.mark_offline(now);
            }
            Err(error)
                if error.kind() == io::ErrorKind::TimedOut
                    || error.kind() == io::ErrorKind::WouldBlock =>
            {
                state.mark_offline(Instant::now());
                print_status(&state);
            }
            Err(error) => return Err(error.into()),
        }
    }
}

fn format_message_type(message_type: MessageType) -> &'static str {
    match message_type {
        MessageType::Hello => "HELLO",
        MessageType::Heartbeat => "HEARTBEAT",
        MessageType::Event => "EVENT",
        MessageType::Ack => "ACK",
    }
}

fn print_status(state: &HubState) {
    for node in state.nodes() {
        if node.status == NodeStatus::Offline {
            eprintln!("Node {} is OFFLINE", node.device_id);
        }
    }
}
