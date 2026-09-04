//! Device configuration.

use heapless::{String, Vec};
use serde::{Deserialize, Serialize};
use uchi_pulse_common::types::text;
use uchi_pulse_common::{
    ActionId, DEFAULT_ACK_TIMEOUT_MS, DEFAULT_EVENT_RETRY_COUNT, DEFAULT_HEARTBEAT_INTERVAL_SEC,
    DeviceId, InputEvent,
};

/// GPIOs exposed on Raspberry Pi Pico W / Pico 2 W headers and available to
/// Uchi-Pulse input configuration. GPIO 23, 24, 25 and 29 are reserved for the
/// onboard CYW43 Wi-Fi interface and are intentionally excluded.
pub const SUPPORTED_GPIO_PINS: &[u8] = &[
    0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 26, 27, 28,
];
pub const MAX_PERSISTED_GPIO_INPUTS: usize = 10;
pub const MAX_PERSISTED_INPUT_MAPPINGS: usize = 32;
pub const WIFI_SSID_CAPACITY: usize = 32;
pub const WIFI_PASSWORD_CAPACITY: usize = 64;
pub const IPV4_TEXT_CAPACITY: usize = 15;
pub const UDP_PORT: u16 = 5000;

pub type WifiSsid = String<WIFI_SSID_CAPACITY>;
pub type WifiPassword = String<WIFI_PASSWORD_CAPACITY>;
pub type Ipv4Text = String<IPV4_TEXT_CAPACITY>;

/// Physical properties of one GPIO input.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct GpioInputConfig {
    pub gpio: u8,
    pub active_high: bool,
    pub debounce_ms: u16,
}

