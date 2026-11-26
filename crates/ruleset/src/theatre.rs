//! Data-defined expedition boards.

use serde::Deserialize;

use crate::{AgeDefinition, RulesetError, validate_id};

/// A flat hex graph with an age capability controlling access.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct TheatreDefinition {
    id: String,
    name: String,
    required_unlock: String,
    width: u32,
    height: u32,
    #[serde(default)]
    wrap_x: bool,
    #[serde(default)]
    wrap_y: bool,
    entry_tile: u32,
}

impl TheatreDefinition {
    /// Stable theatre identifier used in orders and saves.
    pub fn id(&self) -> &str {
        &self.id
    }
    /// Display name of the board.
    pub fn name(&self) -> &str {
        &self.name
    }
    /// Cumulative age capability needed to enter.
    pub fn required_unlock(&self) -> &str {
        &self.required_unlock
    }
    /// Number of columns in the secondary graph.
    pub const fn width(&self) -> u32 {
        self.width
    }
    /// Number of rows in the secondary graph.
    pub const fn height(&self) -> u32 {
        self.height
    }
    /// Whether horizontal boundaries connect.
    pub const fn wrap_x(&self) -> bool {
        self.wrap_x
    }
    /// Whether vertical boundaries connect.
    pub const fn wrap_y(&self) -> bool {
        self.wrap_y
    }
    /// Local tile identifier where expeditions arrive and return.
    pub const fn entry_tile(&self) -> u32 {
        self.entry_tile
    }
}

pub(super) fn validated(
    mut theatres: Vec<TheatreDefinition>,
    ages: &[AgeDefinition],
) -> Result<Vec<TheatreDefinition>, RulesetError> {
    theatres.sort_by(|a, b| a.id.cmp(&b.id));
    for pair in theatres.windows(2) {
        if pair[0].id == pair[1].id {
            return Err(RulesetError::DuplicateId {
                kind: "theatre",
                id: pair[0].id.clone(),
            });
        }
    }
    for theatre in &theatres {
        validate_id("theatre", &theatre.id)?;
        if theatre.name.trim().is_empty() {
            return Err(RulesetError::MissingField("theatre.name"));
        }
        if !ages
            .iter()
            .flat_map(AgeDefinition::unlocks)
            .any(|id| id == &theatre.required_unlock)
        {
            return Err(RulesetError::InvalidValue {
                field: "theatre.required_unlock",
                value: theatre.required_unlock.clone(),
            });
        }
        let cells = u64::from(theatre.width) * u64::from(theatre.height);
        if !(2..=4096).contains(&cells) || u64::from(theatre.entry_tile) >= cells {
            return Err(RulesetError::InvalidValue {
                field: "theatre.board",
                value: theatre.id.clone(),
            });
        }
    }
    Ok(theatres)
}

#[cfg(test)]
mod tests {
    use crate::{Ruleset, base_ruleset};

    #[test]
    fn orbital_board_references_a_granted_capability_and_a_real_entry() {
        let orbit = base_ruleset().theatre("orbit").unwrap();
        assert_eq!(
            (
                orbit.width(),
                orbit.height(),
                orbit.wrap_x(),
                orbit.wrap_y()
            ),
            (8, 4, true, false)
        );
        assert_eq!(orbit.entry_tile(), 0);
        assert_eq!(orbit.required_unlock(), "orbital_access");
        let raw = include_str!("../../../data/rulesets/base/base.toml");
        for (valid, invalid) in [
            ("entry_tile = 0", "entry_tile = 32"),
            (
                "required_unlock = \"orbital_access\"",
                "required_unlock = \"unknown\"",
            ),
            ("width = 8", "width = 0"),
        ] {
            assert!(Ruleset::from_toml(&raw.replace(valid, invalid)).is_err());
        }
    }
}
