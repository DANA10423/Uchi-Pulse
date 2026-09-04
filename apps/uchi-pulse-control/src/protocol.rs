use std::io::{ErrorKind, Read, Write};
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::thread;
use std::time::Duration;

use serde_json::Value;
use uchi_pulse_common::action::command_name;
use uchi_pulse_common::cdc::{CdcRequest, CdcResponse};
use uchi_pulse_common::types::text;

pub const CDC_PROTOCOL_VERSION: u8 = 1;
pub const SERIAL_BAUD_RATE: u32 = 115_200;
/// Parent configuration can contain many Actions and notification entries.
pub const MAX_CDC_LINE_SIZE: usize = 64 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkerEvent {
    Opened,
    Line(String),
    Error(String),
    Closed,
}

#[derive(Debug)]
pub enum WorkerCommand {
    Send(String),
    Close,
}

/// Incremental LF framing for USB CDC reads, which may split a JSON message
/// at any byte boundary.
#[derive(Debug, Default)]
pub struct LineDecoder {
    current: Vec<u8>,
    dropping_overlong: bool,
}

impl LineDecoder {
    pub fn feed(&mut self, bytes: &[u8]) -> Vec<Result<String, String>> {
        let mut lines = Vec::new();
        for &byte in bytes {
            if self.dropping_overlong {
                if byte == b'\n' {
                    self.dropping_overlong = false;
                }
                continue;
            }

            if self.current.len() >= MAX_CDC_LINE_SIZE {
                self.current.clear();
                self.dropping_overlong = byte != b'\n';
                lines.push(Err(format!(
                    "CDC response exceeded {MAX_CDC_LINE_SIZE} bytes"
                )));
                continue;
            }

            self.current.push(byte);
            if byte == b'\n' {
                let line = std::mem::take(&mut self.current);
                let line = line.strip_suffix(b"\n").unwrap_or(&line);
                let line = line.strip_suffix(b"\r").unwrap_or(line);
                lines.push(
                    String::from_utf8(line.to_vec())
                        .map_err(|error| format!("CDC response was not UTF-8: {error}")),
                );
            }
        }
        lines
    }
}

pub fn spawn_serial_worker(path: String) -> (Sender<WorkerCommand>, Receiver<WorkerEvent>) {
    let (command_tx, command_rx) = mpsc::channel();
    let (event_tx, event_rx) = mpsc::channel();
    thread::spawn(move || serial_worker(path, command_rx, event_tx));
    (command_tx, event_rx)
}

fn serial_worker(path: String, command_rx: Receiver<WorkerCommand>, event_tx: Sender<WorkerEvent>) {
    let port_result = serialport::new(&path, SERIAL_BAUD_RATE)
        .timeout(Duration::from_millis(100))
        .open();
    let mut port = match port_result {
        Ok(port) => port,
        Err(error) => {
            let _ = event_tx.send(WorkerEvent::Error(format!(
                "Failed to open {path}: {error}"
            )));
            return;
        }
    };

    if let Err(error) = port.write_data_terminal_ready(true) {
        let _ = event_tx.send(WorkerEvent::Error(format!(
            "Failed to assert CDC DTR: {error}"
        )));
        return;
    }
    let _ = event_tx.send(WorkerEvent::Opened);

    let mut decoder = LineDecoder::default();
    let mut buffer = [0_u8; 512];
    loop {
        loop {
            match command_rx.try_recv() {
                Ok(WorkerCommand::Send(line)) => {
                    if let Err(error) = port.write_all(line.as_bytes()) {
                        let _ = event_tx.send(WorkerEvent::Error(format!(
                            "Failed to write CDC request: {error}"
                        )));
                        return;
                    }
                    if let Err(error) = port.flush() {
                        let _ = event_tx.send(WorkerEvent::Error(format!(
                            "Failed to flush CDC request: {error}"
                        )));
                        return;
                    }
                }
                Ok(WorkerCommand::Close) => {
                    let _ = port.write_data_terminal_ready(false);
                    let _ = event_tx.send(WorkerEvent::Closed);
                    return;
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => return,
            }
        }

        match port.read(&mut buffer) {
            Ok(length) => {
                for line in decoder.feed(&buffer[..length]) {
                    match line {
                        Ok(line) => {
                            let _ = event_tx.send(WorkerEvent::Line(line));
                        }
                        Err(error) => {
                            let _ = event_tx.send(WorkerEvent::Error(error));
                        }
                    }
                }
            }
            Err(error) if matches!(error.kind(), ErrorKind::TimedOut | ErrorKind::WouldBlock) => {}
            Err(error) => {
                let _ = event_tx.send(WorkerEvent::Error(format!(
                    "Failed to read CDC response: {error}"
                )));
                return;
            }
        }
    }
}

pub fn build_request(command: &str, params: Value, request_number: u64) -> Result<String, String> {
    let request_id = text(&format!("desktop-{request_number:06}"))
        .map_err(|_| "request_id is too long".to_owned())?;
    let command = command_name(command).map_err(|_| "command name is too long".to_owned())?;
    let request = CdcRequest {
        version: CDC_PROTOCOL_VERSION,
        request_id,
        command,
        params,
    };
    let mut encoded = serde_json::to_string(&request)
        .map_err(|error| format!("failed to encode CDC request: {error}"))?;
    encoded.push('\n');
    Ok(encoded)
}

pub fn parse_response(line: &str) -> Result<CdcResponse<Value>, String> {
    serde_json::from_str(line).map_err(|error| format!("invalid CDC response JSON: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decoder_handles_fragmented_lines_and_crlf() {
        let mut decoder = LineDecoder::default();
        assert!(decoder.feed(br#"{"#).is_empty());
        let lines = decoder.feed(b"\"version\":1}\r\n");
        assert_eq!(lines, vec![Ok(r#"{"version":1}"#.to_owned())]);
    }

    #[test]
    fn decoder_reports_overlong_lines_and_recovers() {
        let mut decoder = LineDecoder::default();
        let mut input = vec![b'x'; MAX_CDC_LINE_SIZE + 1];
        input.push(b'\n');
        input.extend_from_slice(b"ok\n");
        let lines = decoder.feed(&input);
        assert!(matches!(lines[0], Err(_)));
        assert_eq!(lines[1], Ok("ok".to_owned()));
    }

    #[test]
    fn request_uses_common_cdc_model_and_lf_termination() {
        let request = build_request("get_config", Value::Object(Default::default()), 7).unwrap();
        assert!(request.ends_with('\n'));
        assert!(request.contains(r#""request_id":"desktop-000007""#));
        assert!(request.contains(r#""command":"get_config""#));
    }

    #[test]
    fn response_parser_accepts_success_and_error_shapes() {
        let response =
            parse_response(r#"{"version":1,"request_id":"desktop-1","status":"ok","data":{}}"#)
                .unwrap();
        assert!(response.data.is_some());

        let response = parse_response(
            r#"{"version":1,"request_id":"desktop-1","status":"error","error":{"code":"NOT_SUPPORTED","message":"no"}}"#,
        )
        .unwrap();
        assert!(response.error.is_some());
    }
}
