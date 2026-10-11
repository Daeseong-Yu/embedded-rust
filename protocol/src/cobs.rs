//! COBS (Consistent Overhead Byte Stuffing): removes every 0x00 from the data
//! so that 0x00 can be used as the frame delimiter.

use crate::Error;

/// Worst-case encoded size for `len` input bytes (delimiter not included).
pub const fn max_encoded_len(len: usize) -> usize {
    len + len / 254 + 1
}

/// Encodes `src` into `dst` and returns the number of bytes written.
/// The trailing 0x00 delimiter is not added here.
pub fn encode(src: &[u8], dst: &mut [u8]) -> Result<usize, Error> {
    if dst.len() < max_encoded_len(src.len()) {
        return Err(Error::BufferTooSmall);
    }

    let mut code_idx = 0;
    let mut out = 1;
    let mut code: u8 = 1;

    for &byte in src {
        if byte == 0 {
            dst[code_idx] = code;
            code_idx = out;
            out += 1;
            code = 1;
        } else {
            dst[out] = byte;
            out += 1;
            code += 1;
            if code == 0xFF {
                dst[code_idx] = code;
                code_idx = out;
                out += 1;
                code = 1;
            }
        }
    }
    dst[code_idx] = code;

    Ok(out)
}

/// Decodes `src` (without the 0x00 delimiter) into `dst` and returns
/// number of bytes written.
pub fn decode(src: &[u8], dst: &mut [u8]) -> Result<usize, Error> {
    let mut i = 0;
    let mut out = 0;

    while i < src.len() {
        let code = src[i];

        if code == 0 {
            return Err(Error::Cobs);
        }

        i += 1;

        let n = code as usize - 1;
        let block = src.get(i..i + n).ok_or(Error::Cobs)?;

        if block.contains(&0) {
            return Err(Error::Cobs);
        }

        dst.get_mut(out..out + n)
            .ok_or(Error::BufferTooSmall)?
            .copy_from_slice(block);

        i += n;
        out += n;

        if code != 0xFF && i < src.len() {
            *dst.get_mut(out).ok_or(Error::BufferTooSmall)? = 0;
            out += 1;
        }
    }

    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn check(raw: &[u8], encoded: &[u8]) {
        let mut buf = [0u8; 600];
        let n = encode(raw, &mut buf).unwrap();

        assert_eq!(&buf[..n], encoded);

        let mut back = [0u8; 600];
        let m = decode(encoded, &mut back).unwrap();

        assert_eq!(&back[..m], raw);
    }

    #[test]
    fn known_vectors() {
        check(&[], &[0x01]);
        check(&[0x00], &[0x01, 0x01]);
        check(&[0x00, 0x00], &[0x01, 0x01, 0x01]);
        check(&[0x11, 0x22, 0x00, 0x33], &[0x03, 0x11, 0x22, 0x02, 0x33]);
        check(&[0x11, 0x22, 0x33, 0x44], &[0x05, 0x11, 0x22, 0x33, 0x44]);
        check(&[0x11, 0x00, 0x00, 0x00], &[0x02, 0x11, 0x01, 0x01, 0x01]);
    }

    #[test]
    fn encoded_never_contains_zero() {
        let raw = [0x00, 0x01, 0x00, 0x00, 0xFF, 0x00];
        let mut buf = [0u8; 16];
        let n = encode(&raw, &mut buf).unwrap();

        assert!(!buf[..n].contains(&0));
    }

    #[test]
    fn round_trip_long_runs() {
        let mut raw = [0u8; 600];

        for len in [253, 254, 255, 508, 600] {
            for (i, b) in raw[..len].iter_mut().enumerate() {
                *b = (i % 255) as u8 + 1;
            }

            let mut enc = [0u8; 700];
            let n = encode(&raw[..len], &mut enc).unwrap();

            assert!(!enc[..n].contains(&0));
            assert!(n <= max_encoded_len(len));

            let mut back = [0u8; 700];
            let m = decode(&enc[..n], &mut back).unwrap();

            assert_eq!(&back[..m], &raw[..len]);
        }
    }

    #[test]
    fn encode_rejects_small_buffer() {
        let mut buf = [0u8; 4];

        assert_eq!(encode(&[1, 2, 3, 4], &mut buf), Err(Error::BufferTooSmall));
    }

    #[test]
    fn decode_rejects_zero_inside() {
        let mut buf = [0u8; 16];

        assert_eq!(decode(&[0x03, 0x11, 0x00], &mut buf), Err(Error::Cobs));
        assert_eq!(decode(&[0x00, 0x11], &mut buf), Err(Error::Cobs));
    }

    #[test]
    fn decode_rejects_truncated() {
        let mut buf = [0u8; 16];

        assert_eq!(decode(&[0x05, 0x11, 0x22], &mut buf), Err(Error::Cobs));
    }

    #[test]
    fn decode_rejects_small_buffer() {
        let mut buf = [0u8; 2];

        assert_eq!(
            decode(&[0x05, 0x11, 0x22, 0x33, 0x44], &mut buf),
            Err(Error::BufferTooSmall)
        );
    }
}
