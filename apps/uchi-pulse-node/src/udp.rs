//! Child-side UDP protocol and EVENT delivery primitives.
//!
//! The wire representation is owned by `uchi-pulse-common`. This module only
//! adds child-specific event-id generation and ACK correlation; it does not
//! interpret the meaning of an Action ID.

use core::fmt::Write;
use serde::Deserialize;

use uchi_pulse_common::codec::{decode, encode};
use uchi_pulse_common::types::text;
use uchi_pulse_common::{DeviceId, EventId, UdpMessage};

use crate::input::TriggeredAction;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProtocolError {
    DeviceIdTooLong,
    EventIdTooLong,
    SequenceExhausted,
    BufferTooSmall,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct HelloRequest {
    #[serde(rename = "type")]
    message_type: heapless::String<16>,
}

/// Returns whether a datagram is the parent-only discovery request.
pub fn is_hello_request(payload: &[u8]) -> bool {
    decode::<HelloRequest>(payload)
        .map(|request| request.message_type.as_str() == "HELLO_REQUEST")
        .unwrap_or(false)
}

/// One initial transmission followed by the configured number of retries.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RetryPolicy {
    retry_count: u8,
    attempts_started: u16,
}

impl RetryPolicy {
    pub const fn new(retry_count: u8) -> Self {
        Self {
            retry_count,
            attempts_started: 0,
        }
    }

    pub fn next_attempt(&mut self) -> Option<u8> {
        if self.attempts_started > u16::from(self.retry_count) {
            return None;
        }
        self.attempts_started += 1;
        Some(self.attempts_started as u8)
    }
}

/// Generates `boot_id + sequence` identifiers for one firmware boot.
///
/// The boot ID is supplied by the platform startup code. On Pico targets it
/// comes from the hardware-backed ring-oscillator random source exposed by
/// Embassy RP. The sequence starts at one and is advanced only when a new
/// EVENT is created, never for retransmissions.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EventIdGenerator {
    boot_id: u64,
    next_sequence: u64,
}

impl EventIdGenerator {
    pub const fn new(boot_id: u64) -> Self {
        Self {
            boot_id,
            next_sequence: 1,
        }
    }

