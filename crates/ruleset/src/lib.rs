//! Validated, data-driven TOML rulesets.

#![forbid(unsafe_code)]

use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
};

use histeries_rules_core::{
    Fixed, Modifier, ModifierCondition, ModifierEffect, ModifierScope, TagId, YieldKind,
};
use serde::Deserialize;

mod age;
pub use age::AgeDefinition;
mod theatre;
pub use theatre::TheatreDefinition;
mod map_generation;
pub use map_generation::{Feature, LandGeneration, MapGeneration, ResourceKind, Terrain};

const SUPPORTED_SCHEMA_VERSION: u32 = 1;

/// The validated bundled rules shared by the simulation and its controllers.
pub fn base_ruleset() -> &'static Ruleset {
    static BASE: std::sync::OnceLock<Ruleset> = std::sync::OnceLock::new();
    BASE.get_or_init(|| {
        Ruleset::from_toml(include_str!("../../../data/rulesets/base/base.toml"))
            .expect("base ruleset must remain valid")
    })
}

/// A validated ruleset ready for simulation use.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Ruleset {
    id: String,
    name: String,
    provenance: String,
    schema_version: u32,
    mapgen: Option<MapGeneration>,
    units: BTreeMap<String, UnitDefinition>,
    technologies: BTreeMap<String, TechnologyDefinition>,
    civics: BTreeMap<String, CivicDefinition>,
    ages: Vec<AgeDefinition>,
    theatres: Vec<TheatreDefinition>,
    policies: BTreeMap<String, PolicyDefinition>,
    districts: BTreeMap<String, DistrictDefinition>,
    buildings: BTreeMap<String, BuildingDefinition>,
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

        if let Some(settings) = &raw.mapgen {
            settings.validate()?;
        }

        let mut units = BTreeMap::new();
        for raw_unit in raw.units {
            validate_id("unit", &raw_unit.id)?;
            if raw_unit.movement == 0 {
                return Err(RulesetError::InvalidValue {
                    field: "unit.movement",
                    value: raw_unit.id,
                });
            }
            if raw_unit.production_cost == 0 {
                return Err(RulesetError::InvalidValue {
                    field: "unit.production_cost",
                    value: raw_unit.id,
                });
            }
            if units
                .insert(
                    raw_unit.id.clone(),
                    UnitDefinition {
                        id: raw_unit.id.clone(),
                        movement: raw_unit.movement,
                        production_cost: raw_unit.production_cost,
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

        let technologies =
            parse_progression_table("technology", raw.technologies, |raw| TechnologyDefinition {
                id: raw.id,
                cost: raw.cost,
                prerequisites: raw.prerequisites,
            })?;
        let civics = parse_progression_table("civic", raw.civics, |raw| CivicDefinition {
            id: raw.id,
            cost: raw.cost,
            prerequisites: raw.prerequisites,
        })?;
        let age_ids = age::validate(&raw.ages, &technologies, &civics)?;
        let theatres = theatre::validated(raw.theatres, &raw.ages)?;
        let mut policies = BTreeMap::new();
        for raw_policy in raw.policies {
            validate_id("policy", &raw_policy.id)?;
            if let Some(required) = &raw_policy.required_age
                && !age_ids.contains(required.as_str())
            {
                return Err(RulesetError::InvalidValue {
                    field: "policy.required_age",
                    value: required.clone(),
                });
            }
            if let Some(required) = &raw_policy.required_civic {
                validate_id("civic", required)?;
                if !civics.contains_key(required) {
                    return Err(RulesetError::InvalidValue {
                        field: "policy.required_civic",
                        value: required.clone(),
                    });
                }
            }
            if policies
                .insert(
                    raw_policy.id.clone(),
                    PolicyDefinition {
                        id: raw_policy.id.clone(),
                        required_civic: raw_policy.required_civic,
                        required_age: raw_policy.required_age,
                        science_bonus: raw_policy.science_bonus,
                        culture_bonus: raw_policy.culture_bonus,
                    },
                )
                .is_some()
            {
                return Err(RulesetError::DuplicateId {
                    kind: "policy",
                    id: raw_policy.id,
                });
            }
        }

        let mut districts = BTreeMap::new();
        for raw_district in raw.districts {
            validate_id("district", &raw_district.id)?;
            if raw_district.production_cost == 0 {
                return Err(RulesetError::InvalidValue {
                    field: "district.production_cost",
                    value: raw_district.id,
                });
            }
            if districts
                .insert(
                    raw_district.id.clone(),
                    DistrictDefinition {
                        id: raw_district.id.clone(),
                        production_cost: raw_district.production_cost,
                        housing_capacity: raw_district.housing_capacity,
                        production_bonus: raw_district.production_bonus,
                        adjacency_production: raw_district.adjacency_production,
                    },
                )
                .is_some()
            {
                return Err(RulesetError::DuplicateId {
                    kind: "district",
                    id: raw_district.id,
                });
            }
        }

        let mut buildings = BTreeMap::new();
        for raw_building in raw.buildings {
            validate_id("building", &raw_building.id)?;
            if raw_building.production_cost == 0 {
                return Err(RulesetError::InvalidValue {
                    field: "building.production_cost",
                    value: raw_building.id,
                });
            }
            validate_id("district", &raw_building.district)?;
            if !districts.contains_key(&raw_building.district) {
                return Err(RulesetError::InvalidValue {
                    field: "building.district",
                    value: raw_building.district,
                });
            }
            if buildings
                .insert(
                    raw_building.id.clone(),
                    BuildingDefinition {
                        id: raw_building.id.clone(),
                        district: raw_building.district,
                        production_cost: raw_building.production_cost,
                        housing: raw_building.housing,
                        maintenance: raw_building.maintenance,
                        food_bonus: raw_building.food_bonus,
                        production_bonus: raw_building.production_bonus,
                        science_bonus: raw_building.science_bonus,
                        culture_bonus: raw_building.culture_bonus,
                    },
                )
                .is_some()
            {
                return Err(RulesetError::DuplicateId {
                    kind: "building",
                    id: raw_building.id,
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
            mapgen: raw.mapgen,
            units,
            technologies,
            civics,
            ages: raw.ages,
            theatres,
            policies,
            districts,
            buildings,
            modifiers,
        })
    }

    /// Returns ages in progression order, starting with the initial age.
    pub fn ages(&self) -> &[AgeDefinition] {
        &self.ages
    }

    /// Additional boards in stable identifier order.
    pub fn theatres(&self) -> &[TheatreDefinition] {
        &self.theatres
    }

    /// An additional board by its stable identifier.
    pub fn theatre(&self, id: &str) -> Option<&TheatreDefinition> {
        self.theatres.iter().find(|theatre| theatre.id() == id)
    }

    /// Whether a seat in `current` has entered the required age.
    pub fn has_reached_age(&self, current: Option<&str>, required: &str) -> bool {
        let Some(index) = self.ages.iter().position(|age| Some(age.id()) == current) else {
            return false;
        };
        self.ages[..=index].iter().any(|age| age.id() == required)
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

    /// Returns the map profile. Schema-v1 documents without one inherit the base profile.
    pub fn map_generation(&self) -> &MapGeneration {
        self.mapgen
            .as_ref()
            .unwrap_or_else(|| base_ruleset().mapgen.as_ref().expect("bundled map profile"))
    }

    /// Returns units in stable identifier order.
    pub fn units(&self) -> impl Iterator<Item = &UnitDefinition> {
        self.units.values()
    }

    /// Returns a unit by stable identifier.
    pub fn unit(&self, id: &str) -> Option<&UnitDefinition> {
        self.units.get(id)
    }

    /// Returns technologies in stable identifier order.
    pub fn technologies(&self) -> impl Iterator<Item = &TechnologyDefinition> {
        self.technologies.values()
    }

    /// Returns a technology by stable identifier.
    pub fn technology(&self, id: &str) -> Option<&TechnologyDefinition> {
        self.technologies.get(id)
    }

    /// Returns civics in stable identifier order.
    pub fn civics(&self) -> impl Iterator<Item = &CivicDefinition> {
        self.civics.values()
    }

    /// Returns a civic by stable identifier.
    pub fn civic(&self, id: &str) -> Option<&CivicDefinition> {
        self.civics.get(id)
    }

    /// Returns policies in stable identifier order.
    pub fn policies(&self) -> impl Iterator<Item = &PolicyDefinition> {
        self.policies.values()
    }

    /// Returns a policy by stable identifier.
    pub fn policy(&self, id: &str) -> Option<&PolicyDefinition> {
        self.policies.get(id)
    }

    /// Returns districts in stable identifier order.
    pub fn districts(&self) -> impl Iterator<Item = &DistrictDefinition> {
        self.districts.values()
    }

    /// Returns a district by stable identifier.
    pub fn district(&self, id: &str) -> Option<&DistrictDefinition> {
        self.districts.get(id)
    }

    /// Returns buildings in stable identifier order.
    pub fn buildings(&self) -> impl Iterator<Item = &BuildingDefinition> {
        self.buildings.values()
    }

    /// Returns a building by stable identifier.
    pub fn building(&self, id: &str) -> Option<&BuildingDefinition> {
        self.buildings.get(id)
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
    production_cost: u16,
}

/// A validated technology progression node.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TechnologyDefinition {
    id: String,
    cost: u16,
    prerequisites: Vec<String>,
}

impl TechnologyDefinition {
    /// Returns the stable technology identifier.
    pub fn id(&self) -> &str {
        &self.id
    }
    /// Returns the research cost.
    pub const fn cost(&self) -> u16 {
        self.cost
    }
    /// Returns prerequisite identifiers in declared order.
    pub fn prerequisites(&self) -> &[String] {
        &self.prerequisites
    }
}

/// A validated civic progression node.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CivicDefinition {
    id: String,
    cost: u16,
    prerequisites: Vec<String>,
}

impl CivicDefinition {
    /// Returns the stable civic identifier.
    pub fn id(&self) -> &str {
        &self.id
    }
    /// Returns the culture cost.
    pub const fn cost(&self) -> u16 {
        self.cost
    }
    /// Returns prerequisite identifiers in declared order.
    pub fn prerequisites(&self) -> &[String] {
        &self.prerequisites
    }
}

/// A validated policy slot option.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PolicyDefinition {
    id: String,
    required_civic: Option<String>,
    required_age: Option<String>,
    science_bonus: i16,
    culture_bonus: i16,
}

impl PolicyDefinition {
    /// Returns the stable policy identifier.
    pub fn id(&self) -> &str {
        &self.id
    }
    /// Returns the civic prerequisite, if any.
    pub fn required_civic(&self) -> Option<&str> {
        self.required_civic.as_deref()
    }

    /// Returns the earliest age in which this policy can be adopted.
    pub fn required_age(&self) -> Option<&str> {
        self.required_age.as_deref()
    }

    /// Returns the per-turn science bonus from this policy.
    pub const fn science_bonus(&self) -> i16 {
        self.science_bonus
    }

    /// Returns the per-turn culture bonus from this policy.
    pub const fn culture_bonus(&self) -> i16 {
        self.culture_bonus
    }
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

    /// Returns the production required to construct the unit.
    pub const fn production_cost(&self) -> u16 {
        self.production_cost
    }
}

/// A validated district table entry.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DistrictDefinition {
    id: String,
    production_cost: u16,
    housing_capacity: u16,
    production_bonus: i16,
    adjacency_production: u16,
}

impl DistrictDefinition {
    /// Returns the stable district identifier.
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Returns the production required to establish the district.
    pub const fn production_cost(&self) -> u16 {
        self.production_cost
    }

    /// Returns the housing capacity provided by the district.
    pub const fn housing_capacity(&self) -> u16 {
        self.housing_capacity
    }

    /// Alias for the district housing capacity.
    pub const fn housing(&self) -> u16 {
        self.housing_capacity
    }

    /// Returns the per-turn production bonus from the district.
    pub const fn production_bonus(&self) -> i16 {
        self.production_bonus
    }

    /// Production per adjacent district or city center of the same city.
    pub const fn adjacency_production(&self) -> u16 {
        self.adjacency_production
    }

    /// Alias for the district production cost.
    pub const fn cost(&self) -> u16 {
        self.production_cost
    }
}

/// A validated building table entry.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BuildingDefinition {
    id: String,
    district: String,
    production_cost: u16,
    housing: u16,
    maintenance: u16,
    food_bonus: i16,
    production_bonus: i16,
    science_bonus: i16,
    culture_bonus: i16,
}

