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

macro_rules! typed_id {
    ($name:ident) => {
        /// A stable generational-arena key for a rules entity.
        #[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
        pub struct $name(u32);

        impl $name {
            /// Creates an ID from a stable arena index.
            pub const fn new(index: u32) -> Self {
                Self(index)
            }

            /// Returns the stable arena index.
            pub const fn index(self) -> u32 {
                self.0
            }
        }
    };
}

typed_id!(PlayerId);
typed_id!(UnitId);
typed_id!(CityId);
typed_id!(TechnologyId);

/// The yield channels understood by the pure rules layer.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[repr(usize)]
pub enum YieldKind {
    /// Food available for growth and support.
    Food = 0,
    /// Production available for construction.
    Production = 1,
    /// Science available for research.
    Science = 2,
    /// Culture available for civic progress.
    Culture = 3,
    /// Gold available for economic actions.
    Gold = 4,
    /// Faith available for spiritual actions.
    Faith = 5,
}

impl YieldKind {
    /// Every yield kind in canonical order.
    pub const ALL: [Self; 6] = [
        Self::Food,
        Self::Production,
        Self::Science,
        Self::Culture,
        Self::Gold,
        Self::Faith,
    ];

    const fn index(self) -> usize {
        self as usize
    }
}

/// A fixed-point set of yields in canonical channel order.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct YieldSet([Fixed; 6]);

impl YieldSet {
    /// Returns an all-zero yield set.
    pub const fn zero() -> Self {
        Self([Fixed::ZERO; 6])
    }

    /// Returns the value for one channel.
    pub const fn get(self, kind: YieldKind) -> Fixed {
        self.0[kind.index()]
    }

    /// Returns a copy with one channel replaced.
    pub const fn with(self, kind: YieldKind, value: Fixed) -> Self {
        let mut values = self.0;
        values[kind.index()] = value;
        Self(values)
    }

    /// Adds two yield sets with overflow checking.
    pub fn checked_add(self, other: Self) -> Option<Self> {
        let mut values = [Fixed::ZERO; 6];
        for kind in YieldKind::ALL {
            values[kind.index()] = self.get(kind).checked_add(other.get(kind))?;
        }
        Some(Self(values))
    }

    /// Subtracts two yield sets with overflow checking.
    pub fn checked_sub(self, other: Self) -> Option<Self> {
        let mut values = [Fixed::ZERO; 6];
        for kind in YieldKind::ALL {
            values[kind.index()] = self.get(kind).checked_sub(other.get(kind))?;
        }
        Some(Self(values))
    }

    /// Returns whether every channel is non-negative.
    pub fn is_non_negative(self) -> bool {
        YieldKind::ALL.iter().all(|kind| self.get(*kind).raw() >= 0)
    }
}

impl Default for YieldSet {
    fn default() -> Self {
        Self::zero()
    }
}

/// The scope in which a modifier is evaluated.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ModifierScope {
    /// Applies to a whole player.
    Player,
    /// Applies to one city.
    City,
    /// Applies to one tile.
    Tile,
    /// Applies to one unit.
    Unit,
}

/// A stable content tag used by modifier conditions.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct TagId(u16);

impl TagId {
    /// Creates a tag identifier.
    pub const fn new(value: u16) -> Self {
        Self(value)
    }

    /// Returns the numeric tag identifier.
    pub const fn value(self) -> u16 {
        self.0
    }
}

/// A condition in the data-driven modifier language.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ModifierCondition {
    /// Requires a content tag in the evaluation context.
    HasTag(TagId),
    /// Requires a specific evaluation scope.
    Scope(ModifierScope),
}

/// An effect in the data-driven modifier language.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ModifierEffect {
    /// Adds a fixed-point amount to one yield channel.
    AddYield {
        /// The channel to change.
        kind: YieldKind,
        /// The fixed-point amount to add.
        amount: Fixed,
    },
}

/// A validated modifier with conditions, scope, and effect.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Modifier {
    scope: ModifierScope,
    conditions: Vec<ModifierCondition>,
    effect: ModifierEffect,
}

