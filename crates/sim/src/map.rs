//! Deterministic map data and Phase 1 map generation.

use std::{collections::BTreeMap, fmt};

use histeries_hex::{FlatHex, TileId, TileTopology, TopologyError};
use histeries_rules_core::NamedRng;

const MAP_MAGIC: &[u8; 4] = b"MAP1";
const MAP_VERSION: u16 = 1;

/// Terrain categories used by the base map generator.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
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
    pub(crate) const fn from_u8(value: u8) -> Option<Self> {
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
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
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
    pub(crate) const fn from_u8(value: u8) -> Option<Self> {
        Some(match value {
            0 => Self::None,
            1 => Self::Forest,
            2 => Self::Jungle,
            3 => Self::Marsh,
            _ => return None,
        })
    }
}

/// One of the six topology edge directions used for a river segment.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[repr(u8)]
pub enum RiverDirection {
    /// First edge direction.
    NorthEast = 0,
    /// Second edge direction.
    East = 1,
    /// Third edge direction.
    SouthEast = 2,
    /// Fourth edge direction.
    SouthWest = 3,
    /// Fifth edge direction.
    West = 4,
    /// Sixth edge direction.
    NorthWest = 5,
}

impl RiverDirection {
    pub(crate) const fn from_u8(value: u8) -> Option<Self> {
        Some(match value {
            0 => Self::NorthEast,
            1 => Self::East,
            2 => Self::SouthEast,
            3 => Self::SouthWest,
            4 => Self::West,
            5 => Self::NorthWest,
            _ => return None,
        })
    }
}

/// Resource categories placed on a tile.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
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
    pub(crate) const fn from_u8(value: u8) -> Option<Self> {
        Some(match value {
            0 => Self::Wheat,
            1 => Self::Stone,
            2 => Self::Iron,
            3 => Self::Gold,
            _ => return None,
        })
    }
}

/// Data stored for one tile in a generated or edited map.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TileState {
    /// Terrain category.
    pub terrain: Terrain,
    /// Feature category.
    pub feature: Feature,
    /// Optional river edge.
    pub river: Option<RiverDirection>,
    /// Optional resource.
    pub resource: Option<ResourceKind>,
}

impl Default for TileState {
    fn default() -> Self {
        Self {
            terrain: Terrain::Plains,
            feature: Feature::None,
            river: None,
            resource: None,
        }
    }
}

/// A deterministic map attached to a simulation state.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MapState {
    topology: FlatHex,
    tiles: BTreeMap<TileId, TileState>,
}

impl MapState {
    /// Creates a blank map with all tiles set to plains.
    pub fn blank(topology: FlatHex) -> Self {
        let tiles = (0..topology.tile_count())
            .map(|index| (TileId::new(index as u32), TileState::default()))
            .collect();
        Self { topology, tiles }
    }

    /// Returns the topology.
    pub const fn topology(&self) -> FlatHex {
        self.topology
    }

    /// Returns a tile in stable ID order.
    pub fn tile(&self, tile: TileId) -> Option<TileState> {
        self.tiles.get(&tile).copied()
    }

