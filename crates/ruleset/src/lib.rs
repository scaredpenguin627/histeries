//! Validated, data-driven TOML rulesets.

#![forbid(unsafe_code)]

use std::{collections::BTreeMap, fmt};

use histeries_rules_core::{
    Fixed, Modifier, ModifierCondition, ModifierEffect, ModifierScope, TagId, YieldKind,
};
use serde::Deserialize;

const SUPPORTED_SCHEMA_VERSION: u32 = 1;

/// A validated ruleset ready for simulation use.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Ruleset {
    id: String,
    name: String,
    provenance: String,
    schema_version: u32,
    units: BTreeMap<String, UnitDefinition>,
    modifiers: BTreeMap<String, Modifier>,
}

impl Ruleset {
    /// Parses and validates one TOML ruleset document.
    pub fn from_toml(source: &str) -> Result<Self, RulesetError> {
        let raw: RawRuleset = toml::from_str(source).map_err(RulesetError::Parse)?;
        if raw.schema_version != SUPPORTED_SCHEMA_VERSION {
            return Err(RulesetError::SchemaVersion(raw.schema_version));
        }
        validate_id("ruleset", &raw.id)?;
        if raw.name.trim().is_empty() {
            return Err(RulesetError::MissingField("name"));
        }
        validate_provenance(&raw.provenance)?;

        let mut units = BTreeMap::new();
        for raw_unit in raw.units {
            validate_id("unit", &raw_unit.id)?;
            if raw_unit.movement == 0 {
                return Err(RulesetError::InvalidValue {
                    field: "unit.movement",
                    value: raw_unit.id,
                });
            }
            if units
                .insert(
                    raw_unit.id.clone(),
                    UnitDefinition {
                        id: raw_unit.id.clone(),
                        movement: raw_unit.movement,
                    },
                )
                .is_some()
            {
                return Err(RulesetError::DuplicateId {
                    kind: "unit",
                    id: raw_unit.id,
                });
            }
        }

        let mut modifiers = BTreeMap::new();
        for raw_modifier in raw.modifiers {
            validate_id("modifier", &raw_modifier.id)?;
            let scope = parse_scope(&raw_modifier.scope)?;
            let conditions = raw_modifier
                .tags
                .into_iter()
                .map(|tag| ModifierCondition::HasTag(TagId::new(tag)))
                .collect();
            let kind = parse_yield_kind(&raw_modifier.effect_kind)?;
            let modifier = Modifier::new(
                scope,
                conditions,
                ModifierEffect::AddYield {
                    kind,
                    amount: Fixed::from_raw(raw_modifier.effect_amount_raw),
                },
            );
            if modifiers
                .insert(raw_modifier.id.clone(), modifier)
                .is_some()
            {
                return Err(RulesetError::DuplicateId {
                    kind: "modifier",
                    id: raw_modifier.id,
                });
            }
        }

        Ok(Self {
            id: raw.id,
            name: raw.name,
            provenance: raw.provenance,
            schema_version: raw.schema_version,
            units,
            modifiers,
        })
    }

    /// Returns the stable ruleset identifier.
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Returns the human-readable ruleset name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the provenance tag carried by this ruleset.
    pub fn provenance(&self) -> &str {
        &self.provenance
    }

    /// Returns the validated schema version.
    pub const fn schema_version(&self) -> u32 {
        self.schema_version
    }

    /// Returns units in stable identifier order.
    pub fn units(&self) -> impl Iterator<Item = &UnitDefinition> {
        self.units.values()
    }

    /// Returns a unit by stable identifier.
    pub fn unit(&self, id: &str) -> Option<&UnitDefinition> {
        self.units.get(id)
    }

    /// Returns a modifier by stable identifier.
    pub fn modifier(&self, id: &str) -> Option<&Modifier> {
        self.modifiers.get(id)
    }
}

/// A validated unit table entry.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UnitDefinition {
    id: String,
    movement: u16,
}

impl UnitDefinition {
    /// Returns the stable unit identifier.
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Returns the integer movement allowance.
    pub const fn movement(&self) -> u16 {
        self.movement
    }
}

/// Errors returned by ruleset parsing and validation.
#[derive(Debug)]
pub enum RulesetError {
    /// TOML syntax or type error.
    Parse(toml::de::Error),
    /// Unsupported schema version.
    SchemaVersion(u32),
    /// A required field was empty or absent.
    MissingField(&'static str),
    /// An identifier was not a valid stable slug.
    InvalidId {
        /// The table kind being validated.
        kind: &'static str,
        /// The invalid identifier.
        id: String,
    },
    /// A value violated a table constraint.
    InvalidValue {
        /// The field being validated.
        field: &'static str,
        /// The invalid value rendered as text.
        value: String,
    },
    /// Two entries used one identifier.
    DuplicateId {
        /// The table kind containing the duplicate.
        kind: &'static str,
        /// The repeated identifier.
        id: String,
    },
    /// A scope string was not supported.
    InvalidScope(String),
    /// A yield string was not supported.
    InvalidYield(String),
    /// A provenance tag was not supported.
    InvalidProvenance(String),
}

impl fmt::Display for RulesetError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Parse(error) => write!(formatter, "TOML parse error: {error}"),
            Self::SchemaVersion(version) => {
                write!(formatter, "unsupported schema version {version}")
            }
            Self::MissingField(field) => write!(formatter, "missing field {field}"),
            Self::InvalidId { kind, id } => write!(formatter, "invalid {kind} id {id:?}"),
            Self::InvalidValue { field, value } => {
                write!(formatter, "invalid {field} value {value:?}")
            }
            Self::DuplicateId { kind, id } => write!(formatter, "duplicate {kind} id {id:?}"),
            Self::InvalidScope(scope) => write!(formatter, "invalid modifier scope {scope:?}"),
            Self::InvalidYield(kind) => write!(formatter, "invalid yield kind {kind:?}"),
            Self::InvalidProvenance(provenance) => {
                write!(formatter, "invalid provenance tag {provenance:?}")
            }
        }
    }
}

