use std::fmt;

use rusqlite::{OptionalExtension, params, types::Type};
use uchi_pulse_common::{ActionId, StateType, StateValue, TargetType};

use super::models::*;
use super::{Database, Result};

#[derive(Debug)]
pub enum DatabaseError {
    Sqlite(rusqlite::Error),
    InvalidStoredValue { field: &'static str, value: String },
}

impl fmt::Display for DatabaseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Sqlite(error) => write!(formatter, "SQLite error: {error}"),
            Self::InvalidStoredValue { field, value } => {
                write!(formatter, "invalid stored {field} value: {value}")
            }
        }
    }
}

impl std::error::Error for DatabaseError {}

impl From<rusqlite::Error> for DatabaseError {
    fn from(error: rusqlite::Error) -> Self {
        Self::Sqlite(error)
    }
}

impl Database {
    pub fn insert_device(&self, device: &DeviceRecord) -> Result<()> {
        self.connection.execute(
            "INSERT INTO devices (device_id, name, device_type, registered_at, updated_at, enabled)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                device.device_id,
                device.name,
                device.device_type,
                device.registered_at,
                device.updated_at,
                bool_to_int(device.enabled),
            ],
        )?;
        Ok(())
    }

    pub fn get_device(&self, device_id: &str) -> Result<Option<DeviceRecord>> {
        self.connection
            .query_row(
                "SELECT device_id, name, device_type, registered_at, updated_at, enabled
                 FROM devices WHERE device_id = ?1",
                params![device_id],
                row_to_device,
            )
            .optional()
            .map_err(DatabaseError::from)
    }

    pub fn list_enabled_devices(&self) -> Result<Vec<DeviceRecord>> {
        let mut statement = self.connection.prepare(
            "SELECT device_id, name, device_type, registered_at, updated_at, enabled
             FROM devices WHERE enabled = 1 ORDER BY device_id",
        )?;
        let rows = statement.query_map([], row_to_device)?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(DatabaseError::from)
    }

    pub fn update_device(&self, device: &DeviceRecord) -> Result<bool> {
        let changed = self.connection.execute(
            "UPDATE devices
             SET name = ?2, device_type = ?3, registered_at = ?4, updated_at = ?5, enabled = ?6
             WHERE device_id = ?1",
            params![
                device.device_id,
                device.name,
                device.device_type,
                device.registered_at,
                device.updated_at,
                bool_to_int(device.enabled),
            ],
        )?;
        Ok(changed == 1)
    }

    pub fn delete_device(&self, device_id: &str) -> Result<bool> {
        Ok(self.connection.execute(
            "DELETE FROM devices WHERE device_id = ?1",
            params![device_id],
        )? == 1)
    }

    pub fn insert_family(&self, family: &FamilyRecord) -> Result<()> {
        self.connection.execute(
            "INSERT INTO families (family_id, display_name, enabled) VALUES (?1, ?2, ?3)",
            params![
                family.family_id,
                family.display_name,
                bool_to_int(family.enabled)
            ],
        )?;
        Ok(())
    }

    pub fn get_family(&self, family_id: u32) -> Result<Option<FamilyRecord>> {
        self.connection
            .query_row(
                "SELECT family_id, display_name, enabled FROM families WHERE family_id = ?1",
                params![family_id],
                row_to_family,
            )
            .optional()
            .map_err(DatabaseError::from)
    }

    pub fn list_families(&self) -> Result<Vec<FamilyRecord>> {
        let mut statement = self
            .connection
            .prepare("SELECT family_id, display_name, enabled FROM families ORDER BY family_id")?;
        let rows = statement.query_map([], row_to_family)?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(DatabaseError::from)
    }

    pub fn update_family(&self, family: &FamilyRecord) -> Result<bool> {
        let changed = self.connection.execute(
            "UPDATE families SET display_name = ?2, enabled = ?3 WHERE family_id = ?1",
            params![
                family.family_id,
                family.display_name,
                bool_to_int(family.enabled)
            ],
        )?;
        Ok(changed == 1)
    }

    pub fn insert_action(&self, action: &ActionRecord) -> Result<()> {
        self.connection.execute(
            "INSERT INTO actions
             (action_id, action_name, target_type, target_family_id, web_message, enabled)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                action.action_id,
                action.action_name,
                target_type_to_str(action.target_type),
                action.target_family_id,
                action.web_message,
                bool_to_int(action.enabled),
            ],
        )?;
        Ok(())
    }

    pub fn get_action(&self, action_id: ActionId) -> Result<Option<ActionRecord>> {
        self.connection
            .query_row(
                "SELECT action_id, action_name, target_type, target_family_id, web_message, enabled
                 FROM actions WHERE action_id = ?1",
                params![action_id],
                row_to_action,
            )
            .optional()
            .map_err(DatabaseError::from)
    }

    pub fn update_action(&self, action: &ActionRecord) -> Result<bool> {
        let changed = self.connection.execute(
            "UPDATE actions
             SET action_name = ?2, target_type = ?3, target_family_id = ?4,
                 web_message = ?5, enabled = ?6
             WHERE action_id = ?1",
            params![
                action.action_id,
                action.action_name,
                target_type_to_str(action.target_type),
                action.target_family_id,
                action.web_message,
                bool_to_int(action.enabled),
            ],
        )?;
        Ok(changed == 1)
    }

    pub fn insert_action_state_change(&self, change: &ActionStateChangeRecord) -> Result<()> {
        self.connection.execute(
            "INSERT INTO action_state_changes (action_id, state_type, state_value)
             VALUES (?1, ?2, ?3)",
            params![
                change.action_id,
                state_type_to_str(change.state_type),
                state_value_to_str(change.state_value),
            ],
        )?;
        Ok(())
    }

    pub fn list_action_state_changes(
        &self,
        action_id: ActionId,
    ) -> Result<Vec<ActionStateChangeRecord>> {
        let mut statement = self.connection.prepare(
            "SELECT action_id, state_type, state_value
             FROM action_state_changes WHERE action_id = ?1 ORDER BY state_type",
        )?;
        let rows = statement.query_map(params![action_id], row_to_state_change)?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(DatabaseError::from)
    }

    pub fn delete_action_state_change(
        &self,
        action_id: ActionId,
        state_type: StateType,
    ) -> Result<bool> {
        Ok(self.connection.execute(
            "DELETE FROM action_state_changes WHERE action_id = ?1 AND state_type = ?2",
            params![action_id, state_type_to_str(state_type)],
        )? == 1)
    }

    pub fn upsert_notification_setting(&self, setting: &NotificationSettingRecord) -> Result<()> {
        self.connection.execute(
            "INSERT INTO action_notification_settings
             (action_id, notification_enabled, notification_message)
             VALUES (?1, ?2, ?3)
             ON CONFLICT(action_id) DO UPDATE SET
               notification_enabled = excluded.notification_enabled,
               notification_message = excluded.notification_message",
            params![
                setting.action_id,
                bool_to_int(setting.notification_enabled),
                setting.notification_message,
            ],
        )?;
        Ok(())
    }

    pub fn get_notification_setting(
        &self,
        action_id: ActionId,
    ) -> Result<Option<NotificationSettingRecord>> {
        self.connection
            .query_row(
                "SELECT action_id, notification_enabled, notification_message
                 FROM action_notification_settings WHERE action_id = ?1",
                params![action_id],
                row_to_notification_setting,
            )
            .optional()
            .map_err(DatabaseError::from)
    }

    pub fn insert_notification_target(&self, target: &NotificationTargetRecord) -> Result<()> {
        self.connection.execute(
            "INSERT INTO action_notification_targets (action_id, family_id) VALUES (?1, ?2)",
            params![target.action_id, target.family_id],
        )?;
        Ok(())
    }

    pub fn list_notification_targets(
        &self,
        action_id: ActionId,
    ) -> Result<Vec<NotificationTargetRecord>> {
        let mut statement = self.connection.prepare(
            "SELECT action_id, family_id FROM action_notification_targets
             WHERE action_id = ?1 ORDER BY family_id",
        )?;
        let rows = statement.query_map(params![action_id], row_to_notification_target)?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(DatabaseError::from)
    }

    pub fn delete_notification_target(&self, target: &NotificationTargetRecord) -> Result<bool> {
        Ok(self.connection.execute(
            "DELETE FROM action_notification_targets WHERE action_id = ?1 AND family_id = ?2",
            params![target.action_id, target.family_id],
        )? == 1)
    }

    pub fn insert_family_notification_destination(
        &self,
        destination: &FamilyNotificationDestination,
    ) -> Result<i64> {
        self.connection.execute(
            "INSERT INTO family_notification_destinations
             (family_id, notification_type, destination, enabled)
             VALUES (?1, ?2, ?3, ?4)",
            params![
                destination.family_id,
                destination.notification_type,
                destination.destination,
                bool_to_int(destination.enabled),
            ],
        )?;
        Ok(self.connection.last_insert_rowid())
    }

    pub fn list_family_notification_destinations(
        &self,
        family_id: u32,
    ) -> Result<Vec<FamilyNotificationDestination>> {
        let mut statement = self.connection.prepare(
            "SELECT id, family_id, notification_type, destination, enabled
             FROM family_notification_destinations WHERE family_id = ?1 ORDER BY id",
        )?;
        let rows = statement.query_map(params![family_id], row_to_family_destination)?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(DatabaseError::from)
    }

    pub fn delete_family_notification_destination(&self, id: i64) -> Result<bool> {
        Ok(self.connection.execute(
            "DELETE FROM family_notification_destinations WHERE id = ?1",
            params![id],
        )? == 1)
    }

    /// Inserts a new event and returns false when `(device_id, event_id)` was
    /// already present. The unique constraint is the final protection against
    /// duplicate registration, including concurrent callers.
    pub fn insert_event(&self, event: &EventRecord) -> Result<bool> {
        let changed = self.connection.execute(
            "INSERT OR IGNORE INTO events (received_at, device_id, event_id, payload)
             VALUES (?1, ?2, ?3, ?4)",
            params![
                event.received_at,
                event.device_id,
                event.event_id,
                event.payload
            ],
        )?;
        Ok(changed == 1)
    }

    pub fn get_event(&self, device_id: &str, event_id: &str) -> Result<Option<EventRecord>> {
        self.connection
            .query_row(
                "SELECT id, received_at, device_id, event_id, payload
                 FROM events WHERE device_id = ?1 AND event_id = ?2",
                params![device_id, event_id],
                row_to_event,
            )
            .optional()
            .map_err(DatabaseError::from)
    }
}

