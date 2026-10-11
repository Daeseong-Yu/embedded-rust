#![no_std]
pub mod cobs;
pub mod crc;
pub mod frame;

pub use frame::{Frame, MAX_FRAME_LEN, Message, decode, encode};

pub const VERSION: u8 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    /// The output buffer is too small.
    BufferTooSmall,
    /// The input is not valid COBS data
    Cobs,
    Crc,
    Version(u8),
    MsgType(u8),
    Length,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_is_one() {
        assert_eq!(VERSION, 1);
    }
}