impl std::error::Error for RulesetError {}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawRuleset {
    schema_version: u32,
    id: String,
    name: String,
    provenance: String,
    #[serde(default)]
    units: Vec<RawUnit>,
    #[serde(default)]
    modifiers: Vec<RawModifier>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawUnit {
    id: String,
    movement: u16,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawModifier {
    id: String,
    scope: String,
    #[serde(default)]
    tags: Vec<u16>,
    effect_kind: String,
    effect_amount_raw: i64,
}

fn validate_id(kind: &'static str, id: &str) -> Result<(), RulesetError> {
    if id.is_empty()
        || !id.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_' || byte == b'-'
        })
    {
        return Err(RulesetError::InvalidId {
            kind,
            id: id.to_owned(),
        });
    }
    Ok(())
}

fn parse_scope(scope: &str) -> Result<ModifierScope, RulesetError> {
    match scope {
        "player" => Ok(ModifierScope::Player),
        "city" => Ok(ModifierScope::City),
        "tile" => Ok(ModifierScope::Tile),
        "unit" => Ok(ModifierScope::Unit),
        other => Err(RulesetError::InvalidScope(other.to_owned())),
    }
}

fn parse_yield_kind(kind: &str) -> Result<YieldKind, RulesetError> {
    match kind {
        "food" => Ok(YieldKind::Food),
        "production" => Ok(YieldKind::Production),
        "science" => Ok(YieldKind::Science),
        "culture" => Ok(YieldKind::Culture),
        "gold" => Ok(YieldKind::Gold),
        "faith" => Ok(YieldKind::Faith),
        other => Err(RulesetError::InvalidYield(other.to_owned())),
    }
}

fn validate_provenance(provenance: &str) -> Result<(), RulesetError> {
    match provenance {
        "observed" | "public-doc" | "designed" => Ok(()),
        other => Err(RulesetError::InvalidProvenance(other.to_owned())),
    }
}

#[cfg(test)]
mod tests {
    use super::{Ruleset, RulesetError};
    use histeries_rules_core::{ModifierContext, ModifierScope, YieldKind};

    const BASE: &str = include_str!("../../../data/rulesets/base/base.toml");

    #[test]
    fn loads_and_validates_base_ruleset() {
        let ruleset = Ruleset::from_toml(BASE).expect("base ruleset");
        assert_eq!(ruleset.id(), "base");
        assert_eq!(ruleset.provenance(), "designed");
        assert_eq!(ruleset.schema_version(), 1);
        assert_eq!(ruleset.unit("scout").unwrap().movement(), 2);
        let modifier = ruleset.modifier("forest_food").unwrap();
        let context = ModifierContext::new(ModifierScope::Tile, &[super::TagId::new(1)]);
        let yields = histeries_rules_core::apply_modifiers(
            histeries_rules_core::YieldSet::zero(),
            std::slice::from_ref(modifier),
            &context,
        )
        .unwrap();
        assert_eq!(yields.get(YieldKind::Food).raw(), 1_000);
    }

    #[test]
    fn rejects_duplicate_and_invalid_entries() {
        let duplicate = "schema_version=1\nid=\"base\"\nname=\"Base\"\nprovenance=\"designed\"\n[[units]]\nid=\"scout\"\nmovement=1\n[[units]]\nid=\"scout\"\nmovement=2\n";
        assert!(matches!(
            Ruleset::from_toml(duplicate),
            Err(RulesetError::DuplicateId { kind: "unit", .. })
        ));

        let invalid_scope = "schema_version=1\nid=\"base\"\nname=\"Base\"\nprovenance=\"designed\"\n[[modifiers]]\nid=\"bonus\"\nscope=\"planet\"\neffect_kind=\"food\"\neffect_amount_raw=1\n";
        assert!(matches!(
            Ruleset::from_toml(invalid_scope),
            Err(RulesetError::InvalidScope(_))
        ));
    }

    #[test]
    fn rejects_unsupported_schema_and_invalid_ids() {
        let schema = "schema_version=2\nid=\"base\"\nname=\"Base\"\nprovenance=\"designed\"\n";
        assert!(matches!(
            Ruleset::from_toml(schema),
            Err(RulesetError::SchemaVersion(2))
        ));
        let id = "schema_version=1\nid=\"Base\"\nname=\"Base\"\nprovenance=\"designed\"\n";
        assert!(matches!(
            Ruleset::from_toml(id),
            Err(RulesetError::InvalidId {
                kind: "ruleset",
                ..
            })
        ));
    }

    #[test]
    fn rejects_unknown_fields_and_invalid_provenance() {
        let unknown =
            "schema_version=1\nid=\"base\"\nname=\"Base\"\nprovenance=\"designed\"\nextra=true\n";
        assert!(matches!(
            Ruleset::from_toml(unknown),
            Err(RulesetError::Parse(_))
        ));
        let provenance = "schema_version=1\nid=\"base\"\nname=\"Base\"\nprovenance=\"guess\"\n";
        assert!(matches!(
            Ruleset::from_toml(provenance),
            Err(RulesetError::InvalidProvenance(_))
        ));
    }
}
