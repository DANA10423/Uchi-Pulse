//! In-memory Hub state and the Node UDP protocol implementation.
//!
//! The registry intentionally has no persistence. This follows the design
//! document: after a Hub restart, Nodes register again with their next packet.

use std::collections::{HashMap, HashSet};
use std::net::SocketAddr;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

pub const PROTOCOL_VERSION: u8 = 1;
pub const DEFAULT_BIND_ADDR: &str = "0.0.0.0:5000";
pub const DEFAULT_OFFLINE_TIMEOUT: Duration = Duration::from_secs(210);

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub enum MessageType {
    #[serde(rename = "HELLO")]
    Hello,
    #[serde(rename = "HEARTBEAT")]
    Heartbeat,
    #[serde(rename = "EVENT")]
    Event,
    #[serde(rename = "ACK")]
    Ack,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum EventType {
    #[serde(rename = "BUTTON")]
    Button,
    #[serde(rename = "SENSOR")]
    Sensor,
    #[serde(rename = "STATE")]
    State,
}

#[derive(Debug, Deserialize)]
pub struct IncomingMessage {
    pub version: u8,
    #[serde(rename = "type")]
    pub message_type: MessageType,
    pub device_id: String,
    pub message_id: u32,
    pub data: serde_json::Value,
}

#[derive(Debug, Deserialize)]
struct HelloData {
    name: String,
    firmware_version: String,
}

#[derive(Debug, Deserialize)]
struct EventData {
    event_type: EventType,
    channel: u8,
    value: i32,
}

#[derive(Serialize)]
struct Ack {
    #[serde(rename = "type")]
    message_type: MessageType,
    message_id: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NodeStatus {
    Online,
    Offline,
}

#[derive(Clone, Debug)]
pub struct NodeRecord {
    pub device_id: String,
    pub name: Option<String>,
    pub firmware_version: Option<String>,
    pub source: SocketAddr,
    pub status: NodeStatus,
    pub first_seen: Instant,
    pub last_seen: Instant,
}

#[derive(Clone, Debug, PartialEq)]
pub struct EventRecord {
    pub device_id: String,
    pub message_id: u32,
    pub event_type: EventType,
    pub channel: u8,
    pub value: i32,
    pub received_at: Instant,
}

#[derive(Debug)]
pub enum ProtocolError {
    InvalidJson(serde_json::Error),
    UnsupportedVersion(u8),
    EmptyDeviceId,
    InvalidData {
        message_type: MessageType,
        source: serde_json::Error,
    },
}

impl std::fmt::Display for ProtocolError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidJson(source) => write!(f, "invalid JSON: {source}"),
            Self::UnsupportedVersion(version) => {
                write!(f, "unsupported protocol version: {version}")
            }
            Self::EmptyDeviceId => f.write_str("device_id must not be empty"),
            Self::InvalidData {
                message_type,
                source,
            } => {
                write!(f, "invalid {message_type:?} data: {source}")
            }
        }
    }
}

impl std::error::Error for ProtocolError {}

#[derive(Debug, Eq, PartialEq)]
pub struct HandleResult {
    pub message_type: MessageType,
    pub device_id: String,
    pub message_id: u32,
    pub duplicate: bool,
}

pub struct HubState {
    nodes: HashMap<String, NodeRecord>,
    processed_events: HashSet<(String, u32)>,
    events: Vec<EventRecord>,
    offline_timeout: Duration,
}

impl HubState {
    pub fn new(offline_timeout: Duration) -> Self {
        Self {
            nodes: HashMap::new(),
            processed_events: HashSet::new(),
            events: Vec::new(),
            offline_timeout,
        }
    }

    pub fn nodes(&self) -> impl Iterator<Item = &NodeRecord> {
        self.nodes.values()
    }

    pub fn node(&self, device_id: &str) -> Option<&NodeRecord> {
        self.nodes.get(device_id)
    }

    pub fn events(&self) -> &[EventRecord] {
        &self.events
    }

    /// Process one Node datagram and update the in-memory registry.
    pub fn handle_datagram(
        &mut self,
        payload: &[u8],
        source: SocketAddr,
        now: Instant,
    ) -> Result<HandleResult, ProtocolError> {
        let message: IncomingMessage =
            serde_json::from_slice(payload).map_err(ProtocolError::InvalidJson)?;
        if message.version != PROTOCOL_VERSION {
            return Err(ProtocolError::UnsupportedVersion(message.version));
        }
        if message.device_id.trim().is_empty() {
            return Err(ProtocolError::EmptyDeviceId);
        }

        let message_type = message.message_type;
        let message_id = message.message_id;
        let device_id = message.device_id;
        let mut hello = None;
        let mut event = None;

        match message_type {
            MessageType::Hello => {
                hello = Some(serde_json::from_value::<HelloData>(message.data).map_err(
                    |source| ProtocolError::InvalidData {
                        message_type,
                        source,
                    },
                )?);
            }
            MessageType::Heartbeat => {
                ensure_object(message.data, message_type)?;
            }
            MessageType::Event => {
                event = Some(serde_json::from_value::<EventData>(message.data).map_err(
                    |source| ProtocolError::InvalidData {
                        message_type,
                        source,
                    },
                )?);
            }
            MessageType::Ack => {
                return Err(ProtocolError::InvalidData {
                    message_type,
                    source: serde_json::Error::io(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        "ACK is Hub-to-Node only",
                    )),
                });
            }
        }

