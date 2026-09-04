use serde::{Serialize, de::Deserialize};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CodecError {
    BufferTooSmall,
    InvalidJson,
    MissingLineFeed,
    TrailingData,
}

pub fn encode<T: Serialize>(value: &T, destination: &mut [u8]) -> Result<usize, CodecError> {
    serde_json_core::to_slice(value, destination).map_err(|_| CodecError::BufferTooSmall)
}

pub fn decode<'a, T: Deserialize<'a>>(payload: &'a [u8]) -> Result<T, CodecError> {
    let (value, used) =
        serde_json_core::from_slice(payload).map_err(|_| CodecError::InvalidJson)?;
    if used == payload.len() {
        Ok(value)
    } else {
        Err(CodecError::TrailingData)
    }
}

pub fn encode_line<T: Serialize>(value: &T, destination: &mut [u8]) -> Result<usize, CodecError> {
    let used = encode(value, destination)?;
    let end = destination
        .get_mut(used)
        .ok_or(CodecError::BufferTooSmall)?;
    *end = b'\n';
    Ok(used + 1)
}

pub fn decode_line<'a, T: Deserialize<'a>>(line: &'a [u8]) -> Result<T, CodecError> {
    if line.last() != Some(&b'\n') {
        return Err(CodecError::MissingLineFeed);
    }
    decode(&line[..line.len() - 1])
}
