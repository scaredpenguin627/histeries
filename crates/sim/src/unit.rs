//! Deterministic unit data and combat statistics.

use histeries_rules_core::{PlayerId, UnitId};

use crate::city::base_ruleset;
use crate::map::Terrain;

/// Unit archetypes available in the headless Phase 1 combat model.
#[derive(
    Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
#[repr(u8)]
pub enum UnitKind {
    /// A basic melee unit.
    Warrior = 0,
    /// A ranged unit.
    Archer = 1,
    /// A civilian unit.
    Settler = 2,
    /// A hostile non-player melee unit.
    Barbarian = 3,
    /// A civilian unit with limited improvement charges.
    Builder = 4,
}

impl UnitKind {
    /// Decodes a persisted unit archetype tag.
    pub const fn from_u8(value: u8) -> Option<Self> {
        Some(match value {
            0 => Self::Warrior,
            1 => Self::Archer,
            2 => Self::Settler,
            3 => Self::Barbarian,
            4 => Self::Builder,
            _ => return None,
        })
    }

    /// Returns the base movement allowance.
    pub const fn movement(self) -> i32 {
        match self {
            Self::Warrior | Self::Archer | Self::Barbarian => 2,
            Self::Settler | Self::Builder => 2,
        }
    }

    /// Returns the base maximum hit points.
    pub const fn max_hp(self) -> i32 {
        match self {
            Self::Warrior | Self::Barbarian => 100,
            Self::Archer => 80,
            Self::Settler | Self::Builder => 60,
        }
    }

    /// Returns the base attack strength.
    pub const fn attack(self) -> i32 {
        match self {
            Self::Warrior => 20,
            Self::Archer => 15,
            Self::Settler | Self::Builder => 0,
            Self::Barbarian => 18,
        }
    }

    /// Returns the base defense strength.
    pub const fn defense(self) -> i32 {
        match self {
            Self::Warrior => 20,
            Self::Archer => 10,
            Self::Settler | Self::Builder => 5,
            Self::Barbarian => 16,
        }
    }

    /// Returns the attack range in topology steps.
    pub const fn range(self) -> u32 {
        match self {
            Self::Archer => 2,
            Self::Warrior | Self::Settler | Self::Builder | Self::Barbarian => 1,
        }
    }

    /// Returns whether the unit is a military unit.
    pub const fn is_military(self) -> bool {
        !matches!(self, Self::Settler | Self::Builder)
    }

    /// Returns the stable ruleset identifier.
    pub const fn ruleset_id(self) -> &'static str {
        match self {
            Self::Warrior => "warrior",
            Self::Archer => "archer",
            Self::Settler => "settler",
            Self::Barbarian => "barbarian",
            Self::Builder => "builder",
        }
    }

    /// Returns the data-defined production cost for this unit.
    pub fn production_cost(self) -> i64 {
        if self == Self::Barbarian {
            i64::MAX
        } else {
            base_ruleset()
                .unit(self.ruleset_id())
                .map_or(i64::MAX, |unit| i64::from(unit.production_cost()))
        }
    }
}

/// Promotion choices for military units.
#[derive(
    Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
#[repr(u8)]
pub enum PromotionKind {
    /// Improves attack strength.
    Shock = 0,
    /// Improves defense strength.
    Drill = 1,
    /// Improves movement allowance.
    Mobility = 2,
}

impl PromotionKind {
    pub(crate) const fn from_u8(value: u8) -> Option<Self> {
        Some(match value {
            0 => Self::Shock,
            1 => Self::Drill,
            2 => Self::Mobility,
            _ => return None,
        })
    }

    const fn bit(self) -> u16 {
        1 << self as u16
    }
}

/// A unit stored by simulation state.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Unit {
    pub(crate) id: UnitId,
    pub(crate) owner: Option<PlayerId>,
    pub(crate) kind: UnitKind,
    pub(crate) tile: histeries_hex::TileId,
    pub(crate) hp: i32,
    pub(crate) movement: i32,
    pub(crate) xp: u8,
    pub(crate) promotions: u16,
    pub(crate) charges: u8,
}

