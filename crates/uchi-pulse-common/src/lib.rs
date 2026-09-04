#![no_std]

pub mod action;
pub mod cdc;
pub mod codec;
pub mod types;
pub mod udp;

pub use action::{ActionDefinition, ActionStateChange, StateType, StateValue, TargetType};
pub use cdc::{CdcError, CdcErrorCode, CdcRequest, CdcResponse, CdcStatus};
pub use types::{ActionId, CommandName, DeviceId, EventId, FamilyId, RequestId};
pub use udp::{InputEvent, MessageType, UdpMessage};

/// Protocol version for the shared CDC messages.
pub const CDC_PROTOCOL_VERSION: u8 = 1;

/// Default values from `docs/parent_child_udp_communication_spec.md`.
pub const DEFAULT_ACK_TIMEOUT_MS: u32 = 60_000;
pub const DEFAULT_EVENT_RETRY_COUNT: u8 = 3;
pub const DEFAULT_HEARTBEAT_INTERVAL_SEC: u32 = 180;
pub const DEFAULT_OFFLINE_TIMEOUT_SEC: u32 = 180;

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CommunicationConfig {
    pub ack_timeout_ms: u32,
    pub event_retry_count: u8,
    pub heartbeat_interval_sec: u32,
    pub offline_timeout_sec: u32,
}

impl Default for CommunicationConfig {
    fn default() -> Self {
        Self {
            ack_timeout_ms: DEFAULT_ACK_TIMEOUT_MS,
            event_retry_count: DEFAULT_EVENT_RETRY_COUNT,
            heartbeat_interval_sec: DEFAULT_HEARTBEAT_INTERVAL_SEC,
            offline_timeout_sec: DEFAULT_OFFLINE_TIMEOUT_SEC,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn communication_defaults_match_udp_spec() {
        assert_eq!(CommunicationConfig::default().ack_timeout_ms, 60_000);
        assert_eq!(CommunicationConfig::default().event_retry_count, 3);
        assert_eq!(CommunicationConfig::default().heartbeat_interval_sec, 180);
        assert_eq!(CommunicationConfig::default().offline_timeout_sec, 180);
    }
}
