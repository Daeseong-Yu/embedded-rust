//! Frame layout before COBS encoding (multi-byte fields are little-endian):
//!
//! | version: 1 | msg_type: 1 | seq: 2 | payload: N | crc: 2 |
//!
//! The CRC covers everything before it. The whole frame is COBS-encoded and
//! followed by a single 0x00 delimiter.

use crate::{Error, VERSION, cobs, crc::crc16};

const HEADER_LEN: usize = 4;
const CRC_LEN: usize = 2;
const MAX_PAYLOAD_LEN: usize = 12;
const MAX_RAW_LEN: usize = HEADER_LEN + MAX_PAYLOAD_LEN + CRC_LEN;

/// Largest encoded frame, including the 0x00 delimiter.
pub const MAX_FRAME_LEN: usize = cobs::max_encoded_len(MAX_RAW_LEN) + 1;

const TYPE_ACCEL: u8 = 1;
const TYPE_MAG: u8 = 2;
const TYPE_GYRO: u8 = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Message {
    /// Acceleration in mg.
    Accel { x: i16, y: i16, z: i16 },
    /// Magnetic field in mgauss.
    Mag { x: i32, y: i32, z: i32 },
    /// Angular rate in mdps.
    Gyro { x: i32, y: i32, z: i32 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Frame {
    pub seq: u16,
    pub msg: Message,
}

/// Encodes `frame` into `dst`, including the trailing 0x00 delimiter, and
/// returns the number of bytes written.
pub fn encode(frame: &Frame, dst: &mut [u8]) -> Result<usize, Error> {
    let mut raw = [0u8; MAX_RAW_LEN];

    raw[0] = VERSION;
    raw[2..4].copy_from_slice(&frame.seq.to_le_bytes());

    let payload = &mut raw[HEADER_LEN..];
    let (msg_type, payload_len) = match frame.msg {
        Message::Accel { x, y, z } => {
            payload[0..2].copy_from_slice(&x.to_le_bytes());
            payload[2..4].copy_from_slice(&y.to_le_bytes());
            payload[4..6].copy_from_slice(&z.to_le_bytes());
            (TYPE_ACCEL, 6)
        }
        Message::Mag { x, y, z } => {
            write_xyz(payload, x, y, z);
            (TYPE_MAG, 12)
        }
        Message::Gyro { x, y, z } => {
            write_xyz(payload, x, y, z);
            (TYPE_GYRO, 12)
        }
    };
    raw[1] = msg_type;

    let crc_at = HEADER_LEN + payload_len;
    let crc = crc16(&raw[..crc_at]);

    raw[crc_at..crc_at + CRC_LEN].copy_from_slice(&crc.to_le_bytes());

    let raw = &raw[..crc_at + CRC_LEN];

    if dst.len() < cobs::max_encoded_len(raw.len()) + 1 {
        return Err(Error::BufferTooSmall);
    }

    let n = cobs::encode(raw, dst)?;
    dst[n] = 0x00;

    Ok(n + 1)
}

/// Decodes one frame. `src` is the bytes between two 0x00 delimiters without
/// the delimiter itself.
pub fn decode(src: &[u8]) -> Result<Frame, Error> {
    let mut raw = [0u8; MAX_RAW_LEN];
    let n = cobs::decode(src, &mut raw).map_err(|e| match e {
        Error::BufferTooSmall => Error::Length,
        other => other,
    })?;

    if n < HEADER_LEN + CRC_LEN {
        return Err(Error::Length);
    }

    let (body, crc_bytes) = raw[..n].split_at(n - CRC_LEN);

    if crc16(body) != u16::from_le_bytes([crc_bytes[0], crc_bytes[1]]) {
        return Err(Error::Crc);
    }

    if body[0] != VERSION {
        return Err(Error::Version(body[0]));
    }

    let seq = u16::from_le_bytes([body[2], body[3]]);
    let payload = &body[HEADER_LEN..];

    let msg = match body[1] {
        TYPE_ACCEL => {
            if payload.len() != 6 {
                return Err(Error::Length);
            }

            Message::Accel {
                x: i16::from_le_bytes([payload[0], payload[1]]),
                y: i16::from_le_bytes([payload[2], payload[3]]),
                z: i16::from_le_bytes([payload[4], payload[5]]),
            }
        }
        TYPE_MAG => {
            let (x, y, z) = read_xyz(payload)?;
            Message::Mag { x, y, z }
        }
        TYPE_GYRO => {
            let (x, y, z) = read_xyz(payload)?;
            Message::Gyro { x, y, z }
        }
        other => return Err(Error::MsgType(other)),
    };

    Ok(Frame { seq, msg })
}

fn write_xyz(payload: &mut [u8], x: i32, y: i32, z: i32) {
    payload[0..4].copy_from_slice(&x.to_le_bytes());
    payload[4..8].copy_from_slice(&y.to_le_bytes());
    payload[8..12].copy_from_slice(&z.to_le_bytes());
}

fn read_xyz(payload: &[u8]) -> Result<(i32, i32, i32), Error> {
    if payload.len() != 12 {
        return Err(Error::Length);
    }

    let at =
        |i: usize| i32::from_le_bytes([payload[i], payload[i + 1], payload[i + 2], payload[i + 3]]);

    Ok((at(0), at(4), at(8)))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Encodes and strips the delimiter, ready to pass to `decode`.
    fn encoded(frame: &Frame, buf: &mut [u8; MAX_FRAME_LEN]) -> usize {
        let n = encode(frame, buf).unwrap();
        assert_eq!(buf[n - 1], 0x00);
        assert!(!buf[..n - 1].contains(&0));
        n - 1
    }

    /// Builds a frame from raw header and payload bytes with a valid CRC, so
    /// tests can produce frames that `encode` never would.
    fn build(body: &[u8], buf: &mut [u8; 64]) -> usize {
        let mut raw = [0u8; 40];
        raw[..body.len()].copy_from_slice(body);
        let crc = crc16(body).to_le_bytes();
        raw[body.len()..body.len() + 2].copy_from_slice(&crc);
        cobs::encode(&raw[..body.len() + 2], buf).unwrap()
    }

    #[test]
    fn round_trip_all_types() {
        let frames = [
            Frame {
                seq: 0,
                msg: Message::Accel {
                    x: 12,
                    y: -34,
                    z: 1000,
                },
            },
            Frame {
                seq: 1,
                msg: Message::Mag {
                    x: -150,
                    y: 300,
                    z: -450,
                },
            },
            Frame {
                seq: u16::MAX,
                msg: Message::Gyro {
                    x: i32::MIN,
                    y: 0,
                    z: i32::MAX,
                },
            },
        ];
        for frame in frames {
            let mut buf = [0u8; MAX_FRAME_LEN];
            let n = encoded(&frame, &mut buf);
            assert_eq!(decode(&buf[..n]), Ok(frame));
        }
    }

    #[test]
    fn round_trip_payload_full_of_zeros() {
        let frame = Frame {
            seq: 0,
            msg: Message::Gyro { x: 0, y: 0, z: 0 },
        };
        let mut buf = [0u8; MAX_FRAME_LEN];
        let n = encoded(&frame, &mut buf);
        assert_eq!(decode(&buf[..n]), Ok(frame));
    }

    #[test]
    fn golden_vector() {
        let frame = Frame {
            seq: 0x0102,
            msg: Message::Accel {
                x: 1,
                y: -1,
                z: 1000,
            },
        };
        let mut buf = [0u8; MAX_FRAME_LEN];
        let n = encode(&frame, &mut buf).unwrap();
        assert_eq!(
            &buf[..n],
            &[
                0x06, 0x01, 0x01, 0x02, 0x01, 0x01, 0x07, 0xFF, 0xFF, 0xE8, 0x03, 0x80, 0x3A, 0x00
            ]
        );
    }

    #[test]
    fn encode_rejects_small_buffer() {
        let frame = Frame {
            seq: 0,
            msg: Message::Accel { x: 1, y: 2, z: 3 },
        };
        let mut buf = [0u8; 13];
        assert_eq!(encode(&frame, &mut buf), Err(Error::BufferTooSmall));
    }

    #[test]
    fn decode_rejects_bad_crc() {
        let mut body = [VERSION, TYPE_ACCEL, 0, 0, 1, 0, 2, 0, 3, 0, 0, 0];
        let crc = crc16(&body[..10]) ^ 0x0001;
        body[10..].copy_from_slice(&crc.to_le_bytes());
        let mut buf = [0u8; 64];
        let n = cobs::encode(&body, &mut buf).unwrap();
        assert_eq!(decode(&buf[..n]), Err(Error::Crc));
    }

    #[test]
    fn decode_rejects_truncated() {
        let frame = Frame {
            seq: 7,
            msg: Message::Mag { x: 1, y: 2, z: 3 },
        };
        let mut buf = [0u8; MAX_FRAME_LEN];
        let n = encoded(&frame, &mut buf);
        assert!(decode(&buf[..n - 3]).is_err());
        assert_eq!(decode(&[]), Err(Error::Length));
        assert_eq!(decode(&[0x02, 0x01]), Err(Error::Length));
    }

    #[test]
    fn decode_rejects_too_long() {
        let mut buf = [0u8; 64];
        let n = build(
            &[
                VERSION, TYPE_GYRO, 0, 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13,
            ],
            &mut buf,
        );
        assert_eq!(decode(&buf[..n]), Err(Error::Length));
    }

    #[test]
    fn decode_rejects_unknown_version() {
        let mut buf = [0u8; 64];
        let n = build(&[2, TYPE_ACCEL, 0, 0, 1, 0, 2, 0, 3, 0], &mut buf);
        assert_eq!(decode(&buf[..n]), Err(Error::Version(2)));
    }

    #[test]
    fn decode_rejects_unknown_type() {
        let mut buf = [0u8; 64];
        let n = build(&[VERSION, 99, 0, 0, 1, 0, 2, 0, 3, 0], &mut buf);
        assert_eq!(decode(&buf[..n]), Err(Error::MsgType(99)));
    }

    #[test]
    fn decode_rejects_wrong_payload_length() {
        let mut buf = [0u8; 64];
        let n = build(&[VERSION, TYPE_ACCEL, 0, 0, 1, 0, 2, 0], &mut buf);
        assert_eq!(decode(&buf[..n]), Err(Error::Length));
        let n = build(&[VERSION, TYPE_MAG, 0, 0, 1, 0, 2, 0, 3, 0], &mut buf);
        assert_eq!(decode(&buf[..n]), Err(Error::Length));
    }
}
