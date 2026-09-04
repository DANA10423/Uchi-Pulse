use heapless::String;

/// Maximum encoded length of a device identifier held by the common model.
pub const DEVICE_ID_CAPACITY: usize = 64;
/// Maximum encoded length of an event identifier held by the common model.
pub const EVENT_ID_CAPACITY: usize = 128;
/// Maximum encoded length of a CDC request identifier held by the common model.
pub const REQUEST_ID_CAPACITY: usize = 64;
/// Maximum encoded length of a CDC command name held by the common model.
pub const COMMAND_NAME_CAPACITY: usize = 32;
pub const ACTION_NAME_CAPACITY: usize = 96;
pub const MESSAGE_CAPACITY: usize = 256;

pub type DeviceId = String<DEVICE_ID_CAPACITY>;
pub type EventId = String<EVENT_ID_CAPACITY>;
pub type RequestId = String<REQUEST_ID_CAPACITY>;
pub type CommandName = String<COMMAND_NAME_CAPACITY>;
pub type ActionId = u32;
pub type FamilyId = u32;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TextError {
    TooLong,
}

pub fn text<const N: usize>(value: &str) -> Result<String<N>, TextError> {
    let mut result = String::new();
    result.push_str(value).map_err(|_| TextError::TooLong)?;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identifiers_are_owned_and_bounded() {
        let device_id: DeviceId = text("node-01").unwrap();
        let event_id: EventId = text("boot-id-00000001").unwrap();
        assert_eq!(device_id.as_str(), "node-01");
        assert_eq!(event_id.as_str(), "boot-id-00000001");

        let too_long = "x".repeat(DEVICE_ID_CAPACITY + 1);
        assert_eq!(
            text::<DEVICE_ID_CAPACITY>(&too_long),
            Err(TextError::TooLong)
        );
    }
}