impl BuildingDefinition {
    /// Returns the stable building identifier.
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Returns the district identifier that accepts this building.
    pub fn district(&self) -> &str {
        &self.district
    }

    /// Returns the district identifier that accepts this building.
    pub fn district_id(&self) -> &str {
        &self.district
    }

    /// Returns the production required to construct the building.
    pub const fn production_cost(&self) -> u16 {
        self.production_cost
    }

    /// Returns the housing provided by the building.
    pub const fn housing(&self) -> u16 {
        self.housing
    }

    /// Alias for the building housing contribution.
    pub const fn housing_capacity(&self) -> u16 {
        self.housing
    }

    /// Returns the per-turn maintenance cost of the building.
    pub const fn maintenance(&self) -> u16 {
        self.maintenance
    }

    /// Returns the per-turn food bonus from the building.
    pub const fn food_bonus(&self) -> i16 {
        self.food_bonus
    }

    /// Returns the per-turn production bonus from the building.
    pub const fn production_bonus(&self) -> i16 {
        self.production_bonus
    }

    /// Returns the per-turn science bonus from the building.
    pub const fn science_bonus(&self) -> i16 {
        self.science_bonus
    }

    /// Returns the per-turn culture bonus from the building.
    pub const fn culture_bonus(&self) -> i16 {
        self.culture_bonus
    }

