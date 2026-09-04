//! Non-volatile configuration storage abstraction.
//!
//! The firmware adapter owns the RP flash handle. This module contains only
//! the bounded storage format and the validation/serialization policy, so the
//! same behavior can be tested on a host with `MemoryConfigStorage`.

use serde::{Deserialize, Serialize};
use uchi_pulse_common::codec::{CodecError, decode, encode};

use crate::config::{ConfigValidationError, PersistedNodeConfig};

pub const CONFIG_STORAGE_FORMAT_VERSION: u8 = 2;
pub const CONFIG_STORAGE_SIZE: usize = 4096;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
struct StorageEnvelope {
    version: u8,
    config: PersistedNodeConfig,
}

/// Device-specific storage implementation used by `ConfigManager`.
pub trait ConfigStorage {
    type Error;

    /// Read the currently stored record. An erased or empty region returns 0.
    fn read(&mut self, destination: &mut [u8]) -> Result<usize, Self::Error>;
    fn write(&mut self, data: &[u8]) -> Result<(), Self::Error>;
    fn clear(&mut self) -> Result<(), Self::Error>;
}

#[derive(Debug)]
pub enum ConfigManagerError<E> {
    InvalidConfig(ConfigValidationError),
    Storage(E),
    Serialization(CodecError),
}

pub struct ConfigManager<S> {
    storage: S,
    config: PersistedNodeConfig,
}

impl<S: ConfigStorage> ConfigManager<S> {
    /// Invalid, corrupt, version-mismatched, or unreadable data is treated as
    /// absent. This makes boot safe even after a torn or stale write.
    pub fn new(mut storage: S, defaults: PersistedNodeConfig) -> Self {
        let mut raw = [0_u8; CONFIG_STORAGE_SIZE];
        let config = match storage.read(&mut raw) {
            Ok(used) if used > 0 && used <= raw.len() => decode::<StorageEnvelope>(&raw[..used])
                .ok()
                .filter(|record| record.version == CONFIG_STORAGE_FORMAT_VERSION)
                .map(|record| record.config)
                .filter(|config| config.validate().is_ok())
                .unwrap_or(defaults),
            _ => defaults,
        };
        Self { storage, config }
    }

    pub fn config(&self) -> &PersistedNodeConfig {
        &self.config
    }

    /// Validate and serialize before changing storage. The active in-memory
    /// configuration is changed only after persistence succeeds.
    pub fn set_config(
        &mut self,
        config: PersistedNodeConfig,
    ) -> Result<(), ConfigManagerError<S::Error>> {
        config
            .validate()
            .map_err(ConfigManagerError::InvalidConfig)?;
        let envelope = StorageEnvelope {
            version: CONFIG_STORAGE_FORMAT_VERSION,
            config: config.clone(),
        };
        let mut raw = [0_u8; CONFIG_STORAGE_SIZE];
        let used = encode(&envelope, &mut raw).map_err(ConfigManagerError::Serialization)?;
        self.storage
            .write(&raw[..used])
            .map_err(ConfigManagerError::Storage)?;
        self.config = config;
        Ok(())
    }

    /// Clear only the configuration record. The current runtime remains
    /// unchanged; defaults are selected on the next boot.
    pub fn factory_reset(&mut self) -> Result<(), ConfigManagerError<S::Error>> {
        self.storage.clear().map_err(ConfigManagerError::Storage)
    }

    pub fn storage(&self) -> &S {
        &self.storage
    }
}

/// In-memory storage used by host tests and higher-level integration tests.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MemoryConfigStorage<const N: usize> {
    bytes: [u8; N],
    len: usize,
    fail_read: bool,
    fail_write: bool,
    fail_clear: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MemoryStorageError {
    Read,
    Write,
    Clear,
}

impl<const N: usize> Default for MemoryConfigStorage<N> {
    fn default() -> Self {
        Self {
            bytes: [0; N],
            len: 0,
            fail_read: false,
            fail_write: false,
            fail_clear: false,
        }
    }
}

impl<const N: usize> MemoryConfigStorage<N> {
    pub fn from_record(bytes: &[u8]) -> Self {
        let mut storage = Self::default();
        let count = core::cmp::min(bytes.len(), N);
        storage.bytes[..count].copy_from_slice(&bytes[..count]);
        storage.len = count;
        storage
    }

    pub fn fail_next_read(&mut self) {
        self.fail_read = true;
    }

    pub fn fail_next_write(&mut self) {
        self.fail_write = true;
    }

    pub fn fail_next_clear(&mut self) {
        self.fail_clear = true;
    }
}

impl<const N: usize> ConfigStorage for MemoryConfigStorage<N> {
    type Error = MemoryStorageError;

    fn read(&mut self, destination: &mut [u8]) -> Result<usize, Self::Error> {
        if self.fail_read {
            self.fail_read = false;
            return Err(MemoryStorageError::Read);
        }
        let count = core::cmp::min(self.len, destination.len());
        destination[..count].copy_from_slice(&self.bytes[..count]);
        Ok(count)
    }

