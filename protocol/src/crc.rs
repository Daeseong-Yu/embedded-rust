//! CRC-16/CCITT-FALSE: poly 0x1021, init 0xFFFF, no reflection, no final XOR.

const POLY: u16 = 0x1021;
const INIT: u16 = 0xFFFF;

pub fn crc16(data: &[u8]) -> u16 {
    let mut crc = INIT;
    for &byte in data {
        crc ^= (byte as u16) << 8;
        for _ in 0..8 {
            crc = if crc & 0x8000 != 0 {
                (crc << 1) ^ POLY
            } else {
                crc << 1
            };
        }
    }
    crc
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_value() {
        assert_eq!(crc16(b"123456789"), 0x29B1);
    }

    #[test]
    fn empty_is_init() {
        assert_eq!(crc16(&[]), 0xFFFF);
    }

    #[test]
    fn one_bit_flip_changes_crc() {
        assert_ne!(crc16(&[0x01, 0x02, 0x03]), crc16(&[0x01, 0x02, 0x07]));
    }
}
