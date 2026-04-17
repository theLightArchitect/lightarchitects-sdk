//! Constant-time comparison utilities — prevents timing side-channels.
//!
//! Delegates to the [`subtle`] crate which uses compiler barriers and
//! volatile reads to survive LLVM optimizations including LTO.

use subtle::ConstantTimeEq;

/// Constant-time equality comparison for byte slices.
///
/// Returns `true` if `a` and `b` have the same length and identical content.
/// Uses [`subtle::ConstantTimeEq`] to guarantee constant-time execution
/// even under aggressive compiler optimizations (LTO, dead-store elimination).
///
/// The length comparison itself is not constant-time, but this is
/// acceptable because HMAC outputs and key hashes are always fixed-length
/// (the length carries no secret information).
///
/// # Examples
///
/// ```
/// use lightarchitects_crypto::compare::constant_time_eq;
///
/// assert!(constant_time_eq(b"secret", b"secret"));
/// assert!(!constant_time_eq(b"secret", b"tamper"));
/// ```
#[must_use]
pub fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.ct_eq(b).into()
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn test_equal_slices() {
        assert!(constant_time_eq(b"hello", b"hello"));
    }

    #[test]
    fn test_different_slices() {
        assert!(!constant_time_eq(b"hello", b"world"));
    }

    #[test]
    fn test_different_lengths() {
        assert!(!constant_time_eq(b"hello", b"hi"));
    }

    #[test]
    fn test_empty_slices() {
        assert!(constant_time_eq(b"", b""));
    }

    #[test]
    fn test_single_bit_difference() {
        let a = [0b1111_1111u8];
        let b = [0b1111_1110u8];
        assert!(!constant_time_eq(&a, &b));
    }

    #[test]
    fn test_all_zeros_vs_all_ones() {
        let a = [0u8; 32];
        let b = [0xffu8; 32];
        assert!(!constant_time_eq(&a, &b));
    }

    #[test]
    fn test_identical_32_byte_keys() {
        let key = [0xab_u8; 32];
        assert!(constant_time_eq(&key, &key));
    }
}