        let record = self
            .nodes
            .entry(device_id.clone())
            .or_insert_with(|| NodeRecord {
                device_id: device_id.clone(),
                name: None,
                firmware_version: None,
                source,
                status: NodeStatus::Online,
                first_seen: now,
                last_seen: now,
            });
        record.source = source;
        record.status = NodeStatus::Online;
        record.last_seen = now;
        if let Some(hello) = hello {
            record.name = Some(hello.name);
            record.firmware_version = Some(hello.firmware_version);
        }

        let duplicate = if let Some(event) = event {
            let key = (device_id.clone(), message_id);
            let duplicate = !self.processed_events.insert(key);
            if !duplicate {
                self.events.push(EventRecord {
                    device_id: device_id.clone(),
                    message_id,
                    event_type: event.event_type,
                    channel: event.channel,
                    value: event.value,
                    received_at: now,
                });
            }
            duplicate
        } else {
            false
        };

        Ok(HandleResult {
            message_type,
            device_id,
            message_id,
            duplicate,
        })
    }

    /// Mark inactive Nodes OFFLINE without deleting them.
    pub fn mark_offline(&mut self, now: Instant) {
        for node in self.nodes.values_mut() {
            if now.duration_since(node.last_seen) >= self.offline_timeout {
                node.status = NodeStatus::Offline;
            }
        }
    }
}

fn ensure_object(data: serde_json::Value, message_type: MessageType) -> Result<(), ProtocolError> {
    if data.is_object() {
        Ok(())
    } else {
        Err(ProtocolError::InvalidData {
            message_type,
            source: serde_json::Error::io(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "data must be a JSON object",
            )),
        })
    }
}

pub fn encode_ack(message_id: u32) -> Vec<u8> {
    serde_json::to_vec(&Ack {
        message_type: MessageType::Ack,
        message_id,
    })
    .expect("ACK has no serialization failure")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source() -> SocketAddr {
        "192.168.1.10:5001".parse().unwrap()
    }

    #[test]
    fn registers_hello_and_updates_endpoint() {
        let now = Instant::now();
        let later = now + Duration::from_secs(1);
        let mut hub = HubState::new(Duration::from_secs(210));
        let hello = r#"{"version":1,"type":"HELLO","device_id":"CHILD-001","message_id":1,"data":{"name":"リビング","firmware_version":"1.0.0"}}"#;

        hub.handle_datagram(hello.as_bytes(), source(), now)
            .unwrap();
        let updated_source = "192.168.1.11:5001".parse().unwrap();
        hub.handle_datagram(
            br#"{"version":1,"type":"HEARTBEAT","device_id":"CHILD-001","message_id":2,"data":{}}"#,
            updated_source,
            later,
        )
        .unwrap();

        let node = hub.node("CHILD-001").unwrap();
        assert_eq!(node.name.as_deref(), Some("リビング"));
        assert_eq!(node.source, updated_source);
        assert_eq!(node.status, NodeStatus::Online);
    }

    #[test]
    fn duplicate_event_is_not_recorded_twice_but_can_be_acked_again() {
        let now = Instant::now();
        let mut hub = HubState::new(Duration::from_secs(210));
        let event = br#"{"version":1,"type":"EVENT","device_id":"CHILD-001","message_id":153,"data":{"event_type":"BUTTON","channel":1,"value":1}}"#;

        let first = hub.handle_datagram(event, source(), now).unwrap();
        let second = hub.handle_datagram(event, source(), now).unwrap();
        assert!(!first.duplicate);
        assert!(second.duplicate);
        assert_eq!(hub.events().len(), 1);
        assert_eq!(encode_ack(153), br#"{"type":"ACK","message_id":153}"#);
    }

    #[test]
    fn offline_node_returns_online_on_next_valid_packet() {
        let now = Instant::now();
        let mut hub = HubState::new(Duration::from_secs(210));
        let heartbeat =
            br#"{"version":1,"type":"HEARTBEAT","device_id":"CHILD-001","message_id":1,"data":{}}"#;
        hub.handle_datagram(heartbeat, source(), now).unwrap();
        hub.mark_offline(now + Duration::from_secs(211));
        assert_eq!(hub.node("CHILD-001").unwrap().status, NodeStatus::Offline);
        hub.handle_datagram(heartbeat, source(), now + Duration::from_secs(212))
            .unwrap();
        assert_eq!(hub.node("CHILD-001").unwrap().status, NodeStatus::Online);
    }
}