fn bool_to_int(value: bool) -> i64 {
    i64::from(value)
}

fn target_type_to_str(value: TargetType) -> &'static str {
    match value {
        TargetType::Family => "FAMILY",
        TargetType::Common => "COMMON",
    }
}

fn state_type_to_str(value: StateType) -> &'static str {
    match value {
        StateType::EntryPermission => "ENTRY_PERMISSION",
        StateType::MealNotice => "MEAL_NOTICE",
        StateType::SnackNotice => "SNACK_NOTICE",
        StateType::HelpNotice => "HELP_NOTICE",
        StateType::Mailbox => "MAILBOX",
    }
}

fn state_value_to_str(value: StateValue) -> &'static str {
    match value {
        StateValue::Unset => "UNSET",
        StateValue::On => "ON",
        StateValue::Off => "OFF",
        StateValue::Ok => "OK",
        StateValue::Ng => "NG",
        StateValue::Meeting => "MEETING",
    }
}

fn invalid_stored_value(field: &'static str, value: String) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(
        0,
        Type::Text,
        Box::new(DatabaseError::InvalidStoredValue { field, value }),
    )
}

fn target_type_from_str(value: String) -> rusqlite::Result<TargetType> {
    match value.as_str() {
        "FAMILY" => Ok(TargetType::Family),
        "COMMON" => Ok(TargetType::Common),
        _ => Err(invalid_stored_value("target_type", value)),
    }
}

