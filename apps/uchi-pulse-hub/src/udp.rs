use std::convert::Infallible;
use std::fmt;
use std::time::{Duration, Instant};

use uchi_pulse_common::codec::{self, CodecError};
use uchi_pulse_common::types::{ActionId, DeviceId, EventId};
use uchi_pulse_common::udp::UdpMessage;

use crate::action::{ActionEngine, ActionError};
use crate::db::{Database, DatabaseError, EventRecord};
use crate::state::{CommunicationStateManager, StateUpdateError};

pub const HELLO_REQUEST_TYPE: &str = "HELLO_REQUEST";

#[derive(serde::Serialize)]
struct HelloRequest {
    #[serde(rename = "type")]
    message_type: &'static str,
}

/// Encodes the parent-only discovery request. It is not one of the four
/// child-to-parent UDP messages and is never accepted as a child HELLO.
pub fn encode_hello_request(destination: &mut [u8]) -> Result<usize, CodecError> {
    codec::encode(
        &HelloRequest {
            message_type: HELLO_REQUEST_TYPE,
        },
        destination,
    )
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EventContext<'a> {
    pub device_id: &'a str,
    pub event_id: &'a str,
    pub action_id: ActionId,
    pub payload: &'a str,
}

pub trait ActionHandler {
    type Error: std::error::Error + Send + Sync + 'static;

    /// Validates an Action before the EVENT becomes accepted and durable.
    fn validate_event(&self, _action_id: ActionId) -> Result<(), Self::Error> {
        Ok(())
    }

    fn handle_event(&mut self, event: EventContext<'_>) -> Result<(), Self::Error>;
}

#[derive(Default)]
pub struct NoopActionHandler;

impl ActionHandler for NoopActionHandler {
    type Error = Infallible;

    fn handle_event(&mut self, _event: EventContext<'_>) -> Result<(), Self::Error> {
        Ok(())
    }
}

impl ActionHandler for ActionEngine {
    type Error = ActionError;

    fn validate_event(&self, action_id: ActionId) -> Result<(), Self::Error> {
        ActionEngine::validate_event(self, action_id).map(|_| ())
    }

    fn handle_event(&mut self, event: EventContext<'_>) -> Result<(), Self::Error> {
        self.execute(event.action_id).map(|_| ())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ActionExecutionStatus {
    Applied,
    Failed(String),
    SkippedDuplicate,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PacketOutcome {
    Ignored,
    HelloAccepted {
        device_id: String,
    },
    HeartbeatAccepted {
        device_id: String,
    },
    EventAccepted {
        device_id: String,
        event_id: String,
        duplicate: bool,
        action_status: ActionExecutionStatus,
        ack: Box<UdpMessage>,
    },
}

#[derive(Debug)]
pub enum UdpProcessingError<E> {
    InvalidMessage(CodecError),
    InvalidDeviceId,
    InvalidEventId,
    State(StateUpdateError),
    Database(DatabaseError),
    ActionValidation(E),
}

impl<E: fmt::Display> fmt::Display for UdpProcessingError<E> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidMessage(error) => write!(formatter, "invalid UDP message: {error:?}"),
            Self::InvalidDeviceId => formatter.write_str("device_id must not be empty"),
            Self::InvalidEventId => formatter.write_str("event_id must not be empty"),
            Self::State(error) => write!(formatter, "state update failed: {error:?}"),
            Self::Database(error) => write!(formatter, "database operation failed: {error}"),
            Self::ActionValidation(error) => {
                write!(formatter, "Action validation failed: {error}")
            }
        }
    }
}

impl<E: std::error::Error + 'static> std::error::Error for UdpProcessingError<E> {}

pub struct HubUdpProcessor<A> {
    database: Database,
    state: CommunicationStateManager,
    action_handler: A,
}

impl<A: ActionHandler> HubUdpProcessor<A> {
    pub fn from_database(
        database: Database,
        initial_wait_started_at: Instant,
        offline_timeout: Duration,
        action_handler: A,
    ) -> Result<Self, DatabaseError> {
        let state = CommunicationStateManager::from_database(
            &database,
            initial_wait_started_at,
            offline_timeout,
        )?;
        Ok(Self {
            database,
            state,
            action_handler,
        })
    }

