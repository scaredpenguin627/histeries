//! Deterministic city, district, building, and production data.

use std::collections::{BTreeMap, BTreeSet};

use histeries_hex::{TileId, TileTopology};
use histeries_rules_core::CityId;
pub(crate) use histeries_ruleset::base_ruleset;

use crate::unit::UnitKind;

/// District categories available to Phase 1 cities.
#[derive(
    Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
#[repr(u8)]
pub enum DistrictKind {
    /// The established city center.
    CityCenter = 0,
    /// Research and scientific work.
    Campus = 1,
    /// Manufacturing and engineering.
    IndustrialZone = 2,
    /// Coastal commerce and shipping.
    Harbor = 3,
}

impl DistrictKind {
    pub(crate) const fn from_u8(value: u8) -> Option<Self> {
        Some(match value {
            0 => Self::CityCenter,
            1 => Self::Campus,
            2 => Self::IndustrialZone,
            3 => Self::Harbor,
            _ => return None,
        })
    }

    /// Returns all district kinds in canonical order.
    pub const ALL: [Self; 4] = [
        Self::CityCenter,
        Self::Campus,
        Self::IndustrialZone,
        Self::Harbor,
    ];

    /// Returns the stable ruleset identifier.
    pub const fn ruleset_id(self) -> &'static str {
        match self {
            Self::CityCenter => "city_center",
            Self::Campus => "campus",
            Self::IndustrialZone => "industrial_zone",
            Self::Harbor => "harbor",
        }
    }

    pub(crate) fn from_ruleset_id(id: &str) -> Option<Self> {
        Some(match id {
            "city_center" => Self::CityCenter,
            "campus" => Self::Campus,
            "industrial_zone" => Self::IndustrialZone,
            "harbor" => Self::Harbor,
            _ => return None,
        })
    }

    /// Returns the data-defined production cost for this district.
    pub fn production_cost(self) -> i64 {
        base_ruleset()
            .district(self.ruleset_id())
            .map_or(i64::MAX, |district| i64::from(district.production_cost()))
    }

    pub(crate) fn production_bonus(self) -> i64 {
        base_ruleset()
            .district(self.ruleset_id())
            .map_or(0, |district| i64::from(district.production_bonus()))
    }
}

/// Building categories available to Phase 1 cities.
#[derive(
    Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
#[repr(u8)]
pub enum BuildingKind {
    /// Adds housing and food support.
    Granary = 0,
    /// Adds civic output.
    Monument = 1,
    /// Campus research building.
    Library = 2,
    /// Advanced campus research building.
    University = 3,
    /// Market district commerce building.
    Market = 4,
    /// Industrial military equipment building.
    Armory = 5,
    /// Industrial production building.
    Workshop = 6,
    /// Advanced industrial production building.
    Factory = 7,
    /// Military training building.
    Barracks = 8,
    /// Harbor navigation building.
    Lighthouse = 9,
    /// Harbor commerce building.
    Shipyard = 10,
    /// Harbor logistics building.
    Warehouse = 11,
}

impl BuildingKind {
    pub(crate) const fn from_u8(value: u8) -> Option<Self> {
        Some(match value {
            0 => Self::Granary,
            1 => Self::Monument,
            2 => Self::Library,
            3 => Self::University,
            4 => Self::Market,
            5 => Self::Armory,
            6 => Self::Workshop,
            7 => Self::Factory,
            8 => Self::Barracks,
            9 => Self::Lighthouse,
            10 => Self::Shipyard,
            11 => Self::Warehouse,
            _ => return None,
        })
    }

    /// Returns all building kinds in canonical order.
    pub const ALL: [Self; 12] = [
        Self::Granary,
        Self::Monument,
        Self::Library,
        Self::University,
        Self::Market,
        Self::Armory,
        Self::Workshop,
        Self::Factory,
        Self::Barracks,
        Self::Lighthouse,
        Self::Shipyard,
        Self::Warehouse,
    ];

