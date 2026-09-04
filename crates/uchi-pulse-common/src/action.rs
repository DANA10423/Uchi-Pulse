use serde::{Deserialize, Serialize};

use crate::types::{ACTION_NAME_CAPACITY, CommandName};
use crate::types::{ActionId, FamilyId, MESSAGE_CAPACITY};
use heapless::String;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum TargetType {
    #[serde(rename = "FAMILY")]
    Family,
    #[serde(rename = "COMMON")]
    Common,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub enum StateType {
    #[serde(rename = "ENTRY_PERMISSION")]
    EntryPermission,
    #[serde(rename = "MEAL_NOTICE")]
    MealNotice,
    #[serde(rename = "SNACK_NOTICE")]
    SnackNotice,
    #[serde(rename = "HELP_NOTICE")]
    HelpNotice,
    #[serde(rename = "MAILBOX")]
    Mailbox,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum StateValue {
    #[serde(rename = "UNSET")]
    Unset,
    #[serde(rename = "ON")]
    On,
    #[serde(rename = "OFF")]
    Off,
    #[serde(rename = "OK")]
    Ok,
    #[serde(rename = "NG")]
    Ng,
    #[serde(rename = "MEETING")]
    Meeting,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ActionDefinition {
    pub action_id: ActionId,
    pub action_name: String<ACTION_NAME_CAPACITY>,
    pub target_type: TargetType,
    pub target_family_id: Option<FamilyId>,
    pub web_message: Option<String<MESSAGE_CAPACITY>>,
    pub enabled: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ActionStateChange {
    pub state_type: StateType,
    pub state_value: StateValue,
}

/// The command name remains a string so an endpoint can return INVALID_COMMAND
/// for an unknown command instead of failing at the wire decoder.
pub fn command_name(value: &str) -> Result<CommandName, crate::types::TextError> {
    crate::types::text(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codec::{decode, encode};
    use crate::types::text;

    #[test]
    fn supports_multi_state_and_notification_only_actions() {
        let action = ActionDefinition {
            action_id: 10,
            action_name: text("食事通知クリア").unwrap(),
            target_type: TargetType::Family,
            target_family_id: Some(2),
            web_message: None,
            enabled: true,
        };
        let changes = [
            ActionStateChange {
                state_type: StateType::MealNotice,
                state_value: StateValue::Off,
            },
            ActionStateChange {
                state_type: StateType::SnackNotice,
                state_value: StateValue::Off,
            },
        ];
        assert_eq!(changes.len(), 2);
        let mut buffer = [0; 512];
        let used = encode(&action, &mut buffer).unwrap();
        let decoded: ActionDefinition = decode(&buffer[..used]).unwrap();
        assert_eq!(decoded, action);
    }
}
