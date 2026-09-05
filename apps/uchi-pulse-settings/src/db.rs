use std::path::Path;

use rusqlite::{Connection, OpenFlags, OptionalExtension, params};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DeviceRecord {
    pub device_id: String,
    pub name: String,
    pub device_type: String,
    pub enabled: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FamilyRecord {
    pub family_id: u32,
    pub display_name: String,
    pub enabled: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StateChangeRecord {
    pub state_type: String,
    pub state_value: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ActionRecord {
    pub action_id: u32,
    pub action_name: String,
    pub target_type: String,
    pub target_family_id: Option<u32>,
    pub web_message: String,
    pub enabled: bool,
    pub state_changes: Vec<StateChangeRecord>,
    pub notification_enabled: bool,
    pub notification_message: String,
    pub notification_targets: Vec<u32>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EventRecord {
    pub id: i64,
    pub received_at: String,
    pub device_id: String,
    pub event_id: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NotificationDestinationRecord {
    pub id: Option<i64>,
    pub family_id: u32,
    pub notification_type: String,
    pub destination: String,
    pub enabled: bool,
}

pub struct SqliteDatabase {
    connection: Connection,
}

impl SqliteDatabase {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, String> {
        let connection = Connection::open_with_flags(
            path.as_ref(),
            OpenFlags::SQLITE_OPEN_READ_WRITE,
        )
        .map_err(|error| {
            format!(
                "設定ファイルを開けません。親機が作成したDBファイルを選んでください。詳細：{error}"
            )
        })?;
        connection
            .execute_batch("PRAGMA foreign_keys = ON;")
            .map_err(|error| format!("SQLite初期化に失敗しました: {error}"))?;
        connection
            .busy_timeout(std::time::Duration::from_millis(250))
            .map_err(sql_error)?;
        let version: u32 = connection
            .pragma_query_value(None, "user_version", |r| r.get(0))
            .map_err(sql_error)?;
        if version != 1 {
            return Err(format!(
                "この設定ファイルの形式（バージョン{version}）には対応していません。親機の設定ファイルを確認してください。"
            ));
        }
        Ok(Self { connection })
    }

    pub fn list_devices(&self) -> Result<Vec<DeviceRecord>, String> {
        let mut statement = self
            .connection
            .prepare(
                "SELECT device_id, name, device_type, enabled
                 FROM devices ORDER BY device_id",
            )
            .map_err(sql_error)?;
        let rows = statement
            .query_map([], |row| {
                Ok(DeviceRecord {
                    device_id: row.get(0)?,
                    name: row.get(1)?,
                    device_type: row.get(2)?,
                    enabled: row.get::<_, i64>(3)? != 0,
                })
            })
            .map_err(sql_error)?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(sql_error)
    }

    pub fn save_device(&self, device: &DeviceRecord) -> Result<(), String> {
        validate_required("子機ID", &device.device_id)?;
        validate_required("子機名", &device.name)?;
        validate_required("子機種別", &device.device_type)?;
        self.connection
            .execute(
                "INSERT INTO devices
                    (device_id, name, device_type, registered_at, updated_at, enabled)
                 VALUES (?1, ?2, ?3, datetime('now'), datetime('now'), ?4)
                 ON CONFLICT(device_id) DO UPDATE SET
                    name = excluded.name,
                    device_type = excluded.device_type,
                    updated_at = datetime('now'),
                    enabled = excluded.enabled",
                params![
                    device.device_id,
                    device.name,
                    device.device_type,
                    bool_int(device.enabled)
                ],
            )
            .map_err(sql_error)?;
        Ok(())
    }

    pub fn delete_device(&self, device_id: &str) -> Result<(), String> {
        self.connection
            .execute("DELETE FROM devices WHERE device_id = ?1", [device_id])
            .map_err(sql_error)?;
        Ok(())
    }

    pub fn list_families(&self) -> Result<Vec<FamilyRecord>, String> {
        let mut statement = self
            .connection
            .prepare("SELECT family_id, display_name, enabled FROM families ORDER BY family_id")
            .map_err(sql_error)?;
        let rows = statement
            .query_map([], |row| {
                Ok(FamilyRecord {
                    family_id: row.get(0)?,
                    display_name: row.get(1)?,
                    enabled: row.get::<_, i64>(2)? != 0,
                })
            })
            .map_err(sql_error)?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(sql_error)
    }

    pub fn save_family(&self, family: &FamilyRecord) -> Result<(), String> {
        if family.family_id == 0 {
            return Err("家族IDは1以上で入力してください".to_owned());
        }
        validate_required("家族表示名", &family.display_name)?;
        self.connection
            .execute(
                "INSERT INTO families (family_id, display_name, enabled)
                 VALUES (?1, ?2, ?3)
                 ON CONFLICT(family_id) DO UPDATE SET
                    display_name = excluded.display_name,
                    enabled = excluded.enabled",
                params![
                    family.family_id,
                    family.display_name,
                    bool_int(family.enabled)
                ],
            )
            .map_err(sql_error)?;
        Ok(())
    }

    pub fn delete_family(&self, family_id: u32) -> Result<(), String> {
        self.connection
            .execute("DELETE FROM families WHERE family_id = ?1", [family_id])
            .map_err(sql_error)?;
        Ok(())
    }

    pub fn list_actions(&self) -> Result<Vec<ActionRecord>, String> {
        let mut statement = self
            .connection
            .prepare(
                "SELECT action_id, action_name, target_type, target_family_id, web_message, enabled
                 FROM actions ORDER BY action_id",
            )
            .map_err(sql_error)?;
        let rows = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, u32>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, Option<u32>>(3)?,
                    row.get::<_, Option<String>>(4)?.unwrap_or_default(),
                    row.get::<_, i64>(5)? != 0,
                ))
            })
            .map_err(sql_error)?;
        let mut actions = Vec::new();
        for row in rows {
            let (action_id, action_name, target_type, target_family_id, web_message, enabled) =
                row.map_err(sql_error)?;
            actions.push(self.load_action(ActionRecord {
                action_id,
                action_name,
                target_type,
                target_family_id,
                web_message,
                enabled,
                ..ActionRecord::default()
            })?);
        }
        Ok(actions)
    }

    fn load_action(&self, mut action: ActionRecord) -> Result<ActionRecord, String> {
        let mut changes = self
            .connection
            .prepare(
                "SELECT state_type, state_value FROM action_state_changes
                 WHERE action_id = ?1 ORDER BY state_type",
            )
            .map_err(sql_error)?;
        action.state_changes = changes
            .query_map([action.action_id], |row| {
                Ok(StateChangeRecord {
                    state_type: row.get(0)?,
                    state_value: row.get(1)?,
                })
            })
            .map_err(sql_error)?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(sql_error)?;

        if let Some((enabled, message)) = self
            .connection
            .query_row(
                "SELECT notification_enabled, notification_message
                 FROM action_notification_settings WHERE action_id = ?1",
                [action.action_id],
                |row| Ok((row.get::<_, i64>(0)? != 0, row.get::<_, Option<String>>(1)?)),
            )
            .optional()
            .map_err(sql_error)?
        {
            action.notification_enabled = enabled;
            action.notification_message = message.unwrap_or_default();
        }

        let mut targets = self
            .connection
            .prepare(
                "SELECT family_id FROM action_notification_targets
                 WHERE action_id = ?1 ORDER BY family_id",
            )
            .map_err(sql_error)?;
        action.notification_targets = targets
            .query_map([action.action_id], |row| row.get(0))
            .map_err(sql_error)?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(sql_error)?;
        Ok(action)
    }

    pub fn save_action(&self, action: &ActionRecord) -> Result<(), String> {
        validate_required("動作の名前", &action.action_name)?;
        if action.target_type == "FAMILY" && action.target_family_id.is_none() {
            return Err("家族対象Actionには対象家族を指定してください".to_owned());
        }
        if action.target_type == "COMMON" && action.target_family_id.is_some() {
            return Err("共通Actionには対象家族を指定できません".to_owned());
        }
        if !matches!(action.target_type.as_str(), "FAMILY" | "COMMON") {
            return Err("対象種別が不正です".to_owned());
        }
        for change in &action.state_changes {
            if !crate::labels::STATES
                .iter()
                .any(|(code, _)| *code == change.state_type)
                || !crate::labels::values(&change.state_type)
                    .iter()
                    .any(|(code, _)| *code == change.state_value)
            {
                return Err("お知らせの種類に対応した状態を選んでください。".into());
            }
        }

        self.connection
            .execute_batch("BEGIN IMMEDIATE")
            .map_err(sql_error)?;
        let result = (|| {
            self.connection.execute(
                "INSERT INTO actions
                    (action_id, action_name, target_type, target_family_id, web_message, enabled)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                 ON CONFLICT(action_id) DO UPDATE SET
                    action_name = excluded.action_name,
                    target_type = excluded.target_type,
                    target_family_id = excluded.target_family_id,
                    web_message = excluded.web_message,
                    enabled = excluded.enabled",
                params![
                    action.action_id,
                    action.action_name,
                    action.target_type,
                    action.target_family_id,
                    null_if_empty(&action.web_message),
                    bool_int(action.enabled)
                ],
            )?;
            self.connection.execute(
                "DELETE FROM action_state_changes WHERE action_id = ?1",
                [action.action_id],
            )?;
            for change in &action.state_changes {
                if change.state_type.is_empty() || change.state_value.is_empty() {
                    return Err(rusqlite::Error::InvalidParameterName(
                        "状態変更が未入力です".to_owned(),
                    ));
                }
                self.connection.execute(
                    "INSERT INTO action_state_changes (action_id, state_type, state_value)
                     VALUES (?1, ?2, ?3)",
                    params![action.action_id, change.state_type, change.state_value],
                )?;
            }
            self.connection.execute(
                "DELETE FROM action_notification_settings WHERE action_id = ?1",
                [action.action_id],
            )?;
            if action.notification_enabled || !action.notification_message.is_empty() {
                self.connection.execute(
                    "INSERT INTO action_notification_settings
                        (action_id, notification_enabled, notification_message)
                     VALUES (?1, ?2, ?3)",
                    params![
                        action.action_id,
                        bool_int(action.notification_enabled),
                        null_if_empty(&action.notification_message)
                    ],
                )?;
            }
            self.connection.execute(
                "DELETE FROM action_notification_targets WHERE action_id = ?1",
                [action.action_id],
            )?;
            for family_id in &action.notification_targets {
                self.connection.execute(
                    "INSERT INTO action_notification_targets (action_id, family_id)
                     VALUES (?1, ?2)",
                    params![action.action_id, family_id],
                )?;
            }
            Ok::<(), rusqlite::Error>(())
        })();
        match result {
            Ok(()) => self.connection.execute_batch("COMMIT").map_err(sql_error),
            Err(error) => {
                let _ = self.connection.execute_batch("ROLLBACK");
                Err(sql_error(error))
            }
        }
    }

    pub fn delete_action(&self, action_id: u32) -> Result<(), String> {
        self.connection
            .execute("DELETE FROM actions WHERE action_id = ?1", [action_id])
            .map_err(sql_error)?;
        Ok(())
    }

    pub fn list_notification_destinations(
        &self,
    ) -> Result<Vec<NotificationDestinationRecord>, String> {
        let mut statement = self
            .connection
            .prepare(
                "SELECT id, family_id, notification_type, destination, enabled
                 FROM family_notification_destinations ORDER BY family_id, id",
            )
            .map_err(sql_error)?;
        let rows = statement
            .query_map([], |row| {
                Ok(NotificationDestinationRecord {
                    id: row.get(0)?,
                    family_id: row.get(1)?,
                    notification_type: row.get(2)?,
                    destination: row.get(3)?,
                    enabled: row.get::<_, i64>(4)? != 0,
                })
            })
            .map_err(sql_error)?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(sql_error)
    }

    pub fn save_notification_destination(
        &self,
        destination: &NotificationDestinationRecord,
    ) -> Result<i64, String> {
        if destination.family_id == 0 {
            return Err("通知先の家族を選択してください".to_owned());
        }
        validate_required("通知種別", &destination.notification_type)?;
        validate_required("通知先", &destination.destination)?;
        match destination.id {
            Some(id) => {
                self.connection
                    .execute(
                        "UPDATE family_notification_destinations
                         SET family_id = ?1, notification_type = ?2,
                             destination = ?3, enabled = ?4
                         WHERE id = ?5",
                        params![
                            destination.family_id,
                            destination.notification_type,
                            destination.destination,
                            bool_int(destination.enabled),
                            id
                        ],
                    )
                    .map_err(sql_error)?;
            }
            None => {
                self.connection
                    .execute(
                        "INSERT INTO family_notification_destinations
                            (family_id, notification_type, destination, enabled)
                         VALUES (?1, ?2, ?3, ?4)",
                        params![
                            destination.family_id,
                            destination.notification_type,
                            destination.destination,
                            bool_int(destination.enabled)
                        ],
                    )
                    .map_err(sql_error)?;
            }
        }
        Ok(destination
            .id
            .unwrap_or_else(|| self.connection.last_insert_rowid()))
    }

    pub fn delete_notification_destination(&self, id: i64) -> Result<(), String> {
        self.connection
            .execute(
                "DELETE FROM family_notification_destinations WHERE id = ?1",
                [id],
            )
            .map_err(sql_error)?;
        Ok(())
    }

    pub fn list_events(&self) -> Result<Vec<EventRecord>, String> {
        let mut statement = self
            .connection
            .prepare(
                "SELECT id, received_at, device_id, event_id FROM events
                 ORDER BY id DESC LIMIT 500",
            )
            .map_err(sql_error)?;
        let rows = statement
            .query_map([], |row| {
                Ok(EventRecord {
                    id: row.get(0)?,
                    received_at: row.get(1)?,
                    device_id: row.get(2)?,
                    event_id: row.get(3)?,
                })
            })
            .map_err(sql_error)?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(sql_error)
    }
}

fn bool_int(value: bool) -> i64 {
    i64::from(value)
}

fn null_if_empty(value: &str) -> Option<&str> {
    (!value.is_empty()).then_some(value)
}

fn validate_required(label: &str, value: &str) -> Result<(), String> {
    if value.trim().is_empty() {
        Err(format!("{label}を入力してください"))
    } else {
        Ok(())
    }
}

fn sql_error(error: rusqlite::Error) -> String {
    if let rusqlite::Error::SqliteFailure(ref code, _) = error {
        if code.code == rusqlite::ErrorCode::DatabaseBusy
            || code.code == rusqlite::ErrorCode::DatabaseLocked
        {
            return "設定ファイルを別の処理が使用しています。親機の停止後に、もう一度お試しください。".into();
        }
        if code.code == rusqlite::ErrorCode::ConstraintViolation {
            return "同じ番号の登録、または関連する設定・履歴があります。対象の番号と関連する家族・動作を確認してください。削除できない場合は無効にできます。".into();
        }
    }
    format!("SQLiteエラー: {error}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> (tempfile::TempDir, SqliteDatabase) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.db");
        drop(uchi_pulse_hub::db::Database::open(&path).unwrap());
        let db = SqliteDatabase::open(&path).unwrap();
        db.save_family(&FamilyRecord {
            family_id: 1,
            display_name: "花子".into(),
            enabled: true,
        })
        .unwrap();
        (dir, db)
    }

    #[test]
    fn missing_file_is_not_created_and_unknown_schema_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("missing.db");
        assert!(SqliteDatabase::open(&path).is_err());
        assert!(!path.exists());
        let connection = Connection::open(&path).unwrap();
        connection.pragma_update(None, "user_version", 99).unwrap();
        assert!(SqliteDatabase::open(&path).is_err());
    }

    #[test]
    fn saving_a_new_destination_then_editing_does_not_duplicate_it() {
        let (_dir, db) = fixture();
        let mut destination = NotificationDestinationRecord {
            family_id: 1,
            notification_type: "LINE".into(),
            destination: "test-recipient".into(),
            enabled: true,
            id: None,
        };
        destination.id = Some(db.save_notification_destination(&destination).unwrap());
        destination.destination = "changed-recipient".into();
        db.save_notification_destination(&destination).unwrap();
        assert_eq!(
            db.list_notification_destinations().unwrap(),
            vec![destination]
        );
    }

    #[test]
    fn action_round_trip_and_failed_related_write_roll_back() {
        let (_dir, db) = fixture();
        let action = ActionRecord {
            action_id: 10,
            action_name: "ご飯のお知らせ".into(),
            target_type: "FAMILY".into(),
            target_family_id: Some(1),
            enabled: true,
            state_changes: vec![StateChangeRecord {
                state_type: "MEAL_NOTICE".into(),
                state_value: "ON".into(),
            }],
            notification_enabled: true,
            notification_targets: vec![1],
            ..Default::default()
        };
        db.save_action(&action).unwrap();
        assert_eq!(db.list_actions().unwrap(), vec![action.clone()]);
        let mut changed = action.clone();
        changed.action_name = "失敗する変更".into();
        changed.notification_targets = vec![999];
        assert!(db.save_action(&changed).is_err());
        assert_eq!(db.list_actions().unwrap(), vec![action.clone()]);
        changed.notification_targets = vec![1];
        changed.state_changes[0].state_value = "MEETING".into();
        assert!(db.save_action(&changed).is_err());
        assert_eq!(db.list_actions().unwrap(), vec![action]);
    }

    #[test]
    fn validates_and_round_trips_device() {
        let database = SqliteDatabase {
            connection: Connection::open_in_memory().unwrap(),
        };
        database
            .connection
            .execute_batch(
                "CREATE TABLE devices (device_id TEXT PRIMARY KEY, name TEXT NOT NULL,
                 device_type TEXT NOT NULL, registered_at TEXT NOT NULL, updated_at TEXT NOT NULL,
                 enabled INTEGER NOT NULL);",
            )
            .unwrap();
        database
            .save_device(&DeviceRecord {
                device_id: "node-01".to_owned(),
                name: "リビング".to_owned(),
                device_type: "pico-w".to_owned(),
                enabled: true,
            })
            .unwrap();
        assert_eq!(database.list_devices().unwrap()[0].name, "リビング");
    }

    #[test]
    fn rejects_invalid_action_target() {
        let database = SqliteDatabase {
            connection: Connection::open_in_memory().unwrap(),
        };
        let result = database.save_action(&ActionRecord {
            action_id: 1,
            target_type: "FAMILY".to_owned(),
            ..ActionRecord::default()
        });
        assert!(result.is_err());
    }
}
