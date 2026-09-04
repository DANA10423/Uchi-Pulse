//! USB CDC management endpoint for the Hub.
//!
//! The USB gadget exposes a serial-like device (normally `/dev/ttyGS0`).
//! This module owns that device on a worker thread and keeps the CDC wire
//! protocol independent from the UDP processing loop.

use std::io::{ErrorKind, Read, Write};
use std::thread;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use uchi_pulse_common::cdc::{CdcErrorCode, CdcRequest, CdcResponse};
use uchi_pulse_common::types::text;
use uchi_pulse_common::{StateType, StateValue, TargetType};

use crate::db::{ActionRecord, Database};

pub const DEFAULT_CDC_BAUD_RATE: u32 = 115_200;
const MAX_CDC_LINE_SIZE: usize = 64 * 1024;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ParentConfig {
    pub families: Vec<FamilyConfig>,
    pub actions: Vec<ActionConfig>,
    pub family_notification_destinations: Vec<FamilyNotificationDestinationConfig>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct FamilyConfig {
    pub family_id: u32,
    pub display_name: String,
    pub enabled: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ActionConfig {
    pub action_id: u32,
    pub action_name: String,
    pub target_type: TargetType,
    pub target_family_id: Option<u32>,
    pub web_message: Option<String>,
    pub enabled: bool,
    pub state_changes: Vec<StateChangeConfig>,
    pub notification_enabled: bool,
    pub notification_message: Option<String>,
    pub notification_targets: Vec<u32>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct StateChangeConfig {
    pub state_type: StateType,
    pub state_value: StateValue,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct FamilyNotificationDestinationConfig {
    pub family_id: u32,
    pub notification_type: String,
    pub destination: String,
    pub enabled: bool,
}

#[derive(Default)]
struct LineDecoder {
    current: Vec<u8>,
    dropping: bool,
}

impl LineDecoder {
    fn feed(&mut self, bytes: &[u8]) -> Vec<String> {
        let mut lines = Vec::new();
        for &byte in bytes {
            if byte == b'\n' {
                if !self.dropping {
                    let line = String::from_utf8_lossy(&self.current)
                        .trim_end_matches('\r')
                        .to_owned();
                    lines.push(line);
                }
                self.current.clear();
                self.dropping = false;
            } else if !self.dropping {
                if self.current.len() >= MAX_CDC_LINE_SIZE {
                    self.current.clear();
                    self.dropping = true;
                } else {
                    self.current.push(byte);
                }
            }
        }
        lines
    }
}

/// Starts the Hub CDC worker. The worker opens its own SQLite connection so
/// the UDP loop and CDC requests can safely operate concurrently.
pub fn spawn_server(device_path: String, database_path: String) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        let database = match Database::open(&database_path) {
            Ok(database) => database,
            Err(error) => {
                eprintln!("CDC database open failed: {error}");
                return;
            }
        };
        let mut port = match serialport::new(&device_path, DEFAULT_CDC_BAUD_RATE)
            .timeout(Duration::from_millis(100))
            .open()
        {
            Ok(port) => port,
            Err(error) => {
                eprintln!("CDC device open failed ({device_path}): {error}");
                return;
            }
        };
        if let Err(error) = port.write_data_terminal_ready(true) {
            eprintln!("CDC DTR setup failed ({device_path}): {error}");
        }
        eprintln!("Uchi Pulse Hub CDC listening on {device_path}");

        let mut decoder = LineDecoder::default();
        let mut buffer = [0u8; 1024];
        loop {
            match port.read(&mut buffer) {
                Ok(length) => {
                    for line in decoder.feed(&buffer[..length]) {
                        let response = handle_line(&database, &line);
                        if let Err(error) = port.write_all(response.as_bytes()) {
                            eprintln!("CDC response write failed: {error}");
                            return;
                        }
                        if let Err(error) = port.flush() {
                            eprintln!("CDC response flush failed: {error}");
                            return;
                        }
                    }
                }
                Err(error)
                    if matches!(error.kind(), ErrorKind::TimedOut | ErrorKind::WouldBlock) => {}
                Err(error) => {
                    eprintln!("CDC read failed: {error}");
                    return;
                }
            }
        }
    })
}

fn handle_line(database: &Database, line: &str) -> String {
    let request: CdcRequest<Value> = match serde_json::from_str(line) {
        Ok(request) => request,
        Err(error) => {
            return response_error(
                text("unknown").expect("static request id fits"),
                CdcErrorCode::InvalidJson,
                format!("invalid CDC JSON: {error}"),
            );
        }
    };

    if request.version != uchi_pulse_common::CDC_PROTOCOL_VERSION {
        return response_error(
            request.request_id,
            CdcErrorCode::UnsupportedVersion,
            format!("unsupported protocol version: {}", request.version),
        );
    }

    match request.command.as_str() {
        "get_info" => response_ok(
            request.request_id,
            json!({
                "device_type": "hub",
                "protocol_version": uchi_pulse_common::CDC_PROTOCOL_VERSION,
                "capabilities": ["get_config", "set_config", "factory_reset"]
            }),
        ),
        "get_config" => match load_config(database) {
            Ok(config) => response_ok(request.request_id, serde_json::to_value(config).unwrap()),
            Err(error) => response_error(request.request_id, CdcErrorCode::OperationFailed, error),
        },
        "set_config" => match serde_json::from_value::<ParentConfig>(request.params)
            .map_err(|error| format!("invalid parent configuration: {error}"))
            .and_then(|config| save_config(database, &config).map(|_| config))
        {
            Ok(config) => response_ok(request.request_id, serde_json::to_value(config).unwrap()),
            Err(error) => response_error(request.request_id, CdcErrorCode::InvalidConfig, error),
        },
        "factory_reset" => match save_config(database, &ParentConfig::default()) {
            Ok(()) => response_ok(request.request_id, json!({})),
            Err(error) => response_error(request.request_id, CdcErrorCode::SaveFailed, error),
        },
        "get_status" => match database.schema_version() {
            Ok(schema_version) => response_ok(
                request.request_id,
                json!({"device_type": "hub", "schema_version": schema_version}),
            ),
            Err(error) => response_error(
                request.request_id,
                CdcErrorCode::OperationFailed,
                error.to_string(),
            ),
        },
        "reboot" => response_error(
            request.request_id,
            CdcErrorCode::NotSupported,
            "Hub reboot is not exposed through CDC".to_owned(),
        ),
        "get_inputs" | "get_outputs" => response_error(
            request.request_id,
            CdcErrorCode::NotSupported,
            "Hub has no GPIO input/output state".to_owned(),
        ),
        _ => response_error(
            request.request_id,
            CdcErrorCode::InvalidCommand,
            format!("unknown command: {}", request.command),
        ),
    }
}

fn response_ok(request_id: uchi_pulse_common::RequestId, data: Value) -> String {
    serde_json::to_string(&CdcResponse::success(
        uchi_pulse_common::CDC_PROTOCOL_VERSION,
        request_id,
        data,
    ))
    .expect("CDC response is serializable")
        + "\n"
}

fn response_error(
    request_id: uchi_pulse_common::RequestId,
    code: CdcErrorCode,
    message: String,
) -> String {
    let message = text::<128>(&message).unwrap_or_else(|_| text("CDC operation failed").unwrap());
    serde_json::to_string(&CdcResponse::<Value>::error(
        uchi_pulse_common::CDC_PROTOCOL_VERSION,
        request_id,
        code,
        message,
    ))
    .expect("CDC response is serializable")
        + "\n"
}

fn load_config(database: &Database) -> Result<ParentConfig, String> {
    let families = database
        .list_families()
        .map_err(|error| error.to_string())?
        .into_iter()
        .map(|family| FamilyConfig {
            family_id: family.family_id,
            display_name: family.display_name,
            enabled: family.enabled,
        })
        .collect::<Vec<_>>();
    let actions = database
        .list_actions()
        .map_err(|error| error.to_string())?
        .into_iter()
        .map(|action| load_action(database, action))
        .collect::<Result<Vec<_>, _>>()?;
    let mut destinations = Vec::new();
    for family in &families {
        destinations.extend(
            database
                .list_family_notification_destinations(family.family_id)
                .map_err(|error| error.to_string())?
                .into_iter()
                .map(|destination| FamilyNotificationDestinationConfig {
                    family_id: destination.family_id,
                    notification_type: destination.notification_type,
                    destination: destination.destination,
                    enabled: destination.enabled,
                }),
        );
    }
    Ok(ParentConfig {
        families,
        actions,
        family_notification_destinations: destinations,
    })
}

fn load_action(database: &Database, action: ActionRecord) -> Result<ActionConfig, String> {
    let state_changes = database
        .list_action_state_changes(action.action_id)
        .map_err(|error| error.to_string())?
        .into_iter()
        .map(|change| StateChangeConfig {
            state_type: change.state_type,
            state_value: change.state_value,
        })
        .collect();
    let notification = database
        .get_notification_setting(action.action_id)
        .map_err(|error| error.to_string())?;
    let notification_targets = database
        .list_notification_targets(action.action_id)
        .map_err(|error| error.to_string())?
        .into_iter()
        .map(|target| target.family_id)
        .collect();
    Ok(ActionConfig {
        action_id: action.action_id,
        action_name: action.action_name,
        target_type: action.target_type,
        target_family_id: action.target_family_id,
        web_message: action.web_message,
        enabled: action.enabled,
        state_changes,
        notification_enabled: notification
            .as_ref()
            .is_some_and(|setting| setting.notification_enabled),
        notification_message: notification.and_then(|setting| setting.notification_message),
        notification_targets,
    })
}

fn save_config(database: &Database, config: &ParentConfig) -> Result<(), String> {
    validate_config(config)?;
    let connection = database.connection();
    connection
        .execute_batch("BEGIN IMMEDIATE")
        .map_err(|error| error.to_string())?;
    let result = (|| {
        connection.execute_batch(
            "DELETE FROM action_notification_targets;
             DELETE FROM action_notification_settings;
             DELETE FROM action_state_changes;
             DELETE FROM actions;
             DELETE FROM family_notification_destinations;
             DELETE FROM families;",
        )?;
        for family in &config.families {
            connection.execute(
                "INSERT INTO families (family_id, display_name, enabled) VALUES (?1, ?2, ?3)",
                rusqlite::params![
                    family.family_id,
                    family.display_name,
                    i64::from(family.enabled)
                ],
            )?;
        }
        for action in &config.actions {
            connection.execute(
                "INSERT INTO actions
                 (action_id, action_name, target_type, target_family_id, web_message, enabled)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                rusqlite::params![
                    action.action_id,
                    action.action_name,
                    target_type_name(action.target_type),
                    action.target_family_id,
                    action.web_message,
                    i64::from(action.enabled),
                ],
            )?;
            for change in &action.state_changes {
                connection.execute(
                    "INSERT INTO action_state_changes (action_id, state_type, state_value)
                     VALUES (?1, ?2, ?3)",
                    rusqlite::params![
                        action.action_id,
                        state_type_name(change.state_type),
                        state_value_name(change.state_value),
                    ],
                )?;
            }
            if action.notification_enabled || action.notification_message.is_some() {
                connection.execute(
                    "INSERT INTO action_notification_settings
                     (action_id, notification_enabled, notification_message)
                     VALUES (?1, ?2, ?3)",
                    rusqlite::params![
                        action.action_id,
                        i64::from(action.notification_enabled),
                        action.notification_message,
                    ],
                )?;
            }
            for family_id in &action.notification_targets {
                connection.execute(
                    "INSERT INTO action_notification_targets (action_id, family_id)
                     VALUES (?1, ?2)",
                    rusqlite::params![action.action_id, family_id],
                )?;
            }
        }
        for destination in &config.family_notification_destinations {
            connection.execute(
                "INSERT INTO family_notification_destinations
                 (family_id, notification_type, destination, enabled)
                 VALUES (?1, ?2, ?3, ?4)",
                rusqlite::params![
                    destination.family_id,
                    destination.notification_type,
                    destination.destination,
                    i64::from(destination.enabled),
                ],
            )?;
        }
        Ok::<(), rusqlite::Error>(())
    })();
    match result {
        Ok(()) => connection
            .execute_batch("COMMIT")
            .map_err(|error| error.to_string()),
        Err(error) => {
            let _ = connection.execute_batch("ROLLBACK");
            Err(error.to_string())
        }
    }
}

fn validate_config(config: &ParentConfig) -> Result<(), String> {
    let family_ids = unique_ids(
        config.families.iter().map(|family| family.family_id),
        "family_id",
    )?;
    unique_ids(
        config.actions.iter().map(|action| action.action_id),
        "action_id",
    )?;
    let mut action_ids = std::collections::HashSet::new();
    for action in &config.actions {
        if !action_ids.insert(action.action_id) {
            return Err(format!("duplicate action_id: {}", action.action_id));
        }
        match (action.target_type, action.target_family_id) {
            (TargetType::Family, Some(family_id)) if family_ids.contains(&family_id) => {}
            (TargetType::Family, _) => {
                return Err(format!(
                    "Action {} has an invalid target family",
                    action.action_id
                ));
            }
            (TargetType::Common, None) => {}
            (TargetType::Common, Some(_)) => {
                return Err(format!(
                    "COMMON Action {} cannot target a family",
                    action.action_id
                ));
            }
        }
        let mut state_types = std::collections::HashSet::new();
        for change in &action.state_changes {
            if !state_types.insert(change.state_type) {
                return Err(format!(
                    "Action {} has duplicate state_type",
                    action.action_id
                ));
            }
        }
        for family_id in &action.notification_targets {
            if !family_ids.contains(family_id) {
                return Err(format!(
                    "Action {} has invalid notification target",
                    action.action_id
                ));
            }
        }
    }
    for destination in &config.family_notification_destinations {
        if !family_ids.contains(&destination.family_id) {
            return Err(format!(
                "invalid destination family_id: {}",
                destination.family_id
            ));
        }
        if destination.notification_type.trim().is_empty()
            || destination.destination.trim().is_empty()
        {
            return Err("notification_type and destination must not be empty".to_owned());
        }
    }
    Ok(())
}

fn unique_ids<I>(ids: I, name: &str) -> Result<std::collections::HashSet<u32>, String>
where
    I: IntoIterator<Item = u32>,
{
    let mut result = std::collections::HashSet::new();
    for id in ids {
        if !result.insert(id) {
            return Err(format!("duplicate {name}: {id}"));
        }
    }
    Ok(result)
}

fn target_type_name(value: TargetType) -> &'static str {
    match value {
        TargetType::Family => "FAMILY",
        TargetType::Common => "COMMON",
    }
}

