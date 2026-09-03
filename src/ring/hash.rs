//! Hashing utilities for the consistent hash ring.

use std::hash::Hasher;
use twox_hash::{XxHash3_128, XxHash64};

/// A hash function: arbitrary bytes in, a ring position out.
///
/// A plain function pointer (rather than a closure or trait object) keeps it
/// `Copy` + `Send` + `Sync` with zero allocation. The `H` parameter is hash result.
pub type HashFn<H> = fn(&[u8]) -> H;

/// Hashes `key` with the xxHash64 algorithm, producing a 64-bit ring position.
///
/// xxHash is non-cryptographic and extremely fast, which is exactly what
/// consistent hashing wants: speed and good dispersion across the ring, not
/// collision resistance against an adversary.
///
/// Prefer [`hash128`] for production rings — on a `u64` ring, two distinct
/// virtual points colliding silently overwrites one another.
pub fn hash64(key: &[u8]) -> u64 {
    let mut hasher = XxHash64::with_seed(0);
    hasher.write(key);
    hasher.finish()
}

/// Hashes `key` with the 128-bit xxHash3 algorithm, producing a `u128` ring
/// position.
///
/// The wider position space makes virtual-point collisions vanishingly rare,
/// so it is the recommended hash for [`crate::ring::ConsistentHashRing`].
pub fn hash128(key: &[u8]) -> u128 {
    XxHash3_128::oneshot(key)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash64_is_deterministic() {
        assert_eq!(hash64(b"hello"), hash64(b"hello"));
        assert_ne!(hash64(b"hello"), hash64(b"world"));
    }

    #[test]
    fn hash64_empty_input_is_defined() {
        assert_eq!(hash64(b""), hash64(b""));
    }

    #[test]
    fn hash128_is_deterministic() {
        assert_eq!(hash128(b"hello"), hash128(b"hello"));
        assert_ne!(hash128(b"hello"), hash128(b"world"));
    }

    #[test]
    fn hash128_empty_input_is_defined() {
        assert_eq!(hash128(b""), hash128(b""));
    }
}
