//! Child-side USB CDC framing and command handling.
//!
//! USB packet I/O is intentionally outside this module. `CdcLineParser` and
//! `NodeCdcHandler` can therefore be tested on a host without a USB device.

use heapless::{Deque, Vec};
use serde::{Deserialize, Serialize};
use uchi_pulse_common::cdc::{CdcErrorCode, CdcRequest, CdcResponse};
use uchi_pulse_common::codec::{CodecError, decode_line, encode_line};
use uchi_pulse_common::types::{RequestId, text};

use crate::config::{
    ConfigValidationError, GpioInputConfig, InputMapping, Ipv4Text, MAX_PERSISTED_GPIO_INPUTS,
    MAX_PERSISTED_INPUT_MAPPINGS, NetworkConfig, PersistedNodeConfig, WifiConfig,
};
use crate::storage::{ConfigManager, ConfigManagerError, ConfigStorage};

pub const CDC_PROTOCOL_VERSION: u8 = 1;
pub const MAX_CDC_LINE_SIZE: usize = 4096;
pub const MAX_CDC_QUEUED_LINES: usize = 4;

pub type CdcLine = Vec<u8, MAX_CDC_LINE_SIZE>;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CdcFrameError {
    LineTooLong,
    QueueFull,
}

/// Bounded LF framing for arbitrary USB read boundaries.
pub struct CdcLineParser {
    current: CdcLine,
    lines: Deque<CdcLine, MAX_CDC_QUEUED_LINES>,
    dropping_overlong: bool,
}

impl CdcLineParser {
    pub const fn new() -> Self {
        Self {
            current: Vec::new(),
            lines: Deque::new(),
            dropping_overlong: false,
        }
    }

    pub fn feed(&mut self, bytes: &[u8]) -> Result<(), CdcFrameError> {
        let mut error = None;
        for &byte in bytes {
            if self.dropping_overlong {
                if byte == b'\n' {
                    self.dropping_overlong = false;
                }
                continue;
            }

            if self.current.len() == MAX_CDC_LINE_SIZE {
                self.current.clear();
                self.dropping_overlong = byte != b'\n';
                error = Some(CdcFrameError::LineTooLong);
                continue;
            }

            let _ = self.current.push(byte);
            if byte == b'\n' {
                let line = core::mem::replace(&mut self.current, Vec::new());
                if self.lines.push_back(line).is_err() {
                    error = Some(CdcFrameError::QueueFull);
                }
            }
        }
        error.map_or(Ok(()), Err)
    }

    pub fn pop_line(&mut self) -> Option<CdcLine> {
        self.lines.pop_front()
    }
}

impl Default for CdcLineParser {
    fn default() -> Self {
        Self::new()
    }
}

/// Direct `set_config.params` representation. The protocol specification
/// shows the child settings fields but does not define a separate envelope;
/// this representation keeps `get_config.params` as `{}` and uses the same
/// field names for both read and write.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct NodeCdcParams {
    pub device_id: Option<uchi_pulse_common::DeviceId>,
    pub wifi: Option<WifiConfig>,
    pub network: Option<NetworkConfig>,
    pub gpio_inputs: Option<Vec<GpioInputConfig, MAX_PERSISTED_GPIO_INPUTS>>,
    pub input_mappings: Option<Vec<InputMapping, MAX_PERSISTED_INPUT_MAPPINGS>>,
    pub double_click_interval_ms: Option<u32>,
    pub long_press_threshold_ms: Option<u32>,
    pub ack_timeout_ms: Option<u32>,
    pub event_retry_count: Option<u8>,
    pub heartbeat_interval_sec: Option<u32>,
}

