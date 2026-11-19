//! Rules for shaping continents and populating their terrain.

use std::collections::BTreeMap;

use serde::Deserialize;

use crate::RulesetError;

/// Terrain categories used by the base map generator.
#[derive(
    Clone, Copy, Debug, Eq, Hash, Deserialize, Ord, PartialEq, PartialOrd, serde::Serialize,
)]
#[serde(rename_all = "lowercase")]
#[repr(u8)]
pub enum Terrain {
    /// Lowland suitable for general settlement.
    Plains = 0,
    /// Productive temperate lowland.
    Grassland = 1,
    /// Dry lowland.
    Desert = 2,
    /// Cold lowland.
    Tundra = 3,
    /// Shallow water.
    Coast = 4,
    /// Deep water.
    Ocean = 5,
    /// Elevated terrain.
    Hill = 6,
    /// Impassable high terrain.
    Mountain = 7,
}

impl Terrain {
    /// Decodes a stable map-format tag.
    pub const fn from_u8(value: u8) -> Option<Self> {
        Some(match value {
            0 => Self::Plains,
            1 => Self::Grassland,
            2 => Self::Desert,
            3 => Self::Tundra,
            4 => Self::Coast,
            5 => Self::Ocean,
            6 => Self::Hill,
            7 => Self::Mountain,
            _ => return None,
        })
    }
}

/// Feature categories placed on terrain.
#[derive(
    Clone, Copy, Debug, Eq, Hash, Deserialize, Ord, PartialEq, PartialOrd, serde::Serialize,
)]
#[serde(rename_all = "lowercase")]
#[repr(u8)]
pub enum Feature {
    /// No feature.
    None = 0,
    /// Temperate woodland.
    Forest = 1,
    /// Tropical woodland.
    Jungle = 2,
    /// Wet lowland.
    Marsh = 3,
}

impl Feature {
    /// Decodes a stable map-format tag.
    pub const fn from_u8(value: u8) -> Option<Self> {
        Some(match value {
            0 => Self::None,
            1 => Self::Forest,
            2 => Self::Jungle,
            3 => Self::Marsh,
            _ => return None,
        })
    }
}

/// Resource categories placed on a tile.
#[derive(
    Clone, Copy, Debug, Eq, Hash, Deserialize, Ord, PartialEq, PartialOrd, serde::Serialize,
)]
#[serde(rename_all = "lowercase")]
#[repr(u8)]
pub enum ResourceKind {
    /// A food resource.
    Wheat = 0,
    /// A construction resource.
    Stone = 1,
    /// A strategic metal.
    Iron = 2,
    /// A trade resource.
    Gold = 3,
}

impl ResourceKind {
    /// Decodes a stable map-format tag.
    pub const fn from_u8(value: u8) -> Option<Self> {
        Some(match value {
            0 => Self::Wheat,
            1 => Self::Stone,
            2 => Self::Iron,
            3 => Self::Gold,
            _ => return None,
        })
    }
}

/// Integer controls for a ruleset's map generator.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct MapGeneration {
    /// Maximum number of landmasses, in 1..=16.
    pub continents: u8,
    /// Target land coverage, rounded to the nearest tile.
    pub land_percent: u8,
    /// Percentage of eligible land tiles that start a river.
    pub river_source_percent: u8,
    /// Relative frequency and placements for each generated land terrain.
    pub land: BTreeMap<Terrain, LandGeneration>,
}

/// Placement rules for one kind of land terrain.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct LandGeneration {
    /// Relative frequency among the land terrains.
    pub weight: u16,
    /// Absolute percentage chances, with the remainder left featureless.
    #[serde(default)]
    pub features: BTreeMap<Feature, u8>,
    /// Absolute percentage chances, with the remainder left without a resource.
    #[serde(default)]
    pub resources: BTreeMap<ResourceKind, u8>,
}

impl MapGeneration {
    pub(super) fn validate(&self) -> Result<(), RulesetError> {
        fn invalid(field: &'static str, value: impl ToString) -> RulesetError {
            RulesetError::InvalidValue {
                field,
                value: value.to_string(),
            }
        }
        if !(1..=16).contains(&self.continents) {
            return Err(invalid("mapgen.continents", self.continents));
        }
        if self.land_percent > 100 {
            return Err(invalid("mapgen.land_percent", self.land_percent));
        }
        if self.river_source_percent > 100 {
            return Err(invalid(
                "mapgen.river_source_percent",
                self.river_source_percent,
            ));
        }
        if self.land.is_empty() {
            return Err(RulesetError::MissingField("mapgen.land"));
        }
        for (terrain, biome) in &self.land {
            if matches!(terrain, Terrain::Ocean | Terrain::Coast) || biome.weight == 0 {
                return Err(invalid(
                    "mapgen.land",
                    format!("{terrain:?} weight {}", biome.weight),
                ));
            }
            if biome.features.contains_key(&Feature::None)
                || (*terrain == Terrain::Mountain
                    && (!biome.features.is_empty() || !biome.resources.is_empty()))
            {
                return Err(invalid(
                    "mapgen.land",
                    format!("incompatible placement on {terrain:?}"),
                ));
            }
            for (field, sum) in [
                (
                    "mapgen.land.features",
                    biome.features.values().copied().map(u16::from).sum::<u16>(),
                ),
                (
                    "mapgen.land.resources",
                    biome.resources.values().copied().map(u16::from).sum(),
                ),
            ] {
                if sum > 100 {
                    return Err(invalid(field, format!("{terrain:?} total {sum}")));
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::{Ruleset, base_ruleset};

    const HEADER: &str = "schema_version=1\nid='sample'\nname='Sample'\nprovenance='designed'\n";
    const PROFILE: &str = "[mapgen]\ncontinents=2\nland_percent=56\nriver_source_percent=5\n[mapgen.land.plains]\nweight=1\nfeatures={forest=80}\nresources={gold=50}\n";

    #[test]
    fn version_one_rulesets_without_map_settings_inherit_bundled_settings() {
        assert_eq!(
            Ruleset::from_toml(HEADER).unwrap().map_generation(),
            base_ruleset().map_generation()
        );
    }

    #[test]
    fn rejects_invalid_counts_probabilities_weights_and_placement_keys() {
        for (from, to) in [
            ("continents=2", "continents=0"),
            ("continents=2", "continents=17"),
            ("land_percent=56", "land_percent=101"),
            ("river_source_percent=5", "river_source_percent=102"),
            ("weight=1", "weight=0"),
            ("forest=80", "forest=80,marsh=21"),
            ("gold=50", "gold=50,iron=51"),
            ("forest=80", "none=2"),
            ("land.plains", "land.mountain"),
            ("land.plains", "land.coast"),
            ("land.plains", "land.unknown"),
            ("gold=50", "unknown=50"),
            ("weight=1", "weight=1\nextra=1"),
        ] {
            assert!(
                Ruleset::from_toml(&format!("{HEADER}{}", PROFILE.replace(from, to))).is_err(),
                "accepted {to}"
            );
        }
    }
}