    /// Alias for the building production cost.
    pub const fn cost(&self) -> u16 {
        self.production_cost
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
    mapgen: Option<MapGeneration>,
    #[serde(default)]
    units: Vec<RawUnit>,
    #[serde(default)]
    technologies: Vec<RawProgression>,
    #[serde(default)]
    civics: Vec<RawProgression>,
    #[serde(default)]
    ages: Vec<AgeDefinition>,
    #[serde(default)]
    theatres: Vec<TheatreDefinition>,
    #[serde(default)]
    policies: Vec<RawPolicy>,
    #[serde(default)]
    districts: Vec<RawDistrict>,
    #[serde(default)]
    buildings: Vec<RawBuilding>,
    #[serde(default)]
    modifiers: Vec<RawModifier>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawUnit {
    id: String,
    movement: u16,
    #[serde(default = "default_production_cost")]
    production_cost: u16,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawProgression {
    id: String,
    cost: u16,
    #[serde(default)]
    prerequisites: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawPolicy {
    id: String,
    required_age: Option<String>,
    #[serde(default)]
    required_civic: Option<String>,
    #[serde(default)]
    science_bonus: i16,
    #[serde(default)]
    culture_bonus: i16,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawDistrict {
    id: String,
    production_cost: u16,
    #[serde(default)]
    housing_capacity: u16,
    #[serde(default)]
    production_bonus: i16,
    #[serde(default)]
    adjacency_production: u16,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawBuilding {
    id: String,
    district: String,
    production_cost: u16,
    #[serde(default)]
    housing: u16,
    #[serde(default)]
    maintenance: u16,
    #[serde(default)]
    food_bonus: i16,
    #[serde(default)]
    production_bonus: i16,
    #[serde(default)]
    science_bonus: i16,
    #[serde(default)]
    culture_bonus: i16,
}

fn default_production_cost() -> u16 {
    1
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

fn parse_progression_table<T, F>(
    kind: &'static str,
    raw_entries: Vec<RawProgression>,
    convert: F,
) -> Result<BTreeMap<String, T>, RulesetError>
where
    F: Fn(RawProgression) -> T,
{
    let mut ids = BTreeSet::new();
    for raw in &raw_entries {
        validate_id(kind, &raw.id)?;
        if raw.cost == 0 {
            return Err(RulesetError::InvalidValue {
                field: "progression.cost",
                value: raw.id.clone(),
            });
        }
        if !ids.insert(raw.id.clone()) {
            return Err(RulesetError::DuplicateId {
                kind,
                id: raw.id.clone(),
            });
        }
    }
    for raw in &raw_entries {
        for prerequisite in &raw.prerequisites {
            validate_id(kind, prerequisite)?;
            if prerequisite == &raw.id || !ids.contains(prerequisite) {
                return Err(RulesetError::InvalidValue {
                    field: "progression.prerequisites",
                    value: prerequisite.clone(),
                });
            }
        }
    }
    let mut indegree = raw_entries
        .iter()
        .map(|raw| (raw.id.clone(), raw.prerequisites.len()))
        .collect::<BTreeMap<_, _>>();
    let mut children: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for raw in &raw_entries {
        for prerequisite in &raw.prerequisites {
            children
                .entry(prerequisite.clone())
                .or_default()
                .push(raw.id.clone());
        }
    }
    let mut ready: BTreeSet<String> = indegree
        .iter()
        .filter(|(_, degree)| **degree == 0)
        .map(|(id, _)| id.clone())
        .collect();
    let mut visited = 0;
    while let Some(id) = ready.pop_first() {
        visited += 1;
        for child in children.get(&id).into_iter().flatten() {
            let degree = indegree.get_mut(child).expect("progression child exists");
            *degree -= 1;
            if *degree == 0 {
                ready.insert(child.clone());
            }
        }
    }
    if visited != indegree.len() {
        return Err(RulesetError::InvalidValue {
            field: "progression.prerequisites",
            value: kind.to_owned(),
        });
    }
    Ok(raw_entries
        .into_iter()
        .map(|raw| (raw.id.clone(), convert(raw)))
        .collect())
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
        assert_eq!(ruleset.unit("warrior").unwrap().production_cost(), 6);
        assert_eq!(ruleset.technologies().count(), 25);
        assert_eq!(ruleset.civics().count(), 25);
        assert_eq!(ruleset.policies().count(), 7);
        assert_eq!(ruleset.technology("tech_25").unwrap().cost(), 58);
        assert_eq!(
            ruleset.civic("civic_02").unwrap().prerequisites(),
            &["civic_01".to_owned()]
        );
        assert_eq!(ruleset.districts().count(), 4);
        assert_eq!(ruleset.district("campus").unwrap().production_cost(), 80);
        assert_eq!(
            ruleset.district("city_center").unwrap().housing_capacity(),
            2
        );
        assert_eq!(
            ruleset
                .district("industrial_zone")
                .unwrap()
                .production_bonus(),
            1
        );
        assert_eq!(ruleset.buildings().count(), 12);
        let granary = ruleset.building("granary").unwrap();
        assert_eq!(granary.district(), "city_center");
        assert_eq!(granary.housing(), 2);
        assert_eq!(granary.maintenance(), 1);
        assert_eq!(granary.food_bonus(), 1);
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

        let missing_district = "schema_version=1\nid=\"base\"\nname=\"Base\"\nprovenance=\"designed\"\n[[districts]]\nid=\"campus\"\nproduction_cost=1\n[[buildings]]\nid=\"library\"\ndistrict=\"missing\"\nproduction_cost=1\n";
        assert!(matches!(
            Ruleset::from_toml(missing_district),
            Err(RulesetError::InvalidValue {
                field: "building.district",
                ..
            })
        ));

        let zero_cost = "schema_version=1\nid=\"base\"\nname=\"Base\"\nprovenance=\"designed\"\n[[districts]]\nid=\"campus\"\nproduction_cost=0\n";
        assert!(matches!(
            Ruleset::from_toml(zero_cost),
            Err(RulesetError::InvalidValue {
                field: "district.production_cost",
                ..
            })
        ));

        let zero_unit_cost = "schema_version=1\nid=\"base\"\nname=\"Base\"\nprovenance=\"designed\"\n[[units]]\nid=\"warrior\"\nmovement=2\nproduction_cost=0\n";
        assert!(matches!(
            Ruleset::from_toml(zero_unit_cost),
            Err(RulesetError::InvalidValue {
                field: "unit.production_cost",
                ..
            })
        ));

        let invalid_district_id = "schema_version=1\nid=\"base\"\nname=\"Base\"\nprovenance=\"designed\"\n[[districts]]\nid=\"Campus\"\nproduction_cost=1\n";
        assert!(matches!(
            Ruleset::from_toml(invalid_district_id),
            Err(RulesetError::InvalidId {
                kind: "district",
                ..
            })
        ));

        let unknown = "schema_version=1\nid=\"base\"\nname=\"Base\"\nprovenance=\"designed\"\n[[buildings]]\nid=\"library\"\ndistrict=\"campus\"\nproduction_cost=1\nextra=true\n";
        assert!(matches!(
            Ruleset::from_toml(unknown),
            Err(RulesetError::Parse(_))
        ));

        let cycle = "schema_version=1\nid=\"base\"\nname=\"Base\"\nprovenance=\"designed\"\n[[technologies]]\nid=\"a\"\ncost=1\nprerequisites=[\"b\"]\n[[technologies]]\nid=\"b\"\ncost=1\nprerequisites=[\"a\"]\n";
        assert!(matches!(
            Ruleset::from_toml(cycle),
            Err(RulesetError::InvalidValue {
                field: "progression.prerequisites",
                ..
            })
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
