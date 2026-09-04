//! Uchi Pulse Hub building blocks.

pub use uchi_pulse_common as common_protocol;

pub mod action;
pub mod cdc;
pub mod db;
pub mod state;
pub mod udp;

pub const DEFAULT_BIND_ADDR: &str = "0.0.0.0:5000";
pub const DEFAULT_DB_PATH: &str = "uchi-pulse.db";
pub const DEFAULT_HELLO_REQUEST_ADDR: &str = "255.255.255.255:5000";
pub const DEFAULT_OFFLINE_TIMEOUT_SEC: u64 = 180;