fn state_type_from_str(value: String) -> rusqlite::Result<StateType> {
    match value.as_str() {
        "ENTRY_PERMISSION" => Ok(StateType::EntryPermission),
        "MEAL_NOTICE" => Ok(StateType::MealNotice),
        "SNACK_NOTICE" => Ok(StateType::SnackNotice),
        "HELP_NOTICE" => Ok(StateType::HelpNotice),
        "MAILBOX" => Ok(StateType::Mailbox),
        _ => Err(invalid_stored_value("state_type", value)),
    }
}

fn state_value_from_str(value: String) -> rusqlite::Result<StateValue> {
    match value.as_str() {
        "UNSET" => Ok(StateValue::Unset),
        "ON" => Ok(StateValue::On),
        "OFF" => Ok(StateValue::Off),
        "OK" => Ok(StateValue::Ok),
        "NG" => Ok(StateValue::Ng),
        "MEETING" => Ok(StateValue::Meeting),
        _ => Err(invalid_stored_value("state_value", value)),
    }
}

fn row_to_device(row: &rusqlite::Row<'_>) -> rusqlite::Result<DeviceRecord> {
    Ok(DeviceRecord {
        device_id: row.get(0)?,
        name: row.get(1)?,
        device_type: row.get(2)?,
        registered_at: row.get(3)?,
        updated_at: row.get(4)?,
        enabled: row.get::<_, i64>(5)? != 0,
    })
}