    /// Returns the stable ruleset identifier.
    pub const fn ruleset_id(self) -> &'static str {
        match self {
            Self::Granary => "granary",
            Self::Monument => "monument",
            Self::Library => "library",
            Self::University => "university",
            Self::Market => "market",
            Self::Armory => "armory",
            Self::Workshop => "workshop",
            Self::Factory => "factory",
            Self::Barracks => "barracks",
            Self::Lighthouse => "lighthouse",
            Self::Shipyard => "shipyard",
            Self::Warehouse => "warehouse",
        }
    }

    /// Returns the data-defined district required by this building.
    pub fn required_district(self) -> Option<DistrictKind> {
        base_ruleset()
            .building(self.ruleset_id())
            .and_then(|building| DistrictKind::from_ruleset_id(building.district()))
    }

    /// Returns the production cost of this building.
    pub fn production_cost(self) -> i64 {
        base_ruleset()
            .building(self.ruleset_id())
            .map_or(i64::MAX, |building| i64::from(building.production_cost()))
    }

    pub(crate) fn housing_bonus(self) -> u32 {
        base_ruleset()
            .building(self.ruleset_id())
            .map_or(0, |building| u32::from(building.housing()))
    }

    pub(crate) fn food_bonus(self) -> i64 {
        base_ruleset()
            .building(self.ruleset_id())
            .map_or(0, |building| i64::from(building.food_bonus()))
    }

    pub(crate) fn production_bonus(self) -> i64 {
        base_ruleset()
            .building(self.ruleset_id())
            .map_or(0, |building| i64::from(building.production_bonus()))
    }
}

/// A city production order.
#[derive(
    Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum ProductionItem {
    /// Produce a unit.
    Unit(UnitKind),
    /// Construct a building.
    Building(BuildingKind),
    /// Construct a district at a selected tile.
    District {
        /// District category.
        kind: DistrictKind,
        /// District tile.
        tile: TileId,
    },
}

impl ProductionItem {
    /// Returns the fixed integer production cost.
    pub fn cost(self) -> i64 {
        match self {
            Self::Unit(kind) => kind.production_cost(),
            Self::Building(kind) => kind.production_cost(),
            Self::District { kind, .. } => kind.production_cost(),
        }
    }

    /// Returns whether the order can be produced by a player city.
    pub const fn is_valid(self) -> bool {
        !matches!(self, Self::Unit(UnitKind::Barbarian))
    }
}

/// A city district placed on one map tile.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize)]
pub struct District {
    /// District category.
    pub kind: DistrictKind,
    /// Occupied tile.
    pub tile: TileId,
}

/// Maximum number of active and waiting production orders in one city.
pub const MAX_PRODUCTION_ORDERS: usize = 8;

/// Canonical city state owned by one player.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct City {
    pub(crate) id: CityId,
    pub(crate) owner: histeries_rules_core::PlayerId,
    pub(crate) tile: TileId,
    pub(crate) population: u32,
    pub(crate) food_stored: i64,
    pub(crate) housing: u32,
    pub(crate) production_stored: i64,
    pub(crate) production: Option<ProductionItem>,
    pub(crate) waiting: Vec<ProductionItem>,
    pub(crate) districts: BTreeMap<TileId, DistrictKind>,
    pub(crate) buildings: BTreeSet<BuildingKind>,
}

impl City {
    /// Creates a size-one city with the base housing capacity.
    pub fn new(id: CityId, owner: histeries_rules_core::PlayerId, tile: TileId) -> Self {
        Self {
            id,
            owner,
            tile,
            population: 1,
            food_stored: 0,
            housing: 2,
            production_stored: 0,
            production: None,
            waiting: Vec::new(),
            districts: BTreeMap::new(),
            buildings: BTreeSet::new(),
        }
    }

    /// Returns the population at which the next growth point is reached.
    pub const fn growth_threshold(&self) -> i64 {
        self.population as i64 * 10
    }