impl Modifier {
    /// Creates a modifier. Ruleset validation supplies canonical condition order.
    pub fn new(
        scope: ModifierScope,
        conditions: Vec<ModifierCondition>,
        effect: ModifierEffect,
    ) -> Self {
        Self {
            scope,
            conditions,
            effect,
        }
    }

    /// Returns the modifier scope.
    pub const fn scope(&self) -> ModifierScope {
        self.scope
    }

    /// Returns whether this modifier matches a context.
    pub fn matches(&self, context: &ModifierContext) -> bool {
        self.scope == context.scope
            && self.conditions.iter().all(|condition| match condition {
                ModifierCondition::HasTag(tag) => context.has_tag(*tag),
                ModifierCondition::Scope(scope) => *scope == context.scope,
            })
    }

    fn apply(&self, yields: YieldSet) -> Option<YieldSet> {
        match self.effect {
            ModifierEffect::AddYield { kind, amount } => {
                yields.checked_add(YieldSet::zero().with(kind, amount))
            }
        }
    }
}

/// The canonical context used while evaluating modifiers.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ModifierContext {
    scope: ModifierScope,
    tags: Vec<TagId>,
}

impl ModifierContext {
    /// Creates a context and canonicalizes its tag list.
    pub fn new(scope: ModifierScope, tags: &[TagId]) -> Self {
        let mut tags = tags.to_vec();
        tags.sort_unstable();
        tags.dedup();
        Self { scope, tags }
    }

    fn has_tag(&self, tag: TagId) -> bool {
        self.tags.binary_search(&tag).is_ok()
    }
}

/// Applies matching modifiers in their supplied canonical order.
pub fn apply_modifiers(
    base: YieldSet,
    modifiers: &[Modifier],
    context: &ModifierContext,
) -> Option<YieldSet> {
    let mut result = base;
    for modifier in modifiers {
        if modifier.matches(context) {
            result = modifier.apply(result)?;
        }
    }
    Some(result)
}

#[cfg(test)]
mod tests {
    use super::{
        CityId, FIXED_SCALE, Fixed, Modifier, ModifierCondition, ModifierContext, ModifierEffect,
        ModifierScope, NamedRng, RNG_ALGORITHM, TagId, YieldKind, YieldSet, apply_modifiers,
    };

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

    #[test]
    fn typed_ids_are_orderable_and_stable() {
        assert!(CityId::new(2) > CityId::new(1));
        assert_eq!(CityId::new(7).index(), 7);
    }

    #[test]
    fn modifiers_require_scope_and_tags() {
        let modifier = Modifier::new(
            ModifierScope::Tile,
            vec![ModifierCondition::HasTag(TagId::new(4))],
            ModifierEffect::AddYield {
                kind: YieldKind::Food,
                amount: Fixed::from_int(2).unwrap(),
            },
        );
        let base = YieldSet::zero();
        let matching = ModifierContext::new(ModifierScope::Tile, &[TagId::new(4), TagId::new(4)]);
        let wrong_scope = ModifierContext::new(ModifierScope::City, &[TagId::new(4)]);
        let wrong_tag = ModifierContext::new(ModifierScope::Tile, &[TagId::new(5)]);
        assert_eq!(
            apply_modifiers(base, std::slice::from_ref(&modifier), &matching)
                .unwrap()
                .get(YieldKind::Food)
                .raw(),
            2 * FIXED_SCALE
        );
        assert_eq!(
            apply_modifiers(base, std::slice::from_ref(&modifier), &wrong_scope),
            Some(base)
        );
        assert_eq!(apply_modifiers(base, &[modifier], &wrong_tag), Some(base));
    }

    #[test]
    fn negative_result_is_detectable_for_yield_invariants() {
        let modifier = Modifier::new(
            ModifierScope::Player,
            Vec::new(),
            ModifierEffect::AddYield {
                kind: YieldKind::Gold,
                amount: Fixed::from_raw(-1),
            },
        );
        let result = apply_modifiers(
            YieldSet::zero(),
            &[modifier],
            &ModifierContext::new(ModifierScope::Player, &[]),
        )
        .unwrap();
        assert!(!result.is_non_negative());
    }
}
