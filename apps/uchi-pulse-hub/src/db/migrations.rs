use rusqlite::Connection;

const MIGRATIONS: &[(u32, &str)] = &[(
    1,
    r#"
CREATE TABLE devices (
    device_id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL,
    device_type TEXT NOT NULL,
    registered_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    enabled INTEGER NOT NULL DEFAULT 1 CHECK (enabled IN (0, 1))
);

CREATE TABLE families (
    family_id INTEGER PRIMARY KEY NOT NULL,
    display_name TEXT NOT NULL,
    enabled INTEGER NOT NULL DEFAULT 1 CHECK (enabled IN (0, 1))
);

CREATE TABLE actions (
    action_id INTEGER PRIMARY KEY NOT NULL,
    action_name TEXT NOT NULL,
    target_type TEXT NOT NULL CHECK (target_type IN ('FAMILY', 'COMMON')),
    target_family_id INTEGER,
    web_message TEXT,
    enabled INTEGER NOT NULL DEFAULT 1 CHECK (enabled IN (0, 1)),
    FOREIGN KEY (target_family_id) REFERENCES families (family_id),
    CHECK (
        (target_type = 'FAMILY' AND target_family_id IS NOT NULL)
        OR (target_type = 'COMMON' AND target_family_id IS NULL)
    )
);

CREATE TABLE action_state_changes (
    action_id INTEGER NOT NULL,
    state_type TEXT NOT NULL CHECK (length(state_type) > 0),
    state_value TEXT NOT NULL CHECK (length(state_value) > 0),
    PRIMARY KEY (action_id, state_type),
    FOREIGN KEY (action_id) REFERENCES actions (action_id) ON DELETE CASCADE
);

CREATE TABLE action_notification_settings (
    action_id INTEGER PRIMARY KEY NOT NULL,
    notification_enabled INTEGER NOT NULL DEFAULT 0 CHECK (notification_enabled IN (0, 1)),
    notification_message TEXT,
    FOREIGN KEY (action_id) REFERENCES actions (action_id) ON DELETE CASCADE
);

CREATE TABLE action_notification_targets (
    action_id INTEGER NOT NULL,
    family_id INTEGER NOT NULL,
    PRIMARY KEY (action_id, family_id),
    FOREIGN KEY (action_id) REFERENCES actions (action_id) ON DELETE CASCADE,
    FOREIGN KEY (family_id) REFERENCES families (family_id) ON DELETE CASCADE
);

CREATE TABLE family_notification_destinations (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    family_id INTEGER NOT NULL,
    notification_type TEXT NOT NULL,
    destination TEXT NOT NULL,
    enabled INTEGER NOT NULL DEFAULT 1 CHECK (enabled IN (0, 1)),
    FOREIGN KEY (family_id) REFERENCES families (family_id) ON DELETE CASCADE
);

CREATE TABLE events (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    received_at TEXT NOT NULL,
    device_id TEXT NOT NULL,
    event_id TEXT NOT NULL,
    payload TEXT NOT NULL,
    UNIQUE (device_id, event_id),
    FOREIGN KEY (device_id) REFERENCES devices (device_id)
);

CREATE INDEX idx_actions_target_family ON actions (target_family_id);
CREATE INDEX idx_action_notification_targets_family ON action_notification_targets (family_id);
CREATE INDEX idx_family_notification_destinations_family ON family_notification_destinations (family_id);
CREATE INDEX idx_events_received_at ON events (received_at);
"#,
)];

pub fn migrate(connection: &mut Connection) -> rusqlite::Result<()> {
    connection.execute_batch("PRAGMA foreign_keys = ON;")?;
    let current: u32 = connection.pragma_query_value(None, "user_version", |row| row.get(0))?;
    let latest = MIGRATIONS.last().map_or(0, |(version, _)| *version);
    if current > latest {
        return Err(rusqlite::Error::InvalidParameterName(format!(
            "database schema version {current} is newer than supported version {latest}"
        )));
    }

    for (version, sql) in MIGRATIONS.iter().filter(|(version, _)| *version > current) {
        let transaction = connection.transaction()?;
        transaction.execute_batch(sql)?;
        transaction.pragma_update(None, "user_version", version)?;
        transaction.commit()?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrations_are_ordered_and_current_schema_is_version_one() {
        assert_eq!(MIGRATIONS, &[(1, MIGRATIONS[0].1)]);
    }
}