    pub fn database(&self) -> &Database {
        &self.database
    }

    pub fn state(&self) -> &CommunicationStateManager {
        &self.state
    }

    pub fn state_mut(&mut self) -> &mut CommunicationStateManager {
        &mut self.state
    }

    pub fn action_handler(&self) -> &A {
        &self.action_handler
    }

    pub fn mark_offline(&mut self, now: Instant) {
        self.state.mark_offline(now);
    }

    /// Processes one already-received UDP datagram without performing socket I/O.
    pub fn process_datagram(
        &mut self,
        payload: &[u8],
        now: Instant,
        received_at: &str,
    ) -> Result<PacketOutcome, UdpProcessingError<A::Error>> {
        let message: UdpMessage =
            codec::decode(payload).map_err(UdpProcessingError::InvalidMessage)?;
        validate_message(&message)?;

        let device_id = device_id_of(&message);
        if !self.is_enabled_managed_device(device_id)? {
            return Ok(PacketOutcome::Ignored);
        }

        if matches!(message, UdpMessage::Ack { .. }) {
            return Ok(PacketOutcome::Ignored);
        }

        if let UdpMessage::Event {
            device_id,
            event_id,
            action_id,
        } = &message
        {
            if self
                .database
                .get_event(device_id, event_id)
                .map_err(UdpProcessingError::Database)?
                .is_some()
            {
                self.state
                    .observe_message(&message, now)
                    .map_err(UdpProcessingError::State)?;
                return Ok(PacketOutcome::EventAccepted {
                    device_id: device_id.to_string(),
                    event_id: event_id.to_string(),
                    duplicate: true,
                    action_status: ActionExecutionStatus::SkippedDuplicate,
                    ack: Box::new(UdpMessage::Ack {
                        device_id: device_id.clone(),
                        event_id: event_id.clone(),
                    }),
                });
            }
            self.action_handler
                .validate_event(*action_id)
                .map_err(UdpProcessingError::ActionValidation)?;
        }

        self.state
            .observe_message(&message, now)
            .map_err(UdpProcessingError::State)?;

        match message {
            UdpMessage::Hello { device_id } => Ok(PacketOutcome::HelloAccepted {
                device_id: device_id.to_string(),
            }),
            UdpMessage::Heartbeat { device_id } => Ok(PacketOutcome::HeartbeatAccepted {
                device_id: device_id.to_string(),
            }),
            UdpMessage::Event {
                device_id,
                event_id,
                action_id,
            } => self.process_event(payload, received_at, device_id, event_id, action_id),
            UdpMessage::Ack { .. } => Ok(PacketOutcome::Ignored),
        }
    }

    fn process_event(
        &mut self,
        payload: &[u8],
        received_at: &str,
        device_id: DeviceId,
        event_id: EventId,
        action_id: ActionId,
    ) -> Result<PacketOutcome, UdpProcessingError<A::Error>> {
        let device_id_text = device_id.as_str();
        let event_id_text = event_id.as_str();
        let payload_text = core::str::from_utf8(payload)
            .map_err(|_| UdpProcessingError::InvalidMessage(CodecError::InvalidJson))?;
        let inserted = self
            .database
            .insert_event(&EventRecord {
                id: None,
                received_at: received_at.into(),
                device_id: device_id_text.into(),
                event_id: event_id_text.into(),
                payload: payload_text.into(),
            })
            .map_err(UdpProcessingError::Database)?;

        let ack = Box::new(UdpMessage::Ack {
            device_id: device_id.clone(),
            event_id: event_id.clone(),
        });
        if !inserted {
            return Ok(PacketOutcome::EventAccepted {
                device_id: device_id.to_string(),
                event_id: event_id.to_string(),
                duplicate: true,
                action_status: ActionExecutionStatus::SkippedDuplicate,
                ack,
            });
        }

        let action_status = self
            .action_handler
            .handle_event(EventContext {
                device_id: device_id_text,
                event_id: event_id_text,
                action_id,
                payload: payload_text,
            })
            .map_or_else(
                |error| ActionExecutionStatus::Failed(error.to_string()),
                |_| ActionExecutionStatus::Applied,
            );

        Ok(PacketOutcome::EventAccepted {
            device_id: device_id.to_string(),
            event_id: event_id.to_string(),
            duplicate: false,
            action_status,
            ack,
        })
    }