impl Unit {
    /// Creates a full-health unit at a tile.
    pub fn new(
        id: UnitId,
        owner: Option<PlayerId>,
        kind: UnitKind,
        tile: histeries_hex::TileId,
    ) -> Self {
        Self {
            id,
            owner,
            kind,
            tile,
            hp: kind.max_hp(),
            movement: kind.movement(),
            xp: 0,
            promotions: 0,
            charges: if kind == UnitKind::Builder { 3 } else { 0 },
        }
    }

    /// Returns the unit's current attack strength.
    pub fn attack(&self) -> i32 {
        self.kind.attack()
            + if self.has_promotion(PromotionKind::Shock) {
                5
            } else {
                0
            }
    }

    /// Returns the unit's current defense strength.
    pub fn defense(&self) -> i32 {
        self.kind.defense()
            + if self.has_promotion(PromotionKind::Drill) {
                5
            } else {
                0
            }
    }

    /// Returns the unit's effective movement allowance.
    pub fn max_movement(&self) -> i32 {
        self.kind.movement()
            + if self.has_promotion(PromotionKind::Mobility) {
                1
            } else {
                0
            }
    }

    /// Returns whether the unit has a promotion.
    pub fn has_promotion(&self, promotion: PromotionKind) -> bool {
        self.promotions & promotion.bit() != 0
    }

    /// Returns whether a promotion can be applied.
    pub fn can_promote(&self, promotion: PromotionKind) -> bool {
        self.kind.is_military() && self.xp > 0 && !self.has_promotion(promotion)
    }

    /// Applies a promotion and consumes one experience point.
    pub fn promote(&mut self, promotion: PromotionKind) -> bool {
        if !self.can_promote(promotion) {
            return false;
        }
        self.promotions |= promotion.bit();
        self.xp -= 1;
        self.movement = self.movement.min(self.max_movement());
        true
    }

    /// Resets movement at the beginning of a turn.
    pub fn reset_turn(&mut self) {
        self.movement = self.max_movement();
    }

    /// Returns remaining builder improvement charges.
    pub const fn charges(&self) -> u8 {
        self.charges
    }

    /// Consumes one builder charge.
    pub(crate) fn consume_charge(&mut self) -> bool {
        if self.kind != UnitKind::Builder || self.charges == 0 {
            return false;
        }
        self.charges -= 1;
        true
    }

    /// Returns the movement cost for a terrain and road combination.
    pub const fn movement_cost(terrain: Terrain, road: bool) -> Option<i32> {
        if matches!(terrain, Terrain::Mountain | Terrain::Ocean) {
            return None;
        }
        if road {
            return Some(1);
        }
        Some(match terrain {
            Terrain::Hill => 2,
            Terrain::Plains
            | Terrain::Grassland
            | Terrain::Desert
            | Terrain::Tundra
            | Terrain::Coast => 1,
            Terrain::Mountain | Terrain::Ocean => return None,
        })
    }
}

#[cfg(test)]
mod tests {
    use histeries_hex::TileId;
    use histeries_rules_core::{PlayerId, UnitId};

    use super::{PromotionKind, Terrain, Unit, UnitKind};

    #[test]
    fn movement_costs_and_promotions_are_deterministic() {
        assert_eq!(Unit::movement_cost(Terrain::Plains, false), Some(1));
        assert_eq!(Unit::movement_cost(Terrain::Hill, false), Some(2));
        assert_eq!(Unit::movement_cost(Terrain::Mountain, false), None);
        assert_eq!(Unit::movement_cost(Terrain::Hill, true), Some(1));
        let mut builder = Unit::new(
            UnitId::new(2),
            Some(PlayerId::new(1)),
            UnitKind::Builder,
            TileId::new(0),
        );
        assert_eq!(builder.charges(), 3);
        assert!(builder.consume_charge());
        assert_eq!(builder.charges(), 2);
        let mut unit = Unit::new(
            UnitId::new(1),
            Some(PlayerId::new(1)),
            UnitKind::Warrior,
            TileId::new(0),
        );
        unit.xp = 1;
        assert!(unit.promote(PromotionKind::Shock));
        assert!(!unit.promote(PromotionKind::Shock));
        unit.movement = 0;
        unit.reset_turn();
        assert_eq!(unit.movement, unit.max_movement());
    }
}
