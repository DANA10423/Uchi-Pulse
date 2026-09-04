mod migrations;
mod models;
mod repository;

use std::path::Path;
use std::rc::Rc;

use rusqlite::Connection;

pub use models::{
    ActionRecord, ActionStateChangeRecord, DeviceRecord, EventRecord,
    FamilyNotificationDestination, FamilyRecord, NotificationSettingRecord,
    NotificationTargetRecord,
};
pub use repository::DatabaseError;

pub type Result<T> = std::result::Result<T, DatabaseError>;

/// SQLite-backed repository for persistent Hub configuration and event history.
///
/// Runtime communication state and last-seen timestamps deliberately do not
/// have a representation in this repository.
#[derive(Clone)]
pub struct Database {
    connection: Rc<Connection>,
}

impl Database {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let mut connection = Connection::open(path).map_err(DatabaseError::from)?;
        migrations::migrate(&mut connection).map_err(DatabaseError::from)?;
        Ok(Self {
            connection: Rc::new(connection),
        })
    }

    pub fn open_in_memory() -> Result<Self> {
        let mut connection = Connection::open_in_memory().map_err(DatabaseError::from)?;
        migrations::migrate(&mut connection).map_err(DatabaseError::from)?;
        Ok(Self {
            connection: Rc::new(connection),
        })
    }

    pub fn connection(&self) -> &Connection {
        &self.connection
    }

    pub fn schema_version(&self) -> Result<u32> {
        self.connection
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .map_err(DatabaseError::from)
    }
}