    fn is_enabled_managed_device(
        &self,
        device_id: &str,
    ) -> Result<bool, UdpProcessingError<A::Error>> {
        if self.state.device(device_id).is_none() {
            return Ok(false);
        }
        self.database
            .get_device(device_id)
            .map(|device| device.is_some_and(|device| device.enabled))
            .map_err(UdpProcessingError::Database)
    }
}

fn validate_message<A>(message: &UdpMessage) -> Result<(), UdpProcessingError<A>> {
    if device_id_of(message).trim().is_empty() {
        return Err(UdpProcessingError::InvalidDeviceId);
    }
    if let UdpMessage::Event { event_id, .. } = message
        && event_id.trim().is_empty()
    {
        return Err(UdpProcessingError::InvalidEventId);
    }
    Ok(())
}

fn device_id_of(message: &UdpMessage) -> &str {
    match message {
        UdpMessage::Hello { device_id }
        | UdpMessage::Heartbeat { device_id }
        | UdpMessage::Event { device_id, .. }
        | UdpMessage::Ack { device_id, .. } => device_id.as_str(),
    }
}

#[cfg(test)]
mod tests {
    use std::fmt;
    use std::sync::{Arc, Mutex};

    use super::*;
    use crate::action::{ActionEngine, ActionStateScope};
    use crate::db::{ActionRecord, ActionStateChangeRecord, DeviceRecord, FamilyRecord};
    use crate::state::CommunicationStatus;
    use uchi_pulse_common::codec::encode;
    use uchi_pulse_common::types::text;
    use uchi_pulse_common::{StateType, StateValue, TargetType};

    #[derive(Clone, Debug, Eq, PartialEq)]
    struct Invocation {
        device_id: String,
        event_id: String,
        action_id: ActionId,
    }

    #[derive(Clone, Default)]
    struct RecordingActionHandler {
        invocations: Arc<Mutex<Vec<Invocation>>>,
    }

    impl ActionHandler for RecordingActionHandler {
        type Error = Infallible;

        fn handle_event(&mut self, event: EventContext<'_>) -> Result<(), Self::Error> {
            self.invocations.lock().unwrap().push(Invocation {
                device_id: event.device_id.into(),
                event_id: event.event_id.into(),
                action_id: event.action_id,
            });
            Ok(())
        }
    }

    #[derive(Debug)]
    struct ActionFailure;

    impl fmt::Display for ActionFailure {
        fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str("test action failure")
        }
    }

    impl std::error::Error for ActionFailure {}

    #[derive(Default)]
    struct FailingActionHandler {
        calls: usize,
    }

    impl ActionHandler for FailingActionHandler {
        type Error = ActionFailure;

        fn handle_event(&mut self, _event: EventContext<'_>) -> Result<(), Self::Error> {
            self.calls += 1;
            Err(ActionFailure)
        }
    }

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

    fn processor() -> (
        HubUdpProcessor<RecordingActionHandler>,
        Arc<Mutex<Vec<Invocation>>>,
    ) {
        let database = Database::open_in_memory().unwrap();
        database
            .insert_device(&device("enabled-node", true))
            .unwrap();
        database
            .insert_device(&device("disabled-node", false))
            .unwrap();
        database
            .insert_family(&FamilyRecord {
                family_id: 1,
                display_name: "家族".into(),
                enabled: true,
            })
            .unwrap();
        let invocations = Arc::new(Mutex::new(Vec::new()));
        let handler = RecordingActionHandler {
            invocations: invocations.clone(),
        };
        let processor = HubUdpProcessor::from_database(
            database,
            Instant::now(),
            Duration::from_secs(10),
            handler,
        )
        .unwrap();
        (processor, invocations)
    }