    /// Returns all tile data in ascending ID order.
    pub fn tiles(&self) -> impl Iterator<Item = (TileId, TileState)> + '_ {
        self.tiles.iter().map(|(tile, state)| (*tile, *state))
    }

    /// Changes one tile's terrain and clears incompatible water features/resources.
    pub fn set_terrain(&mut self, tile: TileId, terrain: Terrain) -> Result<(), MapError> {
        let state = self
            .tiles
            .get_mut(&tile)
            .ok_or(MapError::UnknownTile(tile))?;
        state.terrain = terrain;
        if matches!(terrain, Terrain::Coast | Terrain::Ocean | Terrain::Mountain) {
            state.feature = Feature::None;
            state.river = None;
            state.resource = None;
        }
        Ok(())
    }

    /// Changes one tile's feature after validating terrain compatibility.
    pub fn set_feature(&mut self, tile: TileId, feature: Feature) -> Result<(), MapError> {
        let state = self
            .tiles
            .get_mut(&tile)
            .ok_or(MapError::UnknownTile(tile))?;
        if !matches!(
            state.terrain,
            Terrain::Plains
                | Terrain::Grassland
                | Terrain::Desert
                | Terrain::Tundra
                | Terrain::Hill
        ) && feature != Feature::None
        {
            return Err(MapError::IncompatibleFeature { tile, feature });
        }
        state.feature = feature;
        Ok(())
    }

    /// Changes one tile's river edge.
    pub fn set_river(
        &mut self,
        tile: TileId,
        river: Option<RiverDirection>,
    ) -> Result<(), MapError> {
        let state = self
            .tiles
            .get_mut(&tile)
            .ok_or(MapError::UnknownTile(tile))?;
        if matches!(
            state.terrain,
            Terrain::Coast | Terrain::Ocean | Terrain::Mountain
        ) && river.is_some()
        {
            return Err(MapError::IncompatibleRiver(tile));
        }
        state.river = river;
        Ok(())
    }

    /// Changes one tile's resource.
    pub fn set_resource(
        &mut self,
        tile: TileId,
        resource: Option<ResourceKind>,
    ) -> Result<(), MapError> {
        let state = self
            .tiles
            .get_mut(&tile)
            .ok_or(MapError::UnknownTile(tile))?;
        if matches!(
            state.terrain,
            Terrain::Coast | Terrain::Ocean | Terrain::Mountain
        ) && resource.is_some()
        {
            return Err(MapError::IncompatibleResource(tile));
        }
        state.resource = resource;
        Ok(())
    }

    /// Encodes map data with strict canonical tile ordering.
    pub fn encode(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(24 + self.tiles.len() * 8);
        bytes.extend_from_slice(MAP_MAGIC);
        bytes.extend_from_slice(&MAP_VERSION.to_le_bytes());
        bytes.extend_from_slice(&self.topology.width().to_le_bytes());
        bytes.extend_from_slice(&self.topology.height().to_le_bytes());
        bytes.push(self.topology.wraps_x() as u8);
        bytes.push(self.topology.wraps_y() as u8);
        bytes.extend_from_slice(&(self.tiles.len() as u32).to_le_bytes());
        for (tile, state) in &self.tiles {
            bytes.extend_from_slice(&tile.index().to_le_bytes());
            bytes.push(state.terrain as u8);
            bytes.push(state.feature as u8);
            bytes.push(state.river.map_or(255, |river| river as u8));
            bytes.push(state.resource.map_or(255, |resource| resource as u8));
        }
        bytes
    }

    /// Decodes a map and validates topology dimensions, IDs, tags, and ordering.
    pub fn decode(bytes: &[u8]) -> Result<Self, MapError> {
        let mut reader = MapReader::new(bytes);
        if reader.take(4)? != MAP_MAGIC {
            return Err(MapError::InvalidFormat);
        }
        if reader.u16()? != MAP_VERSION {
            return Err(MapError::UnsupportedVersion);
        }
        let width = reader.u32()?;
        let height = reader.u32()?;
        let wrap_x = match reader.u8()? {
            0 => false,
            1 => true,
            _ => return Err(MapError::InvalidFormat),
        };
        let wrap_y = match reader.u8()? {
            0 => false,
            1 => true,
            _ => return Err(MapError::InvalidFormat),
        };
        let topology = FlatHex::new(width, height, wrap_x, wrap_y).map_err(MapError::Topology)?;
        let count = reader.u32()? as usize;
        if count != topology.tile_count() {
            return Err(MapError::InvalidTileCount);
        }
        let mut tiles = BTreeMap::new();
        let mut previous = None;
        for _ in 0..count {
            let tile = TileId::new(reader.u32()?);
            if !topology.contains(tile) {
                return Err(MapError::UnknownTile(tile));
            }
            if previous.is_some_and(|previous| tile <= previous) {
                return Err(MapError::NonCanonicalOrder);
            }
            previous = Some(tile);
            let state = TileState {
                terrain: Terrain::from_u8(reader.u8()?).ok_or(MapError::InvalidTag)?,
                feature: Feature::from_u8(reader.u8()?).ok_or(MapError::InvalidTag)?,
                river: match reader.u8()? {
                    255 => None,
                    value => Some(RiverDirection::from_u8(value).ok_or(MapError::InvalidTag)?),
                },
                resource: match reader.u8()? {
                    255 => None,
                    value => Some(ResourceKind::from_u8(value).ok_or(MapError::InvalidTag)?),
                },
            };
            validate_tile_state(tile, state)?;
            tiles.insert(tile, state);
        }
        if reader.remaining() != 0 {
            return Err(MapError::TrailingBytes);
        }
        Ok(Self { topology, tiles })
    }
}

