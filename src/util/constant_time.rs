//! Constant-time comparison utilities.
//!
//! Webhook-signature comparison is security-critical, so it uses the audited
//! [`subtle`] crate's `ConstantTimeEq`. An earlier in-house XOR/`black_box`
//! fold was replaced: `std::hint::black_box` is *not* a constant-time guarantee
//! and the compiler is free to short-circuit the comparison.

use subtle::ConstantTimeEq;

/// Compare two byte slices in constant time.
///
/// Returns `true` if and only if the slices have the same length and identical
/// contents. The comparison takes the same amount of time regardless of where
/// the first difference occurs. Slices of differing length return `false`
/// (lengths of HMAC tags are public, so leaking length is acceptable).
pub fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    bool::from(a.ct_eq(b))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn equal_slices_match() {
        assert!(constant_time_eq(b"hello", b"hello"));
    }

    #[test]
    fn different_slices_do_not_match() {
        assert!(!constant_time_eq(b"hello", b"world"));
        assert!(!constant_time_eq(b"hello", b"hallo"));
    }

    #[test]
    fn different_lengths_do_not_match() {
        assert!(!constant_time_eq(b"hi", b"hello"));
    }

    #[test]
    fn empty_slices_match() {
        assert!(constant_time_eq(b"", b""));
    }
}
