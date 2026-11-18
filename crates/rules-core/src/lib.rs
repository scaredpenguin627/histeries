//! Pure deterministic rules primitives.

#![forbid(unsafe_code)]

use rand_chacha::{
    ChaCha8Rng,
    rand_core::{RngCore, SeedableRng},
};

/// The fixed-point scale used by rules calculations.
pub const FIXED_SCALE: i64 = 1_000;

/// A signed fixed-point value with three decimal places.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Fixed(i64);

impl Fixed {
    /// The additive identity.
    pub const ZERO: Self = Self(0);
    /// The multiplicative identity.
    pub const ONE: Self = Self(FIXED_SCALE);

    /// Creates a fixed-point value from its raw scaled representation.
    pub const fn from_raw(raw: i64) -> Self {
        Self(raw)
    }

    /// Creates a fixed-point value from an integer.
    pub const fn from_int(value: i64) -> Option<Self> {
        match value.checked_mul(FIXED_SCALE) {
            Some(raw) => Some(Self(raw)),
            None => None,
        }
    }

    /// Returns the raw scaled representation.
    pub const fn raw(self) -> i64 {
        self.0
    }

    /// Adds two values with overflow checking.
    pub const fn checked_add(self, other: Self) -> Option<Self> {
        match self.0.checked_add(other.0) {
            Some(raw) => Some(Self(raw)),
            None => None,
        }
    }

    /// Subtracts two values with overflow checking.
    pub const fn checked_sub(self, other: Self) -> Option<Self> {
        match self.0.checked_sub(other.0) {
            Some(raw) => Some(Self(raw)),
            None => None,
        }
    }

    /// Multiplies two values and rescales the result with overflow checking.
    pub const fn checked_mul(self, other: Self) -> Option<Self> {
        let product = (self.0 as i128) * (other.0 as i128);
        let scaled = product / FIXED_SCALE as i128;
        if scaled < i64::MIN as i128 || scaled > i64::MAX as i128 {
            None
        } else {
            Some(Self(scaled as i64))
        }
    }

    /// Divides two values and rescales the result with overflow checking.
    pub const fn checked_div(self, other: Self) -> Option<Self> {
        if other.0 == 0 {
            return None;
        }
        let numerator = (self.0 as i128) * FIXED_SCALE as i128;
        let quotient = numerator / other.0 as i128;
        if quotient < i64::MIN as i128 || quotient > i64::MAX as i128 {
            None
        } else {
            Some(Self(quotient as i64))
        }
    }
}

/// The algorithm identifier stored with named random streams.
pub const RNG_ALGORITHM: &str = "chacha8-v0.9.0-named-fnv1a-seed-v1";

/// A reproducible random stream derived from a master seed and a stable name.
#[derive(Clone, Debug)]
pub struct NamedRng {
    name: String,
    rng: ChaCha8Rng,
}

impl NamedRng {
    /// Creates a stream from a 32-byte master seed and a stable stream name.
    pub fn new(master_seed: [u8; 32], name: &str) -> Self {
        Self {
            name: name.to_owned(),
            rng: ChaCha8Rng::from_seed(derive_seed(master_seed, name)),
        }
    }

    /// Returns the algorithm identifier for save-format metadata.
    pub const fn algorithm() -> &'static str {
        RNG_ALGORITHM
    }

    /// Returns the stable stream name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the next 64-bit value from this stream.
    pub fn next_u64(&mut self) -> u64 {
        self.rng.next_u64()
    }

    /// Returns an unbiased value in `0..upper`, or `None` when `upper` is zero.
    pub fn next_bounded(&mut self, upper: u64) -> Option<u64> {
        if upper == 0 {
            return None;
        }
        let threshold = upper.wrapping_neg() % upper;
        loop {
            let value = self.next_u64();
            if value >= threshold {
                return Some(value % upper);
            }
        }
    }
}

fn derive_seed(master_seed: [u8; 32], name: &str) -> [u8; 32] {
    let mut seed = [0_u8; 32];
    for lane in 0..4_u64 {
        let mut hash = 0xcbf29ce484222325_u64 ^ lane;
        for byte in master_seed.iter().copied().chain(name.bytes()) {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x100000001b3);
        }
        seed[(lane as usize) * 8..(lane as usize + 1) * 8].copy_from_slice(&hash.to_le_bytes());
    }
    seed
}

#[cfg(test)]
mod tests {
    use super::{FIXED_SCALE, Fixed, NamedRng, RNG_ALGORITHM};

    #[test]
    fn fixed_point_arithmetic_is_scaled_and_checked() {
        let two = Fixed::from_int(2).unwrap();
        let half = Fixed::from_raw(FIXED_SCALE / 2);
        assert_eq!(two.checked_mul(half).unwrap().raw(), FIXED_SCALE);
        assert_eq!(two.checked_div(half).unwrap().raw(), 4 * FIXED_SCALE);
        assert_eq!(two.checked_div(Fixed::ZERO), None);
        assert_eq!(Fixed::from_int(i64::MAX), None);
    }

    #[test]
    fn named_streams_are_reproducible_and_independent() {
        let seed = [7_u8; 32];
        let mut first = NamedRng::new(seed, "mapgen");
        let mut second = NamedRng::new(seed, "mapgen");
        let mut other = NamedRng::new(seed, "combat");
        assert_eq!(NamedRng::algorithm(), RNG_ALGORITHM);
        assert_eq!(first.name(), "mapgen");
        assert_eq!(first.next_u64(), second.next_u64());
        assert_ne!(first.next_u64(), other.next_u64());
    }

    #[test]
    fn bounded_values_stay_in_range() {
        let mut rng = NamedRng::new([1_u8; 32], "player-1");
        assert_eq!(rng.next_bounded(0), None);
        for _ in 0..128 {
            assert!(rng.next_bounded(7).unwrap() < 7);
        }
    }
}