    fn action_processor(
        action_id: ActionId,
        target_type: TargetType,
        target_family_id: Option<u32>,
        enabled: bool,
        changes: &[ActionStateChangeRecord],
    ) -> HubUdpProcessor<ActionEngine> {
        let database = Database::open_in_memory().unwrap();
        database
            .insert_device(&device("enabled-node", true))
            .unwrap();
        database
            .insert_family(&FamilyRecord {
                family_id: 1,
                display_name: "家族".into(),
                enabled: true,
            })
            .unwrap();
        database
            .insert_action(&ActionRecord {
                action_id,
                action_name: "テストAction".into(),
                target_type,
                target_family_id,
                web_message: Some("Web表示メッセージ".into()),
                enabled,
            })
            .unwrap();
        for change in changes {
            database.insert_action_state_change(change).unwrap();
        }
        let action_engine = ActionEngine::new(database.clone());
        HubUdpProcessor::from_database(
            database,
            Instant::now(),
            Duration::from_secs(10),
            action_engine,
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

    fn event() -> UdpMessage {
        UdpMessage::Event {
            device_id: text("enabled-node").unwrap(),
            event_id: text("boot-1-1").unwrap(),
            action_id: 42,
        }
    }

    fn event_with(action_id: ActionId, event_id: &str) -> UdpMessage {
        UdpMessage::Event {
            device_id: text("enabled-node").unwrap(),
            event_id: text(event_id).unwrap(),
            action_id,
        }
    }

    #[test]
    fn accepts_hello_and_heartbeat_and_updates_online() {
        let (mut processor, _) = processor();
        let now = Instant::now();
        assert!(matches!(
            processor
                .process_datagram(&encoded(hello()), now, "2026-09-04T00:00:00Z")
                .unwrap(),
            PacketOutcome::HelloAccepted { .. }
        ));
        assert_eq!(
            processor.state().device("enabled-node").unwrap().status,
            CommunicationStatus::Online
        );

        assert!(matches!(
            processor
                .process_datagram(
                    &encoded(UdpMessage::Heartbeat {
                        device_id: text("enabled-node").unwrap(),
                    }),
                    now,
                    "2026-09-04T00:00:01Z"
                )
                .unwrap(),
            PacketOutcome::HeartbeatAccepted { .. }
        ));
    }

    #[test]
    fn new_event_is_saved_action_is_called_and_ack_has_formal_fields() {
        let (mut processor, invocations) = processor();
        let now = Instant::now();
        let result = processor
            .process_datagram(&encoded(event()), now, "2026-09-04T00:00:00Z")
            .unwrap();
        let PacketOutcome::EventAccepted {
            device_id,
            event_id,
            duplicate,
            action_status,
            ack,
        } = result
        else {
            panic!("expected event outcome");
        };
        assert_eq!(device_id, "enabled-node");
        assert_eq!(event_id, "boot-1-1");
        assert!(!duplicate);
        assert_eq!(action_status, ActionExecutionStatus::Applied);
        assert_eq!(
            *ack,
            UdpMessage::Ack {
                device_id: text("enabled-node").unwrap(),
                event_id: text("boot-1-1").unwrap(),
            }
        );
        let mut ack_buffer = [0; 128];
        let ack_length = codec::encode(&ack, &mut ack_buffer).unwrap();
        assert_eq!(
            core::str::from_utf8(&ack_buffer[..ack_length]).unwrap(),
            r#"{"type":"ACK","device_id":"enabled-node","event_id":"boot-1-1"}"#
        );
        assert!(
            processor
                .database()
                .get_event("enabled-node", "boot-1-1")
                .unwrap()
                .is_some()
        );
        assert_eq!(invocations.lock().unwrap().len(), 1);
        assert_eq!(invocations.lock().unwrap()[0].action_id, 42);
    }

    #[test]
    fn duplicate_event_is_not_saved_again_or_sent_to_action_but_ack_is_returned() {
        let (mut processor, invocations) = processor();
        let payload = encoded(event());
        let now = Instant::now();
        let first = processor
            .process_datagram(&payload, now, "2026-09-04T00:00:00Z")
            .unwrap();
        let second = processor
            .process_datagram(&payload, now, "2026-09-04T00:00:01Z")
            .unwrap();
        assert!(matches!(
            first,
            PacketOutcome::EventAccepted {
                duplicate: false,
                ..
            }
        ));
        assert!(matches!(
            second,
            PacketOutcome::EventAccepted {
                duplicate: true,
                action_status: ActionExecutionStatus::SkippedDuplicate,
                ..
            }
        ));
        assert_eq!(invocations.lock().unwrap().len(), 1);
        assert_eq!(
            processor
                .database()
                .get_event("enabled-node", "boot-1-1")
                .unwrap()
                .unwrap()
                .received_at,
            "2026-09-04T00:00:00Z"
        );
    }

    #[test]
    fn ack_does_not_update_state() {
        let (mut processor, _) = processor();
        let message = UdpMessage::Ack {
            device_id: text("enabled-node").unwrap(),
            event_id: text("boot-1-1").unwrap(),
        };
        assert_eq!(
            processor
                .process_datagram(&encoded(message), Instant::now(), "2026-09-04T00:00:00Z")
                .unwrap(),
            PacketOutcome::Ignored
        );
        assert_eq!(
            processor.state().device("enabled-node").unwrap().status,
            CommunicationStatus::InitialWait
        );
    }

    #[test]
    fn invalid_and_old_messages_do_not_update_state() {
        let (mut processor, _) = processor();
        let invalid = br#"{"type":"EVENT","device_id":"enabled-node","action_id":10}"#;
        assert!(matches!(
            processor.process_datagram(invalid, Instant::now(), "2026-09-04T00:00:00Z"),
            Err(UdpProcessingError::InvalidMessage(_))
        ));
        let old = br#"{"version":1,"type":"EVENT","device_id":"enabled-node","message_id":10,"data":{"event_type":"BUTTON","channel":1,"value":1}}"#;
        assert!(matches!(
            processor.process_datagram(old, Instant::now(), "2026-09-04T00:00:00Z"),
            Err(UdpProcessingError::InvalidMessage(_))
        ));
        assert_eq!(
            processor.state().device("enabled-node").unwrap().status,
            CommunicationStatus::InitialWait
        );
        assert!(
            processor
                .database()
                .get_event("enabled-node", "boot-1-1")
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn unknown_and_disabled_devices_are_ignored_without_ack_or_history() {
        let (mut processor, _) = processor();
        for device_id in ["unknown-node", "disabled-node"] {
            let payload = encoded(UdpMessage::Hello {
                device_id: text(device_id).unwrap(),
            });
            assert_eq!(
                processor
                    .process_datagram(&payload, Instant::now(), "2026-09-04T00:00:00Z")
                    .unwrap(),
                PacketOutcome::Ignored
            );
        }
        let event_payload = encoded(UdpMessage::Event {
            device_id: text("disabled-node").unwrap(),
            event_id: text("boot-disabled-1").unwrap(),
            action_id: 1,
        });
        assert_eq!(
            processor
                .process_datagram(&event_payload, Instant::now(), "2026-09-04T00:00:00Z")
                .unwrap(),
            PacketOutcome::Ignored
        );
        assert!(
            processor
                .database()
                .get_event("disabled-node", "boot-disabled-1")
                .unwrap()
                .is_none()
        );
        assert_eq!(processor.state().len(), 1);
    }

    #[test]
    fn hello_request_is_a_separate_parent_to_child_message() {
        let mut buffer = [0; 64];
        let used = encode_hello_request(&mut buffer).unwrap();
        assert_eq!(
            core::str::from_utf8(&buffer[..used]).unwrap(),
            r#"{"type":"HELLO_REQUEST"}"#
        );

        let (mut processor, _) = processor();
        assert!(matches!(
            processor.process_datagram(&buffer[..used], Instant::now(), "2026-09-04T00:00:00Z"),
            Err(UdpProcessingError::InvalidMessage(_))
        ));
    }

    #[test]
    fn action_engine_accepts_family_action_and_applies_state_changes() {
        let changes = [
            ActionStateChangeRecord {
                action_id: 42,
                state_type: StateType::EntryPermission,
                state_value: StateValue::Ok,
            },
            ActionStateChangeRecord {
                action_id: 42,
                state_type: StateType::MealNotice,
                state_value: StateValue::On,
            },
        ];
        let mut processor = action_processor(42, TargetType::Family, Some(1), true, &changes);
        let result = processor
            .process_datagram(&encoded(event()), Instant::now(), "2026-09-04T00:00:00Z")
            .unwrap();
        assert!(matches!(
            result,
            PacketOutcome::EventAccepted {
                duplicate: false,
                action_status: ActionExecutionStatus::Applied,
                ..
            }
        ));
        assert_eq!(
            processor
                .action_handler()
                .state()
                .get(ActionStateScope::Family(1), StateType::EntryPermission),
            Some(StateValue::Ok)
        );
        assert_eq!(
            processor
                .action_handler()
                .state()
                .get(ActionStateScope::Family(1), StateType::MealNotice),
            Some(StateValue::On)
        );
        assert_eq!(
            processor
                .action_handler()
                .validate_event(42)
                .unwrap()
                .action
                .web_message
                .as_deref(),
            Some("Web表示メッセージ")
        );
    }

    #[test]
    fn action_engine_accepts_common_action_and_notification_only_action() {
        let mut common = action_processor(50, TargetType::Common, None, true, &[]);
        let result = common
            .process_datagram(
                &encoded(event_with(50, "common-1")),
                Instant::now(),
                "2026-09-04T00:00:00Z",
            )
            .unwrap();
        assert!(matches!(
            result,
            PacketOutcome::EventAccepted {
                action_status: ActionExecutionStatus::Applied,
                ..
            }
        ));
        assert_eq!(common.action_handler().state().values().count(), 0);

        let mut notification_only = action_processor(51, TargetType::Family, Some(1), true, &[]);
        notification_only
            .process_datagram(
                &encoded(event_with(51, "inquiry-1")),
                Instant::now(),
                "2026-09-04T00:00:00Z",
            )
            .unwrap();
        assert_eq!(
            notification_only.action_handler().state().values().count(),
            0
        );
        assert_eq!(
            notification_only
                .state()
                .device("enabled-node")
                .unwrap()
                .status,
            CommunicationStatus::Online
        );
    }

    #[test]
    fn unavailable_action_is_rejected_before_history_state_or_ack() {
        let mut missing = action_processor(52, TargetType::Common, None, true, &[]);
        let missing_result = missing.process_datagram(
            &encoded(event_with(999, "missing-action")),
            Instant::now(),
            "2026-09-04T00:00:00Z",
        );
        assert!(matches!(
            missing_result,
            Err(UdpProcessingError::ActionValidation(ActionError::NotFound(
                999
            )))
        ));
        assert!(
            missing
                .database()
                .get_event("enabled-node", "missing-action")
                .unwrap()
                .is_none()
        );
        assert_eq!(
            missing.state().device("enabled-node").unwrap().status,
            CommunicationStatus::InitialWait
        );

        let mut disabled = action_processor(53, TargetType::Common, None, false, &[]);
        let disabled_result = disabled.process_datagram(
            &encoded(event_with(53, "disabled-action")),
            Instant::now(),
            "2026-09-04T00:00:00Z",
        );
        assert!(matches!(
            disabled_result,
            Err(UdpProcessingError::ActionValidation(ActionError::Disabled(
                53
            )))
        ));
        assert!(
            disabled
                .database()
                .get_event("enabled-node", "disabled-action")
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn action_engine_updates_all_defined_state_types() {
        let changes = [
            ActionStateChangeRecord {
                action_id: 54,
                state_type: StateType::EntryPermission,
                state_value: StateValue::Ng,
            },
            ActionStateChangeRecord {
                action_id: 54,
                state_type: StateType::MealNotice,
                state_value: StateValue::Off,
            },
            ActionStateChangeRecord {
                action_id: 54,
                state_type: StateType::SnackNotice,
                state_value: StateValue::On,
            },
            ActionStateChangeRecord {
                action_id: 54,
                state_type: StateType::HelpNotice,
                state_value: StateValue::On,
            },
        ];
        let mut family = action_processor(54, TargetType::Family, Some(1), true, &changes);
        family
            .process_datagram(
                &encoded(event_with(54, "all-family-states")),
                Instant::now(),
                "2026-09-04T00:00:00Z",
            )
            .unwrap();
        for (state_type, value) in [
            (StateType::EntryPermission, StateValue::Ng),
            (StateType::MealNotice, StateValue::Off),
            (StateType::SnackNotice, StateValue::On),
            (StateType::HelpNotice, StateValue::On),
        ] {
            assert_eq!(
                family
                    .action_handler()
                    .state()
                    .get(ActionStateScope::Family(1), state_type),
                Some(value)
            );
        }

        let mailbox_change = [ActionStateChangeRecord {
            action_id: 55,
            state_type: StateType::Mailbox,
            state_value: StateValue::On,
        }];
        let mut common = action_processor(55, TargetType::Common, None, true, &mailbox_change);
        common
            .process_datagram(
                &encoded(event_with(55, "mailbox-1")),
                Instant::now(),
                "2026-09-04T00:00:00Z",
            )
            .unwrap();
        assert_eq!(
            common
                .action_handler()
                .state()
                .get(ActionStateScope::Common, StateType::Mailbox),
            Some(StateValue::On)
        );

        let meal_clear_changes = [
            ActionStateChangeRecord {
                action_id: 56,
                state_type: StateType::MealNotice,
                state_value: StateValue::Off,
            },
            ActionStateChangeRecord {
                action_id: 56,
                state_type: StateType::SnackNotice,
                state_value: StateValue::Off,
            },
        ];
        let mut meal_clear =
            action_processor(56, TargetType::Family, Some(1), true, &meal_clear_changes);
        meal_clear
            .process_datagram(
                &encoded(event_with(56, "meal-clear-1")),
                Instant::now(),
                "2026-09-04T00:00:00Z",
            )
            .unwrap();
        assert_eq!(
            meal_clear
                .action_handler()
                .state()
                .get(ActionStateScope::Family(1), StateType::MealNotice),
            Some(StateValue::Off)
        );
        assert_eq!(
            meal_clear
                .action_handler()
                .state()
                .get(ActionStateScope::Family(1), StateType::SnackNotice),
            Some(StateValue::Off)
        );
    }

    #[test]
    fn action_failure_after_history_still_returns_ack_without_retrying_action() {
        let database = Database::open_in_memory().unwrap();
        database
            .insert_device(&device("enabled-node", true))
            .unwrap();
        let mut processor = HubUdpProcessor::from_database(
            database,
            Instant::now(),
            Duration::from_secs(10),
            FailingActionHandler::default(),
        )
        .unwrap();
        let payload = encoded(event_with(42, "failed-action-1"));
        let first = processor
            .process_datagram(&payload, Instant::now(), "2026-09-04T00:00:00Z")
            .unwrap();
        assert!(matches!(
            first,
            PacketOutcome::EventAccepted {
                duplicate: false,
                action_status: ActionExecutionStatus::Failed(_),
                ..
            }
        ));
        let second = processor
            .process_datagram(&payload, Instant::now(), "2026-09-04T00:00:01Z")
            .unwrap();
        assert!(matches!(
            second,
            PacketOutcome::EventAccepted {
                duplicate: true,
                action_status: ActionExecutionStatus::SkippedDuplicate,
                ..
            }
        ));
        assert_eq!(processor.action_handler().calls, 1);
    }
}