    fn write(&mut self, data: &[u8]) -> Result<(), Self::Error> {
        if self.fail_write {
            self.fail_write = false;
            return Err(MemoryStorageError::Write);
        }
        if data.len() > N {
            return Err(MemoryStorageError::Write);
        }
        self.bytes.fill(0);
        self.bytes[..data.len()].copy_from_slice(data);
        self.len = data.len();
        Ok(())
    }

    fn clear(&mut self) -> Result<(), Self::Error> {
        if self.fail_clear {
            self.fail_clear = false;
            return Err(MemoryStorageError::Clear);
        }
        self.bytes.fill(0);
        self.len = 0;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{GpioInputConfig, InputMapping, PersistedNodeConfig};
    use uchi_pulse_common::InputEvent;

    fn config(action_id: u32) -> PersistedNodeConfig {
        let mut config = PersistedNodeConfig::defaults();
        config
            .input_mappings
            .push(InputMapping {
                gpio: 2,
                input_event: InputEvent::Click,
                action_id,
                enabled: true,
            })
            .unwrap();
        config
    }

    #[test]
    fn saves_and_restores_valid_config() {
        let mut storage = MemoryConfigStorage::<CONFIG_STORAGE_SIZE>::default();
        let defaults = PersistedNodeConfig::defaults();
        let mut manager = ConfigManager::new(storage.clone(), defaults.clone());
        manager.set_config(config(42)).unwrap();
        storage = manager.storage().clone();
        let restored = ConfigManager::new(storage, defaults);
        assert_eq!(restored.config().input_mappings[0].action_id, 42);
    }

    #[test]
    fn missing_or_corrupt_data_falls_back_to_defaults() {
        let defaults = PersistedNodeConfig::defaults();
        let empty = ConfigManager::new(
            MemoryConfigStorage::<CONFIG_STORAGE_SIZE>::default(),
            defaults.clone(),
        );
        assert_eq!(empty.config(), &defaults);

        let corrupt = ConfigManager::new(
            MemoryConfigStorage::<CONFIG_STORAGE_SIZE>::from_record(b"not-json"),
            defaults.clone(),
        );
        assert_eq!(corrupt.config(), &defaults);
    }

    #[test]
    fn invalid_saved_config_falls_back_to_defaults() {
        let mut invalid = config(7);
        invalid.gpio_inputs[0] = GpioInputConfig {
            gpio: 99,
            active_high: true,
            debounce_ms: 30,
        };
        let envelope = StorageEnvelope {
            version: CONFIG_STORAGE_FORMAT_VERSION,
            config: invalid,
        };
        let mut raw = [0; CONFIG_STORAGE_SIZE];
        let used = encode(&envelope, &mut raw).unwrap();
        let manager = ConfigManager::new(
            MemoryConfigStorage::<CONFIG_STORAGE_SIZE>::from_record(&raw[..used]),
            PersistedNodeConfig::defaults(),
        );
        assert_eq!(manager.config(), &PersistedNodeConfig::defaults());
    }

    #[test]
    fn version_mismatch_falls_back_and_factory_reset_clears_only_storage() {
        let stored_config = config(12);
        let envelope = StorageEnvelope {
            version: 99,
            config: stored_config,
        };
        let mut raw = [0; CONFIG_STORAGE_SIZE];
        let used = encode(&envelope, &mut raw).unwrap();
        let mut manager = ConfigManager::new(
            MemoryConfigStorage::<CONFIG_STORAGE_SIZE>::from_record(&raw[..used]),
            PersistedNodeConfig::defaults(),
        );
        assert_eq!(manager.config(), &PersistedNodeConfig::defaults());
        manager.set_config(config(12)).unwrap();
        manager.factory_reset().unwrap();
        assert_eq!(manager.config().input_mappings[0].action_id, 12);
        let restored =
            ConfigManager::new(manager.storage().clone(), PersistedNodeConfig::defaults());
        assert_eq!(restored.config(), &PersistedNodeConfig::defaults());
    }

    #[test]
    fn validation_failure_and_storage_failure_do_not_replace_active_config() {
        let defaults = PersistedNodeConfig::defaults();
        let mut storage = MemoryConfigStorage::<CONFIG_STORAGE_SIZE>::default();
        storage.fail_next_write();
        let mut manager = ConfigManager::new(storage, defaults.clone());
        let mut invalid = defaults.clone();
        invalid.double_click_interval_ms = 0;
        assert!(matches!(
            manager.set_config(invalid),
            Err(ConfigManagerError::InvalidConfig(_))
        ));
        assert_eq!(manager.config(), &defaults);
        assert!(matches!(
            manager.set_config(config(8)),
            Err(ConfigManagerError::Storage(_))
        ));
        assert_eq!(manager.config(), &defaults);
    }
}
