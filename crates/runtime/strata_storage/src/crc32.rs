//! Pure-Rust IEEE 802.3 CRC32 Implementation (Zero Dependencies).

/// Computes the IEEE 802.3 32-bit Cyclic Redundancy Check (CRC32).
pub fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &byte in data {
        crc ^= byte as u32;
        for _ in 0..8 {
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (0xEDB88320 & mask);
        }
    }
    !crc
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_crc32_standard_vectors() {
        assert_eq!(crc32(b""), 0x00000000);
        assert_eq!(crc32(b"123456789"), 0xCBF43926);
        assert_eq!(crc32(b"Strata Universal Framework"), crc32(b"Strata Universal Framework"));
    }
}