    /// Returns the effective housing cap including building bonuses.
    pub fn housing_capacity(&self) -> u32 {
        self.housing
            + self
                .buildings
                .iter()
                .map(|building| building.housing_bonus())
                .sum::<u32>()
    }

    /// Returns the city's deterministic food output for one turn.
    pub fn food_yield(&self) -> i64 {
        i64::from(self.population).saturating_mul(2).saturating_add(
            self.buildings
                .iter()
                .map(|building| building.food_bonus())
                .sum::<i64>(),
        )
    }

    /// Returns production before district adjacency is applied.
    pub fn production_yield(&self) -> i64 {
        i64::from(self.population)
            .saturating_add(
                self.districts
                    .values()
                    .map(|kind| kind.production_bonus())
                    .sum(),
            )
            .saturating_add(
                self.buildings
                    .iter()
                    .map(|building| building.production_bonus())
                    .sum::<i64>(),
            )
    }

    /// Returns the city's deterministic science output for one turn.
    pub fn science_yield(&self) -> i64 {
        i64::from(self.population).saturating_add(
            self.buildings
                .iter()
                .map(|building| {
                    i64::from(
                        base_ruleset()
                            .building(building.ruleset_id())
                            .map_or(0, |definition| definition.science_bonus()),
                    )
                })
                .sum::<i64>(),
        )
    }

    /// Returns the city's deterministic culture output for one turn.
    pub fn culture_yield(&self) -> i64 {
        i64::from(self.population).saturating_add(
            self.buildings
                .iter()
                .map(|building| {
                    i64::from(
                        base_ruleset()
                            .building(building.ruleset_id())
                            .map_or(0, |definition| definition.culture_bonus()),
                    )
                })
                .sum::<i64>(),
        )
    }

    /// Returns the city production order, if one is queued.
    pub const fn production(&self) -> Option<ProductionItem> {
        self.production
    }

    /// Returns whether this city has a district of the requested kind.
    pub fn has_district(&self, kind: DistrictKind) -> bool {
        kind == DistrictKind::CityCenter
            || self.districts.values().any(|district| *district == kind)
    }

    /// Returns whether this city has a building of the requested kind.
    pub fn has_building(&self, kind: BuildingKind) -> bool {
        self.buildings.contains(&kind)
    }