fn state_type_name(value: StateType) -> &'static str {
    match value {
        StateType::EntryPermission => "ENTRY_PERMISSION",
        StateType::MealNotice => "MEAL_NOTICE",
        StateType::SnackNotice => "SNACK_NOTICE",
        StateType::HelpNotice => "HELP_NOTICE",
        StateType::Mailbox => "MAILBOX",
    }
}

fn state_value_name(value: StateValue) -> &'static str {
    match value {
        StateValue::Unset => "UNSET",
        StateValue::On => "ON",
        StateValue::Off => "OFF",
        StateValue::Ok => "OK",
        StateValue::Ng => "NG",
        StateValue::Meeting => "MEETING",
    }
}

impl Default for ParentConfig {
    fn default() -> Self {
        Self {
            families: Vec::new(),
            actions: Vec::new(),
            family_notification_destinations: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uchi_pulse_common::cdc::{CdcRequest, CdcStatus};
    use uchi_pulse_common::types::text;

    #[test]
    fn rejects_common_action_with_family_target() {
        let config = ParentConfig {
            families: vec![FamilyConfig {
                family_id: 1,
                display_name: "家族".into(),
                enabled: true,
            }],
            actions: vec![ActionConfig {
                action_id: 1,
                action_name: "test".into(),
                target_type: TargetType::Common,
                target_family_id: Some(1),
                web_message: None,
                enabled: true,
                state_changes: vec![],
                notification_enabled: false,
                notification_message: None,
                notification_targets: vec![],
            }],
            family_notification_destinations: vec![],
        };
        assert!(validate_config(&config).is_err());
    }

    #[test]
    fn set_config_is_persisted_and_returned_by_get_config() {
        let database = Database::open_in_memory().unwrap();
        let config = ParentConfig {
            families: vec![FamilyConfig {
                family_id: 1,
                display_name: "太郎".into(),
                enabled: true,
            }],
            actions: vec![ActionConfig {
                action_id: 10,
                action_name: "ご飯通知".into(),
                target_type: TargetType::Family,
                target_family_id: Some(1),
                web_message: Some("ご飯です".into()),
                enabled: true,
                state_changes: vec![StateChangeConfig {
                    state_type: StateType::MealNotice,
                    state_value: StateValue::On,
                }],
                notification_enabled: true,
                notification_message: Some("通知".into()),
                notification_targets: vec![1],
            }],
            family_notification_destinations: vec![FamilyNotificationDestinationConfig {
                family_id: 1,
                notification_type: "LINE".into(),
                destination: "line-user-id".into(),
                enabled: true,
            }],
        };
        let request = CdcRequest {
            version: uchi_pulse_common::CDC_PROTOCOL_VERSION,
            request_id: text("set-1").unwrap(),
            command: text("set_config").unwrap(),
            params: serde_json::to_value(&config).unwrap(),
        };
        let response: CdcResponse<Value> = serde_json::from_str(&handle_line(
            &database,
            &serde_json::to_string(&request).unwrap(),
        ))
        .unwrap();
        assert_eq!(response.status, CdcStatus::Ok);

        let request = CdcRequest {
            version: uchi_pulse_common::CDC_PROTOCOL_VERSION,
            request_id: text("get-1").unwrap(),
            command: text("get_config").unwrap(),
            params: json!({}),
        };
        let response: CdcResponse<Value> = serde_json::from_str(&handle_line(
            &database,
            &serde_json::to_string(&request).unwrap(),
        ))
        .unwrap();
        assert_eq!(response.status, CdcStatus::Ok);
        assert_eq!(
            response.data.unwrap(),
            serde_json::to_value(config).unwrap()
        );
    }
}