/// A data-only mapping from a physical input gesture to a parent Action ID.
///
/// The child does not know or interpret the business meaning of `action_id`.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct InputMapping {
    pub gpio: u8,
    pub input_event: InputEvent,
    pub action_id: ActionId,
    pub enabled: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct WifiConfig {
    pub ssid: WifiSsid,
    pub password: WifiPassword,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum NetworkMode {
    #[serde(rename = "DHCP")]
    Dhcp,
    #[serde(rename = "STATIC")]
    Static,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct StaticIpv4Config {
    pub ip_address: Ipv4Text,
    pub prefix_length: u8,
    pub gateway: Ipv4Text,
    pub dns: Ipv4Text,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct NetworkConfig {
    pub mode: NetworkMode,
    pub static_ipv4: Option<StaticIpv4Config>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OutputBinding {
    pub output: u8,
    pub gpio: u8,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NodeConfig {
    pub device_id: &'static str,
    pub user_id: &'static str,
    pub name: &'static str,
    pub firmware_version: &'static str,
    pub wifi_ssid: &'static str,
    pub wifi_password: &'static str,
    pub local_port: u16,
    pub ack_timeout_ms: u32,
    pub event_retry_count: u8,
    pub heartbeat_interval_sec: u32,
    pub double_click_interval_ms: u32,
    pub long_press_threshold_ms: u32,
    pub gpio_inputs: &'static [GpioInputConfig],
    pub input_mappings: &'static [InputMapping],
    pub outputs: &'static [OutputBinding],
}

/// The configuration exposed through CDC and persisted in flash.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PersistedNodeConfig {
    pub device_id: DeviceId,
    pub wifi: WifiConfig,
    pub network: NetworkConfig,
    pub gpio_inputs: Vec<GpioInputConfig, MAX_PERSISTED_GPIO_INPUTS>,
    pub input_mappings: Vec<InputMapping, MAX_PERSISTED_INPUT_MAPPINGS>,
    pub double_click_interval_ms: u32,
    pub long_press_threshold_ms: u32,
    pub ack_timeout_ms: u32,
    pub event_retry_count: u8,
    pub heartbeat_interval_sec: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConfigValidationError {
    EmptyDeviceId,
    UnsupportedGpio(u8),
    DuplicateGpio(u8),
    MappingReferencesUnknownGpio(u8),
    DuplicateMapping { gpio: u8, input_event: InputEvent },
    ZeroDebounce(u8),
    ZeroDoubleClickInterval,
    ZeroLongPressThreshold,
    ZeroAckTimeout,
    ZeroHeartbeatInterval,
    EmptyWifiSsid,
    InvalidWifiCredential,
    MissingStaticIpv4,
    InvalidPrefixLength(u8),
    InvalidIpv4,
}

impl PersistedNodeConfig {
    pub fn defaults() -> Self {
        let mut gpio_inputs = Vec::new();
        for input in DEFAULT_GPIO_INPUTS {
            let _ = gpio_inputs.push(*input);
        }
        Self {
            device_id: uchi_pulse_common::types::text(DEFAULT_CONFIG.device_id).unwrap(),
            wifi: WifiConfig {
                ssid: text(DEFAULT_CONFIG.wifi_ssid).unwrap(),
                password: text(DEFAULT_CONFIG.wifi_password).unwrap(),
            },
            network: NetworkConfig {
                mode: NetworkMode::Dhcp,
                static_ipv4: None,
            },
            gpio_inputs,
            input_mappings: Vec::new(),
            double_click_interval_ms: DEFAULT_CONFIG.double_click_interval_ms,
            long_press_threshold_ms: DEFAULT_CONFIG.long_press_threshold_ms,
            ack_timeout_ms: DEFAULT_ACK_TIMEOUT_MS,
            event_retry_count: DEFAULT_EVENT_RETRY_COUNT,
            heartbeat_interval_sec: DEFAULT_HEARTBEAT_INTERVAL_SEC,
        }
    }

    pub fn from_node_config(config: &NodeConfig) -> Result<Self, ConfigValidationError> {
        let device_id = uchi_pulse_common::types::text(config.device_id)
            .map_err(|_| ConfigValidationError::EmptyDeviceId)?;
        let mut gpio_inputs = Vec::new();
        for input in config.gpio_inputs {
            gpio_inputs
                .push(*input)
                .map_err(|_| ConfigValidationError::UnsupportedGpio(input.gpio))?;
        }
        let mut input_mappings = Vec::new();
        for mapping in config.input_mappings {
            input_mappings
                .push(*mapping)
                .map_err(|_| ConfigValidationError::DuplicateMapping {
                    gpio: mapping.gpio,
                    input_event: mapping.input_event,
                })?;
        }
        let result = Self {
            device_id,
            wifi: WifiConfig {
                ssid: text(config.wifi_ssid)
                    .map_err(|_| ConfigValidationError::InvalidWifiCredential)?,
                password: text(config.wifi_password)
                    .map_err(|_| ConfigValidationError::InvalidWifiCredential)?,
            },
            network: NetworkConfig {
                mode: NetworkMode::Dhcp,
                static_ipv4: None,
            },
            gpio_inputs,
            input_mappings,
            double_click_interval_ms: config.double_click_interval_ms,
            long_press_threshold_ms: config.long_press_threshold_ms,
            ack_timeout_ms: config.ack_timeout_ms,
            event_retry_count: config.event_retry_count,
            heartbeat_interval_sec: config.heartbeat_interval_sec,
        };
        result.validate()?;
        Ok(result)
    }

    pub fn validate(&self) -> Result<(), ConfigValidationError> {
        if self.device_id.is_empty() {
            return Err(ConfigValidationError::EmptyDeviceId);
        }
        if self.wifi.ssid.is_empty() {
            return Err(ConfigValidationError::EmptyWifiSsid);
        }
        match self.network.mode {
            NetworkMode::Dhcp => {}
            NetworkMode::Static => {
                let static_ipv4 = self
                    .network
                    .static_ipv4
                    .as_ref()
                    .ok_or(ConfigValidationError::MissingStaticIpv4)?;
                if static_ipv4.prefix_length > 32 {
                    return Err(ConfigValidationError::InvalidPrefixLength(
                        static_ipv4.prefix_length,
                    ));
                }
                if parse_ipv4(static_ipv4.ip_address.as_str()).is_none()
                    || parse_ipv4(static_ipv4.gateway.as_str()).is_none()
                    || parse_ipv4(static_ipv4.dns.as_str()).is_none()
                {
                    return Err(ConfigValidationError::InvalidIpv4);
                }
            }
        }
        for input in &self.gpio_inputs {
            if !SUPPORTED_GPIO_PINS.contains(&input.gpio) {
                return Err(ConfigValidationError::UnsupportedGpio(input.gpio));
            }
            if self
                .gpio_inputs
                .iter()
                .filter(|other| other.gpio == input.gpio)
                .count()
                > 1
            {
                return Err(ConfigValidationError::DuplicateGpio(input.gpio));
            }
            if input.debounce_ms == 0 {
                return Err(ConfigValidationError::ZeroDebounce(input.gpio));
            }
        }
        if self.double_click_interval_ms == 0 {
            return Err(ConfigValidationError::ZeroDoubleClickInterval);
        }
        if self.long_press_threshold_ms == 0 {
            return Err(ConfigValidationError::ZeroLongPressThreshold);
        }
        if self.ack_timeout_ms == 0 {
            return Err(ConfigValidationError::ZeroAckTimeout);
        }
        if self.heartbeat_interval_sec == 0 {
            return Err(ConfigValidationError::ZeroHeartbeatInterval);
        }
        for (index, mapping) in self.input_mappings.iter().enumerate() {
            if !self
                .gpio_inputs
                .iter()
                .any(|input| input.gpio == mapping.gpio)
            {
                return Err(ConfigValidationError::MappingReferencesUnknownGpio(
                    mapping.gpio,
                ));
            }
            if self
                .input_mappings
                .iter()
                .skip(index + 1)
                .any(|other| other.gpio == mapping.gpio && other.input_event == mapping.input_event)
            {
                return Err(ConfigValidationError::DuplicateMapping {
                    gpio: mapping.gpio,
                    input_event: mapping.input_event,
                });
            }
        }
        Ok(())
    }

    pub fn input_config(&self) -> crate::input::InputConfig<'_> {
        crate::input::InputConfig {
            gpio_inputs: self.gpio_inputs.as_slice(),
            mappings: self.input_mappings.as_slice(),
            double_click_interval_ms: self.double_click_interval_ms,
            long_press_threshold_ms: self.long_press_threshold_ms,
        }
    }
}

impl NodeConfig {
    pub const fn input_config(&self) -> crate::input::InputConfig<'_> {
        crate::input::InputConfig {
            gpio_inputs: self.gpio_inputs,
            mappings: self.input_mappings,
            double_click_interval_ms: self.double_click_interval_ms,
            long_press_threshold_ms: self.long_press_threshold_ms,
        }
    }
}

/// Existing firmware used 30 ms per-input debounce. Keep that hardware
/// setting and the confirmed CDC default.
pub const DEFAULT_GPIO_INPUTS: &[GpioInputConfig] = &[
    GpioInputConfig {
        gpio: 2,
        active_high: false,
        debounce_ms: 30,
    },
    GpioInputConfig {
        gpio: 3,
        active_high: false,
        debounce_ms: 30,
    },
    GpioInputConfig {
        gpio: 4,
        active_high: false,
        debounce_ms: 30,
    },
];

/// No Action IDs are assumed by the firmware defaults. CDC configuration can
/// populate this table in a later phase.
pub const DEFAULT_INPUT_MAPPINGS: &[InputMapping] = &[];

pub const DEFAULT_OUTPUTS: &[OutputBinding] = &[
    OutputBinding {
        output: 1,
        gpio: 10,
    },
    OutputBinding {
        output: 2,
        gpio: 11,
    },
    OutputBinding {
        output: 3,
        gpio: 12,
    },
];

pub const DEFAULT_CONFIG: NodeConfig = NodeConfig {
    device_id: "family-node-01",
    user_id: "father",
    name: "リビング",
    firmware_version: env!("CARGO_PKG_VERSION"),
    wifi_ssid: "change-me",
    wifi_password: "change-me",
    local_port: UDP_PORT,
    ack_timeout_ms: DEFAULT_ACK_TIMEOUT_MS,
    event_retry_count: DEFAULT_EVENT_RETRY_COUNT,
    heartbeat_interval_sec: DEFAULT_HEARTBEAT_INTERVAL_SEC,
    double_click_interval_ms: 400,
    long_press_threshold_ms: 1_000,
    gpio_inputs: DEFAULT_GPIO_INPUTS,
    input_mappings: DEFAULT_INPUT_MAPPINGS,
    outputs: DEFAULT_OUTPUTS,
};

pub fn parse_ipv4(value: &str) -> Option<[u8; 4]> {
    let mut octets = [0; 4];
    let mut count = 0;
    for part in value.split('.') {
        if count == 4 || part.is_empty() {
            return None;
        }
        octets[count] = part.parse().ok()?;
        count += 1;
    }
    (count == 4).then_some(octets)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_phase_eight_values() {
        let config = PersistedNodeConfig::defaults();
        assert_eq!(config.gpio_inputs[0].debounce_ms, 30);
        assert_eq!(config.double_click_interval_ms, 400);
        assert_eq!(config.long_press_threshold_ms, 1_000);
        assert_eq!(config.ack_timeout_ms, 60_000);
        assert_eq!(config.event_retry_count, 3);
        assert_eq!(config.heartbeat_interval_sec, 180);
    }

    #[test]
    fn validation_allows_multiple_gpios_and_multiple_events_per_gpio() {
        let mut config = PersistedNodeConfig::defaults();
        config
            .input_mappings
            .push(InputMapping {
                gpio: 2,
                input_event: InputEvent::Click,
                action_id: 10,
                enabled: true,
            })
            .unwrap();
        config
            .input_mappings
            .push(InputMapping {
                gpio: 2,
                input_event: InputEvent::LongPress,
                action_id: 20,
                enabled: false,
            })
            .unwrap();
        config
            .input_mappings
            .push(InputMapping {
                gpio: 3,
                input_event: InputEvent::Click,
                action_id: 30,
                enabled: true,
            })
            .unwrap();
        assert!(config.validate().is_ok());
        assert_eq!(config.input_mappings[1].action_id, 20);
        assert!(!config.input_mappings[1].enabled);
    }

    #[test]
    fn validation_accepts_all_exposed_gpio_numbers() {
        let mut config = PersistedNodeConfig::defaults();
        config.gpio_inputs.clear();
        for &gpio in &[0, 1, 5, 9, 13, 17, 21, 26, 27, 28] {
            config
                .gpio_inputs
                .push(GpioInputConfig {
                    gpio,
                    active_high: false,
                    debounce_ms: 30,
                })
                .unwrap();
        }
        assert_eq!(config.gpio_inputs.len(), MAX_PERSISTED_GPIO_INPUTS);
        assert!(config.validate().is_ok());
    }

    #[test]
    fn validation_rejects_reserved_wifi_gpio() {
        let mut config = PersistedNodeConfig::defaults();
        config.gpio_inputs[0].gpio = 23;
        assert_eq!(
            config.validate(),
            Err(ConfigValidationError::UnsupportedGpio(23))
        );
    }

    #[test]
    fn defaults_use_dhcp_and_persist_wifi_credentials() {
        let config = PersistedNodeConfig::defaults();
        assert_eq!(config.wifi.ssid.as_str(), "change-me");
        assert_eq!(config.wifi.password.as_str(), "change-me");
        assert_eq!(config.network.mode, NetworkMode::Dhcp);
        assert!(config.network.static_ipv4.is_none());
    }

    #[test]
    fn static_network_requires_valid_ipv4_values() {
        let mut config = PersistedNodeConfig::defaults();
        config.network = NetworkConfig {
            mode: NetworkMode::Static,
            static_ipv4: Some(StaticIpv4Config {
                ip_address: text("192.168.1.50").unwrap(),
                prefix_length: 24,
                gateway: text("192.168.1.1").unwrap(),
                dns: text("192.168.1.1").unwrap(),
            }),
        };
        assert!(config.validate().is_ok());

        config.network.static_ipv4.as_mut().unwrap().prefix_length = 33;
        assert_eq!(
            config.validate(),
            Err(ConfigValidationError::InvalidPrefixLength(33))
        );

        config.network.static_ipv4.as_mut().unwrap().prefix_length = 24;
        config.network.static_ipv4.as_mut().unwrap().dns = text("192.168.1").unwrap();
        assert_eq!(config.validate(), Err(ConfigValidationError::InvalidIpv4));
    }

    #[test]
    fn static_network_requires_static_ipv4_block() {
        let mut config = PersistedNodeConfig::defaults();
        config.network.mode = NetworkMode::Static;
        assert_eq!(
            config.validate(),
            Err(ConfigValidationError::MissingStaticIpv4)
        );
    }
}
