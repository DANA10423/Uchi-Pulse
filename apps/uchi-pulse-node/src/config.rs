//! Device configuration.
//!
//! The first firmware version keeps the configuration in a compile-time value.
//! The USB CDC configuration protocol can replace this module later without
//! changing the GPIO/event/network layers.

use crate::protocol::EventType;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InputFunction {
    MealReady,
    Call,
    Busy,
    BusyClear,
    EntryRequest,
    EntryOk,
    EntryLater,
    EntryNg,
    MailDetected,
    MailCleared,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InputBinding {
    /// Logical channel sent to the Hub. Physical GPIO numbers never go on the wire.
    pub channel: u8,
    pub gpio: u8,
    pub active_high: bool,
    pub debounce_ms: u16,
    pub event_type: EventType,
    pub function: InputFunction,
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
    pub hub_ipv4: [u8; 4],
    pub hub_port: u16,
    pub local_port: u16,
    pub ack_timeout_ms: u32,
    pub event_retry_count: u8,
    pub heartbeat_interval_sec: u32,
    pub inputs: &'static [InputBinding],
    pub outputs: &'static [OutputBinding],
}

pub const DEFAULT_INPUTS: &[InputBinding] = &[
    InputBinding {
        channel: 1,
        gpio: 2,
        active_high: false,
        debounce_ms: 30,
        event_type: EventType::Button,
        function: InputFunction::Busy,
    },
    InputBinding {
        channel: 2,
        gpio: 3,
        active_high: false,
        debounce_ms: 30,
        event_type: EventType::Button,
        function: InputFunction::EntryRequest,
    },
    InputBinding {
        channel: 3,
        gpio: 4,
        active_high: false,
        debounce_ms: 30,
        event_type: EventType::Button,
        function: InputFunction::Call,
    },
];

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
    hub_ipv4: [192, 168, 1, 2],
    hub_port: 5000,
    local_port: 5001,
    ack_timeout_ms: 3_000,
    event_retry_count: 3,
    heartbeat_interval_sec: 180,
    inputs: DEFAULT_INPUTS,
    outputs: DEFAULT_OUTPUTS,
};
