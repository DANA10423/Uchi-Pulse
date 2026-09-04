use serde::de::{MapAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize};

use crate::types::{ActionId, DeviceId, EventId};

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

/// The UDP wire format defined by the current communication specification.
/// GPIO and Input Event details intentionally do not appear here.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(tag = "type")]
pub enum UdpMessage {
    #[serde(rename = "HELLO")]
    Hello { device_id: DeviceId },
    #[serde(rename = "HEARTBEAT")]
    Heartbeat { device_id: DeviceId },
    #[serde(rename = "EVENT")]
    Event {
        device_id: DeviceId,
        event_id: EventId,
        action_id: ActionId,
    },
    #[serde(rename = "ACK")]
    Ack {
        device_id: DeviceId,
        event_id: EventId,
    },
}

impl<'de> Deserialize<'de> for UdpMessage {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_struct("UdpMessage", FIELDS, UdpMessageVisitor)
    }
}

const FIELDS: &[&str] = &["type", "device_id", "event_id", "action_id"];

struct UdpMessageVisitor;

impl<'de> Visitor<'de> for UdpMessageVisitor {
    type Value = UdpMessage;

    fn expecting(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str("a Uchi Pulse UDP message object")
    }

    fn visit_map<M>(self, mut map: M) -> Result<Self::Value, M::Error>
    where
        M: MapAccess<'de>,
    {
        let mut message_type = None;
        let mut device_id = None;
        let mut event_id = None;
        let mut action_id = None;

        while let Some(field) = map.next_key::<Field>()? {
            match field {
                Field::MessageType => message_type = Some(map.next_value()?),
                Field::DeviceId => device_id = Some(map.next_value()?),
                Field::EventId => event_id = Some(map.next_value()?),
                Field::ActionId => action_id = Some(map.next_value()?),
                Field::Ignore => {
                    let _: serde::de::IgnoredAny = map.next_value()?;
                }
            }
        }

        let message_type = message_type.ok_or_else(|| serde::de::Error::missing_field("type"))?;
        let device_id = device_id.ok_or_else(|| serde::de::Error::missing_field("device_id"))?;

        match message_type {
            MessageType::Hello if event_id.is_none() && action_id.is_none() => {
                Ok(UdpMessage::Hello { device_id })
            }
            MessageType::Heartbeat if event_id.is_none() && action_id.is_none() => {
                Ok(UdpMessage::Heartbeat { device_id })
            }
            MessageType::Event if event_id.is_some() && action_id.is_some() => {
                Ok(UdpMessage::Event {
                    device_id,
                    event_id: event_id.unwrap(),
                    action_id: action_id.unwrap(),
                })
            }
            MessageType::Ack if event_id.is_some() && action_id.is_none() => Ok(UdpMessage::Ack {
                device_id,
                event_id: event_id.unwrap(),
            }),
            _ => Err(serde::de::Error::custom(
                "invalid fields for UDP message type",
            )),
        }
    }
}

#[derive(Deserialize)]
#[serde(field_identifier, rename_all = "snake_case")]
enum Field {
    #[serde(rename = "type")]
    MessageType,
    DeviceId,
    EventId,
    ActionId,
    #[serde(other)]
    Ignore,
}

impl UdpMessage {
    pub fn message_type(&self) -> MessageType {
        match self {
            Self::Hello { .. } => MessageType::Hello,
            Self::Heartbeat { .. } => MessageType::Heartbeat,
            Self::Event { .. } => MessageType::Event,
            Self::Ack { .. } => MessageType::Ack,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum InputEvent {
    #[serde(rename = "OFF_TO_ON")]
    OffToOn,
    #[serde(rename = "ON_TO_OFF")]
    OnToOff,
    #[serde(rename = "CLICK")]
    Click,
    #[serde(rename = "DOUBLE_CLICK")]
    DoubleClick,
    #[serde(rename = "LONG_PRESS")]
    LongPress,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codec::{decode, encode};
    use crate::types::text;

    #[test]
    fn encodes_current_event_wire_format() {
        let message = UdpMessage::Event {
            device_id: text("node-01").unwrap(),
            event_id: text("boot-id-00000001").unwrap(),
            action_id: 10,
        };
        let mut buffer = [0; 256];
        let used = encode(&message, &mut buffer).unwrap();
        assert_eq!(
            core::str::from_utf8(&buffer[..used]).unwrap(),
            r#"{"type":"EVENT","device_id":"node-01","event_id":"boot-id-00000001","action_id":10}"#
        );
    }

    #[test]
    fn round_trips_all_udp_messages_and_preserves_identifiers() {
        let messages = [
            UdpMessage::Hello {
                device_id: text("node-01").unwrap(),
            },
            UdpMessage::Heartbeat {
                device_id: text("node-01").unwrap(),
            },
            UdpMessage::Event {
                device_id: text("node-01").unwrap(),
                event_id: text("boot-id-00000001").unwrap(),
                action_id: 42,
            },
            UdpMessage::Ack {
                device_id: text("node-01").unwrap(),
                event_id: text("boot-id-00000001").unwrap(),
            },
        ];

        for message in messages {
            let mut buffer = [0; 256];
            let used = encode(&message, &mut buffer).unwrap();
            let decoded: UdpMessage = decode(&buffer[..used]).unwrap();
            assert_eq!(decoded, message);
        }
    }

    #[test]
    fn rejects_old_message_id_and_data_format() {
        let old = br#"{"version":1,"type":"EVENT","device_id":"node-01","message_id":10,"data":{"event_type":"BUTTON","channel":1,"value":1}}"#;
        assert!(crate::codec::decode::<UdpMessage>(old).is_err());
    }

    #[test]
    fn rejects_missing_required_event_fields() {
        let missing_event_id = br#"{"type":"EVENT","device_id":"node-01","action_id":10}"#;
        let missing_action_id = br#"{"type":"EVENT","device_id":"node-01","event_id":"boot-1"}"#;
        assert!(crate::codec::decode::<UdpMessage>(missing_event_id).is_err());
        assert!(crate::codec::decode::<UdpMessage>(missing_action_id).is_err());
    }

    #[test]
    fn rejects_unknown_message_type() {
        let invalid = br#"{"type":"UNKNOWN","device_id":"node-01"}"#;
        assert!(crate::codec::decode::<UdpMessage>(invalid).is_err());
    }
}