fn validate_tile_state(tile: TileId, state: TileState) -> Result<(), MapError> {
    if !matches!(
        state.terrain,
        Terrain::Plains | Terrain::Grassland | Terrain::Desert | Terrain::Tundra | Terrain::Hill
    ) && state.feature != Feature::None
    {
        return Err(MapError::IncompatibleFeature {
            tile,
            feature: state.feature,
        });
    }
    if matches!(
        state.terrain,
        Terrain::Coast | Terrain::Ocean | Terrain::Mountain
    ) && state.river.is_some()
    {
        return Err(MapError::IncompatibleRiver(tile));
    }
    if matches!(
        state.terrain,
        Terrain::Coast | Terrain::Ocean | Terrain::Mountain
    ) && state.resource.is_some()
    {
        return Err(MapError::IncompatibleResource(tile));
    }
    Ok(())
}

/// Generates a deterministic map from a topology and master seed.
pub fn generate_map(topology: FlatHex, seed: [u8; 32]) -> MapState {
    let mut rng = NamedRng::new(seed, "mapgen");
    let mut map = MapState::blank(topology);
    for tile in (0..topology.tile_count()).map(|index| TileId::new(index as u32)) {
        let roll = rng.next_bounded(100).unwrap_or(0);
        let terrain = match roll {
            0..=9 => Terrain::Ocean,
            10..=15 => Terrain::Coast,
            16..=45 => Terrain::Plains,
            46..=70 => Terrain::Grassland,
            71..=82 => Terrain::Desert,
            83..=92 => Terrain::Tundra,
            93..=97 => Terrain::Hill,
            _ => Terrain::Mountain,
        };
        map.set_terrain(tile, terrain)
            .expect("generated tile exists");
        if matches!(
            terrain,
            Terrain::Plains
                | Terrain::Grassland
                | Terrain::Desert
                | Terrain::Tundra
                | Terrain::Hill
        ) {
            let feature_roll = rng.next_bounded(100).unwrap_or(0);
            let feature = match feature_roll {
                0..=9 => Feature::Forest,
                10..=13 => Feature::Jungle,
                14..=17 => Feature::Marsh,
                _ => Feature::None,
            };
            map.set_feature(tile, feature)
                .expect("generated feature is compatible");
            if rng.next_bounded(100).unwrap_or(0) < 10 {
                let resource = match rng.next_bounded(4).unwrap_or(0) {
                    0 => ResourceKind::Wheat,
                    1 => ResourceKind::Stone,
                    2 => ResourceKind::Iron,
                    _ => ResourceKind::Gold,
                };
                map.set_resource(tile, Some(resource))
                    .expect("generated resource is compatible");
            }
            if rng.next_bounded(100).unwrap_or(0) < 8 {
                let river = match rng.next_bounded(6).unwrap_or(0) {
                    0 => RiverDirection::NorthEast,
                    1 => RiverDirection::East,
                    2 => RiverDirection::SouthEast,
                    3 => RiverDirection::SouthWest,
                    4 => RiverDirection::West,
                    _ => RiverDirection::NorthWest,
                };
                map.set_river(tile, Some(river))
                    .expect("generated river is compatible");
            }
        }
    }
    map
}

/// Map editing and serialization errors.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MapError {
    /// A tile ID was absent.
    UnknownTile(TileId),
    /// A feature was incompatible with the current terrain.
    IncompatibleFeature {
        /// The tile with the incompatible feature.
        tile: TileId,
        /// The rejected feature.
        feature: Feature,
    },
    /// A river was incompatible with the current terrain.
    IncompatibleRiver(TileId),
    /// A resource was incompatible with the current terrain.
    IncompatibleResource(TileId),
    /// The map topology could not be constructed.
    Topology(TopologyError),
    /// Binary data had invalid magic or structure.
    InvalidFormat,
    /// Binary data used an unsupported version.
    UnsupportedVersion,
    /// Binary data used an invalid tag.
    InvalidTag,
    /// Binary data contained an invalid tile count.
    InvalidTileCount,
    /// Binary data was not in ascending tile order.
    NonCanonicalOrder,
    /// Binary data had trailing bytes.
    TrailingBytes,
}

