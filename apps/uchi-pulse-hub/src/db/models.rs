use uchi_pulse_common::{ActionId, StateType, StateValue, TargetType};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeviceRecord {
    pub device_id: String,
    pub name: String,
    pub device_type: String,
    pub registered_at: String,
    pub updated_at: String,
    pub enabled: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FamilyRecord {
    pub family_id: u32,
    pub display_name: String,
    pub enabled: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ActionRecord {
    pub action_id: ActionId,
    pub action_name: String,
    pub target_type: TargetType,
    pub target_family_id: Option<u32>,
    pub web_message: Option<String>,
    pub enabled: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ActionStateChangeRecord {
    pub action_id: ActionId,
    pub state_type: StateType,
    pub state_value: StateValue,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NotificationSettingRecord {
    pub action_id: ActionId,
    pub notification_enabled: bool,
    pub notification_message: Option<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NotificationTargetRecord {
    pub action_id: ActionId,
    pub family_id: u32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FamilyNotificationDestination {
    pub id: Option<i64>,
    pub family_id: u32,
    pub notification_type: String,
    pub destination: String,
    pub enabled: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EventRecord {
    pub id: Option<i64>,
    pub received_at: String,
    pub device_id: String,
    pub event_id: String,
    pub payload: String,
}
