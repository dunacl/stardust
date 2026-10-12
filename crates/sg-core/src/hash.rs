//! Stable fingerprints for parameters and snapshots.

/// 64-bit FNV-1a (Fowler, Noll & Vo; offset basis `0xcbf29ce484222325`,
/// prime `0x100000001b3`). Not cryptographic: it identifies a parameter set or
/// a snapshot, and makes determinism tests cheap to compare across platforms.
pub fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in bytes {
        hash ^= u64::from(b);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// [`fnv1a64`] as 16 lowercase hex digits.
pub fn fnv1a64_hex(bytes: &[u8]) -> String {
    format!("{:016x}", fnv1a64(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Published FNV-1a 64-bit test vectors.
    #[test]
    fn matches_reference_vectors() {
        assert_eq!(fnv1a64(b""), 0xcbf2_9ce4_8422_2325);
        assert_eq!(fnv1a64(b"a"), 0xaf63_dc4c_8601_ec8c);
        assert_eq!(fnv1a64(b"foobar"), 0x8594_4171_f739_67e8);
        assert_eq!(fnv1a64_hex(b"a"), "af63dc4c8601ec8c");
    }
}