impl fmt::Display for MapError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownTile(tile) => write!(formatter, "unknown tile {}", tile.index()),
            Self::IncompatibleFeature { tile, .. } => {
                write!(formatter, "incompatible feature at tile {}", tile.index())
            }
            Self::IncompatibleRiver(tile) => {
                write!(formatter, "incompatible river at tile {}", tile.index())
            }
            Self::IncompatibleResource(tile) => {
                write!(formatter, "incompatible resource at tile {}", tile.index())
            }
            Self::Topology(error) => write!(formatter, "invalid topology: {error}"),
            Self::InvalidFormat => formatter.write_str("invalid map format"),
            Self::UnsupportedVersion => formatter.write_str("unsupported map version"),
            Self::InvalidTag => formatter.write_str("invalid map tag"),
            Self::InvalidTileCount => formatter.write_str("invalid map tile count"),
            Self::NonCanonicalOrder => formatter.write_str("non-canonical map tile order"),
            Self::TrailingBytes => formatter.write_str("trailing map bytes"),
        }
    }
}

impl std::error::Error for MapError {}

struct MapReader<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> MapReader<'a> {
    const fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }

    fn take(&mut self, length: usize) -> Result<&'a [u8], MapError> {
        let end = self
            .offset
            .checked_add(length)
            .ok_or(MapError::InvalidFormat)?;
        let bytes = self
            .bytes
            .get(self.offset..end)
            .ok_or(MapError::InvalidFormat)?;
        self.offset = end;
        Ok(bytes)
    }

    fn u8(&mut self) -> Result<u8, MapError> {
        Ok(self.take(1)?[0])
    }

    fn u16(&mut self) -> Result<u16, MapError> {
        Ok(u16::from_le_bytes(
            self.take(2)?
                .try_into()
                .map_err(|_| MapError::InvalidFormat)?,
        ))
    }

    fn u32(&mut self) -> Result<u32, MapError> {
        Ok(u32::from_le_bytes(
            self.take(4)?
                .try_into()
                .map_err(|_| MapError::InvalidFormat)?,
        ))
    }

    fn remaining(&self) -> usize {
        self.bytes.len() - self.offset
    }
}

#[cfg(test)]
mod tests {
    use histeries_hex::{FlatHex, TileId, TileTopology};

    use super::{Feature, MapError, MapState, ResourceKind, Terrain, generate_map};

    #[test]
    fn generated_maps_are_reproducible_and_roundtrip() {
        let topology = FlatHex::new(8, 6, true, false).unwrap();
        let first = generate_map(topology, [9; 32]);
        let second = generate_map(topology, [9; 32]);
        assert_eq!(first, second);
        assert_eq!(MapState::decode(&first.encode()).unwrap(), first);
        assert_eq!(first.tiles().count(), topology.tile_count());
    }

    #[test]
    fn editor_mutations_validate_terrain_compatibility() {
        let topology = FlatHex::new(2, 2, false, false).unwrap();
        let mut map = MapState::blank(topology);
        let tile = TileId::new(0);
        map.set_terrain(tile, Terrain::Ocean).unwrap();
        assert!(matches!(
            map.set_feature(tile, Feature::Forest),
            Err(MapError::IncompatibleFeature { .. })
        ));
        assert!(matches!(
            map.set_resource(tile, Some(ResourceKind::Gold)),
            Err(MapError::IncompatibleResource(_))
        ));
        assert!(map.set_feature(tile, Feature::None).is_ok());
    }

    #[test]
    fn terrain_edits_clear_incompatible_rivers() {
        let topology = FlatHex::new(2, 2, false, false).unwrap();
        let mut map = MapState::blank(topology);
        let tile = TileId::new(0);
        map.set_river(tile, Some(super::RiverDirection::East))
            .unwrap();
        map.set_terrain(tile, Terrain::Ocean).unwrap();
        assert_eq!(map.tile(tile).unwrap().river, None);
    }

    #[test]
    fn map_binary_rejects_trailing_bytes() {
        let map = MapState::blank(FlatHex::new(2, 2, false, false).unwrap());
        let mut bytes = map.encode();
        bytes.push(0);
        assert_eq!(MapState::decode(&bytes), Err(MapError::TrailingBytes));

        let mut invalid_wrap = map.encode();
        invalid_wrap[14] = 2;
        assert_eq!(
            MapState::decode(&invalid_wrap),
            Err(MapError::InvalidFormat)
        );
    }
}