    /// Returns districts in canonical tile order.
    pub fn districts(&self) -> impl Iterator<Item = District> + '_ {
        self.districts.iter().map(|(tile, kind)| District {
            kind: *kind,
            tile: *tile,
        })
    }

    /// Returns buildings in canonical enum order.
    pub fn buildings(&self) -> impl Iterator<Item = BuildingKind> + '_ {
        self.buildings.iter().copied()
    }

    /// Returns the number of adjacent districts at a candidate tile.
    pub fn adjacency_bonus<T: TileTopology>(&self, topology: &T, tile: TileId) -> u32 {
        let neighbors = topology.neighbors(tile);
        let center_bonus = u32::from(neighbors.contains(&self.tile));
        center_bonus
            + neighbors
                .into_iter()
                .filter(|neighbor| *neighbor != tile && self.districts.contains_key(neighbor))
                .count() as u32
    }

    pub(crate) fn orders(&self) -> impl Iterator<Item = ProductionItem> + Clone + '_ {
        self.production
            .into_iter()
            .chain(self.waiting.iter().copied())
    }

    pub(crate) fn validate_next_order(
        &self,
        item: ProductionItem,
        mut earlier: impl Iterator<Item = ProductionItem> + Clone,
    ) -> Result<(), crate::DispatchError> {
        use crate::DispatchError;
        match item {
            ProductionItem::Unit(_) if !item.is_valid() => Err(DispatchError::InvalidProduction),
            ProductionItem::Unit(_) => Ok(()),
            ProductionItem::Building(building) => {
                if self.has_building(building) || earlier.clone().any(|prior| prior == item) {
                    return Err(DispatchError::BuildingAlreadyBuilt(building));
                }
                if let Some(required) = building.required_district()
                    && !self.has_district(required)
                    && !earlier.any(|prior| matches!(prior, ProductionItem::District { kind, .. } if kind == required))
                {
                    return Err(DispatchError::MissingDistrict(required));
                }
                Ok(())
            }
            ProductionItem::District { kind, tile } => {
                if self.has_district(kind)
                    || self.districts.contains_key(&tile)
                    || earlier.any(|prior| matches!(prior, ProductionItem::District {kind: k, tile: t} if k == kind || t == tile))
                {
                    return Err(DispatchError::Occupied(tile));
                }
                Ok(())
            }
        }
    }

    pub(crate) fn adjacency_production(&self, topology: &impl TileTopology) -> i64 {
        self.districts
            .iter()
            .map(|(&tile, kind)| {
                let per_edge = base_ruleset()
                    .district(kind.ruleset_id())
                    .map_or(0, |rule| rule.adjacency_production());
                i64::from(per_edge) * i64::from(self.adjacency_bonus(topology, tile))
            })
            .sum()
    }

    pub(crate) fn queue(&mut self, item: ProductionItem) {
        self.production = Some(item);
        self.waiting = Vec::new();
        self.production_stored = 0;
    }

    pub(crate) fn append_waiting(&mut self, item: ProductionItem) {
        // Keep heap usage identical to a freshly loaded queue of the same length.
        self.waiting.reserve_exact(1);
        self.waiting.push(item);
    }

    pub(crate) fn start_next_order(&mut self) {
        if !self.waiting.is_empty() {
            self.production = Some(self.waiting.remove(0));
            self.waiting.shrink_to_fit();
        }
    }

    pub(crate) fn advance_turn(&mut self, adjacency: i64) -> Option<ProductionItem> {
        self.food_stored = self.food_stored.saturating_add(self.food_yield());
        if self.population < self.housing_capacity() && self.food_stored >= self.growth_threshold()
        {
            self.food_stored -= self.growth_threshold();
            self.population = self.population.saturating_add(1);
        }
        self.production_stored = self
            .production_stored
            .saturating_add(self.production_yield())
            .saturating_add(adjacency);
        let item = self.production?;
        if self.production_stored < item.cost() {
            return None;
        }
        self.production_stored -= item.cost();
        self.production = None;
        Some(item)
    }

    pub(crate) fn restore_order(&mut self, item: ProductionItem) {
        self.production = Some(item);
        self.production_stored = self.production_stored.saturating_add(item.cost());
    }

    pub(crate) fn insert_district(&mut self, kind: DistrictKind, tile: TileId) {
        self.districts.insert(tile, kind);
    }

    pub(crate) fn insert_building(&mut self, kind: BuildingKind) {
        self.buildings.insert(kind);
    }
}

#[cfg(test)]
mod tests {
    use histeries_hex::{FlatHex, TileId};
    use histeries_rules_core::{CityId, PlayerId};

    use super::{BuildingKind, City, DistrictKind};

    #[test]
    fn growth_housing_and_adjacency_are_deterministic() {
        let topology = FlatHex::new(5, 5, false, false).unwrap();
        let mut city = City::new(CityId::new(0), PlayerId::new(1), TileId::new(0));
        assert_eq!(city.growth_threshold(), 10);
        assert_eq!(city.housing_capacity(), 2);
        city.insert_district(DistrictKind::Campus, TileId::new(1));
        assert_eq!(city.adjacency_bonus(&topology, TileId::new(1)), 1);
        assert_eq!(city.adjacency_bonus(&topology, TileId::new(2)), 1);
        city.insert_building(BuildingKind::Granary);
        assert_eq!(city.housing_capacity(), 4);
        assert_eq!(city.food_yield(), 3);
    }
}
