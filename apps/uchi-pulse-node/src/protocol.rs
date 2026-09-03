//! Wire format for the Hub/Node UDP protocol described in `docs/parent_child_udp_communication_spec.md`.

use serde::{Deserialize, Serialize};

pub const PROTOCOL_VERSION: u8 = 1;
pub type MessageId = u32;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
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

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum EventType {
    #[serde(rename = "BUTTON")]
    Button,
    #[serde(rename = "SENSOR")]
    Sensor,
    #[serde(rename = "STATE")]
    State,
}

#[derive(Serialize)]
struct HelloData<'a> {
    name: &'a str,
    firmware_version: &'a str,
}

#[derive(Serialize)]
struct EmptyData {}

#[derive(Serialize)]
struct EventData {
    event_type: EventType,
    channel: u8,
    value: i32,
}

#[derive(Serialize)]
struct Message<'a, D> {
    version: u8,
    #[serde(rename = "type")]
    message_type: MessageType,
    device_id: &'a str,
    message_id: MessageId,
    data: D,
}

#[derive(Debug, Deserialize)]
pub struct Ack {
    #[serde(rename = "type")]
    pub message_type: MessageType,
    pub message_id: MessageId,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EncodeError {
    BufferTooSmall,
}

pub fn encode_hello(
    dst: &mut [u8],
    device_id: &str,
    message_id: MessageId,
    name: &str,
    firmware_version: &str,
) -> Result<usize, EncodeError> {
    encode(
        dst,
        Message {
            version: PROTOCOL_VERSION,
            message_type: MessageType::Hello,
            device_id,
            message_id,
            data: HelloData {
                name,
                firmware_version,
            },
        },
    )
}

pub fn encode_heartbeat(
    dst: &mut [u8],
    device_id: &str,
    message_id: MessageId,
) -> Result<usize, EncodeError> {
    encode(
        dst,
        Message {
            version: PROTOCOL_VERSION,
            message_type: MessageType::Heartbeat,
            device_id,
            message_id,
            data: EmptyData {},
        },
    )
}

pub fn encode_event(
    dst: &mut [u8],
    device_id: &str,
    message_id: MessageId,
    event_type: EventType,
    channel: u8,
    value: i32,
) -> Result<usize, EncodeError> {
    encode(
        dst,
        Message {
            version: PROTOCOL_VERSION,
            message_type: MessageType::Event,
            device_id,
            message_id,
            data: EventData {
                event_type,
                channel,
                value,
            },
        },
    )
}

fn encode<D: Serialize>(dst: &mut [u8], message: Message<'_, D>) -> Result<usize, EncodeError> {
    serde_json_core::to_slice(&message, dst).map_err(|_| EncodeError::BufferTooSmall)
}

pub fn decode_ack(payload: &[u8]) -> Option<Ack> {
    let (ack, used) = serde_json_core::from_slice::<Ack>(payload).ok()?;
    (used == payload.len() && ack.message_type == MessageType::Ack).then_some(ack)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_spec_compatible_event() {
        let mut buf = [0; 128];
        let len = encode_event(&mut buf, "CHILD-001", 153, EventType::Button, 1, 1).unwrap();
        let json = core::str::from_utf8(&buf[..len]).unwrap();
        assert_eq!(
            json,
            r#"{"version":1,"type":"EVENT","device_id":"CHILD-001","message_id":153,"data":{"event_type":"BUTTON","channel":1,"value":1}}"#
        );
    }

    #[test]
    fn accepts_only_matching_ack_type() {
        let ack = decode_ack(br#"{"type":"ACK","message_id":153}"#).unwrap();
        assert_eq!(ack.message_id, 153);
        assert!(decode_ack(br#"{"type":"EVENT","message_id":153}"#).is_none());
    }
}
