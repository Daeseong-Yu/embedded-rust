#![no_std]
pub const VERSION: u8 = 1;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_is_one() {
        assert_eq!(VERSION, 1);
    }
}