    pub fn next_event_id(&mut self) -> Result<EventId, ProtocolError> {
        let sequence = self
            .next_sequence
            .checked_add(1)
            .ok_or(ProtocolError::SequenceExhausted)?;
        let mut event_id = EventId::new();
        write!(
            &mut event_id,
            "{:016x}-{:016x}",
            self.boot_id, self.next_sequence
        )
        .map_err(|_| ProtocolError::EventIdTooLong)?;
        self.next_sequence = sequence;
        Ok(event_id)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PendingEvent {
    message: UdpMessage,
}

impl PendingEvent {
    pub fn message(&self) -> &UdpMessage {
        &self.message
    }

    pub fn event_id(&self) -> &EventId {
        match &self.message {
            UdpMessage::Event { event_id, .. } => event_id,
            _ => unreachable!("PendingEvent must contain an EVENT"),
        }
    }

    pub fn action_id(&self) -> u32 {
        match &self.message {
            UdpMessage::Event { action_id, .. } => *action_id,
            _ => unreachable!("PendingEvent must contain an EVENT"),
        }
    }
}

pub struct NodeUdpProtocol {
    device_id: DeviceId,
    event_ids: EventIdGenerator,
}

impl NodeUdpProtocol {
    pub fn new(device_id: &str, boot_id: u64) -> Result<Self, ProtocolError> {
        Ok(Self {
            device_id: text(device_id).map_err(|_| ProtocolError::DeviceIdTooLong)?,
            event_ids: EventIdGenerator::new(boot_id),
        })
    }

    pub fn hello(&self) -> UdpMessage {
        UdpMessage::Hello {
            device_id: self.device_id.clone(),
        }
    }

    pub fn heartbeat(&self) -> UdpMessage {
        UdpMessage::Heartbeat {
            device_id: self.device_id.clone(),
        }
    }

    pub fn event_from_action(
        &mut self,
        action: TriggeredAction,
    ) -> Result<PendingEvent, ProtocolError> {
        Ok(PendingEvent {
            message: UdpMessage::Event {
                device_id: self.device_id.clone(),
                event_id: self.event_ids.next_event_id()?,
                action_id: action.action_id,
            },
        })
    }

    pub fn encode_message(
        &self,
        message: &UdpMessage,
        destination: &mut [u8],
    ) -> Result<usize, ProtocolError> {
        encode(message, destination).map_err(|_| ProtocolError::BufferTooSmall)
    }

    pub fn ack_matches(&self, event: &PendingEvent, payload: &[u8]) -> bool {
        let Ok(message) = decode::<UdpMessage>(payload) else {
            return false;
        };
        match message {
            UdpMessage::Ack {
                device_id,
                event_id,
            } => device_id == self.device_id && event_id == *event.event_id(),
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uchi_pulse_common::codec::decode;

    fn protocol() -> NodeUdpProtocol {
        NodeUdpProtocol::new("node-01", 0x1234).unwrap()
    }

    #[test]
    fn triggered_action_encodes_formal_event() {
        let mut protocol = protocol();
        let event = protocol
            .event_from_action(TriggeredAction { action_id: 42 })
            .unwrap();
        let mut buffer = [0; 256];
        let used = protocol
            .encode_message(event.message(), &mut buffer)
            .unwrap();
        assert_eq!(
            core::str::from_utf8(&buffer[..used]).unwrap(),
            r#"{"type":"EVENT","device_id":"node-01","event_id":"0000000000001234-0000000000000001","action_id":42}"#
        );
    }

    #[test]
    fn event_ids_change_only_for_new_events() {
        let mut protocol = protocol();
        let first = protocol
            .event_from_action(TriggeredAction { action_id: 10 })
            .unwrap();
        let first_retry = first.clone();
        let second = protocol
            .event_from_action(TriggeredAction { action_id: 10 })
            .unwrap();
        assert_ne!(first.event_id(), second.event_id());
        assert_eq!(first.event_id(), first_retry.event_id());
    }

    #[test]
    fn ack_requires_matching_type_device_and_event_id() {
        let mut protocol = protocol();
        let event = protocol
            .event_from_action(TriggeredAction { action_id: 10 })
            .unwrap();
        let matching = br#"{"type":"ACK","device_id":"node-01","event_id":"0000000000001234-0000000000000001"}"#;
        let wrong_event = br#"{"type":"ACK","device_id":"node-01","event_id":"other"}"#;
        let wrong_device = br#"{"type":"ACK","device_id":"node-02","event_id":"0000000000001234-0000000000000001"}"#;
        let event_message = br#"{"type":"EVENT","device_id":"node-01","event_id":"0000000000001234-0000000000000001","action_id":10}"#;
        assert!(protocol.ack_matches(&event, matching));
        assert!(!protocol.ack_matches(&event, wrong_event));
        assert!(!protocol.ack_matches(&event, wrong_device));
        assert!(!protocol.ack_matches(&event, event_message));
    }

    #[test]
    fn hello_and_heartbeat_use_formal_wire_format() {
        let protocol = protocol();
        let mut buffer = [0; 128];
        let hello_len = protocol
            .encode_message(&protocol.hello(), &mut buffer)
            .unwrap();
        assert_eq!(
            core::str::from_utf8(&buffer[..hello_len]).unwrap(),
            r#"{"type":"HELLO","device_id":"node-01"}"#
        );
        let heartbeat_len = protocol
            .encode_message(&protocol.heartbeat(), &mut buffer)
            .unwrap();
        assert_eq!(
            core::str::from_utf8(&buffer[..heartbeat_len]).unwrap(),
            r#"{"type":"HEARTBEAT","device_id":"node-01"}"#
        );
    }

    #[test]
    fn formal_ack_decodes_and_legacy_ack_does_not_match() {
        let formal = br#"{"type":"ACK","device_id":"node-01","event_id":"event-1"}"#;
        assert!(decode::<UdpMessage>(formal).is_ok());
        let legacy = br#"{"type":"ACK","message_id":1}"#;
        assert!(decode::<UdpMessage>(legacy).is_err());
    }

    #[test]
    fn recognizes_only_parent_hello_request() {
        assert!(is_hello_request(br#"{"type":"HELLO_REQUEST"}"#));
        assert!(!is_hello_request(
            br#"{"type":"HELLO_REQUEST","device_id":"node-01"}"#
        ));
        assert!(!is_hello_request(br#"{"type":"HELLO"}"#));
        assert!(!is_hello_request(br#"not-json"#));
    }

    #[test]
    fn retry_policy_allows_initial_send_and_three_retries() {
        let mut policy = RetryPolicy::new(3);
        assert_eq!(policy.next_attempt(), Some(1));
        assert_eq!(policy.next_attempt(), Some(2));
        assert_eq!(policy.next_attempt(), Some(3));
        assert_eq!(policy.next_attempt(), Some(4));
        assert_eq!(policy.next_attempt(), None);
    }
}