fn row_to_family(row: &rusqlite::Row<'_>) -> rusqlite::Result<FamilyRecord> {
    Ok(FamilyRecord {
        family_id: row.get(0)?,
        display_name: row.get(1)?,
        enabled: row.get::<_, i64>(2)? != 0,
    })
}

fn row_to_action(row: &rusqlite::Row<'_>) -> rusqlite::Result<ActionRecord> {
    Ok(ActionRecord {
        action_id: row.get(0)?,
        action_name: row.get(1)?,
        target_type: target_type_from_str(row.get(2)?)?,
        target_family_id: row.get(3)?,
        web_message: row.get(4)?,
        enabled: row.get::<_, i64>(5)? != 0,
    })
}

fn row_to_state_change(row: &rusqlite::Row<'_>) -> rusqlite::Result<ActionStateChangeRecord> {
    Ok(ActionStateChangeRecord {
        action_id: row.get(0)?,
        state_type: state_type_from_str(row.get(1)?)?,
        state_value: state_value_from_str(row.get(2)?)?,
    })
}

fn row_to_notification_setting(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<NotificationSettingRecord> {
    Ok(NotificationSettingRecord {
        action_id: row.get(0)?,
        notification_enabled: row.get::<_, i64>(1)? != 0,
        notification_message: row.get(2)?,
    })
}

fn row_to_notification_target(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<NotificationTargetRecord> {
    Ok(NotificationTargetRecord {
        action_id: row.get(0)?,
        family_id: row.get(1)?,
    })
}

fn row_to_family_destination(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<FamilyNotificationDestination> {
    Ok(FamilyNotificationDestination {
        id: row.get(0)?,
        family_id: row.get(1)?,
        notification_type: row.get(2)?,
        destination: row.get(3)?,
        enabled: row.get::<_, i64>(4)? != 0,
    })
}

fn row_to_event(row: &rusqlite::Row<'_>) -> rusqlite::Result<EventRecord> {
    Ok(EventRecord {
        id: row.get(0)?,
        received_at: row.get(1)?,
        device_id: row.get(2)?,
        event_id: row.get(3)?,
        payload: row.get(4)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{Database, EventRecord};

    fn database() -> Database {
        Database::open_in_memory().unwrap()
    }

    fn device(enabled: bool) -> DeviceRecord {
        DeviceRecord {
            device_id: "node-01".into(),
            name: "リビング".into(),
            device_type: "FAMILY".into(),
            registered_at: "2026-09-04T00:00:00Z".into(),
            updated_at: "2026-09-04T00:00:00Z".into(),
            enabled,
        }
    }

    fn family(family_id: u32) -> FamilyRecord {
        FamilyRecord {
            family_id,
            display_name: format!("家族{family_id}"),
            enabled: true,
        }
    }

    fn action(action_id: ActionId, target_family_id: Option<u32>) -> ActionRecord {
        ActionRecord {
            action_id,
            action_name: "食事通知クリア".into(),
            target_type: if target_family_id.is_some() {
                TargetType::Family
            } else {
                TargetType::Common
            },
            target_family_id,
            web_message: Some("{target}：食事通知を解除しました".into()),
            enabled: true,
        }
    }

    #[test]
    fn creates_schema_without_runtime_state_columns() {
        let database = database();
        assert_eq!(database.schema_version().unwrap(), 1);
        let tables: Vec<String> = database
            .connection()
            .prepare("SELECT name FROM sqlite_master WHERE type = 'table' ORDER BY name")
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap();
        assert_eq!(
            tables,
            vec![
                "action_notification_settings",
                "action_notification_targets",
                "action_state_changes",
                "actions",
                "devices",
                "events",
                "families",
                "family_notification_destinations",
                "sqlite_sequence",
            ]
        );

        let columns: Vec<String> = database
            .connection()
            .prepare("PRAGMA table_info(devices)")
            .unwrap()
            .query_map([], |row| row.get(1))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap();
        assert!(!columns.iter().any(|column| column == "status"));
        assert!(!columns.iter().any(|column| column == "last_seen_at"));
    }

    #[test]
    fn device_crud_and_enabled_listing_work() {
        let database = database();
        let mut enabled = device(true);
        database.insert_device(&enabled).unwrap();
        assert_eq!(
            database.get_device("node-01").unwrap(),
            Some(enabled.clone())
        );
        assert_eq!(
            database.list_enabled_devices().unwrap(),
            vec![enabled.clone()]
        );

        enabled.name = "更新後".into();
        enabled.enabled = false;
        assert!(database.update_device(&enabled).unwrap());
        assert!(database.list_enabled_devices().unwrap().is_empty());
        assert_eq!(database.get_device("node-01").unwrap(), Some(enabled));
        assert!(database.delete_device("node-01").unwrap());
        assert!(database.get_device("node-01").unwrap().is_none());
    }

    #[test]
    fn action_state_changes_allow_zero_or_multiple_rows() {
        let database = database();
        database.insert_family(&family(1)).unwrap();
        database.insert_action(&action(10, Some(1))).unwrap();
        database.insert_action(&action(11, Some(1))).unwrap();
        database.insert_action(&action(12, None)).unwrap();

        assert!(database.list_action_state_changes(10).unwrap().is_empty());
        database
            .insert_action_state_change(&ActionStateChangeRecord {
                action_id: 10,
                state_type: StateType::MealNotice,
                state_value: StateValue::Off,
            })
            .unwrap();
        database
            .insert_action_state_change(&ActionStateChangeRecord {
                action_id: 10,
                state_type: StateType::SnackNotice,
                state_value: StateValue::Off,
            })
            .unwrap();
        assert_eq!(database.list_action_state_changes(10).unwrap().len(), 2);
        assert!(
            database
                .delete_action_state_change(10, StateType::MealNotice)
                .unwrap()
        );
        assert_eq!(database.list_action_state_changes(10).unwrap().len(), 1);
        assert_eq!(
            database.get_action(12).unwrap().unwrap().target_type,
            TargetType::Common
        );
    }

    #[test]
    fn notification_targets_and_destinations_are_independent_from_action_target() {
        let database = database();
        database.insert_family(&family(1)).unwrap();
        database.insert_family(&family(2)).unwrap();
        database.insert_action(&action(10, Some(1))).unwrap();
        database
            .insert_notification_target(&NotificationTargetRecord {
                action_id: 10,
                family_id: 1,
            })
            .unwrap();
        database
            .insert_notification_target(&NotificationTargetRecord {
                action_id: 10,
                family_id: 2,
            })
            .unwrap();
        database
            .upsert_notification_setting(&NotificationSettingRecord {
                action_id: 10,
                notification_enabled: true,
                notification_message: Some("通知: {target}".into()),
            })
            .unwrap();
        let destination_id = database
            .insert_family_notification_destination(&FamilyNotificationDestination {
                id: None,
                family_id: 1,
                notification_type: "Slack".into(),
                destination: "channel-id".into(),
                enabled: true,
            })
            .unwrap();

        assert_eq!(database.list_notification_targets(10).unwrap().len(), 2);
        assert_eq!(
            database
                .get_notification_setting(10)
                .unwrap()
                .unwrap()
                .notification_message
                .as_deref(),
            Some("通知: {target}")
        );
        assert_eq!(
            database.list_family_notification_destinations(1).unwrap()[0].id,
            Some(destination_id)
        );
        assert!(
            database
                .delete_notification_target(&NotificationTargetRecord {
                    action_id: 10,
                    family_id: 2,
                })
                .unwrap()
        );
    }

    #[test]
    fn event_unique_key_prevents_duplicate_registration() {
        let database = database();
        database.insert_device(&device(true)).unwrap();
        let event = EventRecord {
            id: None,
            received_at: "2026-09-04T00:00:01Z".into(),
            device_id: "node-01".into(),
            event_id: "boot-1-1".into(),
            payload: "{\"type\":\"EVENT\"}".into(),
        };
        assert!(database.insert_event(&event).unwrap());
        assert!(!database.insert_event(&event).unwrap());
        assert_eq!(
            database
                .get_event("node-01", "boot-1-1")
                .unwrap()
                .unwrap()
                .payload,
            "{\"type\":\"EVENT\"}"
        );
    }

    #[test]
    fn foreign_keys_reject_invalid_references() {
        let database = database();
        let result = database.insert_action(&action(10, Some(999)));
        assert!(matches!(result, Err(DatabaseError::Sqlite(_))));
    }
}
