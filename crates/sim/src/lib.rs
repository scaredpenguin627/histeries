//! Deterministic world-simulation primitives.

#![forbid(unsafe_code)]

/// The versioned algorithm identifier for per-turn state hashes.
pub const STATE_HASH_ALGORITHM: &str = "fnv1a64-v1-little-endian-length-prefixed";

/// A deterministic hash of canonical simulation state bytes.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct StateHash(u64);

impl StateHash {
    /// Returns the numeric hash value.
    pub const fn value(self) -> u64 {
        self.0
    }
}

/// Incrementally hashes canonical state fields in insertion order.
#[derive(Clone, Debug)]
pub struct StateHasher {
    state: u64,
}

impl StateHasher {
    /// Creates a hasher with the FNV-1a offset basis.
    pub const fn new() -> Self {
        Self {
            state: 0xcbf29ce484222325,
        }
    }

    /// Adds a length-prefixed byte field.
    pub fn write_bytes(&mut self, bytes: &[u8]) {
        self.write_u64(bytes.len() as u64);
        for byte in bytes {
            self.state ^= u64::from(*byte);
            self.state = self.state.wrapping_mul(0x100000001b3);
        }
    }

    /// Adds a little-endian unsigned integer field.
    pub fn write_u64(&mut self, value: u64) {
        self.write_raw(&value.to_le_bytes());
    }

    /// Adds a little-endian signed integer field.
    pub fn write_i64(&mut self, value: i64) {
        self.write_raw(&value.to_le_bytes());
    }

    /// Finishes the hash without changing the hasher.
    pub const fn finish(&self) -> StateHash {
        StateHash(self.state)
    }

    fn write_raw(&mut self, bytes: &[u8]) {
        for byte in bytes {
            self.state ^= u64::from(*byte);
            self.state = self.state.wrapping_mul(0x100000001b3);
        }
    }
}

impl Default for StateHasher {
    fn default() -> Self {
        Self::new()
    }
}

/// Hashes one canonical byte field.
pub fn hash_bytes(bytes: &[u8]) -> StateHash {
    let mut hasher = StateHasher::new();
    hasher.write_bytes(bytes);
    hasher.finish()
}

#[cfg(test)]
mod tests {
    use super::{STATE_HASH_ALGORITHM, StateHasher, hash_bytes};

    #[test]
    fn equal_canonical_fields_have_equal_hashes() {
        assert_eq!(hash_bytes(b"state"), hash_bytes(b"state"));
        assert_ne!(hash_bytes(b"state"), hash_bytes(b"State"));
        assert!(STATE_HASH_ALGORITHM.contains("little-endian"));
    }

    #[test]
    fn field_boundaries_and_order_are_observable() {
        let mut first = StateHasher::new();
        first.write_bytes(b"ab");
        first.write_bytes(b"c");
        let mut second = StateHasher::new();
        second.write_bytes(b"a");
        second.write_bytes(b"bc");
        assert_ne!(first.finish(), second.finish());

        let mut numbers = StateHasher::new();
        numbers.write_u64(7);
        numbers.write_i64(-3);
        assert_ne!(numbers.finish().value(), 0);
    }
}