impl NodeCdcParams {
    fn into_config(self) -> Result<PersistedNodeConfig, CdcParameterError> {
        let config = PersistedNodeConfig {
            device_id: self.device_id.ok_or(CdcParameterError::MissingField)?,
            wifi: self.wifi.ok_or(CdcParameterError::MissingField)?,
            network: self.network.ok_or(CdcParameterError::MissingField)?,
            gpio_inputs: self.gpio_inputs.ok_or(CdcParameterError::MissingField)?,
            input_mappings: self.input_mappings.ok_or(CdcParameterError::MissingField)?,
            double_click_interval_ms: self
                .double_click_interval_ms
                .ok_or(CdcParameterError::MissingField)?,
            long_press_threshold_ms: self
                .long_press_threshold_ms
                .ok_or(CdcParameterError::MissingField)?,
            ack_timeout_ms: self.ack_timeout_ms.ok_or(CdcParameterError::MissingField)?,
            event_retry_count: self
                .event_retry_count
                .ok_or(CdcParameterError::MissingField)?,
            heartbeat_interval_sec: self
                .heartbeat_interval_sec
                .ok_or(CdcParameterError::MissingField)?,
        };
        config
            .validate()
            .map_err(CdcParameterError::InvalidConfig)?;
        Ok(config)
    }
}

