use serde::{Deserialize, Serialize};

use crate::types::{CommandName, RequestId, TextError};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CdcRequest<P> {
    pub version: u8,
    pub request_id: RequestId,
    pub command: CommandName,
    pub params: P,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum CdcStatus {
    #[serde(rename = "ok")]
    Ok,
    #[serde(rename = "error")]
    Error,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum CdcErrorCode {
    #[serde(rename = "INVALID_JSON")]
    InvalidJson,
    #[serde(rename = "UNSUPPORTED_VERSION")]
    UnsupportedVersion,
    #[serde(rename = "INVALID_COMMAND")]
    InvalidCommand,
    #[serde(rename = "INVALID_PARAMETER")]
    InvalidParameter,
    #[serde(rename = "INVALID_CONFIG")]
    InvalidConfig,
    #[serde(rename = "SAVE_FAILED")]
    SaveFailed,
    #[serde(rename = "OPERATION_FAILED")]
    OperationFailed,
    #[serde(rename = "NOT_SUPPORTED")]
    NotSupported,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CdcError {
    pub code: CdcErrorCode,
    pub message: heapless::String<128>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CdcResponse<D> {
    pub version: u8,
    pub request_id: RequestId,
    pub status: CdcStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<D>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<CdcError>,
}

impl<D> CdcResponse<D> {
    pub fn success(version: u8, request_id: RequestId, data: D) -> Self {
        Self {
            version,
            request_id,
            status: CdcStatus::Ok,
            data: Some(data),
            error: None,
        }
    }

    pub fn error(
        version: u8,
        request_id: RequestId,
        code: CdcErrorCode,
        message: heapless::String<128>,
    ) -> Self {
        Self {
            version,
            request_id,
            status: CdcStatus::Error,
            data: None,
            error: Some(CdcError { code, message }),
        }
    }
}

pub fn request_id(value: &str) -> Result<RequestId, TextError> {
    crate::types::text(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codec::{decode_line, encode_line};
    use crate::types::{RequestId, text};

    #[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
    struct EmptyParams {}

    #[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
    struct Info {
        kind: heapless::String<32>,
    }

    #[test]
    fn round_trips_cdc_request_with_lf_termination() {
        let request = CdcRequest {
            version: 1,
            request_id: request_id("123").unwrap(),
            command: crate::action::command_name("get_config").unwrap(),
            params: EmptyParams {},
        };
        let mut buffer = [0; 256];
        let used = encode_line(&request, &mut buffer).unwrap();
        assert_eq!(buffer[used - 1], b'\n');
        let decoded: CdcRequest<EmptyParams> = decode_line(&buffer[..used]).unwrap();
        assert_eq!(decoded, request);
    }

    #[test]
    fn preserves_request_id_on_success_and_error_responses() {
        let request_id: RequestId = request_id("123").unwrap();
        let success = CdcResponse::success(
            1,
            request_id.clone(),
            Info {
                kind: text("node").unwrap(),
            },
        );
        let mut buffer = [0; 256];
        let used = encode_line(&success, &mut buffer).unwrap();
        let decoded: CdcResponse<Info> = decode_line(&buffer[..used]).unwrap();
        assert_eq!(decoded.request_id, request_id);
        assert_eq!(decoded.status, CdcStatus::Ok);
        assert!(decoded.data.is_some());

        let error = CdcResponse::<Info>::error(
            1,
            request_id.clone(),
            CdcErrorCode::InvalidCommand,
            text("unknown command").unwrap(),
        );
        let used = encode_line(&error, &mut buffer).unwrap();
        let decoded: CdcResponse<Info> = decode_line(&buffer[..used]).unwrap();
        assert_eq!(decoded.request_id, request_id);
        assert_eq!(decoded.status, CdcStatus::Error);
        assert_eq!(decoded.error.unwrap().code, CdcErrorCode::InvalidCommand);
    }

    #[test]
    fn rejects_cdc_line_without_lf() {
        let input = br#"{"version":1,"request_id":"123","command":"get_config","params":{}}"#;
        assert!(decode_line::<CdcRequest<EmptyParams>>(input).is_err());
    }
}
