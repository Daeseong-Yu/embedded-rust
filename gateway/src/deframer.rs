//! Splits a serial byte stream into frames.
use protocol::{Error, Frame, MAX_FRAME_LEN};

const DELIMITER: u8 = 0x00;

/// Longest run of bytes between two delimiters.
const MAX_BODY_LEN: usize = MAX_FRAME_LEN - 1;

/// Collects bytes until a delimiter arrives, then decodes them.
pub struct Deframer {
    buf: [u8; MAX_BODY_LEN],
    len: usize,
    /// False until the first delimiter. The port usually opens in the
    /// middle of a frame, so those bytes are dropped.
    synced: bool,
    /// Set when more bytes arrived than a frame can hold.
    overflow: bool,
}

impl Deframer {
    pub fn new() -> Self {
        Self {
            buf: [0; MAX_BODY_LEN],
            len: 0,
            synced: false,
            overflow: false,
        }
    }

    /// Feeds one byte. Returns a result when the byte ends a frame.
    pub fn push(&mut self, byte: u8) -> Option<Result<Frame, Error>> {
        if byte != DELIMITER {
            if !self.synced {
                return None;
            }

            match self.buf.get_mut(self.len) {
                Some(slot) => {
                    *slot = byte;
                    self.len += 1;
                }

                None => self.overflow = true,
            }

            return None;
        }

        let result = if !self.synced {
            None
        } else if self.overflow {
            Some(Err(Error::Length))
        } else if self.len == 0 {
            // Two delimiters in a row carry no frame.
            None
        } else {
            Some(protocol::decode(&self.buf[..self.len]))
        };

        self.synced = true;
        self.len = 0;
        self.overflow = false;
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use protocol::Message;

    const FRAME: Frame = Frame {
        seq: 7,
        msg: Message::Accel {
            x: 1,
            y: -1,
            z: 1000,
        },
    };

    /// Encodes `FRAME`, delimiter included.
    fn encoded() -> Vec<u8> {
        let mut buf = [0u8; MAX_FRAME_LEN];
        let n = protocol::encode(&FRAME, &mut buf).unwrap();
        buf[..n].to_vec()
    }

    /// Feeds every byte and collects what comes out.
    fn feed(deframer: &mut Deframer, bytes: &[u8]) -> Vec<Result<Frame, Error>> {
        bytes.iter().filter_map(|&b| deframer.push(b)).collect()
    }

    #[test]
    fn bytes_before_first_delimiter_are_dropped() {
        let mut deframer = Deframer::new();
        let bytes = encoded();

        // Start in the middle of a frame.
        assert_eq!(feed(&mut deframer, &bytes[3..]), []);
        assert_eq!(feed(&mut deframer, &bytes), [Ok(FRAME)]);
    }

    #[test]
    fn back_to_back_frames_are_all_decoded() {
        let mut deframer = Deframer::new();
        let mut bytes = vec![DELIMITER];

        bytes.extend(encoded());
        bytes.extend(encoded());

        assert_eq!(feed(&mut deframer, &bytes), [Ok(FRAME), Ok(FRAME)]);
    }

    #[test]
    fn repeated_delimiters_yield_nothing() {
        let mut deframer = Deframer::new();

        assert_eq!(feed(&mut deframer, &[DELIMITER; 4]), []);
    }

    #[test]
    fn corrupted_frame_is_rejected_and_next_one_survives() {
        let mut deframer = Deframer::new();
        let mut bad = encoded();

        // Flip one bit of a data byte (not a COBS code byte).
        bad[2] ^= 0x10;

        let mut bytes = vec![DELIMITER];
        bytes.extend(bad);
        bytes.extend(encoded());

        assert_eq!(feed(&mut deframer, &bytes), [Err(Error::Crc), Ok(FRAME)]);
    }

    #[test]
    fn oversized_frame_is_rejected_and_next_one_survives() {
        let mut deframer = Deframer::new();
        let mut bytes = vec![DELIMITER];

        bytes.extend([0x55; MAX_FRAME_LEN + 10]);
        bytes.push(DELIMITER);
        bytes.extend(encoded());

        assert_eq!(feed(&mut deframer, &bytes), [Err(Error::Length), Ok(FRAME)]);
    }
}
