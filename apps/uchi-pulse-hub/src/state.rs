use std::collections::HashMap;
use std::time::{Duration, Instant};

use uchi_pulse_common::codec::{self, CodecError};
use uchi_pulse_common::udp::UdpMessage;

use crate::db::{Database, DatabaseError};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CommunicationStatus {
    InitialWait,
    Online,
    Offline,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeviceState {
    pub device_id: String,
    pub status: CommunicationStatus,
    /// The in-memory start point for INITIAL_WAIT timeout evaluation.
    pub initial_wait_started_at: Instant,
    /// None until a valid HELLO, HEARTBEAT, or EVENT is observed.
    pub last_seen_at: Option<Instant>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StateUpdateError {
    InvalidMessage(CodecError),
    EmptyDeviceId,
}

pub struct CommunicationStateManager {
    devices: HashMap<String, DeviceState>,
    offline_timeout: Duration,
}

impl CommunicationStateManager {
    /// Builds the runtime state only from enabled devices in the persistent DB.
    pub fn from_database(
        database: &Database,
        initial_wait_started_at: Instant,
        offline_timeout: Duration,
    ) -> Result<Self, DatabaseError> {
        let devices = database
            .list_enabled_devices()?
            .into_iter()
            .map(|device| {
                let state = DeviceState {
                    device_id: device.device_id.clone(),
                    status: CommunicationStatus::InitialWait,
                    initial_wait_started_at,
                    last_seen_at: None,
                };
                (device.device_id, state)
            })
            .collect();

        Ok(Self {
            devices,
            offline_timeout,
        })
    }

    pub fn device(&self, device_id: &str) -> Option<&DeviceState> {
        self.devices.get(device_id)
    }

    pub fn devices(&self) -> impl Iterator<Item = &DeviceState> {
        self.devices.values()
    }

    pub fn len(&self) -> usize {
        self.devices.len()
    }

    pub fn is_empty(&self) -> bool {
        self.devices.is_empty()
    }

    /// Decodes and validates a current UDP message before changing state.
    /// Returns true only when a registered enabled device was updated.
    pub fn observe_payload(
        &mut self,
        payload: &[u8],
        now: Instant,
    ) -> Result<bool, StateUpdateError> {
        let message: UdpMessage =
            codec::decode(payload).map_err(StateUpdateError::InvalidMessage)?;
        self.observe_message(&message, now)
    }

    /// Updates state for valid node-originated messages only. ACK is valid UDP
    /// syntax but is Hub-to-Node traffic and therefore does not mark a device
    /// online.
    pub fn observe_message(
        &mut self,
        message: &UdpMessage,
        now: Instant,
    ) -> Result<bool, StateUpdateError> {
        let device_id = match message {
            UdpMessage::Hello { device_id }
            | UdpMessage::Heartbeat { device_id }
            | UdpMessage::Event { device_id, .. }
            | UdpMessage::Ack { device_id, .. } => device_id,
        };
        if device_id.trim().is_empty() {
            return Err(StateUpdateError::EmptyDeviceId);
        }

        if matches!(message, UdpMessage::Ack { .. }) {
            return Ok(false);
        }

        let Some(state) = self.devices.get_mut(device_id.as_str()) else {
            return Ok(false);
        };
        state.status = CommunicationStatus::Online;
        state.last_seen_at = Some(now);
        Ok(true)
    }

    /// Marks registered devices offline when either their first-wait deadline
    /// or their last valid reception deadline has elapsed.
    pub fn mark_offline(&mut self, now: Instant) {
        for state in self.devices.values_mut() {
            let reference = state.last_seen_at.unwrap_or(state.initial_wait_started_at);
            let elapsed = now.checked_duration_since(reference).unwrap_or_default();
            if elapsed >= self.offline_timeout {
                state.status = CommunicationStatus::Offline;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{Database, DeviceRecord};
    use uchi_pulse_common::codec::encode;
    use uchi_pulse_common::types::text;

    fn device(device_id: &str, enabled: bool) -> DeviceRecord {
        DeviceRecord {
            device_id: device_id.into(),
            name: device_id.into(),
            device_type: "FAMILY".into(),
            registered_at: "2026-09-04T00:00:00Z".into(),
            updated_at: "2026-09-04T00:00:00Z".into(),
            enabled,
        }
    }

    fn database_with_devices() -> Database {
        let database = Database::open_in_memory().unwrap();
        database
            .insert_device(&device("enabled-node", true))
            .unwrap();
        database
            .insert_device(&device("disabled-node", false))
            .unwrap();
        database
    }

    fn manager(start: Instant) -> CommunicationStateManager {
        CommunicationStateManager::from_database(
            &database_with_devices(),
            start,
            Duration::from_secs(10),
        )
        .unwrap()
    }

    fn encoded(message: UdpMessage) -> Vec<u8> {
        let mut buffer = [0; 256];
        let used = encode(&message, &mut buffer).unwrap();
        buffer[..used].to_vec()
    }

    fn hello() -> UdpMessage {
        UdpMessage::Hello {
            device_id: text("enabled-node").unwrap(),
        }
    }

    #[test]
    fn initializes_only_enabled_devices_as_initial_wait() {
        let start = Instant::now();
        let manager = manager(start);
        assert_eq!(manager.len(), 1);
        let state = manager.device("enabled-node").unwrap();
        assert_eq!(state.status, CommunicationStatus::InitialWait);
        assert_eq!(state.last_seen_at, None);
        assert_eq!(state.initial_wait_started_at, start);
        assert!(manager.device("disabled-node").is_none());
    }

    #[test]
    fn normal_hello_heartbeat_and_event_move_initial_wait_online() {
        let start = Instant::now();
        for message in [
            hello(),
            UdpMessage::Heartbeat {
                device_id: text("enabled-node").unwrap(),
            },
            UdpMessage::Event {
                device_id: text("enabled-node").unwrap(),
                event_id: text("boot-1").unwrap(),
                action_id: 10,
            },
        ] {
            let mut manager = manager(start);
            let received = start + Duration::from_secs(1);
            assert!(
                manager
                    .observe_payload(&encoded(message), received)
                    .unwrap()
            );
            let state = manager.device("enabled-node").unwrap();
            assert_eq!(state.status, CommunicationStatus::Online);
            assert_eq!(state.last_seen_at, Some(received));
        }
    }

    #[test]
    fn initial_wait_times_out_without_a_valid_reception() {
        let start = Instant::now();
        let mut manager = manager(start);
        manager.mark_offline(start + Duration::from_secs(10));
        assert_eq!(
            manager.device("enabled-node").unwrap().status,
            CommunicationStatus::Offline
        );
        assert_eq!(manager.device("enabled-node").unwrap().last_seen_at, None);
    }

    #[test]
    fn online_is_maintained_by_normal_reception_and_times_out_from_last_seen() {
        let start = Instant::now();
        let mut manager = manager(start);
        let first = start + Duration::from_secs(1);
        manager.observe_payload(&encoded(hello()), first).unwrap();
        manager.mark_offline(start + Duration::from_secs(9));
        assert_eq!(
            manager.device("enabled-node").unwrap().status,
            CommunicationStatus::Online
        );

        let second = start + Duration::from_secs(11);
        manager
            .observe_payload(
                &encoded(UdpMessage::Heartbeat {
                    device_id: text("enabled-node").unwrap(),
                }),
                second,
            )
            .unwrap();
        assert_eq!(
            manager.device("enabled-node").unwrap().status,
            CommunicationStatus::Online
        );
        assert_eq!(
            manager.device("enabled-node").unwrap().last_seen_at,
            Some(second)
        );

        manager.mark_offline(start + Duration::from_secs(22));
        assert_eq!(
            manager.device("enabled-node").unwrap().status,
            CommunicationStatus::Offline
        );
    }

    #[test]
    fn offline_device_returns_online_and_remains_managed() {
        let start = Instant::now();
        let mut manager = manager(start);
        manager.mark_offline(start + Duration::from_secs(10));
        let received = start + Duration::from_secs(11);
        assert!(
            manager
                .observe_payload(&encoded(hello()), received)
                .unwrap()
        );
        assert_eq!(manager.len(), 1);
        assert_eq!(
            manager.device("enabled-node").unwrap().status,
            CommunicationStatus::Online
        );
    }

    #[test]
    fn invalid_message_does_not_update_state() {
        let start = Instant::now();
        let mut manager = manager(start);
        let invalid = br#"{"type":"EVENT","device_id":"enabled-node","action_id":10}"#;
        assert!(matches!(
            manager.observe_payload(invalid, start + Duration::from_secs(1)),
            Err(StateUpdateError::InvalidMessage(_))
        ));
        let state = manager.device("enabled-node").unwrap();
        assert_eq!(state.status, CommunicationStatus::InitialWait);
        assert_eq!(state.last_seen_at, None);
    }

    #[test]
    fn unknown_and_disabled_devices_are_not_added_or_updated() {
        let start = Instant::now();
        let mut manager = manager(start);
        let unknown = UdpMessage::Hello {
            device_id: text("unknown-node").unwrap(),
        };
        assert!(
            !manager
                .observe_payload(&encoded(unknown), start + Duration::from_secs(1))
                .unwrap()
        );
        assert_eq!(manager.len(), 1);
        assert!(manager.device("unknown-node").is_none());
        assert!(manager.device("disabled-node").is_none());
    }

    #[test]
    fn ack_does_not_mark_a_device_online() {
        let start = Instant::now();
        let mut manager = manager(start);
        let ack = UdpMessage::Ack {
            device_id: text("enabled-node").unwrap(),
            event_id: text("boot-1").unwrap(),
        };
        assert!(
            !manager
                .observe_payload(&encoded(ack), start + Duration::from_secs(1))
                .unwrap()
        );
        assert_eq!(
            manager.device("enabled-node").unwrap().status,
            CommunicationStatus::InitialWait
        );
    }
}