impl From<&PersistedNodeConfig> for NodeCdcParams {
    fn from(config: &PersistedNodeConfig) -> Self {
        Self {
            device_id: Some(config.device_id.clone()),
            wifi: Some(config.wifi.clone()),
            network: Some(config.network.clone()),
            gpio_inputs: Some(config.gpio_inputs.clone()),
            input_mappings: Some(config.input_mappings.clone()),
            double_click_interval_ms: Some(config.double_click_interval_ms),
            long_press_threshold_ms: Some(config.long_press_threshold_ms),
            ack_timeout_ms: Some(config.ack_timeout_ms),
            event_retry_count: Some(config.event_retry_count),
            heartbeat_interval_sec: Some(config.heartbeat_interval_sec),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct EmptyData {}

/// Runtime status returned by the child CDC endpoint.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct NodeCdcStatus {
    pub device_id: uchi_pulse_common::DeviceId,
    pub ip_address: Option<Ipv4Text>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CdcAction {
    None,
    Reboot,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CdcHandleResult {
    pub response_len: usize,
    pub action: CdcAction,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CdcParameterError {
    MissingField,
    InvalidConfig(ConfigValidationError),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CdcHandlerError {
    ResponseTooLarge,
}

#[derive(Deserialize)]
struct RequestIdOnly {
    request_id: Option<RequestId>,
}

pub struct NodeCdcHandler<S> {
    manager: ConfigManager<S>,
}

impl<S: ConfigStorage> NodeCdcHandler<S> {
    pub fn new(manager: ConfigManager<S>) -> Self {
        Self { manager }
    }

    pub fn config(&self) -> &PersistedNodeConfig {
        self.manager.config()
    }

    pub fn handle_line(
        &mut self,
        line: &[u8],
        destination: &mut [u8],
    ) -> Result<CdcHandleResult, CdcHandlerError> {
        self.handle_line_with_status(line, destination, None)
    }

    pub fn handle_line_with_status(
        &mut self,
        line: &[u8],
        destination: &mut [u8],
        ip_address: Option<Ipv4Text>,
    ) -> Result<CdcHandleResult, CdcHandlerError> {
        let request: CdcRequest<NodeCdcParams> = match decode_line(line) {
            Ok(request) => request,
            Err(_) => {
                let request_id = decode_line::<RequestIdOnly>(line)
                    .ok()
                    .and_then(|request| request.request_id)
                    .unwrap_or_default();
                let response = CdcResponse::<EmptyData>::error(
                    CDC_PROTOCOL_VERSION,
                    request_id,
                    CdcErrorCode::InvalidJson,
                    message("invalid JSON"),
                );
                return encode_response(response, destination).map(|response_len| {
                    CdcHandleResult {
                        response_len,
                        action: CdcAction::None,
                    }
                });
            }
        };

        if request.version != CDC_PROTOCOL_VERSION {
            let response = CdcResponse::<EmptyData>::error(
                CDC_PROTOCOL_VERSION,
                request.request_id,
                CdcErrorCode::UnsupportedVersion,
                message("unsupported protocol version"),
            );
            return encode_response(response, destination).map(|response_len| CdcHandleResult {
                response_len,
                action: CdcAction::None,
            });
        }

        let request_id = request.request_id;
        match request.command.as_str() {
            "get_config" => encode_response(
                CdcResponse::success(
                    CDC_PROTOCOL_VERSION,
                    request_id,
                    self.manager.config().clone(),
                ),
                destination,
            )
            .map(|response_len| CdcHandleResult {
                response_len,
                action: CdcAction::None,
            }),
            "get_status" => encode_response(
                CdcResponse::success(
                    CDC_PROTOCOL_VERSION,
                    request_id,
                    NodeCdcStatus {
                        device_id: self.manager.config().device_id.clone(),
                        ip_address,
                    },
                ),
                destination,
            )
            .map(|response_len| CdcHandleResult {
                response_len,
                action: CdcAction::None,
            }),
            "set_config" => {
                let config = match request.params.into_config() {
                    Ok(config) => config,
                    Err(CdcParameterError::MissingField) => {
                        return self.error_response(
                            request_id,
                            CdcErrorCode::InvalidParameter,
                            "missing configuration field",
                            destination,
                        );
                    }
                    Err(CdcParameterError::InvalidConfig(_)) => {
                        return self.error_response(
                            request_id,
                            CdcErrorCode::InvalidConfig,
                            "invalid configuration",
                            destination,
                        );
                    }
                };
                match self.manager.set_config(config) {
                    Ok(()) => encode_response(
                        CdcResponse::success(CDC_PROTOCOL_VERSION, request_id, EmptyData {}),
                        destination,
                    )
                    .map(|response_len| CdcHandleResult {
                        response_len,
                        action: CdcAction::None,
                    }),
                    Err(ConfigManagerError::InvalidConfig(_)) => self.error_response(
                        request_id,
                        CdcErrorCode::InvalidConfig,
                        "invalid configuration",
                        destination,
                    ),
                    Err(ConfigManagerError::Storage(_))
                    | Err(ConfigManagerError::Serialization(_)) => self.error_response(
                        request_id,
                        CdcErrorCode::SaveFailed,
                        "configuration save failed",
                        destination,
                    ),
                }
            }
            "factory_reset" => match self.manager.factory_reset() {
                Ok(()) => encode_response(
                    CdcResponse::success(CDC_PROTOCOL_VERSION, request_id, EmptyData {}),
                    destination,
                )
                .map(|response_len| CdcHandleResult {
                    response_len,
                    action: CdcAction::None,
                }),
                Err(ConfigManagerError::Storage(_)) => self.error_response(
                    request_id,
                    CdcErrorCode::SaveFailed,
                    "configuration reset failed",
                    destination,
                ),
                Err(ConfigManagerError::InvalidConfig(_))
                | Err(ConfigManagerError::Serialization(_)) => self.error_response(
                    request_id,
                    CdcErrorCode::OperationFailed,
                    "configuration reset failed",
                    destination,
                ),
            },
            "reboot" => encode_response(
                CdcResponse::success(CDC_PROTOCOL_VERSION, request_id, EmptyData {}),
                destination,
            )
            .map(|response_len| CdcHandleResult {
                response_len,
                action: CdcAction::Reboot,
            }),
            "get_info" | "get_inputs" | "get_outputs" => self.error_response(
                request_id,
                CdcErrorCode::NotSupported,
                "command is not supported on the node yet",
                destination,
            ),
            _ => self.error_response(
                request_id,
                CdcErrorCode::InvalidCommand,
                "unknown command",
                destination,
            ),
        }
    }

    fn error_response(
        &self,
        request_id: RequestId,
        code: CdcErrorCode,
        error_message: &str,
        destination: &mut [u8],
    ) -> Result<CdcHandleResult, CdcHandlerError> {
        encode_response(
            CdcResponse::<EmptyData>::error(
                CDC_PROTOCOL_VERSION,
                request_id,
                code,
                message(error_message),
            ),
            destination,
        )
        .map(|response_len| CdcHandleResult {
            response_len,
            action: CdcAction::None,
        })
    }
}

fn message(value: &str) -> heapless::String<128> {
    text(value).unwrap_or_default()
}

fn encode_response<T: Serialize>(
    response: CdcResponse<T>,
    destination: &mut [u8],
) -> Result<usize, CdcHandlerError> {
    encode_line(&response, destination).map_err(|error| match error {
        CodecError::BufferTooSmall => CdcHandlerError::ResponseTooLarge,
        CodecError::InvalidJson | CodecError::MissingLineFeed | CodecError::TrailingData => {
            CdcHandlerError::ResponseTooLarge
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{
        GpioInputConfig, InputMapping, NetworkConfig, NetworkMode, PersistedNodeConfig,
        StaticIpv4Config,
    };
    use crate::storage::{CONFIG_STORAGE_SIZE, MemoryConfigStorage};
    use uchi_pulse_common::cdc::{CdcErrorCode, CdcResponse};
    use uchi_pulse_common::codec::decode_line;
    use uchi_pulse_common::{InputEvent, cdc::CdcStatus, types::text};

    fn request(command: &str, params: NodeCdcParams) -> CdcRequest<NodeCdcParams> {
        CdcRequest {
            version: CDC_PROTOCOL_VERSION,
            request_id: text("req-1").unwrap(),
            command: text(command).unwrap(),
            params,
        }
    }

    fn full_params(action_id: u32) -> NodeCdcParams {
        let mut mappings = Vec::new();
        mappings
            .push(InputMapping {
                gpio: 2,
                input_event: InputEvent::Click,
                action_id,
                enabled: true,
            })
            .unwrap();
        NodeCdcParams {
            device_id: Some(text("node-configured").unwrap()),
            wifi: Some(WifiConfig {
                ssid: text("MyHomeWiFi").unwrap(),
                password: text("secret-password").unwrap(),
            }),
            network: Some(NetworkConfig {
                mode: crate::config::NetworkMode::Dhcp,
                static_ipv4: None,
            }),
            gpio_inputs: Some(PersistedNodeConfig::defaults().gpio_inputs),
            input_mappings: Some(mappings),
            double_click_interval_ms: Some(400),
            long_press_threshold_ms: Some(1_000),
            ack_timeout_ms: Some(60_000),
            event_retry_count: Some(3),
            heartbeat_interval_sec: Some(180),
        }
    }

    fn handler() -> NodeCdcHandler<MemoryConfigStorage<CONFIG_STORAGE_SIZE>> {
        NodeCdcHandler::new(ConfigManager::new(
            MemoryConfigStorage::default(),
            PersistedNodeConfig::defaults(),
        ))
    }

    fn send_empty(
        handler: &mut NodeCdcHandler<MemoryConfigStorage<CONFIG_STORAGE_SIZE>>,
        request: CdcRequest<NodeCdcParams>,
    ) -> (CdcHandleResult, CdcResponse<EmptyData>) {
        let mut input = [0; 4096];
        let used = encode_line(&request, &mut input).unwrap();
        let mut output = [0; 4096];
        let result = handler.handle_line(&input[..used], &mut output).unwrap();
        let response = decode_line(&output[..result.response_len]).unwrap();
        (result, response)
    }

    fn send_config(
        handler: &mut NodeCdcHandler<MemoryConfigStorage<CONFIG_STORAGE_SIZE>>,
        request: CdcRequest<NodeCdcParams>,
    ) -> (CdcHandleResult, CdcResponse<PersistedNodeConfig>) {
        let mut input = [0; 4096];
        let used = encode_line(&request, &mut input).unwrap();
        let mut output = [0; 4096];
        let result = handler.handle_line(&input[..used], &mut output).unwrap();
        let response = decode_line(&output[..result.response_len]).unwrap();
        (result, response)
    }

    fn send_status(
        handler: &mut NodeCdcHandler<MemoryConfigStorage<CONFIG_STORAGE_SIZE>>,
        request: CdcRequest<NodeCdcParams>,
        ip_address: Option<Ipv4Text>,
    ) -> (CdcHandleResult, CdcResponse<NodeCdcStatus>) {
        let mut input = [0; 4096];
        let used = encode_line(&request, &mut input).unwrap();
        let mut output = [0; 4096];
        let result = handler
            .handle_line_with_status(&input[..used], &mut output, ip_address)
            .unwrap();
        let response = decode_line(&output[..result.response_len]).unwrap();
        (result, response)
    }

    #[test]
    fn get_config_returns_defaults_and_preserves_request_id() {
        let mut handler = handler();
        let (_, response) = send_config(
            &mut handler,
            request("get_config", NodeCdcParams::default()),
        );
        assert_eq!(response.status, CdcStatus::Ok);
        assert_eq!(response.request_id.as_str(), "req-1");
        assert_eq!(response.data.unwrap(), PersistedNodeConfig::defaults());
    }

    #[test]
    fn set_config_validates_saves_and_is_visible_to_get_config() {
        let mut handler = handler();
        let (_, response) = send_empty(&mut handler, request("set_config", full_params(77)));
        assert_eq!(response.status, CdcStatus::Ok);
        assert_eq!(handler.config().device_id.as_str(), "node-configured");
        assert_eq!(handler.config().input_mappings[0].action_id, 77);
    }

    #[test]
    fn set_config_persists_wifi_and_static_network_settings() {
        let mut handler = handler();
        let mut params = full_params(78);
        params.wifi = Some(WifiConfig {
            ssid: text("configured-wifi").unwrap(),
            password: text("configured-password").unwrap(),
        });
        params.network = Some(NetworkConfig {
            mode: NetworkMode::Static,
            static_ipv4: Some(StaticIpv4Config {
                ip_address: text("192.168.1.50").unwrap(),
                prefix_length: 24,
                gateway: text("192.168.1.1").unwrap(),
                dns: text("1.1.1.1").unwrap(),
            }),
        });

        let (_, response) = send_empty(&mut handler, request("set_config", params));
        assert_eq!(response.status, CdcStatus::Ok);
        assert_eq!(handler.config().wifi.ssid.as_str(), "configured-wifi");
        assert_eq!(
            handler.config().wifi.password.as_str(),
            "configured-password"
        );
        assert_eq!(handler.config().network.mode, NetworkMode::Static);
        assert_eq!(
            handler
                .config()
                .network
                .static_ipv4
                .as_ref()
                .unwrap()
                .ip_address
                .as_str(),
            "192.168.1.50"
        );

        let (_, response) = send_config(
            &mut handler,
            request("get_config", NodeCdcParams::default()),
        );
        assert_eq!(response.status, CdcStatus::Ok);
        assert_eq!(response.data.unwrap(), handler.config().clone());
    }

    #[test]
    fn set_config_rejects_invalid_static_network_settings() {
        let mut handler = handler();
        let mut params = full_params(79);
        params.network = Some(NetworkConfig {
            mode: NetworkMode::Static,
            static_ipv4: Some(StaticIpv4Config {
                ip_address: text("192.168.1.50").unwrap(),
                prefix_length: 24,
                gateway: text("not-an-ip").unwrap(),
                dns: text("1.1.1.1").unwrap(),
            }),
        });
        let (_, response) = send_empty(&mut handler, request("set_config", params));
        assert_eq!(response.error.unwrap().code, CdcErrorCode::InvalidConfig);
        assert_eq!(handler.config(), &PersistedNodeConfig::defaults());
    }

    #[test]
    fn invalid_config_is_rejected_without_changing_storage() {
        let mut handler = handler();
        let mut params = full_params(10);
        params.gpio_inputs.as_mut().unwrap()[0] = GpioInputConfig {
            gpio: 99,
            active_high: true,
            debounce_ms: 30,
        };
        let (_, response) = send_empty(&mut handler, request("set_config", params));
        assert_eq!(response.error.unwrap().code, CdcErrorCode::InvalidConfig);
        assert_eq!(handler.config(), &PersistedNodeConfig::defaults());
    }

    #[test]
    fn protocol_and_command_errors_are_distinguished() {
        let mut handler = handler();
        let mut unsupported = request("get_config", NodeCdcParams::default());
        unsupported.version = 2;
        let (_, response) = send_empty(&mut handler, unsupported);
        assert_eq!(
            response.error.unwrap().code,
            CdcErrorCode::UnsupportedVersion
        );

        let (_, response) = send_empty(
            &mut handler,
            request("no_such_command", NodeCdcParams::default()),
        );
        assert_eq!(response.error.unwrap().code, CdcErrorCode::InvalidCommand);
    }

    #[test]
    fn malformed_json_gets_invalid_json_response() {
        let mut handler = handler();
        let mut output = [0; 4096];
        let result = handler
            .handle_line(b"{\"version\":1,\"request_id\":\"bad\",}\n", &mut output)
            .unwrap();
        let response: CdcResponse<EmptyData> = decode_line(&output[..result.response_len]).unwrap();
        assert_eq!(response.error.unwrap().code, CdcErrorCode::InvalidJson);
    }

    #[test]
    fn get_status_returns_runtime_ip_address() {
        let mut handler = handler();
        let (_, response) = send_status(
            &mut handler,
            request("get_status", NodeCdcParams::default()),
            Some(text("192.168.1.42").unwrap()),
        );
        assert_eq!(response.status, CdcStatus::Ok);
        let status = response.data.unwrap();
        assert_eq!(status.device_id.as_str(), "family-node-01");
        assert_eq!(status.ip_address.unwrap().as_str(), "192.168.1.42");

        let (_, response) = send_status(
            &mut handler,
            request("get_status", NodeCdcParams::default()),
            None,
        );
        assert_eq!(response.data.unwrap().ip_address, None);
    }

    #[test]
    fn unsupported_commands_and_reboot_have_explicit_boundaries() {
        let mut handler = handler();
        let (result, response) = send_empty(
            &mut handler,
            request("get_inputs", NodeCdcParams::default()),
        );
        assert_eq!(response.error.unwrap().code, CdcErrorCode::NotSupported);
        assert_eq!(result.action, CdcAction::None);
        let (result, response) =
            send_empty(&mut handler, request("reboot", NodeCdcParams::default()));
        assert_eq!(response.status, CdcStatus::Ok);
        assert_eq!(result.action, CdcAction::Reboot);
    }

    #[test]
    fn factory_reset_returns_success_without_reconfiguring_running_state() {
        let mut handler = handler();
        send_empty(&mut handler, request("set_config", full_params(91)));
        let (_, response) = send_empty(
            &mut handler,
            request("factory_reset", NodeCdcParams::default()),
        );
        assert_eq!(response.status, CdcStatus::Ok);
        assert_eq!(handler.config().input_mappings[0].action_id, 91);
    }

    #[test]
    fn storage_failure_is_reported_as_save_failed() {
        let mut storage = MemoryConfigStorage::<CONFIG_STORAGE_SIZE>::default();
        storage.fail_next_write();
        let mut handler =
            NodeCdcHandler::new(ConfigManager::new(storage, PersistedNodeConfig::defaults()));
        let (_, response) = send_empty(&mut handler, request("set_config", full_params(5)));
        assert_eq!(response.error.unwrap().code, CdcErrorCode::SaveFailed);
    }

    #[test]
    fn line_parser_handles_fragments_multiple_lines_empty_lines_and_overlong_lines() {
        let mut parser = CdcLineParser::new();
        assert!(parser.feed(b"{\"a\"").is_ok());
        assert!(parser.feed(b":1}\n{\"b\":2}\n").is_ok());
        assert_eq!(parser.pop_line().unwrap().as_slice(), b"{\"a\":1}\n");
        assert_eq!(parser.pop_line().unwrap().as_slice(), b"{\"b\":2}\n");
        assert!(parser.feed(b"\n").is_ok());
        assert_eq!(parser.pop_line().unwrap().as_slice(), b"\n");

        let mut overlong = Vec::<u8, { MAX_CDC_LINE_SIZE + 2 }>::new();
        for _ in 0..MAX_CDC_LINE_SIZE + 1 {
            overlong.push(b'x').unwrap();
        }
        overlong.push(b'\n').unwrap();
        assert_eq!(
            parser.feed(overlong.as_slice()),
            Err(CdcFrameError::LineTooLong)
        );
        assert!(parser.feed(b"{\"ok\":true}\n").is_ok());
        assert!(parser.pop_line().is_some());
    }
}
