//! Deterministic world-simulation state, commands, observations, events, and replay.

#![forbid(unsafe_code)]

use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
};

use histeries_hex::{TileId, TileTopology};
use histeries_rules_core::{CityId, PlayerId, UnitId};

pub mod city;
pub mod map;
pub mod progression;
pub mod unit;
pub use city::{BuildingKind, City, District, DistrictKind, ProductionItem};
pub use map::{Feature, MapError, MapState, ResourceKind, RiverDirection, Terrain, generate_map};
pub use progression::{PlayerProgress, VictoryKind};
pub use unit::{PromotionKind, Unit, UnitKind};

/// The versioned algorithm identifier for per-turn state hashes.
pub const STATE_HASH_ALGORITHM: &str = "fnv1a64-v1-little-endian-length-prefixed";
const STATE_MAGIC: &[u8; 4] = b"HST1";
const COMMAND_MAGIC: &[u8; 4] = b"HCM1";
const REPLAY_MAGIC: &[u8; 4] = b"HRP1";
const COMMAND_FORMAT_VERSION: u16 = 3;
const STATE_FORMAT_VERSION: u16 = 4;
const REPLAY_FORMAT_VERSION: u16 = 4;

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
        self.write_raw(bytes);
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

/// A state-changing command with a stable string ID.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Command {
    /// Advances the simulation after a seat has completed its actions.
    TurnEnd {
        /// The seat completing its turn.
        player: PlayerId,
    },
    /// A deterministic editor command used to set a seat marker.
    EditorSetMarker {
        /// The seat whose marker changes.
        player: PlayerId,
        /// The new marker value.
        value: i64,
    },
    /// Sets one tile's terrain through the editor command boundary.
    EditorSetTerrain {
        /// The seat issuing the editor command.
        player: PlayerId,
        /// The target tile ID.
        tile: TileId,
        /// The replacement terrain.
        terrain: Terrain,
    },
    /// Sets one tile's feature through the editor command boundary.
    EditorSetFeature {
        /// The seat issuing the editor command.
        player: PlayerId,
        /// The target tile ID.
        tile: TileId,
        /// The replacement feature.
        feature: Feature,
    },
    /// Sets or clears one tile's river edge.
    EditorSetRiver {
        /// The seat issuing the editor command.
        player: PlayerId,
        /// The target tile ID.
        tile: TileId,
        /// The replacement river edge.
        river: Option<RiverDirection>,
    },
    /// Sets or clears one tile's resource.
    EditorSetResource {
        /// The seat issuing the editor command.
        player: PlayerId,
        /// The target tile ID.
        tile: TileId,
        /// The replacement resource.
        resource: Option<ResourceKind>,
    },
    /// Moves one unit to an adjacent tile.
    UnitMove {
        /// The issuing seat.
        player: PlayerId,
        /// The unit to move.
        unit: UnitId,
        /// The destination tile.
        destination: TileId,
    },
    /// Builds a road under a unit.
    UnitBuildRoad {
        /// The issuing seat.
        player: PlayerId,
        /// The builder unit.
        unit: UnitId,
    },
    /// Attacks an enemy unit.
    UnitAttack {
        /// The issuing seat.
        player: PlayerId,
        /// The attacking unit.
        attacker: UnitId,
        /// The defending unit.
        defender: UnitId,
    },
    /// Heals a unit in a safe position.
    UnitHeal {
        /// The issuing seat.
        player: PlayerId,
        /// The unit to heal.
        unit: UnitId,
    },
    /// Holds the unit's tile and spends its remaining movement.
    UnitHold {
        /// The issuing seat.
        player: PlayerId,
        /// The unit receiving the order.
        unit: UnitId,
    },
    /// Applies a unit promotion.
    UnitPromote {
        /// The issuing seat.
        player: PlayerId,
        /// The unit to promote.
        unit: UnitId,
        /// The promotion choice.
        promotion: PromotionKind,
    },
    /// Spawns a player-owned unit for deterministic scenarios.
    UnitSpawn {
        /// The issuing seat.
        player: PlayerId,
        /// Stable unit ID.
        unit: UnitId,
        /// Unit archetype.
        kind: UnitKind,
        /// Target tile.
        tile: TileId,
    },
    /// Spawns a barbarian unit for deterministic scenarios.
    BarbarianSpawn {
        /// The target tile.
        tile: TileId,
        /// The barbarian archetype.
        kind: UnitKind,
    },
    /// Founds a city using an owned settler.
    CityFound {
        /// The founding seat.
        player: PlayerId,
        /// Stable city ID.
        city: CityId,
        /// Settler unit to consume.
        unit: UnitId,
    },
    /// Founds a city using an ID allocated during dispatch.
    CitySettle {
        /// The founding seat.
        player: PlayerId,
        /// Owned settler on the proposed city site.
        unit: UnitId,
    },
    /// Queues one city production order.
    CitySetProduction {
        /// The issuing seat.
        player: PlayerId,
        /// Target city.
        city: CityId,
        /// Production order.
        item: ProductionItem,
    },
    /// Queues a district at a selected tile after adjacency validation.
    CityBuildDistrict {
        /// The issuing seat.
        player: PlayerId,
        /// Target city.
        city: CityId,
        /// District category.
        kind: DistrictKind,
        /// Target tile.
        tile: TileId,
    },
    /// Queues a building after prerequisite validation.
    CityBuildBuilding {
        /// The issuing seat.
        player: PlayerId,
        /// Target city.
        city: CityId,
        /// Building category.
        building: BuildingKind,
    },
    /// Selects a technology for the issuing seat's research progress.
    ResearchTechnology {
        /// The issuing seat.
        player: PlayerId,
        /// Stable technology identifier.
        technology: String,
    },
    /// Selects a civic for the issuing seat's culture progress.
    ResearchCivic {
        /// The issuing seat.
        player: PlayerId,
        /// Stable civic identifier.
        civic: String,
    },
    /// Adopts or replaces a policy in one slot.
    PolicyAdopt {
        /// The issuing seat.
        player: PlayerId,
        /// Zero-based policy slot.
        slot: u8,
        /// Stable policy identifier.
        policy: String,
    },
    /// Clears one policy slot.
    PolicyRemove {
        /// The issuing seat.
        player: PlayerId,
        /// Zero-based policy slot.
        slot: u8,
    },
}

impl Command {
    /// Returns the stable registry ID for this command.
    pub const fn id(&self) -> &'static str {
        match self {
            Self::TurnEnd { .. } => "turn.end",
            Self::EditorSetMarker { .. } => "editor.set_marker",
            Self::EditorSetTerrain { .. } => "editor.set_terrain",
            Self::EditorSetFeature { .. } => "editor.set_feature",
            Self::EditorSetRiver { .. } => "editor.set_river",
            Self::EditorSetResource { .. } => "editor.set_resource",
            Self::UnitMove { .. } => "unit.move",
            Self::UnitBuildRoad { .. } => "unit.build_road",
            Self::UnitAttack { .. } => "unit.attack",
            Self::UnitHeal { .. } => "unit.heal",
            Self::UnitHold { .. } => "unit.hold",
            Self::UnitPromote { .. } => "unit.promote",
            Self::UnitSpawn { .. } => "unit.spawn",
            Self::BarbarianSpawn { .. } => "barbarian.spawn",
            Self::CityFound { .. } => "city.found",
            Self::CitySettle { .. } => "city.settle",
            Self::CitySetProduction { .. } => "city.set_production",
            Self::CityBuildDistrict { .. } => "city.build_district",
            Self::CityBuildBuilding { .. } => "city.build_building",
            Self::ResearchTechnology { .. } => "research.technology",
            Self::ResearchCivic { .. } => "research.civic",
            Self::PolicyAdopt { .. } => "policy.adopt",
            Self::PolicyRemove { .. } => "policy.remove",
        }
    }

    /// Encodes this command using the versioned custom binary format.
    pub fn encode(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(25);
        bytes.extend_from_slice(COMMAND_MAGIC);
        put_u16(&mut bytes, COMMAND_FORMAT_VERSION);
        match self {
            Self::TurnEnd { player } => {
                bytes.push(0);
                put_u32(&mut bytes, player.index());
            }
            Self::EditorSetMarker { player, value } => {
                bytes.push(1);
                put_u32(&mut bytes, player.index());
                put_i64(&mut bytes, *value);
            }
            Self::EditorSetTerrain {
                player,
                tile,
                terrain,
            } => {
                bytes.push(2);
                put_u32(&mut bytes, player.index());
                put_u32(&mut bytes, tile.index());
                bytes.push(*terrain as u8);
            }
            Self::EditorSetFeature {
                player,
                tile,
                feature,
            } => {
                bytes.push(3);
                put_u32(&mut bytes, player.index());
                put_u32(&mut bytes, tile.index());
                bytes.push(*feature as u8);
            }
            Self::EditorSetRiver {
                player,
                tile,
                river,
            } => {
                bytes.push(4);
                put_u32(&mut bytes, player.index());
                put_u32(&mut bytes, tile.index());
                bytes.push(river.map_or(255, |river| river as u8));
            }
            Self::EditorSetResource {
                player,
                tile,
                resource,
            } => {
                bytes.push(5);
                put_u32(&mut bytes, player.index());
                put_u32(&mut bytes, tile.index());
                bytes.push(resource.map_or(255, |resource| resource as u8));
            }
            Self::UnitMove {
                player,
                unit,
                destination,
            } => {
                bytes.push(6);
                put_u32(&mut bytes, player.index());
                put_u32(&mut bytes, unit.index());
                put_u32(&mut bytes, destination.index());
            }
            Self::UnitBuildRoad { player, unit } => {
                bytes.push(7);
                put_u32(&mut bytes, player.index());
                put_u32(&mut bytes, unit.index());
            }
            Self::UnitAttack {
                player,
                attacker,
                defender,
            } => {
                bytes.push(8);
                put_u32(&mut bytes, player.index());
                put_u32(&mut bytes, attacker.index());
                put_u32(&mut bytes, defender.index());
            }
            Self::UnitHold { player, unit } => {
                bytes.push(21);
                put_u32(&mut bytes, player.index());
                put_u32(&mut bytes, unit.index());
            }
            Self::UnitHeal { player, unit } => {
                bytes.push(9);
                put_u32(&mut bytes, player.index());
                put_u32(&mut bytes, unit.index());
            }
            Self::UnitPromote {
                player,
                unit,
                promotion,
            } => {
                bytes.push(10);
                put_u32(&mut bytes, player.index());
                put_u32(&mut bytes, unit.index());
                bytes.push(*promotion as u8);
            }
            Self::BarbarianSpawn { tile, kind } => {
                bytes.push(11);
                put_u32(&mut bytes, tile.index());
                bytes.push(*kind as u8);
            }
            Self::UnitSpawn {
                player,
                unit,
                kind,
                tile,
            } => {
                bytes.push(12);
                put_u32(&mut bytes, player.index());
                put_u32(&mut bytes, unit.index());
                bytes.push(*kind as u8);
                put_u32(&mut bytes, tile.index());
            }
            Self::CitySettle { player, unit } => {
                bytes.push(22);
                put_u32(&mut bytes, player.index());
                put_u32(&mut bytes, unit.index());
            }
            Self::CityFound { player, city, unit } => {
                bytes.push(13);
                put_u32(&mut bytes, player.index());
                put_u32(&mut bytes, city.index());
                put_u32(&mut bytes, unit.index());
            }
            Self::CitySetProduction { player, city, item } => {
                bytes.push(14);
                put_u32(&mut bytes, player.index());
                put_u32(&mut bytes, city.index());
                encode_production_item(&mut bytes, *item);
            }
            Self::CityBuildDistrict {
                player,
                city,
                kind,
                tile,
            } => {
                bytes.push(15);
                put_u32(&mut bytes, player.index());
                put_u32(&mut bytes, city.index());
                put_u8(&mut bytes, *kind as u8);
                put_u32(&mut bytes, tile.index());
            }
            Self::CityBuildBuilding {
                player,
                city,
                building,
            } => {
                bytes.push(16);
                put_u32(&mut bytes, player.index());
                put_u32(&mut bytes, city.index());
                put_u8(&mut bytes, *building as u8);
            }
            Self::ResearchTechnology { player, technology } => {
                bytes.push(17);
                put_u32(&mut bytes, player.index());
                put_string(&mut bytes, technology);
            }
            Self::ResearchCivic { player, civic } => {
                bytes.push(18);
                put_u32(&mut bytes, player.index());
                put_string(&mut bytes, civic);
            }
            Self::PolicyAdopt {
                player,
                slot,
                policy,
            } => {
                bytes.push(19);
                put_u32(&mut bytes, player.index());
                put_u8(&mut bytes, *slot);
                put_string(&mut bytes, policy);
            }
            Self::PolicyRemove { player, slot } => {
                bytes.push(20);
                put_u32(&mut bytes, player.index());
                put_u8(&mut bytes, *slot);
            }
        }
        bytes
    }

    /// Decodes one command and rejects trailing or malformed bytes.
    pub fn decode(bytes: &[u8]) -> Result<Self, SerializationError> {
        let mut reader = Reader::new(bytes);
        reader.expect_magic(COMMAND_MAGIC)?;
        reader.expect_version(COMMAND_FORMAT_VERSION)?;
        let kind = reader.u8()?;
        let command = match kind {
            0 => Self::TurnEnd {
                player: PlayerId::new(reader.u32()?),
            },
            1 => Self::EditorSetMarker {
                player: PlayerId::new(reader.u32()?),
                value: reader.i64()?,
            },
            2 => Self::EditorSetTerrain {
                player: PlayerId::new(reader.u32()?),
                tile: TileId::new(reader.u32()?),
                terrain: decode_terrain(&mut reader)?,
            },
            3 => Self::EditorSetFeature {
                player: PlayerId::new(reader.u32()?),
                tile: TileId::new(reader.u32()?),
                feature: decode_feature(&mut reader)?,
            },
            4 => Self::EditorSetRiver {
                player: PlayerId::new(reader.u32()?),
                tile: TileId::new(reader.u32()?),
                river: decode_river(&mut reader)?,
            },
            5 => Self::EditorSetResource {
                player: PlayerId::new(reader.u32()?),
                tile: TileId::new(reader.u32()?),
                resource: decode_resource(&mut reader)?,
            },
            6 => Self::UnitMove {
                player: PlayerId::new(reader.u32()?),
                unit: UnitId::new(reader.u32()?),
                destination: TileId::new(reader.u32()?),
            },
            7 => Self::UnitBuildRoad {
                player: PlayerId::new(reader.u32()?),
                unit: UnitId::new(reader.u32()?),
            },
            8 => Self::UnitAttack {
                player: PlayerId::new(reader.u32()?),
                attacker: UnitId::new(reader.u32()?),
                defender: UnitId::new(reader.u32()?),
            },
            9 => Self::UnitHeal {
                player: PlayerId::new(reader.u32()?),
                unit: UnitId::new(reader.u32()?),
            },
            10 => Self::UnitPromote {
                player: PlayerId::new(reader.u32()?),
                unit: UnitId::new(reader.u32()?),
                promotion: PromotionKind::from_u8(reader.u8()?)
                    .ok_or(SerializationError::InvalidTag(10))?,
            },
            11 => Self::BarbarianSpawn {
                tile: TileId::new(reader.u32()?),
                kind: UnitKind::from_u8(reader.u8()?).ok_or(SerializationError::InvalidTag(11))?,
            },
            12 => Self::UnitSpawn {
                player: PlayerId::new(reader.u32()?),
                unit: UnitId::new(reader.u32()?),
                kind: UnitKind::from_u8(reader.u8()?).ok_or(SerializationError::InvalidTag(12))?,
                tile: TileId::new(reader.u32()?),
            },
            13 => Self::CityFound {
                player: PlayerId::new(reader.u32()?),
                city: CityId::new(reader.u32()?),
                unit: UnitId::new(reader.u32()?),
            },
            22 => Self::CitySettle {
                player: PlayerId::new(reader.u32()?),
                unit: UnitId::new(reader.u32()?),
            },
            14 => Self::CitySetProduction {
                player: PlayerId::new(reader.u32()?),
                city: CityId::new(reader.u32()?),
                item: decode_production_item(&mut reader)?,
            },
            15 => Self::CityBuildDistrict {
                player: PlayerId::new(reader.u32()?),
                city: CityId::new(reader.u32()?),
                kind: DistrictKind::from_u8(reader.u8()?)
                    .ok_or(SerializationError::InvalidTag(15))?,
                tile: TileId::new(reader.u32()?),
            },
            16 => Self::CityBuildBuilding {
                player: PlayerId::new(reader.u32()?),
                city: CityId::new(reader.u32()?),
                building: BuildingKind::from_u8(reader.u8()?)
                    .ok_or(SerializationError::InvalidTag(16))?,
            },
            17 => Self::ResearchTechnology {
                player: PlayerId::new(reader.u32()?),
                technology: reader.string()?,
            },
            18 => Self::ResearchCivic {
                player: PlayerId::new(reader.u32()?),
                civic: reader.string()?,
            },
            19 => Self::PolicyAdopt {
                player: PlayerId::new(reader.u32()?),
                slot: reader.u8()?,
                policy: reader.string()?,
            },
            21 => Self::UnitHold {
                player: PlayerId::new(reader.u32()?),
                unit: UnitId::new(reader.u32()?),
            },
            20 => Self::PolicyRemove {
                player: PlayerId::new(reader.u32()?),
                slot: reader.u8()?,
            },
            other => return Err(SerializationError::InvalidTag(other)),
        };
        reader.finish()?;
        Ok(command)
    }
}

/// Metadata for one command exposed by the registry.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CommandSpec {
    /// Stable command ID.
    pub id: &'static str,
    /// Human-readable schema identifier.
    pub schema: &'static str,
}

/// The canonical command registry.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CommandRegistry {
    specs: &'static [CommandSpec],
}

impl CommandRegistry {
    /// Returns all registered commands in stable ID order.
    pub const fn specs(self) -> &'static [CommandSpec] {
        self.specs
    }

    /// Finds a command schema by stable ID.
    pub fn find(self, id: &str) -> Option<CommandSpec> {
        self.specs.iter().copied().find(|spec| spec.id == id)
    }

    /// Validates that a command is registered before dispatch.
    pub fn validate(self, command: &Command) -> Result<(), DispatchError> {
        self.find(command.id())
            .map(|_| ())
            .ok_or(DispatchError::UnregisteredCommand(command.id()))
    }
}

static COMMAND_SPECS: &[CommandSpec] = &[
    CommandSpec {
        id: "barbarian.spawn",
        schema: "tile:u32,kind:u8",
    },
    CommandSpec {
        id: "city.build_building",
        schema: "player:u32,city:u32,building:u8",
    },
    CommandSpec {
        id: "city.build_district",
        schema: "player:u32,city:u32,kind:u8,tile:u32",
    },
    CommandSpec {
        id: "city.found",
        schema: "player:u32,city:u32,unit:u32",
    },
    CommandSpec {
        id: "city.set_production",
        schema: "player:u32,city:u32,item:production",
    },
    CommandSpec {
        id: "city.settle",
        schema: "player:u32,unit:u32",
    },
    CommandSpec {
        id: "policy.adopt",
        schema: "player:u32,slot:u8,policy:string",
    },
    CommandSpec {
        id: "policy.remove",
        schema: "player:u32,slot:u8",
    },
    CommandSpec {
        id: "research.civic",
        schema: "player:u32,civic:string",
    },
    CommandSpec {
        id: "research.technology",
        schema: "player:u32,technology:string",
    },
    CommandSpec {
        id: "editor.set_feature",
        schema: "player:u32,tile:u32,feature:u8",
    },
    CommandSpec {
        id: "editor.set_marker",
        schema: "player:u32,value:i64",
    },
    CommandSpec {
        id: "editor.set_resource",
        schema: "player:u32,tile:u32,resource:u8?",
    },
    CommandSpec {
        id: "editor.set_river",
        schema: "player:u32,tile:u32,river:u8?",
    },
    CommandSpec {
        id: "editor.set_terrain",
        schema: "player:u32,tile:u32,terrain:u8",
    },
    CommandSpec {
        id: "turn.end",
        schema: "player:u32",
    },
    CommandSpec {
        id: "unit.attack",
        schema: "player:u32,attacker:u32,defender:u32",
    },
    CommandSpec {
        id: "unit.build_road",
        schema: "player:u32,unit:u32",
    },
    CommandSpec {
        id: "unit.heal",
        schema: "player:u32,unit:u32",
    },
    CommandSpec {
        id: "unit.hold",
        schema: "player:u32,unit:u32",
    },
    CommandSpec {
        id: "unit.move",
        schema: "player:u32,unit:u32,destination:u32",
    },
    CommandSpec {
        id: "unit.promote",
        schema: "player:u32,unit:u32,promotion:u8",
    },
    CommandSpec {
        id: "unit.spawn",
        schema: "player:u32,unit:u32,kind:u8,tile:u32",
    },
];

/// Returns the process-wide command registry.
pub const fn command_registry() -> CommandRegistry {
    CommandRegistry {
        specs: COMMAND_SPECS,
    }
}

/// Canonical world state for the Phase 1 session boundary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SimulationState {
    turn: u32,
    markers: BTreeMap<PlayerId, i64>,
    map: Option<MapState>,
    units: BTreeMap<UnitId, Unit>,
    roads: BTreeSet<TileId>,
    cities: BTreeMap<CityId, City>,
    progress: BTreeMap<PlayerId, PlayerProgress>,
    victories: BTreeMap<PlayerId, VictoryKind>,
    ended_players: BTreeSet<PlayerId>,
}

impl SimulationState {
    /// Creates a state with the supplied seats and zero markers.
    pub fn new(players: impl IntoIterator<Item = PlayerId>) -> Self {
        let markers: BTreeMap<PlayerId, i64> =
            players.into_iter().map(|player| (player, 0)).collect();
        let progress = markers
            .keys()
            .copied()
            .map(|player| (player, PlayerProgress::default()))
            .collect();
        Self {
            turn: 0,
            markers,
            map: None,
            units: BTreeMap::new(),
            roads: BTreeSet::new(),
            cities: BTreeMap::new(),
            progress,
            victories: BTreeMap::new(),
            ended_players: BTreeSet::new(),
        }
    }

    /// Creates a state with seats and a generated or edited map.
    pub fn with_map(players: impl IntoIterator<Item = PlayerId>, map: MapState) -> Self {
        let mut state = Self::new(players);
        state.map = Some(map);
        state
    }

    /// Returns the current turn.
    pub const fn turn(&self) -> u32 {
        self.turn
    }

    /// Returns whether a player is part of the session.
    pub fn contains_player(&self, player: PlayerId) -> bool {
        self.markers.contains_key(&player)
    }

    /// Returns all seats in canonical ID order.
    pub fn players(&self) -> impl Iterator<Item = PlayerId> + '_ {
        self.markers.keys().copied()
    }

    /// Returns seats that have ended the current turn in canonical order.
    pub fn ended_players(&self) -> impl Iterator<Item = PlayerId> + '_ {
        self.ended_players.iter().copied()
    }

    /// Returns the map attached to this state.
    pub fn map(&self) -> Option<&MapState> {
        self.map.as_ref()
    }

    /// Returns the marker visible to the owning seat.
    pub fn marker(&self, player: PlayerId) -> Option<i64> {
        self.markers.get(&player).copied()
    }

    /// Computes the canonical state hash.
    pub fn state_hash(&self) -> StateHash {
        let mut hasher = StateHasher::new();
        hasher.write_u64(self.turn as u64);
        hasher.write_u64(self.markers.len() as u64);
        for (player, marker) in &self.markers {
            hasher.write_u64(player.index() as u64);
            hasher.write_i64(*marker);
        }
        match &self.map {
            Some(map) => {
                hasher.write_u64(1);
                hasher.write_bytes(&map.encode());
            }
            None => hasher.write_u64(0),
        }
        hasher.write_u64(self.units.len() as u64);
        for (id, unit) in &self.units {
            hasher.write_u64(id.index() as u64);
            hasher.write_u64(unit.owner.map_or(u64::MAX, |owner| owner.index() as u64));
            hasher.write_u64(unit.kind as u64);
            hasher.write_u64(unit.tile.index() as u64);
            hasher.write_i64(unit.hp as i64);
            hasher.write_i64(unit.movement as i64);
            hasher.write_u64(unit.xp as u64);
            hasher.write_u64(unit.promotions as u64);
            hasher.write_u64(unit.charges as u64);
        }
        hasher.write_u64(self.roads.len() as u64);
        for tile in &self.roads {
            hasher.write_u64(tile.index() as u64);
        }
        hasher.write_u64(self.cities.len() as u64);
        for (id, city) in &self.cities {
            hash_city(&mut hasher, *id, city);
        }
        hasher.write_u64(self.progress.len() as u64);
        for (player, progress) in &self.progress {
            hash_progress(&mut hasher, *player, progress);
        }
        hasher.write_u64(self.victories.len() as u64);
        for (player, victory) in &self.victories {
            hasher.write_u64(player.index() as u64);
            hasher.write_u64(*victory as u64);
        }
        hasher.write_u64(self.ended_players.len() as u64);
        for player in &self.ended_players {
            hasher.write_u64(player.index() as u64);
        }
        hasher.finish()
    }

    /// Encodes state with a magic, format version, and canonical map ordering.
    pub fn encode(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(18 + self.markers.len() * 12);
        bytes.extend_from_slice(STATE_MAGIC);
        put_u16(&mut bytes, STATE_FORMAT_VERSION);
        put_u32(&mut bytes, self.turn);
        put_u32(&mut bytes, self.markers.len() as u32);
        for (player, marker) in &self.markers {
            put_u32(&mut bytes, player.index());
            put_i64(&mut bytes, *marker);
        }
        if let Some(map) = &self.map {
            let map_bytes = map.encode();
            put_u32(&mut bytes, map_bytes.len() as u32);
            bytes.extend_from_slice(&map_bytes);
        } else {
            put_u32(&mut bytes, 0);
        }
        put_u32(&mut bytes, self.units.len() as u32);
        for (id, unit) in &self.units {
            put_u32(&mut bytes, id.index());
            put_u32(
                &mut bytes,
                unit.owner.map_or(u32::MAX, |owner| owner.index()),
            );
            put_u8(&mut bytes, unit.kind as u8);
            put_u32(&mut bytes, unit.tile.index());
            put_i64(&mut bytes, unit.hp as i64);
            put_i64(&mut bytes, unit.movement as i64);
            put_u8(&mut bytes, unit.xp);
            put_u16(&mut bytes, unit.promotions);
            put_u8(&mut bytes, unit.charges);
        }
        put_u32(&mut bytes, self.roads.len() as u32);
        for tile in &self.roads {
            put_u32(&mut bytes, tile.index());
        }
        put_u32(&mut bytes, self.cities.len() as u32);
        for (id, city) in &self.cities {
            encode_city(&mut bytes, *id, city);
        }
        put_u32(&mut bytes, self.progress.len() as u32);
        for (player, progress) in &self.progress {
            encode_progress(&mut bytes, *player, progress);
        }
        put_u32(&mut bytes, self.victories.len() as u32);
        for (player, victory) in &self.victories {
            put_u32(&mut bytes, player.index());
            put_u8(&mut bytes, *victory as u8);
        }
        put_u32(&mut bytes, self.ended_players.len() as u32);
        for player in &self.ended_players {
            put_u32(&mut bytes, player.index());
        }
        bytes
    }

    /// Decodes state and rejects duplicate players, truncation, or trailing bytes.
    pub fn decode(bytes: &[u8]) -> Result<Self, SerializationError> {
        let mut reader = Reader::new(bytes);
        reader.expect_magic(STATE_MAGIC)?;
        reader.expect_version(STATE_FORMAT_VERSION)?;
        let turn = reader.u32()?;
        let count = reader.count()?;
        let mut markers = BTreeMap::new();
        let mut previous_player = None;
        for _ in 0..count {
            let player = PlayerId::new(reader.u32()?);
            let marker = reader.i64()?;
            if previous_player.is_some_and(|previous| player <= previous) {
                if markers.contains_key(&player) {
                    return Err(SerializationError::DuplicateKey);
                }
                return Err(SerializationError::NonCanonicalOrder);
            }
            previous_player = Some(player);
            if markers.insert(player, marker).is_some() {
                return Err(SerializationError::DuplicateKey);
            }
        }
        let map_bytes = reader.bytes()?;
        let map = if map_bytes.is_empty() {
            None
        } else {
            Some(MapState::decode(map_bytes).map_err(|_| SerializationError::InvalidMap)?)
        };
        let unit_count = reader.count()?;
        let mut units = BTreeMap::new();
        let mut previous_unit = None;
        for _ in 0..unit_count {
            let id = UnitId::new(reader.u32()?);
            if previous_unit.is_some_and(|previous| id <= previous) {
                return Err(SerializationError::NonCanonicalOrder);
            }
            previous_unit = Some(id);
            let owner_raw = reader.u32()?;
            let owner = (owner_raw != u32::MAX).then(|| PlayerId::new(owner_raw));
            let kind = UnitKind::from_u8(reader.u8()?).ok_or(SerializationError::InvalidTag(0))?;
            let tile = TileId::new(reader.u32()?);
            let hp = reader.i64()?;
            let movement = reader.i64()?;
            let xp = reader.u8()?;
            let promotions = reader.u16()?;
            let charges = reader.u8()?;
            if hp <= 0
                || hp > i32::MAX as i64
                || movement < 0
                || movement > i32::MAX as i64
                || promotions & !0b111 != 0
            {
                return Err(SerializationError::InvalidTag(0));
            }
            units.insert(
                id,
                Unit {
                    id,
                    owner,
                    kind,
                    tile,
                    hp: hp as i32,
                    movement: movement as i32,
                    xp,
                    promotions,
                    charges,
                },
            );
        }
        let road_count = reader.count()?;
        let mut roads = BTreeSet::new();
        let mut previous_road = None;
        for _ in 0..road_count {
            let tile = TileId::new(reader.u32()?);
            if previous_road.is_some_and(|previous| tile <= previous) {
                return Err(SerializationError::NonCanonicalOrder);
            }
            previous_road = Some(tile);
            roads.insert(tile);
        }
        if map.is_none() && (!units.is_empty() || !roads.is_empty()) {
            return Err(SerializationError::InvalidUnitState);
        }
        let mut occupied = BTreeSet::new();
        for unit in units.values() {
            if unit
                .owner
                .is_some_and(|owner| !markers.contains_key(&owner))
            {
                return Err(SerializationError::InvalidUnitState);
            }
            if !occupied.insert(unit.tile) {
                return Err(SerializationError::InvalidUnitState);
            }
            if unit.hp <= 0
                || unit.hp > unit.kind.max_hp()
                || unit.movement < 0
                || unit.movement > unit.max_movement()
                || (unit.kind == UnitKind::Builder && unit.charges > 3)
                || (unit.kind != UnitKind::Builder && unit.charges != 0)
            {
                return Err(SerializationError::InvalidUnitState);
            }
            if let Some(map) = &map {
                let terrain = map
                    .tile(unit.tile)
                    .ok_or(SerializationError::InvalidUnitState)?
                    .terrain;
                Unit::movement_cost(terrain, roads.contains(&unit.tile))
                    .ok_or(SerializationError::InvalidUnitState)?;
            }
        }
        if let Some(map) = &map {
            for road in &roads {
                let terrain = map
                    .tile(*road)
                    .ok_or(SerializationError::InvalidUnitState)?
                    .terrain;
                Unit::movement_cost(terrain, false).ok_or(SerializationError::InvalidUnitState)?;
            }
        }
        let city_count = reader.count()?;
        let mut cities = BTreeMap::new();
        let mut previous_city = None;
        let mut occupied_city_tiles = BTreeSet::new();
        let mut occupied_district_tiles = BTreeSet::new();
        for _ in 0..city_count {
            let (id, city) = decode_city(&mut reader)?;
            if previous_city.is_some_and(|previous| id <= previous) {
                return Err(SerializationError::NonCanonicalOrder);
            }
            previous_city = Some(id);
            if !markers.contains_key(&city.owner)
                || city.id != id
                || city.population == 0
                || city.food_stored < 0
                || city.production_stored < 0
                || city.production_stored > i64::MAX / 2
                || !occupied_city_tiles.insert(city.tile)
            {
                return Err(SerializationError::InvalidCityState);
            }
            let map = map.as_ref().ok_or(SerializationError::InvalidCityState)?;
            let terrain = map
                .tile(city.tile)
                .ok_or(SerializationError::InvalidCityState)?
                .terrain;
            Unit::movement_cost(terrain, false).ok_or(SerializationError::InvalidCityState)?;
            if city.population > city.housing_capacity() {
                return Err(SerializationError::InvalidCityState);
            }
            for tile in city.districts.keys() {
                let terrain = map
                    .tile(*tile)
                    .ok_or(SerializationError::InvalidCityState)?
                    .terrain;
                if *tile == city.tile
                    || map.topology().distance(city.tile, *tile) > 3
                    || !occupied_district_tiles.insert(*tile)
                    || Unit::movement_cost(terrain, false).is_none()
                {
                    return Err(SerializationError::InvalidCityState);
                }
            }
            for building in &city.buildings {
                if let Some(required) = building.required_district()
                    && !city.has_district(required)
                {
                    return Err(SerializationError::InvalidCityState);
                }
            }
            if let Some(ProductionItem::District { kind, tile }) = city.production
                && (city.districts.contains_key(&tile)
                    || tile == city.tile
                    || map.topology().distance(city.tile, tile) > 3
                    || city.has_district(kind))
            {
                return Err(SerializationError::InvalidCityState);
            }
            if let Some(ProductionItem::District { tile, .. }) = city.production {
                let terrain = map
                    .tile(tile)
                    .ok_or(SerializationError::InvalidCityState)?
                    .terrain;
                if Unit::movement_cost(terrain, false).is_none() {
                    return Err(SerializationError::InvalidCityState);
                }
            }
            if let Some(ProductionItem::Building(building)) = city.production
                && (city.has_building(building)
                    || building
                        .required_district()
                        .is_some_and(|required| !city.has_district(required)))
            {
                return Err(SerializationError::InvalidCityState);
            }
            if city
                .production
                .is_some_and(|item| city.production_stored >= item.cost())
            {
                return Err(SerializationError::InvalidCityState);
            }
            if cities.insert(id, city).is_some() {
                return Err(SerializationError::DuplicateKey);
            }
        }
        if map.is_none() && !cities.is_empty() {
            return Err(SerializationError::InvalidCityState);
        }
        if cities.values().any(|city| {
            city.districts
                .keys()
                .any(|tile| occupied_city_tiles.contains(tile))
        }) {
            return Err(SerializationError::InvalidCityState);
        }
        let mut queued_district_tiles = BTreeSet::new();
        for city in cities.values() {
            if let Some(ProductionItem::District { tile, .. }) = city.production
                && (occupied_city_tiles.contains(&tile)
                    || occupied_district_tiles.contains(&tile)
                    || !queued_district_tiles.insert(tile))
            {
                return Err(SerializationError::InvalidCityState);
            }
        }
        for unit in units.values() {
            if occupied_city_tiles.contains(&unit.tile)
                || occupied_district_tiles.contains(&unit.tile)
            {
                return Err(SerializationError::InvalidUnitState);
            }
        }
        let progress_count = reader.count()?;
        let mut progress = BTreeMap::new();
        let mut previous_progress_player = None;
        for _ in 0..progress_count {
            let (player, value) = decode_progress(&mut reader)?;
            if previous_progress_player.is_some_and(|previous| player <= previous) {
                return Err(SerializationError::NonCanonicalOrder);
            }
            previous_progress_player = Some(player);
            if !markers.contains_key(&player) || progress.insert(player, value).is_some() {
                return Err(SerializationError::InvalidProgressState);
            }
        }
        if progress.len() != markers.len() {
            return Err(SerializationError::InvalidProgressState);
        }
        let victory_count = reader.count()?;
        let mut victories = BTreeMap::new();
        let mut previous_victory_player = None;
        for _ in 0..victory_count {
            let player = PlayerId::new(reader.u32()?);
            if previous_victory_player.is_some_and(|previous| player <= previous) {
                return Err(SerializationError::NonCanonicalOrder);
            }
            previous_victory_player = Some(player);
            let victory =
                VictoryKind::from_u8(reader.u8()?).ok_or(SerializationError::InvalidTag(0))?;
            if !markers.contains_key(&player) || victories.insert(player, victory).is_some() {
                return Err(SerializationError::InvalidProgressState);
            }
        }
        if victories.len() > 1 {
            return Err(SerializationError::InvalidProgressState);
        }
        for (player, victory) in &victories {
            let own_cities = cities.values().filter(|city| city.owner == *player).count();
            let valid = match victory {
                VictoryKind::Science => {
                    own_cities > 0
                        && progress.get(player).is_some_and(|value| {
                            value.technologies.len() == city::base_ruleset().technologies().count()
                        })
                }
                VictoryKind::Domination => own_cities > 0 && own_cities == cities.len(),
            };
            if !valid {
                return Err(SerializationError::InvalidProgressState);
            }
        }
        let ended_count = reader.count()?;
        let mut ended_players = BTreeSet::new();
        let mut previous_ended = None;
        for _ in 0..ended_count {
            let player = PlayerId::new(reader.u32()?);
            if previous_ended.is_some_and(|previous| player <= previous) {
                return Err(SerializationError::NonCanonicalOrder);
            }
            previous_ended = Some(player);
            if !markers.contains_key(&player) || !ended_players.insert(player) {
                return Err(SerializationError::InvalidUnitState);
            }
        }
        for unit in units.values() {
            if unit.owner.is_none() != (unit.kind == UnitKind::Barbarian) {
                return Err(SerializationError::InvalidUnitState);
            }
        }
        reader.finish()?;
        Ok(Self {
            turn,
            markers,
            map,
            units,
            roads,
            cities,
            progress,
            victories,
            ended_players,
        })
    }

    fn apply(&mut self, command: &Command) -> Result<EventKind, DispatchError> {
        match command {
            Command::TurnEnd { player } => {
                self.require_player(*player)?;
                let completes_turn =
                    self.ended_players.len().saturating_add(1) == self.markers.len();
                if completes_turn && self.turn.checked_add(1).is_none() {
                    return Err(DispatchError::TurnOverflow);
                }
                if !self.ended_players.insert(*player) {
                    return Err(DispatchError::TurnAlreadyEnded);
                }
                if !completes_turn {
                    return Ok(EventKind::TurnEnded { player: *player });
                }
                let next_turn = self.turn.checked_add(1).expect("turn overflow was checked");
                self.turn = next_turn;
                for unit in self.units.values_mut() {
                    unit.reset_turn();
                }
                self.advance_cities();
                self.advance_progress();
                self.ended_players.clear();
                Ok(EventKind::TurnAdvanced { turn: self.turn })
            }
            Command::EditorSetMarker { player, value } => {
                self.require_player(*player)?;
                self.markers.insert(*player, *value);
                Ok(EventKind::MarkerSet {
                    player: *player,
                    value: *value,
                })
            }
            Command::EditorSetTerrain {
                player,
                tile,
                terrain,
            } => {
                self.require_player(*player)?;
                if Unit::movement_cost(*terrain, false).is_none()
                    && (self.tile_occupied(*tile)
                        || self.roads.contains(tile)
                        || self.queued_district_at(*tile))
                {
                    return Err(DispatchError::Impassable(*tile));
                }
                self.map
                    .as_mut()
                    .ok_or(DispatchError::NoMap)?
                    .set_terrain(*tile, *terrain)
                    .map_err(DispatchError::Map)?;
                Ok(EventKind::MapEdited { tile: *tile })
            }
            Command::EditorSetFeature {
                player,
                tile,
                feature,
            } => {
                self.require_player(*player)?;
                self.map
                    .as_mut()
                    .ok_or(DispatchError::NoMap)?
                    .set_feature(*tile, *feature)
                    .map_err(DispatchError::Map)?;
                Ok(EventKind::MapEdited { tile: *tile })
            }
            Command::EditorSetRiver {
                player,
                tile,
                river,
            } => {
                self.require_player(*player)?;
                self.map
                    .as_mut()
                    .ok_or(DispatchError::NoMap)?
                    .set_river(*tile, *river)
                    .map_err(DispatchError::Map)?;
                Ok(EventKind::MapEdited { tile: *tile })
            }
            Command::EditorSetResource {
                player,
                tile,
                resource,
            } => {
                self.require_player(*player)?;
                self.map
                    .as_mut()
                    .ok_or(DispatchError::NoMap)?
                    .set_resource(*tile, *resource)
                    .map_err(DispatchError::Map)?;
                Ok(EventKind::MapEdited { tile: *tile })
            }
            Command::UnitMove {
                player,
                unit,
                destination,
            } => self.unit_move(*player, *unit, *destination),
            Command::UnitBuildRoad { player, unit } => self.unit_build_road(*player, *unit),
            Command::UnitAttack {
                player,
                attacker,
                defender,
            } => self.unit_attack(*player, *attacker, *defender),
            Command::UnitHeal { player, unit } => self.unit_heal(*player, *unit),
            Command::UnitHold { player, unit } => {
                let held = self
                    .units
                    .get_mut(unit)
                    .ok_or(DispatchError::UnknownUnit(*unit))?;
                if held.owner != Some(*player) {
                    return Err(DispatchError::UnauthorizedUnit(*unit));
                }
                held.movement = 0;
                Ok(EventKind::UnitChanged { unit: *unit })
            }
            Command::UnitPromote {
                player,
                unit,
                promotion,
            } => self.unit_promote(*player, *unit, *promotion),
            Command::UnitSpawn {
                player,
                unit,
                kind,
                tile,
            } => self.unit_spawn(*player, *unit, *kind, *tile),
            Command::BarbarianSpawn { tile, kind } => self.barbarian_spawn(*tile, *kind),
            Command::CityFound { player, city, unit } => self.city_found(*player, *city, *unit),
            Command::CitySettle { player, unit } => {
                let mut candidate = 0_u32;
                for city in self.cities.keys() {
                    if city.index() != candidate {
                        break;
                    }
                    candidate = candidate
                        .checked_add(1)
                        .ok_or(DispatchError::CityIdOverflow)?;
                }
                self.city_found(*player, CityId::new(candidate), *unit)
            }
            Command::CitySetProduction { player, city, item } => {
                self.city_set_production(*player, *city, *item)
            }
            Command::CityBuildDistrict {
                player,
                city,
                kind,
                tile,
            } => self.city_build_district(*player, *city, *kind, *tile),
            Command::CityBuildBuilding {
                player,
                city,
                building,
            } => self.city_build_building(*player, *city, *building),
            Command::ResearchTechnology { player, technology } => {
                self.research_technology(*player, technology)
            }
            Command::ResearchCivic { player, civic } => self.research_civic(*player, civic),
            Command::PolicyAdopt {
                player,
                slot,
                policy,
            } => self.policy_adopt(*player, *slot, policy),
            Command::PolicyRemove { player, slot } => self.policy_remove(*player, *slot),
        }
    }

    fn unit_move(
        &mut self,
        player: PlayerId,
        unit_id: UnitId,
        destination: TileId,
    ) -> Result<EventKind, DispatchError> {
        let map = self.map.as_ref().ok_or(DispatchError::NoMap)?;
        let unit = self
            .units
            .get(&unit_id)
            .ok_or(DispatchError::UnknownUnit(unit_id))?
            .clone();
        self.require_unit_owner(&unit, player)?;
        if !map.topology().neighbors(unit.tile).contains(&destination) {
            return Err(DispatchError::InvalidMove);
        }
        if self.tile_occupied(destination) {
            return Err(DispatchError::Occupied(destination));
        }
        if self.has_enemy_zoc(player, unit.tile) && !self.has_enemy_zoc(player, destination) {
            return Err(DispatchError::ZocBlocked);
        }
        let terrain = map
            .tile(destination)
            .ok_or(DispatchError::UnknownTile(destination))?
            .terrain;
        let cost = Unit::movement_cost(terrain, self.roads.contains(&destination))
            .ok_or(DispatchError::Impassable(destination))?;
        if unit.movement < cost {
            return Err(DispatchError::InsufficientMovement);
        }
        let moved = self.units.get_mut(&unit_id).expect("unit was checked");
        moved.tile = destination;
        moved.movement -= cost;
        Ok(EventKind::UnitChanged { unit: unit_id })
    }

    fn unit_build_road(
        &mut self,
        player: PlayerId,
        unit_id: UnitId,
    ) -> Result<EventKind, DispatchError> {
        let unit = self
            .units
            .get(&unit_id)
            .ok_or(DispatchError::UnknownUnit(unit_id))?
            .clone();
        self.require_unit_owner(&unit, player)?;
        if unit.kind != UnitKind::Builder {
            return Err(DispatchError::InvalidBuilder);
        }
        if unit.charges == 0 {
            return Err(DispatchError::NoBuilderCharges);
        }
        let map = self.map.as_ref().ok_or(DispatchError::NoMap)?;
        let terrain = map
            .tile(unit.tile)
            .ok_or(DispatchError::UnknownTile(unit.tile))?
            .terrain;
        Unit::movement_cost(terrain, false).ok_or(DispatchError::Impassable(unit.tile))?;
        self.roads.insert(unit.tile);
        let builder = self.units.get_mut(&unit_id).expect("unit was checked");
        builder.consume_charge();
        builder.movement = 0;
        Ok(EventKind::RoadBuilt { tile: unit.tile })
    }

    fn unit_attack(
        &mut self,
        player: PlayerId,
        attacker_id: UnitId,
        defender_id: UnitId,
    ) -> Result<EventKind, DispatchError> {
        let attacker = self
            .units
            .get(&attacker_id)
            .ok_or(DispatchError::UnknownUnit(attacker_id))?
            .clone();
        let defender = self
            .units
            .get(&defender_id)
            .ok_or(DispatchError::UnknownUnit(defender_id))?
            .clone();
        self.require_unit_owner(&attacker, player)?;
        let map = self.map.as_ref().ok_or(DispatchError::NoMap)?;
        let distance = map.topology().distance(attacker.tile, defender.tile);
        let damage = VisibleUnitObservation::from(&attacker)
            .forecast_attack(VisibleUnitObservation::from(&defender), distance)?;
        let attacker_damage = damage.to_defender;
        let defender_damage = damage.to_attacker;
        let defender_hp = defender.hp - attacker_damage;
        let attacker_hp = attacker.hp - defender_damage;
        if defender_hp <= 0 {
            self.units.remove(&defender_id);
        } else if let Some(unit) = self.units.get_mut(&defender_id) {
            unit.hp = defender_hp;
        }
        if attacker_hp <= 0 {
            self.units.remove(&attacker_id);
        } else if let Some(unit) = self.units.get_mut(&attacker_id) {
            unit.hp = attacker_hp;
            unit.movement = 0;
            unit.xp = unit.xp.saturating_add(1);
        }
        Ok(EventKind::Combat {
            attacker: attacker_id,
            defender: defender_id,
            attacker_damage,
            defender_damage,
        })
    }

    fn unit_heal(&mut self, player: PlayerId, unit_id: UnitId) -> Result<EventKind, DispatchError> {
        let unit = self
            .units
            .get(&unit_id)
            .ok_or(DispatchError::UnknownUnit(unit_id))?
            .clone();
        self.require_unit_owner(&unit, player)?;
        let hp = VisibleUnitObservation::from(&unit)
            .forecast_heal(self.has_enemy_zoc(player, unit.tile))?;
        let healed = self.units.get_mut(&unit_id).expect("unit was checked");
        healed.hp = hp;
        healed.movement = 0;
        Ok(EventKind::UnitChanged { unit: unit_id })
    }

    fn unit_promote(
        &mut self,
        player: PlayerId,
        unit_id: UnitId,
        promotion: PromotionKind,
    ) -> Result<EventKind, DispatchError> {
        let owner = self
            .units
            .get(&unit_id)
            .ok_or(DispatchError::UnknownUnit(unit_id))?
            .owner;
        if owner != Some(player) {
            return Err(DispatchError::UnauthorizedUnit(unit_id));
        }
        let unit = self.units.get_mut(&unit_id).expect("unit was checked");
        if !unit.promote(promotion) {
            return Err(DispatchError::PromotionUnavailable);
        }
        Ok(EventKind::UnitChanged { unit: unit_id })
    }

    fn unit_spawn(
        &mut self,
        player: PlayerId,
        unit_id: UnitId,
        kind: UnitKind,
        tile: TileId,
    ) -> Result<EventKind, DispatchError> {
        self.require_player(player)?;
        if kind == UnitKind::Barbarian {
            return Err(DispatchError::InvalidBarbarianKind);
        }
        let map = self.map.as_ref().ok_or(DispatchError::NoMap)?;
        let terrain = map
            .tile(tile)
            .ok_or(DispatchError::UnknownTile(tile))?
            .terrain;
        Unit::movement_cost(terrain, false).ok_or(DispatchError::Impassable(tile))?;
        if self.units.contains_key(&unit_id) {
            return Err(DispatchError::UnitAlreadyExists(unit_id));
        }
        if self.tile_occupied(tile) {
            return Err(DispatchError::Occupied(tile));
        }
        self.units
            .insert(unit_id, Unit::new(unit_id, Some(player), kind, tile));
        Ok(EventKind::UnitSpawned { unit: unit_id })
    }

    fn barbarian_spawn(
        &mut self,
        tile: TileId,
        kind: UnitKind,
    ) -> Result<EventKind, DispatchError> {
        let map = self.map.as_ref().ok_or(DispatchError::NoMap)?;
        if !map.topology().contains(tile) {
            return Err(DispatchError::UnknownTile(tile));
        }
        if kind != UnitKind::Barbarian {
            return Err(DispatchError::InvalidBarbarianKind);
        }
        let terrain = map
            .tile(tile)
            .ok_or(DispatchError::UnknownTile(tile))?
            .terrain;
        Unit::movement_cost(terrain, false).ok_or(DispatchError::Impassable(tile))?;
        if self.tile_occupied(tile) {
            return Err(DispatchError::Occupied(tile));
        }
        let next_id = match self.units.keys().next_back() {
            Some(id) => id
                .index()
                .checked_add(1)
                .ok_or(DispatchError::UnitIdOverflow)?,
            None => 0,
        };
        let id = UnitId::new(next_id);
        if self.units.contains_key(&id) {
            return Err(DispatchError::UnitAlreadyExists(id));
        }
        self.units.insert(id, Unit::new(id, None, kind, tile));
        Ok(EventKind::BarbarianSpawned { unit: id })
    }

    fn city_found(
        &mut self,
        player: PlayerId,
        city_id: CityId,
        unit_id: UnitId,
    ) -> Result<EventKind, DispatchError> {
        self.require_player(player)?;
        if self.cities.contains_key(&city_id) {
            return Err(DispatchError::CityAlreadyExists(city_id));
        }
        let settler = self
            .units
            .get(&unit_id)
            .ok_or(DispatchError::UnknownUnit(unit_id))?
            .clone();
        self.require_unit_owner(&settler, player)?;
        if settler.kind != UnitKind::Settler {
            return Err(DispatchError::InvalidFounder(unit_id));
        }
        let map = self.map.as_ref().ok_or(DispatchError::NoMap)?;
        let terrain = map
            .tile(settler.tile)
            .ok_or(DispatchError::UnknownTile(settler.tile))?
            .terrain;
        Unit::movement_cost(terrain, false).ok_or(DispatchError::InvalidCityTile(settler.tile))?;
        if self.tile_occupied_except_unit(settler.tile, Some(unit_id))
            || self.queued_district_at(settler.tile)
        {
            return Err(DispatchError::Occupied(settler.tile));
        }
        self.units.remove(&unit_id);
        self.cities
            .insert(city_id, City::new(city_id, player, settler.tile));
        Ok(EventKind::CityFounded { city: city_id })
    }

    fn city_set_production(
        &mut self,
        player: PlayerId,
        city_id: CityId,
        item: ProductionItem,
    ) -> Result<EventKind, DispatchError> {
        self.require_city_owner(city_id, player)?;
        self.validate_production(city_id, item)?;
        self.cities
            .get_mut(&city_id)
            .expect("city owner was checked")
            .queue(item);
        Ok(EventKind::ProductionQueued { city: city_id })
    }

    fn city_build_district(
        &mut self,
        player: PlayerId,
        city_id: CityId,
        kind: DistrictKind,
        tile: TileId,
    ) -> Result<EventKind, DispatchError> {
        self.require_city_owner(city_id, player)?;
        self.validate_district_site(city_id, kind, tile)?;
        self.cities
            .get_mut(&city_id)
            .expect("city owner was checked")
            .queue(ProductionItem::District { kind, tile });
        Ok(EventKind::ProductionQueued { city: city_id })
    }

    fn city_build_building(
        &mut self,
        player: PlayerId,
        city_id: CityId,
        building: BuildingKind,
    ) -> Result<EventKind, DispatchError> {
        self.require_city_owner(city_id, player)?;
        self.validate_production(city_id, ProductionItem::Building(building))?;
        self.cities
            .get_mut(&city_id)
            .expect("city owner was checked")
            .queue(ProductionItem::Building(building));
        Ok(EventKind::ProductionQueued { city: city_id })
    }

    fn validate_production(
        &self,
        city_id: CityId,
        item: ProductionItem,
    ) -> Result<(), DispatchError> {
        if !item.is_valid() {
            return Err(DispatchError::InvalidProduction);
        }
        let city = self
            .cities
            .get(&city_id)
            .ok_or(DispatchError::UnknownCity(city_id))?;
        match item {
            ProductionItem::Unit(kind) => {
                if kind == UnitKind::Barbarian {
                    return Err(DispatchError::InvalidProduction);
                }
            }
            ProductionItem::Building(building) => {
                if city.has_building(building) {
                    return Err(DispatchError::BuildingAlreadyBuilt(building));
                }
                if let Some(required) = building.required_district()
                    && !city.has_district(required)
                {
                    return Err(DispatchError::MissingDistrict(required));
                }
            }
            ProductionItem::District { kind, tile } => {
                self.validate_district_site(city_id, kind, tile)?;
            }
        }
        Ok(())
    }

    fn validate_district_site(
        &self,
        city_id: CityId,
        kind: DistrictKind,
        tile: TileId,
    ) -> Result<(), DispatchError> {
        let city = self
            .cities
            .get(&city_id)
            .ok_or(DispatchError::UnknownCity(city_id))?;
        let map = self.map.as_ref().ok_or(DispatchError::NoMap)?;
        if tile == city.tile
            || map.topology().distance(city.tile, tile) > 3
            || !map.topology().contains(tile)
        {
            return Err(DispatchError::InvalidDistrictTile(tile));
        }
        let terrain = map
            .tile(tile)
            .ok_or(DispatchError::UnknownTile(tile))?
            .terrain;
        Unit::movement_cost(terrain, false).ok_or(DispatchError::InvalidDistrictTile(tile))?;
        if city.has_district(kind)
            || city.districts.contains_key(&tile)
            || self.cities.values().any(|other| other.tile == tile)
            || self.units.values().any(|unit| unit.tile == tile)
            || self
                .cities
                .values()
                .any(|other| other.districts.contains_key(&tile))
            || self.queued_district_at(tile)
        {
            return Err(DispatchError::Occupied(tile));
        }
        Ok(())
    }

    fn advance_cities(&mut self) {
        let city_ids: Vec<_> = self.cities.keys().copied().collect();
        for city_id in city_ids {
            let Some(item) = self.cities.get_mut(&city_id).and_then(City::advance_turn) else {
                continue;
            };
            if !self.complete_production(city_id, item)
                && let Some(city) = self.cities.get_mut(&city_id)
            {
                city.restore_order(item);
            }
        }
    }

    fn research_technology(
        &mut self,
        player: PlayerId,
        technology: &str,
    ) -> Result<EventKind, DispatchError> {
        self.require_player(player)?;
        self.ensure_no_victory(player)?;
        let definition = city::base_ruleset()
            .technology(technology)
            .ok_or(DispatchError::UnknownTechnology)?;
        let completed_immediately = {
            let progress = self
                .progress
                .get_mut(&player)
                .expect("player progress exists");
            if progress.has_technology(technology) {
                return Err(DispatchError::AlreadyComplete);
            }
            if definition
                .prerequisites()
                .iter()
                .any(|required| !progress.has_technology(required))
            {
                return Err(DispatchError::MissingPrerequisite);
            }
            progress.current_technology = Some(technology.to_owned());
            if progress.technology_progress >= i64::from(definition.cost()) {
                progress.technology_progress -= i64::from(definition.cost());
                progress.technologies.insert(technology.to_owned());
                progress.current_technology = None;
                true
            } else {
                false
            }
        };
        if completed_immediately {
            self.refresh_policy_slots(player);
            self.check_victories();
        }
        Ok(EventKind::ResearchQueued {
            player,
            id: technology.to_owned(),
        })
    }

    fn research_civic(
        &mut self,
        player: PlayerId,
        civic: &str,
    ) -> Result<EventKind, DispatchError> {
        self.require_player(player)?;
        self.ensure_no_victory(player)?;
        let definition = city::base_ruleset()
            .civic(civic)
            .ok_or(DispatchError::UnknownCivic)?;
        let completed_immediately = {
            let progress = self
                .progress
                .get_mut(&player)
                .expect("player progress exists");
            if progress.has_civic(civic) {
                return Err(DispatchError::AlreadyComplete);
            }
            if definition
                .prerequisites()
                .iter()
                .any(|required| !progress.has_civic(required))
            {
                return Err(DispatchError::MissingPrerequisite);
            }
            progress.current_civic = Some(civic.to_owned());
            if progress.civic_progress >= i64::from(definition.cost()) {
                progress.civic_progress -= i64::from(definition.cost());
                progress.civics.insert(civic.to_owned());
                progress.current_civic = None;
                true
            } else {
                false
            }
        };
        if completed_immediately {
            self.refresh_policy_slots(player);
            self.check_victories();
        }
        Ok(EventKind::ResearchQueued {
            player,
            id: civic.to_owned(),
        })
    }

    fn policy_adopt(
        &mut self,
        player: PlayerId,
        slot: u8,
        policy: &str,
    ) -> Result<EventKind, DispatchError> {
        self.require_player(player)?;
        self.ensure_no_victory(player)?;
        let definition = city::base_ruleset()
            .policy(policy)
            .ok_or(DispatchError::UnknownPolicy)?;
        let progress = self
            .progress
            .get_mut(&player)
            .expect("player progress exists");
        if slot >= progress.policy_slots {
            return Err(DispatchError::InvalidPolicySlot);
        }
        if progress
            .policies
            .iter()
            .any(|(other_slot, active)| *other_slot != slot && active == policy)
        {
            return Err(DispatchError::PolicyAlreadyActive);
        }
        if definition
            .required_civic()
            .is_some_and(|required| !progress.has_civic(required))
        {
            return Err(DispatchError::MissingPrerequisite);
        }
        progress.policies.insert(slot, policy.to_owned());
        Ok(EventKind::PolicyChanged { player, slot })
    }

    fn policy_remove(&mut self, player: PlayerId, slot: u8) -> Result<EventKind, DispatchError> {
        self.require_player(player)?;
        self.ensure_no_victory(player)?;
        let progress = self
            .progress
            .get_mut(&player)
            .expect("player progress exists");
        if slot >= progress.policy_slots {
            return Err(DispatchError::InvalidPolicySlot);
        }
        progress.policies.remove(&slot);
        Ok(EventKind::PolicyChanged { player, slot })
    }

    fn ensure_no_victory(&self, player: PlayerId) -> Result<(), DispatchError> {
        if self.victories.contains_key(&player) {
            Err(DispatchError::VictoryAlreadyReached)
        } else {
            Ok(())
        }
    }

    fn advance_progress(&mut self) {
        let players: Vec<_> = self.markers.keys().copied().collect();
        for player in players {
            let science = self
                .cities
                .values()
                .filter(|city| city.owner == player)
                .map(City::science_yield)
                .sum::<i64>();
            let culture = self
                .cities
                .values()
                .filter(|city| city.owner == player)
                .map(City::culture_yield)
                .sum::<i64>();
            let Some(progress) = self.progress.get_mut(&player) else {
                continue;
            };
            let policy_science = progress
                .policies
                .values()
                .filter_map(|policy| city::base_ruleset().policy(policy))
                .map(|policy| i64::from(policy.science_bonus()))
                .sum::<i64>();
            let policy_culture = progress
                .policies
                .values()
                .filter_map(|policy| city::base_ruleset().policy(policy))
                .map(|policy| i64::from(policy.culture_bonus()))
                .sum::<i64>();
            let science = science.saturating_add(policy_science);
            let culture = culture.saturating_add(policy_culture);
            if progress.current_technology.is_some() {
                progress.technology_progress = progress.technology_progress.saturating_add(science);
            }
            if progress.current_civic.is_some() {
                progress.civic_progress = progress.civic_progress.saturating_add(culture);
            }
            if let Some(target) = progress.current_technology.clone()
                && let Some(definition) = city::base_ruleset().technology(&target)
                && progress.technology_progress >= i64::from(definition.cost())
            {
                progress.technology_progress -= i64::from(definition.cost());
                progress.technologies.insert(target);
                progress.current_technology = None;
            }
            if let Some(target) = progress.current_civic.clone()
                && let Some(definition) = city::base_ruleset().civic(&target)
                && progress.civic_progress >= i64::from(definition.cost())
            {
                progress.civic_progress -= i64::from(definition.cost());
                progress.civics.insert(target);
                progress.current_civic = None;
            }
            self.refresh_policy_slots(player);
        }
        self.check_victories();
    }

    fn refresh_policy_slots(&mut self, player: PlayerId) {
        if let Some(progress) = self.progress.get_mut(&player) {
            progress.policy_slots = PlayerProgress::slots_for_civics(progress.civics.len());
            progress
                .policies
                .retain(|slot, _| *slot < progress.policy_slots);
        }
    }

    fn check_victories(&mut self) {
        let players: Vec<_> = self.markers.keys().copied().collect();
        let technology_count = city::base_ruleset().technologies().count();
        let city_count = self.cities.len();
        for player in players {
            if self.victories.contains_key(&player) {
                continue;
            }
            let own_cities = self
                .cities
                .values()
                .filter(|city| city.owner == player)
                .count();
            let science = own_cities > 0
                && self
                    .progress
                    .get(&player)
                    .is_some_and(|progress| progress.technologies.len() == technology_count);
            let domination = own_cities > 0 && own_cities == city_count;
            if science {
                self.victories.insert(player, VictoryKind::Science);
                break;
            } else if domination {
                self.victories.insert(player, VictoryKind::Domination);
                break;
            }
        }
    }

    fn complete_production(&mut self, city_id: CityId, item: ProductionItem) -> bool {
        match item {
            ProductionItem::Building(building) => {
                let Some(city) = self.cities.get_mut(&city_id) else {
                    return false;
                };
                if city.has_building(building)
                    || building
                        .required_district()
                        .is_some_and(|required| !city.has_district(required))
                {
                    return false;
                }
                city.insert_building(building);
                true
            }
            ProductionItem::District { kind, tile } => {
                if self.validate_district_site(city_id, kind, tile).is_err() {
                    return false;
                }
                self.cities
                    .get_mut(&city_id)
                    .expect("district city was validated")
                    .insert_district(kind, tile);
                true
            }
            ProductionItem::Unit(kind) => {
                let Some(city) = self.cities.get(&city_id) else {
                    return false;
                };
                let owner = city.owner;
                let center = city.tile;
                let Some(map) = &self.map else {
                    return false;
                };
                let Some(tile) = map.topology().area(center, 1).into_iter().find(|tile| {
                    *tile != center
                        && map.tile(*tile).is_some_and(|state| {
                            Unit::movement_cost(state.terrain, false).is_some()
                        })
                        && !self.units.values().any(|unit| unit.tile == *tile)
                        && !self.cities.values().any(|other| other.tile == *tile)
                        && !self
                            .cities
                            .values()
                            .any(|other| other.districts.contains_key(tile))
                }) else {
                    return false;
                };
                let next_id = match self.units.keys().next_back() {
                    Some(id) => match id.index().checked_add(1) {
                        Some(index) => UnitId::new(index),
                        None => return false,
                    },
                    None => UnitId::new(0),
                };
                self.units
                    .insert(next_id, Unit::new(next_id, Some(owner), kind, tile));
                true
            }
        }
    }

    fn require_unit_owner(&self, unit: &Unit, player: PlayerId) -> Result<(), DispatchError> {
        if unit.owner == Some(player) {
            Ok(())
        } else {
            Err(DispatchError::UnauthorizedUnit(unit.id))
        }
    }

    fn tile_occupied(&self, tile: TileId) -> bool {
        self.tile_occupied_except_unit(tile, None)
    }

    fn tile_occupied_except_unit(&self, tile: TileId, ignored_unit: Option<UnitId>) -> bool {
        self.cities.values().any(|city| city.tile == tile)
            || self
                .cities
                .values()
                .any(|city| city.districts.contains_key(&tile))
            || self
                .units
                .values()
                .any(|unit| Some(unit.id) != ignored_unit && unit.tile == tile)
    }

    fn queued_district_at(&self, tile: TileId) -> bool {
        self.cities.values().any(|city| {
            matches!(
                city.production,
                Some(ProductionItem::District { tile: queued, .. }) if queued == tile
            )
        })
    }

    fn has_enemy_zoc(&self, player: PlayerId, tile: TileId) -> bool {
        let Some(map) = &self.map else { return false };
        self.units.values().any(|unit| {
            unit.owner != Some(player)
                && unit.kind.is_military()
                && map.topology().neighbors(unit.tile).contains(&tile)
        })
    }

    fn require_player(&self, player: PlayerId) -> Result<(), DispatchError> {
        if self.contains_player(player) {
            Ok(())
        } else {
            Err(DispatchError::UnknownPlayer(player))
        }
    }

    fn require_city_owner(&self, city: CityId, player: PlayerId) -> Result<(), DispatchError> {
        let owner = self
            .cities
            .get(&city)
            .ok_or(DispatchError::UnknownCity(city))?
            .owner;
        if owner == player {
            Ok(())
        } else {
            Err(DispatchError::UnauthorizedCity(city))
        }
    }
}

/// A seat-scoped observation with fog-of-war applied.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Observation {
    seat: PlayerId,
    turn: u32,
    own_marker: i64,
    units: Vec<UnitObservation>,
    cities: Vec<CityObservation>,
    progress: ProgressObservation,
    victory: Option<VictoryKind>,
    visible_tiles: Vec<VisibleTileObservation>,
    visible_units: Vec<VisibleUnitObservation>,
    visible_cities: Vec<VisibleCityObservation>,
}

impl Observation {
    /// Returns the observing seat.
    pub const fn seat(&self) -> PlayerId {
        self.seat
    }

    /// Returns the observed turn.
    pub const fn turn(&self) -> u32 {
        self.turn
    }

    /// Returns the marker visible to this seat.
    pub const fn own_marker(&self) -> i64 {
        self.own_marker
    }

    /// Returns the observing seat's visible units in stable ID order.
    pub fn units(&self) -> &[UnitObservation] {
        &self.units
    }

    /// Returns the observing seat's cities in stable ID order.
    pub fn cities(&self) -> &[CityObservation] {
        &self.cities
    }

    /// Returns the observing seat's technology, civic, and policy state.
    pub const fn progress(&self) -> &ProgressObservation {
        &self.progress
    }

    /// Returns the observing seat's victory, if one has been reached.
    pub const fn victory(&self) -> Option<VictoryKind> {
        self.victory
    }

    /// Returns visible map tiles with terrain and graph neighbors.
    pub fn visible_tiles(&self) -> &[VisibleTileObservation] {
        &self.visible_tiles
    }

    /// Returns units on visible tiles, including public owner identity.
    pub fn visible_units(&self) -> &[VisibleUnitObservation] {
        &self.visible_units
    }

    /// Returns cities on visible tiles, including public owner identity.
    pub fn visible_cities(&self) -> &[VisibleCityObservation] {
        &self.visible_cities
    }
}

/// The seat-scoped projection of progression state.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProgressObservation {
    /// Stored science toward the selected technology.
    pub technology_progress: i64,
    /// Stored culture toward the selected civic.
    pub civic_progress: i64,
    /// Selected technology.
    pub current_technology: Option<String>,
    /// Selected civic.
    pub current_civic: Option<String>,
    /// Completed technologies in stable identifier order.
    pub technologies: Vec<String>,
    /// Completed civics in stable identifier order.
    pub civics: Vec<String>,
    /// Number of unlocked policy slots.
    pub policy_slots: u8,
    /// Active policies in slot order.
    pub policies: Vec<(u8, String)>,
}

/// Fog-safe tile data available to an observing seat.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VisibleTileObservation {
    /// Stable tile ID.
    pub tile: TileId,
    /// Terrain category.
    pub terrain: Terrain,
    /// Feature category.
    pub feature: Feature,
    /// Optional river edge.
    pub river: Option<RiverDirection>,
    /// Optional visible resource.
    pub resource: Option<ResourceKind>,
    /// Whether the observing seat knows a road exists.
    pub road: bool,
    /// Whether a unit, city center, or district occupies this visible tile.
    pub occupied: bool,
    /// A visible construction site reserved by a pending district order.
    pub district_reserved: bool,
    /// Topology neighbors in stable order.
    pub neighbors: Vec<TileId>,
}

/// Fog-safe unit data on a visible tile.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VisibleUnitObservation {
    /// Stable unit ID.
    pub id: UnitId,
    /// Public owner identity for a visible unit.
    pub owner: Option<PlayerId>,
    /// Unit archetype.
    pub kind: UnitKind,
    /// Occupied tile.
    pub tile: TileId,
    /// Current hit points.
    pub hp: i32,
    /// Current movement points.
    pub movement: i32,
    /// Effective attack strength, including visible promotions.
    pub attack: i32,
    /// Effective defense strength, including visible promotions.
    pub defense: i32,
}

/// Deterministic damage exchanged by two visible units in a single attack.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CombatDamage {
    /// Damage dealt to the defender, including any excess over remaining HP.
    pub to_defender: i32,
    /// Retaliation damage dealt to the attacker.
    pub to_attacker: i32,
}

impl From<&Unit> for VisibleUnitObservation {
    fn from(unit: &Unit) -> Self {
        Self {
            id: unit.id,
            owner: unit.owner,
            kind: unit.kind,
            tile: unit.tile,
            hp: unit.hp,
            movement: unit.movement,
            attack: unit.attack(),
            defense: unit.defense(),
        }
    }
}

impl VisibleUnitObservation {
    /// Returns hit points after one legal heal in the projected position.
    pub fn forecast_heal(self, in_enemy_zoc: bool) -> Result<i32, DispatchError> {
        if in_enemy_zoc {
            return Err(DispatchError::UnsafeHeal);
        }
        if self.hp >= self.kind.max_hp() {
            return Err(DispatchError::AlreadyFullHealth);
        }
        Ok((self.hp + 20).min(self.kind.max_hp()))
    }

    /// Resolves one combat exchange using the same rules as live dispatch.
    pub fn forecast_attack(
        self,
        defender: Self,
        distance: u32,
    ) -> Result<CombatDamage, DispatchError> {
        if self.owner == defender.owner
            || !self.kind.is_military()
            || !defender.kind.is_military()
            || distance > self.kind.range()
        {
            return Err(DispatchError::InvalidCombat);
        }
        if self.movement <= 0 {
            return Err(DispatchError::InsufficientMovement);
        }
        Ok(CombatDamage {
            to_defender: (self.attack - defender.defense / 2).max(1),
            to_attacker: if distance == 1 {
                (defender.attack - self.defense / 2).max(1)
            } else {
                0
            },
        })
    }
}

/// Fog-safe city data on a visible tile.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VisibleCityObservation {
    /// Stable city ID.
    pub id: CityId,
    /// Public owner identity for a visible city.
    pub owner: PlayerId,
    /// City center tile.
    pub tile: TileId,
    /// Public population estimate.
    pub population: u32,
    /// Visible district tiles that also block unit movement.
    pub districts: Vec<TileId>,
}

/// The fog-of-war-safe projection of one unit.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UnitObservation {
    /// Stable unit ID.
    pub id: UnitId,
    /// Unit archetype.
    pub kind: UnitKind,
    /// Tile occupied by the unit.
    pub tile: TileId,
    /// Current hit points.
    pub hp: i32,
    /// Current movement points.
    pub movement: i32,
    /// Current experience.
    pub xp: u8,
    /// Promotion bitset.
    pub promotions: u16,
    /// Remaining builder improvement charges.
    pub charges: u8,
}

/// The fog-of-war-safe projection of one city.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CityObservation {
    /// Stable city ID.
    pub id: CityId,
    /// City center tile.
    pub tile: TileId,
    /// Current population.
    pub population: u32,
    /// Stored food toward growth.
    pub food_stored: i64,
    /// Effective housing capacity.
    pub housing: u32,
    /// Stored production toward the queue item.
    pub production_stored: i64,
    /// Current production order.
    pub production: Option<ProductionItem>,
    /// Districts in stable tile order.
    pub districts: Vec<District>,
    /// Buildings in canonical enum order.
    pub buildings: Vec<BuildingKind>,
}

/// An event emitted after a command is accepted.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Event {
    sequence: u64,
    kind: EventKind,
}

impl Event {
    /// Returns the monotonically increasing event sequence.
    pub const fn sequence(&self) -> u64 {
        self.sequence
    }

    /// Returns the event payload.
    pub const fn kind(&self) -> &EventKind {
        &self.kind
    }
}

/// Event payloads emitted by the simulation boundary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EventKind {
    /// A seat marker changed.
    MarkerSet {
        /// The seat whose marker changed.
        player: PlayerId,
        /// The new marker value.
        value: i64,
    },
    /// The turn advanced.
    TurnAdvanced {
        /// The new turn number.
        turn: u32,
    },
    /// A player ended their turn while other seats remain active.
    TurnEnded {
        /// The seat that ended its turn.
        player: PlayerId,
    },
    /// A map tile was edited.
    MapEdited {
        /// The edited tile ID.
        tile: TileId,
    },
    /// A unit moved or changed health/promotion state.
    UnitChanged {
        /// The changed unit.
        unit: UnitId,
    },
    /// A player-owned unit was spawned.
    UnitSpawned {
        /// The spawned unit.
        unit: UnitId,
    },
    /// A road was built.
    RoadBuilt {
        /// The road tile.
        tile: TileId,
    },
    /// A combat exchange occurred.
    Combat {
        /// The attacking unit.
        attacker: UnitId,
        /// The defending unit.
        defender: UnitId,
        /// Damage dealt to the defender.
        attacker_damage: i32,
        /// Damage dealt to the attacker.
        defender_damage: i32,
    },
    /// A barbarian unit was spawned.
    BarbarianSpawned {
        /// The spawned unit.
        unit: UnitId,
    },
    /// A city was founded.
    CityFounded {
        /// The new city.
        city: CityId,
    },
    /// A city production order was queued.
    ProductionQueued {
        /// The city whose queue changed.
        city: CityId,
    },
    /// A technology or civic was selected for progress.
    ResearchQueued {
        /// The seat selecting the node.
        player: PlayerId,
        /// Stable node identifier.
        id: String,
    },
    /// A policy slot changed.
    PolicyChanged {
        /// The seat changing its policies.
        player: PlayerId,
        /// The changed slot.
        slot: u8,
    },
}

/// A deterministic command log that can be replayed from its initial state.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReplayLog {
    initial: SimulationState,
    commands: Vec<Command>,
}

impl ReplayLog {
    /// Creates an empty replay from an initial state.
    pub fn new(initial: SimulationState) -> Self {
        Self {
            initial,
            commands: Vec::new(),
        }
    }

    /// Returns the recorded commands.
    pub fn commands(&self) -> &[Command] {
        &self.commands
    }

    /// Replays commands and returns the resulting state.
    pub fn replay(&self) -> Result<SimulationState, DispatchError> {
        let mut state = self.initial.clone();
        for command in &self.commands {
            state.apply(command)?;
        }
        Ok(state)
    }

    /// Encodes the initial snapshot and command log.
    pub fn encode(&self) -> Vec<u8> {
        let initial = self.initial.encode();
        let mut bytes = Vec::new();
        bytes.extend_from_slice(REPLAY_MAGIC);
        put_u16(&mut bytes, REPLAY_FORMAT_VERSION);
        put_u32(&mut bytes, initial.len() as u32);
        bytes.extend_from_slice(&initial);
        put_u32(&mut bytes, self.commands.len() as u32);
        for command in &self.commands {
            let encoded = command.encode();
            put_u32(&mut bytes, encoded.len() as u32);
            bytes.extend_from_slice(&encoded);
        }
        bytes
    }

    /// Decodes a replay log with strict length and version checks.
    pub fn decode(bytes: &[u8]) -> Result<Self, SerializationError> {
        let mut reader = Reader::new(bytes);
        reader.expect_magic(REPLAY_MAGIC)?;
        reader.expect_version(REPLAY_FORMAT_VERSION)?;
        let initial = SimulationState::decode(reader.bytes()?)?;
        let count = reader.count()?;
        let mut commands = Vec::with_capacity(count);
        for _ in 0..count {
            commands.push(Command::decode(reader.bytes()?)?);
        }
        reader.finish()?;
        Ok(Self { initial, commands })
    }
}

/// A session owns canonical state and exposes only commands and observations.
#[derive(Clone, Debug)]
pub struct Session {
    state: SimulationState,
    replay: ReplayLog,
    events: Vec<Event>,
    next_event_sequence: u64,
}

impl Session {
    /// Creates a session with the supplied seats.
    pub fn new(players: impl IntoIterator<Item = PlayerId>) -> Self {
        let state = SimulationState::new(players);
        Self {
            replay: ReplayLog::new(state.clone()),
            state,
            events: Vec::new(),
            next_event_sequence: 0,
        }
    }

    /// Creates a session with the supplied seats and map.
    pub fn with_map(players: impl IntoIterator<Item = PlayerId>, map: MapState) -> Self {
        let state = SimulationState::with_map(players, map);
        Self {
            replay: ReplayLog::new(state.clone()),
            state,
            events: Vec::new(),
            next_event_sequence: 0,
        }
    }

    /// Creates a session from a decoded state snapshot.
    pub fn from_state(state: SimulationState) -> Self {
        Self {
            replay: ReplayLog::new(state.clone()),
            state,
            events: Vec::new(),
            next_event_sequence: 0,
        }
    }

    /// Returns a clone of the canonical state snapshot.
    pub fn state_snapshot(&self) -> SimulationState {
        self.state.clone()
    }

    /// Returns the canonical command registry.
    pub const fn registry(&self) -> CommandRegistry {
        command_registry()
    }

    /// Dispatches one command and records its event and replay entry.
    pub fn dispatch(&mut self, command: Command) -> Result<&Event, DispatchError> {
        self.registry().validate(&command)?;
        let next_sequence = self
            .next_event_sequence
            .checked_add(1)
            .ok_or(DispatchError::EventSequenceOverflow)?;
        let kind = self.state.apply(&command)?;
        self.replay.commands.push(command);
        self.events.push(Event {
            sequence: self.next_event_sequence,
            kind,
        });
        self.next_event_sequence = next_sequence;
        Ok(self.events.last().expect("event was just pushed"))
    }

    /// Projects private seat data and the board within two steps of its cities and units.
    pub fn observe(&self, seat: PlayerId) -> Result<Observation, DispatchError> {
        if !self.state.contains_player(seat) {
            return Err(DispatchError::UnknownPlayer(seat));
        }
        let units = self
            .state
            .units
            .values()
            .filter(|unit| unit.owner == Some(seat))
            .map(|unit| UnitObservation {
                id: unit.id,
                kind: unit.kind,
                tile: unit.tile,
                hp: unit.hp,
                movement: unit.movement,
                xp: unit.xp,
                promotions: unit.promotions,
                charges: unit.charges,
            })
            .collect();
        let cities = self
            .state
            .cities
            .values()
            .filter(|city| city.owner == seat)
            .map(|city| CityObservation {
                id: city.id,
                tile: city.tile,
                population: city.population,
                food_stored: city.food_stored,
                housing: city.housing_capacity(),
                production_stored: city.production_stored,
                production: city.production,
                districts: city.districts().collect(),
                buildings: city.buildings().collect(),
            })
            .collect();
        let progress = self
            .state
            .progress
            .get(&seat)
            .ok_or(DispatchError::UnknownPlayer(seat))?;
        let visible_tiles = self.visible_tiles(seat);
        let mut occupied: BTreeSet<_> = self.state.units.values().map(|unit| unit.tile).collect();
        let mut construction_sites = BTreeSet::new();
        for city in self.state.cities.values() {
            occupied.insert(city.tile);
            occupied.extend(city.districts.keys().copied());
            if let Some(ProductionItem::District { tile, .. }) = city.production {
                construction_sites.insert(tile);
            }
        }
        let visible_tiles_observation = self
            .state
            .map
            .as_ref()
            .into_iter()
            .flat_map(|map| {
                visible_tiles.iter().filter_map(|tile| {
                    map.tile(*tile).map(|state| VisibleTileObservation {
                        tile: *tile,
                        terrain: state.terrain,
                        feature: state.feature,
                        river: state.river,
                        resource: state.resource,
                        road: self.state.roads.contains(tile),
                        occupied: occupied.contains(tile),
                        district_reserved: construction_sites.contains(tile),
                        neighbors: map
                            .topology()
                            .neighbors(*tile)
                            .into_iter()
                            .filter(|neighbor| visible_tiles.contains(neighbor))
                            .collect(),
                    })
                })
            })
            .collect();
        let visible_units = self
            .state
            .units
            .values()
            .filter(|unit| visible_tiles.contains(&unit.tile))
            .map(VisibleUnitObservation::from)
            .collect();
        let visible_cities = self
            .state
            .cities
            .values()
            .filter(|city| visible_tiles.contains(&city.tile))
            .map(|city| VisibleCityObservation {
                id: city.id,
                owner: city.owner,
                tile: city.tile,
                population: city.population,
                districts: city
                    .districts()
                    .filter_map(|district| {
                        visible_tiles
                            .contains(&district.tile)
                            .then_some(district.tile)
                    })
                    .collect(),
            })
            .collect();
        Ok(Observation {
            seat,
            turn: self.state.turn,
            own_marker: self
                .state
                .marker(seat)
                .ok_or(DispatchError::UnknownPlayer(seat))?,
            units,
            cities,
            progress: ProgressObservation {
                technology_progress: progress.technology_progress,
                civic_progress: progress.civic_progress,
                current_technology: progress.current_technology.clone(),
                current_civic: progress.current_civic.clone(),
                technologies: progress.technologies.iter().cloned().collect(),
                civics: progress.civics.iter().cloned().collect(),
                policy_slots: progress.policy_slots,
                policies: progress
                    .policies
                    .iter()
                    .map(|(slot, id)| (*slot, id.clone()))
                    .collect(),
            },
            victory: self.state.victories.get(&seat).copied(),
            visible_tiles: visible_tiles_observation,
            visible_units,
            visible_cities,
        })
    }

    /// Returns the current state hash.
    pub fn state_hash(&self) -> StateHash {
        self.state.state_hash()
    }

    /// Returns the declared winner for a referee checking after each dispatch.
    /// Sessions continued beyond victory can contain multiple declarations.
    /// In that case this returns the lowest seat ID.
    pub fn winner(&self) -> Option<(PlayerId, VictoryKind)> {
        self.state
            .victories
            .first_key_value()
            .map(|(&seat, &kind)| (seat, kind))
    }

    /// Returns all events in sequence order.
    pub fn events(&self) -> &[Event] {
        &self.events
    }

    /// Drains delivered events so long-running consumers can release them.
    pub fn drain_events(&mut self) -> Vec<Event> {
        std::mem::take(&mut self.events)
    }

    /// Returns the current replay log.
    pub fn replay(&self) -> &ReplayLog {
        &self.replay
    }

    fn visible_tiles(&self, seat: PlayerId) -> BTreeSet<TileId> {
        let Some(map) = &self.state.map else {
            return BTreeSet::new();
        };
        let sources = self
            .state
            .units
            .values()
            .filter(|unit| unit.owner == Some(seat))
            .map(|unit| unit.tile)
            .chain(
                self.state
                    .cities
                    .values()
                    .filter(|city| city.owner == seat)
                    .map(|city| city.tile),
            );
        let mut tiles: BTreeSet<_> = sources.collect();
        let mut frontier: Vec<_> = tiles.iter().copied().collect();
        for _ in 0..2 {
            frontier = frontier
                .into_iter()
                .flat_map(|tile| map.topology().neighbors(tile))
                .filter(|tile| tiles.insert(*tile))
                .collect();
        }
        tiles
    }
}

/// Errors raised while applying commands.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DispatchError {
    /// A command named a seat absent from the state.
    UnknownPlayer(PlayerId),
    /// The turn counter would overflow.
    TurnOverflow,
    /// The seat already ended its current turn.
    TurnAlreadyEnded,
    /// The command ID was absent from the registry.
    UnregisteredCommand(&'static str),
    /// A map was required by an editor command.
    NoMap,
    /// A map edit failed validation.
    Map(MapError),
    /// A unit ID was absent.
    UnknownUnit(UnitId),
    /// A unit ID was already present.
    UnitAlreadyExists(UnitId),
    /// A unit is owned by another seat.
    UnauthorizedUnit(UnitId),
    /// A unit cannot perform a builder action.
    InvalidBuilder,
    /// A builder has no remaining charges.
    NoBuilderCharges,
    /// A destination or combat target was invalid.
    InvalidMove,
    /// A tile is occupied.
    Occupied(TileId),
    /// A tile was absent from the map.
    UnknownTile(TileId),
    /// A movement or attack was blocked by enemy zones of control.
    ZocBlocked,
    /// A unit lacks enough movement points.
    InsufficientMovement,
    /// A tile cannot be traversed.
    Impassable(TileId),
    /// An attack was invalid.
    InvalidCombat,
    /// A unit cannot heal because it is in enemy control.
    UnsafeHeal,
    /// A unit already has full health.
    AlreadyFullHealth,
    /// A promotion is unavailable.
    PromotionUnavailable,
    /// A barbarian command named a non-barbarian kind.
    InvalidBarbarianKind,
    /// Unit IDs are exhausted.
    UnitIdOverflow,
    /// A city ID was already present.
    CityAlreadyExists(CityId),
    /// Every representable city identifier has been allocated.
    CityIdOverflow,
    /// A city ID was absent.
    UnknownCity(CityId),
    /// A city is owned by another seat.
    UnauthorizedCity(CityId),
    /// The founding unit is not a settler.
    InvalidFounder(UnitId),
    /// A city center tile is not valid.
    InvalidCityTile(TileId),
    /// A district tile is invalid or outside the city area.
    InvalidDistrictTile(TileId),
    /// A production order is invalid.
    InvalidProduction,
    /// A building is already complete.
    BuildingAlreadyBuilt(BuildingKind),
    /// A required district has not been built.
    MissingDistrict(DistrictKind),
    /// The event sequence would overflow.
    EventSequenceOverflow,
    /// A technology identifier was absent from the ruleset.
    UnknownTechnology,
    /// A civic identifier was absent from the ruleset.
    UnknownCivic,
    /// A policy identifier was absent from the ruleset.
    UnknownPolicy,
    /// A progression item was already complete.
    AlreadyComplete,
    /// A progression prerequisite was incomplete.
    MissingPrerequisite,
    /// A policy slot was outside the unlocked range.
    InvalidPolicySlot,
    /// A policy was already active in another slot.
    PolicyAlreadyActive,
    /// A player already reached a victory and cannot change state.
    VictoryAlreadyReached,
}

impl fmt::Display for DispatchError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownPlayer(player) => write!(formatter, "unknown player {}", player.index()),
            Self::TurnOverflow => formatter.write_str("turn counter overflow"),
            Self::TurnAlreadyEnded => formatter.write_str("player already ended this turn"),
            Self::UnregisteredCommand(id) => write!(formatter, "unregistered command {id}"),
            Self::NoMap => formatter.write_str("command requires a map"),
            Self::Map(error) => write!(formatter, "map command rejected: {error}"),
            Self::UnknownUnit(unit) => write!(formatter, "unknown unit {}", unit.index()),
            Self::UnitAlreadyExists(unit) => {
                write!(formatter, "unit {} already exists", unit.index())
            }
            Self::UnauthorizedUnit(unit) => write!(formatter, "unauthorized unit {}", unit.index()),
            Self::InvalidBuilder => formatter.write_str("unit is not a builder"),
            Self::NoBuilderCharges => formatter.write_str("builder has no charges remaining"),
            Self::InvalidMove => formatter.write_str("invalid unit move"),
            Self::Occupied(tile) => write!(formatter, "occupied tile {}", tile.index()),
            Self::UnknownTile(tile) => write!(formatter, "unknown tile {}", tile.index()),
            Self::ZocBlocked => formatter.write_str("movement blocked by zone of control"),
            Self::InsufficientMovement => formatter.write_str("insufficient movement points"),
            Self::Impassable(tile) => write!(formatter, "impassable tile {}", tile.index()),
            Self::InvalidCombat => formatter.write_str("invalid combat action"),
            Self::UnsafeHeal => formatter.write_str("healing is unsafe in enemy control"),
            Self::AlreadyFullHealth => formatter.write_str("unit is already at full health"),
            Self::PromotionUnavailable => formatter.write_str("promotion is unavailable"),
            Self::InvalidBarbarianKind => {
                formatter.write_str("barbarian command requires barbarian kind")
            }
            Self::UnitIdOverflow => formatter.write_str("unit ID space exhausted"),
            Self::CityAlreadyExists(city) => {
                write!(formatter, "city {} already exists", city.index())
            }
            Self::CityIdOverflow => formatter.write_str("city ID space exhausted"),
            Self::UnknownCity(city) => write!(formatter, "unknown city {}", city.index()),
            Self::UnauthorizedCity(city) => write!(formatter, "unauthorized city {}", city.index()),
            Self::InvalidFounder(unit) => {
                write!(formatter, "unit {} cannot found a city", unit.index())
            }
            Self::InvalidCityTile(tile) => write!(formatter, "invalid city tile {}", tile.index()),
            Self::InvalidDistrictTile(tile) => {
                write!(formatter, "invalid district tile {}", tile.index())
            }
            Self::InvalidProduction => formatter.write_str("invalid production order"),
            Self::BuildingAlreadyBuilt(building) => {
                write!(formatter, "building {:?} already exists", building)
            }
            Self::MissingDistrict(district) => {
                write!(formatter, "building requires district {:?}", district)
            }
            Self::EventSequenceOverflow => formatter.write_str("event sequence overflow"),
            Self::UnknownTechnology => formatter.write_str("unknown technology"),
            Self::UnknownCivic => formatter.write_str("unknown civic"),
            Self::UnknownPolicy => formatter.write_str("unknown policy"),
            Self::AlreadyComplete => formatter.write_str("progression item already complete"),
            Self::MissingPrerequisite => {
                formatter.write_str("progression prerequisite is incomplete")
            }
            Self::InvalidPolicySlot => formatter.write_str("invalid policy slot"),
            Self::PolicyAlreadyActive => formatter.write_str("policy is already active"),
            Self::VictoryAlreadyReached => formatter.write_str("victory already reached"),
        }
    }
}

impl std::error::Error for DispatchError {}

/// Errors raised by the custom binary format.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SerializationError {
    /// Input ended before a complete field was available.
    Truncated,
    /// Input had the wrong magic bytes.
    InvalidMagic,
    /// Input used an unsupported format version.
    UnsupportedVersion(u16),
    /// A length exceeded the format's safe bound.
    InvalidLength,
    /// A command or table tag was unknown.
    InvalidTag(u8),
    /// A length-prefixed string was not valid UTF-8.
    InvalidUtf8,
    /// A table contained a duplicate key.
    DuplicateKey,
    /// Canonical map keys were not strictly ascending.
    NonCanonicalOrder,
    /// Embedded map data failed map validation.
    InvalidMap,
    /// Unit or road data violated a simulation invariant.
    InvalidUnitState,
    /// City, district, building, or production data violated a simulation invariant.
    InvalidCityState,
    /// Technology, civic, policy, or victory data violated a simulation invariant.
    InvalidProgressState,
    /// Bytes remained after the expected document.
    TrailingBytes,
}

impl fmt::Display for SerializationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Truncated => formatter.write_str("truncated binary document"),
            Self::InvalidMagic => formatter.write_str("invalid binary document magic"),
            Self::UnsupportedVersion(version) => {
                write!(formatter, "unsupported format version {version}")
            }
            Self::InvalidLength => formatter.write_str("invalid binary document length"),
            Self::InvalidTag(tag) => write!(formatter, "invalid binary document tag {tag}"),
            Self::InvalidUtf8 => formatter.write_str("invalid UTF-8 in binary document"),
            Self::DuplicateKey => formatter.write_str("duplicate key in binary document"),
            Self::NonCanonicalOrder => {
                formatter.write_str("non-canonical key order in binary document")
            }
            Self::InvalidMap => formatter.write_str("invalid embedded map document"),
            Self::InvalidUnitState => formatter.write_str("invalid unit state in binary document"),
            Self::InvalidCityState => formatter.write_str("invalid city state in binary document"),
            Self::InvalidProgressState => {
                formatter.write_str("invalid progression state in binary document")
            }
            Self::TrailingBytes => formatter.write_str("trailing bytes in binary document"),
        }
    }
}

impl std::error::Error for SerializationError {}

struct Reader<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> Reader<'a> {
    const fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }

    fn expect_magic(&mut self, expected: &[u8; 4]) -> Result<(), SerializationError> {
        if self.take(4)? == expected {
            Ok(())
        } else {
            Err(SerializationError::InvalidMagic)
        }
    }

    fn expect_version(&mut self, expected: u16) -> Result<(), SerializationError> {
        let version = self.u16()?;
        if version == expected {
            Ok(())
        } else {
            Err(SerializationError::UnsupportedVersion(version))
        }
    }

    fn u8(&mut self) -> Result<u8, SerializationError> {
        Ok(*self.take(1)?.first().ok_or(SerializationError::Truncated)?)
    }

    fn u16(&mut self) -> Result<u16, SerializationError> {
        Ok(u16::from_le_bytes(self.take_array()?))
    }

    fn u32(&mut self) -> Result<u32, SerializationError> {
        Ok(u32::from_le_bytes(self.take_array()?))
    }

    fn i64(&mut self) -> Result<i64, SerializationError> {
        Ok(i64::from_le_bytes(self.take_array()?))
    }

    fn count(&mut self) -> Result<usize, SerializationError> {
        let count = self.u32()? as usize;
        if count > 1_000_000 {
            Err(SerializationError::InvalidLength)
        } else {
            Ok(count)
        }
    }

    fn bytes(&mut self) -> Result<&'a [u8], SerializationError> {
        let length = self.count()?;
        self.take(length)
    }

    fn string(&mut self) -> Result<String, SerializationError> {
        std::str::from_utf8(self.bytes()?)
            .map(str::to_owned)
            .map_err(|_| SerializationError::InvalidUtf8)
    }

    fn take_array<const N: usize>(&mut self) -> Result<[u8; N], SerializationError> {
        self.take(N)?
            .try_into()
            .map_err(|_| SerializationError::Truncated)
    }

    fn take(&mut self, length: usize) -> Result<&'a [u8], SerializationError> {
        let end = self
            .offset
            .checked_add(length)
            .ok_or(SerializationError::InvalidLength)?;
        let slice = self
            .bytes
            .get(self.offset..end)
            .ok_or(SerializationError::Truncated)?;
        self.offset = end;
        Ok(slice)
    }

    fn finish(self) -> Result<(), SerializationError> {
        if self.offset == self.bytes.len() {
            Ok(())
        } else {
            Err(SerializationError::TrailingBytes)
        }
    }
}

fn put_u16(bytes: &mut Vec<u8>, value: u16) {
    bytes.extend_from_slice(&value.to_le_bytes());
}

fn put_u8(bytes: &mut Vec<u8>, value: u8) {
    bytes.push(value);
}

fn put_u32(bytes: &mut Vec<u8>, value: u32) {
    bytes.extend_from_slice(&value.to_le_bytes());
}

fn put_i64(bytes: &mut Vec<u8>, value: i64) {
    bytes.extend_from_slice(&value.to_le_bytes());
}

fn put_string(bytes: &mut Vec<u8>, value: &str) {
    put_u32(bytes, value.len() as u32);
    bytes.extend_from_slice(value.as_bytes());
}

fn encode_production_item(bytes: &mut Vec<u8>, item: ProductionItem) {
    match item {
        ProductionItem::Unit(kind) => {
            put_u8(bytes, 0);
            put_u8(bytes, kind as u8);
        }
        ProductionItem::Building(kind) => {
            put_u8(bytes, 1);
            put_u8(bytes, kind as u8);
        }
        ProductionItem::District { kind, tile } => {
            put_u8(bytes, 2);
            put_u8(bytes, kind as u8);
            put_u32(bytes, tile.index());
        }
    }
}

fn decode_production_item(reader: &mut Reader<'_>) -> Result<ProductionItem, SerializationError> {
    let tag = reader.u8()?;
    match tag {
        0 => Ok(ProductionItem::Unit(
            UnitKind::from_u8(reader.u8()?).ok_or(SerializationError::InvalidTag(tag))?,
        )),
        1 => Ok(ProductionItem::Building(
            BuildingKind::from_u8(reader.u8()?).ok_or(SerializationError::InvalidTag(tag))?,
        )),
        2 => Ok(ProductionItem::District {
            kind: DistrictKind::from_u8(reader.u8()?).ok_or(SerializationError::InvalidTag(tag))?,
            tile: TileId::new(reader.u32()?),
        }),
        other => Err(SerializationError::InvalidTag(other)),
    }
}

fn hash_production_item(hasher: &mut StateHasher, item: Option<ProductionItem>) {
    match item {
        None => hasher.write_u64(u64::MAX),
        Some(ProductionItem::Unit(kind)) => {
            hasher.write_u64(0);
            hasher.write_u64(kind as u64);
        }
        Some(ProductionItem::Building(kind)) => {
            hasher.write_u64(1);
            hasher.write_u64(kind as u64);
        }
        Some(ProductionItem::District { kind, tile }) => {
            hasher.write_u64(2);
            hasher.write_u64(kind as u64);
            hasher.write_u64(tile.index() as u64);
        }
    }
}

fn hash_city(hasher: &mut StateHasher, id: CityId, city: &City) {
    hasher.write_u64(id.index() as u64);
    hasher.write_u64(city.owner.index() as u64);
    hasher.write_u64(city.tile.index() as u64);
    hasher.write_u64(city.population as u64);
    hasher.write_i64(city.food_stored);
    hasher.write_u64(city.housing as u64);
    hasher.write_i64(city.production_stored);
    hash_production_item(hasher, city.production);
    hasher.write_u64(city.districts.len() as u64);
    for (tile, kind) in &city.districts {
        hasher.write_u64(tile.index() as u64);
        hasher.write_u64(*kind as u64);
    }
    hasher.write_u64(city.buildings.len() as u64);
    for building in &city.buildings {
        hasher.write_u64(*building as u64);
    }
}

fn hash_progress(hasher: &mut StateHasher, player: PlayerId, progress: &PlayerProgress) {
    hasher.write_u64(player.index() as u64);
    hasher.write_i64(progress.technology_progress);
    hasher.write_i64(progress.civic_progress);
    match &progress.current_technology {
        Some(id) => hasher.write_bytes(id.as_bytes()),
        None => hasher.write_u64(u64::MAX),
    }
    match &progress.current_civic {
        Some(id) => hasher.write_bytes(id.as_bytes()),
        None => hasher.write_u64(u64::MAX),
    }
    hasher.write_u64(progress.technologies.len() as u64);
    for id in &progress.technologies {
        hasher.write_bytes(id.as_bytes());
    }
    hasher.write_u64(progress.civics.len() as u64);
    for id in &progress.civics {
        hasher.write_bytes(id.as_bytes());
    }
    hasher.write_u64(progress.policy_slots as u64);
    hasher.write_u64(progress.policies.len() as u64);
    for (slot, policy) in &progress.policies {
        hasher.write_u64(*slot as u64);
        hasher.write_bytes(policy.as_bytes());
    }
}

fn encode_progress(bytes: &mut Vec<u8>, player: PlayerId, progress: &PlayerProgress) {
    put_u32(bytes, player.index());
    put_i64(bytes, progress.technology_progress);
    put_i64(bytes, progress.civic_progress);
    put_u8(
        bytes,
        if progress.current_technology.is_some() {
            1
        } else {
            255
        },
    );
    if let Some(id) = &progress.current_technology {
        put_string(bytes, id);
    }
    put_u8(
        bytes,
        if progress.current_civic.is_some() {
            1
        } else {
            255
        },
    );
    if let Some(id) = &progress.current_civic {
        put_string(bytes, id);
    }
    put_u32(bytes, progress.technologies.len() as u32);
    for id in &progress.technologies {
        put_string(bytes, id);
    }
    put_u32(bytes, progress.civics.len() as u32);
    for id in &progress.civics {
        put_string(bytes, id);
    }
    put_u8(bytes, progress.policy_slots);
    put_u32(bytes, progress.policies.len() as u32);
    for (slot, policy) in &progress.policies {
        put_u8(bytes, *slot);
        put_string(bytes, policy);
    }
}

fn decode_progress(
    reader: &mut Reader<'_>,
) -> Result<(PlayerId, PlayerProgress), SerializationError> {
    let player = PlayerId::new(reader.u32()?);
    let technology_progress = reader.i64()?;
    let civic_progress = reader.i64()?;
    let current_technology = match reader.u8()? {
        255 => None,
        1 => Some(reader.string()?),
        tag => return Err(SerializationError::InvalidTag(tag)),
    };
    let current_civic = match reader.u8()? {
        255 => None,
        1 => Some(reader.string()?),
        tag => return Err(SerializationError::InvalidTag(tag)),
    };
    let technology_count = reader.count()?;
    let mut technologies = BTreeSet::new();
    for _ in 0..technology_count {
        let id = reader.string()?;
        if technologies.last().is_some_and(|previous| id <= *previous) {
            return Err(SerializationError::NonCanonicalOrder);
        }
        if !technologies.insert(id) {
            return Err(SerializationError::DuplicateKey);
        }
    }
    let civic_count = reader.count()?;
    let mut civics = BTreeSet::new();
    for _ in 0..civic_count {
        let id = reader.string()?;
        if civics.last().is_some_and(|previous| id <= *previous) {
            return Err(SerializationError::NonCanonicalOrder);
        }
        if !civics.insert(id) {
            return Err(SerializationError::DuplicateKey);
        }
    }
    let policy_slots = reader.u8()?;
    let policy_count = reader.count()?;
    let mut policies = BTreeMap::new();
    for _ in 0..policy_count {
        let slot = reader.u8()?;
        let policy = reader.string()?;
        if policies
            .keys()
            .next_back()
            .is_some_and(|previous| slot <= *previous)
        {
            return Err(SerializationError::NonCanonicalOrder);
        }
        if slot >= policy_slots || policies.insert(slot, policy).is_some() {
            return Err(SerializationError::InvalidProgressState);
        }
    }
    if technology_progress < 0 || civic_progress < 0 {
        return Err(SerializationError::InvalidProgressState);
    }
    let progress = PlayerProgress {
        technology_progress,
        civic_progress,
        current_technology,
        current_civic,
        technologies,
        civics,
        policy_slots,
        policies,
    };
    validate_progress_state(&progress)?;
    Ok((player, progress))
}

fn validate_progress_state(progress: &PlayerProgress) -> Result<(), SerializationError> {
    let ruleset = city::base_ruleset();
    for id in &progress.technologies {
        let definition = ruleset
            .technology(id)
            .ok_or(SerializationError::InvalidProgressState)?;
        if definition
            .prerequisites()
            .iter()
            .any(|required| !progress.technologies.contains(required))
        {
            return Err(SerializationError::InvalidProgressState);
        }
    }
    for id in &progress.civics {
        let definition = ruleset
            .civic(id)
            .ok_or(SerializationError::InvalidProgressState)?;
        if definition
            .prerequisites()
            .iter()
            .any(|required| !progress.civics.contains(required))
        {
            return Err(SerializationError::InvalidProgressState);
        }
    }
    let expected_slots = PlayerProgress::slots_for_civics(progress.civics.len());
    if progress.policy_slots != expected_slots {
        return Err(SerializationError::InvalidProgressState);
    }
    let mut active_policies = BTreeSet::new();
    for policy in progress.policies.values() {
        if !active_policies.insert(policy) {
            return Err(SerializationError::InvalidProgressState);
        }
        let definition = ruleset
            .policy(policy)
            .ok_or(SerializationError::InvalidProgressState)?;
        if definition
            .required_civic()
            .is_some_and(|required| !progress.civics.contains(required))
        {
            return Err(SerializationError::InvalidProgressState);
        }
    }
    if let Some(id) = &progress.current_technology {
        let definition = ruleset
            .technology(id)
            .ok_or(SerializationError::InvalidProgressState)?;
        if progress.technologies.contains(id)
            || definition
                .prerequisites()
                .iter()
                .any(|required| !progress.technologies.contains(required))
        {
            return Err(SerializationError::InvalidProgressState);
        }
        if progress.technology_progress >= i64::from(definition.cost()) {
            return Err(SerializationError::InvalidProgressState);
        }
    }
    if let Some(id) = &progress.current_civic {
        let definition = ruleset
            .civic(id)
            .ok_or(SerializationError::InvalidProgressState)?;
        if progress.civics.contains(id)
            || definition
                .prerequisites()
                .iter()
                .any(|required| !progress.civics.contains(required))
        {
            return Err(SerializationError::InvalidProgressState);
        }
        if progress.civic_progress >= i64::from(definition.cost()) {
            return Err(SerializationError::InvalidProgressState);
        }
    }
    Ok(())
}

fn encode_city(bytes: &mut Vec<u8>, id: CityId, city: &City) {
    put_u32(bytes, id.index());
    put_u32(bytes, city.owner.index());
    put_u32(bytes, city.tile.index());
    put_u32(bytes, city.population);
    put_i64(bytes, city.food_stored);
    put_u32(bytes, city.housing);
    put_i64(bytes, city.production_stored);
    put_u8(bytes, city.production.map_or(255, |_| 0));
    if let Some(item) = city.production {
        let mut item_bytes = Vec::new();
        encode_production_item(&mut item_bytes, item);
        bytes.extend_from_slice(&item_bytes);
    }
    put_u32(bytes, city.districts.len() as u32);
    for (tile, kind) in &city.districts {
        put_u32(bytes, tile.index());
        put_u8(bytes, *kind as u8);
    }
    put_u32(bytes, city.buildings.len() as u32);
    for building in &city.buildings {
        put_u8(bytes, *building as u8);
    }
}

fn decode_city(reader: &mut Reader<'_>) -> Result<(CityId, City), SerializationError> {
    let id = CityId::new(reader.u32()?);
    let owner = PlayerId::new(reader.u32()?);
    let tile = TileId::new(reader.u32()?);
    let population = reader.u32()?;
    let food_stored = reader.i64()?;
    let housing = reader.u32()?;
    let production_stored = reader.i64()?;
    let production = match reader.u8()? {
        255 => None,
        0 => Some(decode_production_item(reader)?),
        tag => return Err(SerializationError::InvalidTag(tag)),
    };
    let district_count = reader.count()?;
    let mut districts = BTreeMap::new();
    let mut district_kinds = BTreeSet::new();
    let mut previous_district_tile = None;
    for _ in 0..district_count {
        let district_tile = TileId::new(reader.u32()?);
        if previous_district_tile.is_some_and(|previous| district_tile <= previous) {
            return Err(SerializationError::NonCanonicalOrder);
        }
        previous_district_tile = Some(district_tile);
        let kind = DistrictKind::from_u8(reader.u8()?).ok_or(SerializationError::InvalidTag(0))?;
        if kind == DistrictKind::CityCenter || !district_kinds.insert(kind) {
            return Err(SerializationError::InvalidCityState);
        }
        if districts.insert(district_tile, kind).is_some() {
            return Err(SerializationError::DuplicateKey);
        }
    }
    let building_count = reader.count()?;
    let mut buildings = BTreeSet::new();
    let mut previous_building = None;
    for _ in 0..building_count {
        let kind = BuildingKind::from_u8(reader.u8()?).ok_or(SerializationError::InvalidTag(0))?;
        if previous_building.is_some_and(|previous| kind <= previous) {
            return Err(SerializationError::NonCanonicalOrder);
        }
        previous_building = Some(kind);
        if !buildings.insert(kind) {
            return Err(SerializationError::DuplicateKey);
        }
    }
    if population == 0 || housing != 2 || food_stored < 0 || production_stored < 0 {
        return Err(SerializationError::InvalidCityState);
    }
    let city = City {
        id,
        owner,
        tile,
        population,
        food_stored,
        housing,
        production_stored,
        production,
        districts,
        buildings,
    };
    if city.production.is_some_and(|item| {
        !item.is_valid()
            || matches!(
                item,
                ProductionItem::District {
                    kind: DistrictKind::CityCenter,
                    ..
                }
            )
    }) {
        return Err(SerializationError::InvalidCityState);
    }
    Ok((id, city))
}

fn decode_terrain(reader: &mut Reader<'_>) -> Result<Terrain, SerializationError> {
    let value = reader.u8()?;
    Terrain::from_u8(value).ok_or(SerializationError::InvalidTag(value))
}

fn decode_feature(reader: &mut Reader<'_>) -> Result<Feature, SerializationError> {
    let value = reader.u8()?;
    Feature::from_u8(value).ok_or(SerializationError::InvalidTag(value))
}

fn decode_river(reader: &mut Reader<'_>) -> Result<Option<RiverDirection>, SerializationError> {
    let value = reader.u8()?;
    if value == 255 {
        Ok(None)
    } else {
        RiverDirection::from_u8(value)
            .map(Some)
            .ok_or(SerializationError::InvalidTag(value))
    }
}

fn decode_resource(reader: &mut Reader<'_>) -> Result<Option<ResourceKind>, SerializationError> {
    let value = reader.u8()?;
    if value == 255 {
        Ok(None)
    } else {
        ResourceKind::from_u8(value)
            .map(Some)
            .ok_or(SerializationError::InvalidTag(value))
    }
}

#[cfg(test)]
mod tests {
    use histeries_hex::{FlatHex, TileTopology};
    use histeries_rules_core::{CityId, PlayerId};

    use super::{
        BuildingKind, City, Command, DistrictKind, EventKind, MapState, ProductionItem,
        PromotionKind, ReplayLog, STATE_HASH_ALGORITHM, SerializationError, Session,
        SimulationState, StateHasher, Terrain, UnitId, UnitKind, VictoryKind, command_registry,
        generate_map,
    };

    #[test]
    fn registry_has_stable_command_ids() {
        let registry = command_registry();
        assert_eq!(registry.specs()[0].id, "barbarian.spawn");
        assert_eq!(registry.find("turn.end").unwrap().schema, "player:u32");
        assert_eq!(
            registry.find("city.found").unwrap().schema,
            "player:u32,city:u32,unit:u32"
        );
        assert_eq!(registry.specs().len(), 23);
        assert!(
            registry
                .validate(&Command::TurnEnd {
                    player: PlayerId::new(1),
                })
                .is_ok()
        );
        let city_command = Command::CitySetProduction {
            player: PlayerId::new(1),
            city: CityId::new(2),
            item: ProductionItem::District {
                kind: DistrictKind::Campus,
                tile: super::TileId::new(4),
            },
        };
        assert_eq!(Command::decode(&city_command.encode()), Ok(city_command));
        let research_command = Command::ResearchTechnology {
            player: PlayerId::new(1),
            technology: "tech_01".to_owned(),
        };
        assert_eq!(
            Command::decode(&research_command.encode()),
            Ok(research_command)
        );
    }

    #[test]
    fn settle_allocates_distinct_ids_and_replays_without_prior_city_knowledge() {
        let first = PlayerId::new(3);
        let second = PlayerId::new(7);
        let map = MapState::blank(FlatHex::new(7, 1, false, false).unwrap());
        let mut session = Session::with_map([first, second], map);
        for (player, tile) in [(first, 0), (second, 6)] {
            let unit = UnitId::new(tile);
            session
                .dispatch(Command::UnitSpawn {
                    player,
                    kind: UnitKind::Settler,
                    unit,
                    tile: super::TileId::new(tile),
                })
                .unwrap();
            let settle = Command::CitySettle { player, unit };
            assert_eq!(Command::decode(&settle.encode()), Ok(settle.clone()));
            session.dispatch(settle).unwrap();
        }
        assert_ne!(
            session.observe(first).unwrap().cities()[0].id,
            session.observe(second).unwrap().cities()[0].id
        );
        assert_eq!(
            session.replay().replay().unwrap().state_hash(),
            session.state_hash()
        );
    }

    #[test]
    fn state_roundtrip_preserves_hash_and_order() {
        let first = SimulationState::new([PlayerId::new(2), PlayerId::new(1)]);
        let bytes = first.encode();
        let second = SimulationState::decode(&bytes).unwrap();
        assert_eq!(first, second);
        assert_eq!(first.state_hash(), second.state_hash());
        assert!(STATE_HASH_ALGORITHM.contains("little-endian"));
    }

    #[test]
    fn commands_events_observations_and_replay_are_consistent() {
        let player = PlayerId::new(1);
        let other = PlayerId::new(2);
        let mut session = Session::new([other, player]);
        session
            .dispatch(Command::EditorSetMarker { player, value: 42 })
            .unwrap();
        session.dispatch(Command::TurnEnd { player }).unwrap();
        assert_eq!(session.events().len(), 2);
        assert_eq!(session.events()[0].sequence(), 0);
        assert_eq!(session.events()[1].sequence(), 1);
        assert_eq!(session.events()[1].kind(), &EventKind::TurnEnded { player });
        assert_eq!(session.observe(player).unwrap().turn(), 0);
        assert_eq!(
            session.dispatch(Command::TurnEnd { player }),
            Err(super::DispatchError::TurnAlreadyEnded)
        );
        session
            .dispatch(Command::TurnEnd { player: other })
            .unwrap();
        assert_eq!(session.observe(player).unwrap().turn(), 1);
        assert_eq!(
            session.events().last().unwrap().kind(),
            &EventKind::TurnAdvanced { turn: 1 }
        );
        assert_eq!(session.observe(player).unwrap().own_marker(), 42);
        assert_eq!(session.observe(other).unwrap().own_marker(), 0);
        let roundtrip = SimulationState::decode(&session.state.encode()).unwrap();
        assert_eq!(roundtrip.state_hash(), session.state_hash());
        let replay = ReplayLog::decode(&session.replay().encode()).unwrap();
        assert_eq!(replay.replay().unwrap().state_hash(), session.state_hash());
        assert_eq!(session.drain_events().len(), 3);
        session.dispatch(Command::TurnEnd { player }).unwrap();
        assert_eq!(session.events()[0].sequence(), 3);
    }

    #[test]
    fn observation_reveals_local_enemies_without_leaking_hidden_state() {
        let player = PlayerId::new(1);
        let enemy = PlayerId::new(2);
        let topology = FlatHex::new(5, 5, false, false).unwrap();
        let mut session = Session::with_map([player, enemy], MapState::blank(topology));
        session
            .dispatch(Command::UnitSpawn {
                player,
                unit: UnitId::new(1),
                kind: UnitKind::Warrior,
                tile: super::TileId::new(0),
            })
            .unwrap();
        let adjacent = topology.neighbors(super::TileId::new(0))[0];
        session
            .dispatch(Command::UnitSpawn {
                player: enemy,
                unit: UnitId::new(2),
                kind: UnitKind::Warrior,
                tile: adjacent,
            })
            .unwrap();
        session
            .dispatch(Command::UnitSpawn {
                player: enemy,
                unit: UnitId::new(3),
                kind: UnitKind::Warrior,
                tile: super::TileId::new(24),
            })
            .unwrap();

        let observation = session.observe(player).unwrap();
        assert!(
            observation
                .visible_units()
                .iter()
                .any(|unit| unit.id == UnitId::new(2) && unit.owner == Some(enemy))
        );
        assert!(
            !observation
                .visible_units()
                .iter()
                .any(|unit| unit.id == UnitId::new(3))
        );
    }

    #[test]
    fn visible_city_projection_hides_districts_outside_fog() {
        let player = PlayerId::new(1);
        let enemy = PlayerId::new(2);
        let topology = FlatHex::new(5, 5, false, false).unwrap();
        let mut session = Session::with_map([player, enemy], MapState::blank(topology));
        session
            .dispatch(Command::UnitSpawn {
                player,
                unit: UnitId::new(1),
                kind: UnitKind::Settler,
                tile: super::TileId::new(0),
            })
            .unwrap();
        session
            .dispatch(Command::CityFound {
                player,
                city: CityId::new(1),
                unit: UnitId::new(1),
            })
            .unwrap();
        session
            .dispatch(Command::UnitSpawn {
                player: enemy,
                unit: UnitId::new(2),
                kind: UnitKind::Settler,
                tile: super::TileId::new(24),
            })
            .unwrap();
        session
            .dispatch(Command::CityFound {
                player: enemy,
                city: CityId::new(2),
                unit: UnitId::new(2),
            })
            .unwrap();
        let hidden_district = super::TileId::new(3);
        session
            .dispatch(Command::CityBuildDistrict {
                player,
                city: CityId::new(1),
                kind: DistrictKind::Campus,
                tile: hidden_district,
            })
            .unwrap();
        session
            .state
            .cities
            .get_mut(&CityId::new(1))
            .expect("city exists")
            .production_stored = 1_000;
        session.dispatch(Command::TurnEnd { player }).unwrap();
        session
            .dispatch(Command::TurnEnd { player: enemy })
            .unwrap();

        let observation = session.observe(player).unwrap();
        assert!(
            observation.cities()[0]
                .districts
                .iter()
                .any(|district| district.tile == hidden_district)
        );
        assert!(
            observation.visible_cities()[0]
                .districts
                .iter()
                .all(|tile| *tile != hidden_district)
        );
    }

    #[test]
    fn editor_commands_mutate_a_generated_map_through_session() {
        let player = PlayerId::new(1);
        let map = generate_map(FlatHex::new(3, 3, false, false).unwrap(), [4; 32]);
        let mut session = Session::with_map([player], map);
        session
            .dispatch(Command::EditorSetTerrain {
                player,
                tile: super::TileId::new(0),
                terrain: Terrain::Plains,
            })
            .unwrap();
        assert!(matches!(
            session.events()[0].kind(),
            EventKind::MapEdited { .. }
        ));
        assert_eq!(
            session.replay().replay().unwrap().state_hash(),
            session.state_hash()
        );
    }

    #[test]
    fn units_move_attack_and_promote_through_commands() {
        let player = PlayerId::new(1);
        let enemy = PlayerId::new(2);
        let map = MapState::blank(FlatHex::new(3, 3, false, false).unwrap());
        let mut session = Session::with_map([player, enemy], map);
        session
            .dispatch(Command::UnitSpawn {
                player,
                unit: UnitId::new(0),
                kind: UnitKind::Warrior,
                tile: super::TileId::new(0),
            })
            .unwrap();
        session
            .dispatch(Command::UnitSpawn {
                player: enemy,
                unit: UnitId::new(1),
                kind: UnitKind::Warrior,
                tile: super::TileId::new(1),
            })
            .unwrap();
        session
            .dispatch(Command::UnitAttack {
                player,
                attacker: UnitId::new(0),
                defender: UnitId::new(1),
            })
            .unwrap();
        assert!(matches!(
            session.events().last().unwrap().kind(),
            EventKind::Combat { .. }
        ));
        session
            .dispatch(Command::UnitPromote {
                player,
                unit: UnitId::new(0),
                promotion: PromotionKind::Shock,
            })
            .unwrap();
        let observation = session.observe(player).unwrap();
        assert_eq!(observation.units().len(), 1);
        assert_eq!(observation.units()[0].xp, 0);
    }

    #[test]
    fn turn_end_resets_unit_movement_and_barbarian_validation_is_strict() {
        let player = PlayerId::new(1);
        let map = MapState::blank(FlatHex::new(3, 3, false, false).unwrap());
        let mut session = Session::with_map([player], map);
        session
            .dispatch(Command::UnitSpawn {
                player,
                unit: UnitId::new(0),
                kind: UnitKind::Warrior,
                tile: super::TileId::new(0),
            })
            .unwrap();
        session
            .dispatch(Command::UnitMove {
                player,
                unit: UnitId::new(0),
                destination: super::TileId::new(1),
            })
            .unwrap();
        assert_eq!(session.observe(player).unwrap().units()[0].movement, 1);
        session.dispatch(Command::TurnEnd { player }).unwrap();
        assert_eq!(session.observe(player).unwrap().units()[0].movement, 2);
        assert_eq!(
            session.dispatch(Command::BarbarianSpawn {
                tile: super::TileId::new(2),
                kind: UnitKind::Warrior,
            }),
            Err(super::DispatchError::InvalidBarbarianKind)
        );
    }

    #[test]
    fn roads_healing_and_zone_of_control_are_command_validated() {
        let player = PlayerId::new(1);
        let map = MapState::blank(FlatHex::new(3, 3, false, false).unwrap());
        let mut road_session = Session::with_map([player], map);
        road_session
            .dispatch(Command::UnitSpawn {
                player,
                unit: UnitId::new(0),
                kind: UnitKind::Builder,
                tile: super::TileId::new(0),
            })
            .unwrap();
        road_session
            .dispatch(Command::UnitBuildRoad {
                player,
                unit: UnitId::new(0),
            })
            .unwrap();
        assert!(matches!(
            road_session.events().last().unwrap().kind(),
            EventKind::RoadBuilt { tile } if *tile == super::TileId::new(0)
        ));
        assert_eq!(
            road_session.dispatch(Command::UnitHeal {
                player,
                unit: UnitId::new(0),
            }),
            Err(super::DispatchError::AlreadyFullHealth)
        );

        let enemy = PlayerId::new(2);
        let map = MapState::blank(FlatHex::new(3, 3, false, false).unwrap());
        let mut zoc_session = Session::with_map([player, enemy], map);
        zoc_session
            .dispatch(Command::UnitSpawn {
                player,
                unit: UnitId::new(0),
                kind: UnitKind::Warrior,
                tile: super::TileId::new(0),
            })
            .unwrap();
        zoc_session
            .dispatch(Command::UnitSpawn {
                player: enemy,
                unit: UnitId::new(1),
                kind: UnitKind::Warrior,
                tile: super::TileId::new(2),
            })
            .unwrap();
        zoc_session
            .dispatch(Command::UnitMove {
                player,
                unit: UnitId::new(0),
                destination: super::TileId::new(1),
            })
            .unwrap();
        assert_eq!(
            zoc_session.dispatch(Command::UnitMove {
                player,
                unit: UnitId::new(0),
                destination: super::TileId::new(0),
            }),
            Err(super::DispatchError::ZocBlocked)
        );
    }

    #[test]
    fn cities_found_grow_queue_production_and_keep_seat_scope() {
        let player = PlayerId::new(1);
        let other = PlayerId::new(2);
        let map = MapState::blank(FlatHex::new(5, 5, false, false).unwrap());
        let mut session = Session::with_map([player, other], map);
        session
            .dispatch(Command::UnitSpawn {
                player,
                unit: UnitId::new(0),
                kind: UnitKind::Settler,
                tile: super::TileId::new(0),
            })
            .unwrap();
        session
            .dispatch(Command::CityFound {
                player,
                city: CityId::new(0),
                unit: UnitId::new(0),
            })
            .unwrap();
        assert_eq!(
            session.dispatch(Command::EditorSetTerrain {
                player,
                tile: super::TileId::new(0),
                terrain: Terrain::Mountain,
            }),
            Err(super::DispatchError::Impassable(super::TileId::new(0)))
        );
        assert_eq!(session.observe(player).unwrap().cities().len(), 1);
        assert!(session.observe(other).unwrap().cities().is_empty());
        session
            .dispatch(Command::CityBuildDistrict {
                player,
                city: CityId::new(0),
                kind: DistrictKind::Campus,
                tile: super::TileId::new(2),
            })
            .unwrap();
        assert_eq!(
            session.dispatch(Command::EditorSetTerrain {
                player,
                tile: super::TileId::new(2),
                terrain: Terrain::Mountain,
            }),
            Err(super::DispatchError::Impassable(super::TileId::new(2)))
        );
        for _ in 0..90 {
            session.dispatch(Command::TurnEnd { player }).unwrap();
            session
                .dispatch(Command::TurnEnd { player: other })
                .unwrap();
        }
        let observation = session.observe(player).unwrap();
        let city = &observation.cities()[0];
        assert_eq!(city.districts.len(), 1);
        assert!(city.population <= city.housing);
        assert_eq!(
            session.dispatch(Command::UnitSpawn {
                player,
                unit: UnitId::new(1),
                kind: UnitKind::Builder,
                tile: super::TileId::new(2),
            }),
            Err(super::DispatchError::Occupied(super::TileId::new(2)))
        );
        session
            .dispatch(Command::CityBuildBuilding {
                player,
                city: CityId::new(0),
                building: BuildingKind::Library,
            })
            .unwrap();
        for _ in 0..100 {
            session.dispatch(Command::TurnEnd { player }).unwrap();
            session
                .dispatch(Command::TurnEnd { player: other })
                .unwrap();
        }
        let observation = session.observe(player).unwrap();
        let city = &observation.cities()[0];
        assert!(city.buildings.contains(&BuildingKind::Library));
        let replay = ReplayLog::decode(&session.replay().encode()).unwrap();
        assert_eq!(replay.replay().unwrap().state_hash(), session.state_hash());
        let encoded = session.replay().replay().unwrap().encode();
        assert_eq!(
            SimulationState::decode(&encoded).unwrap().state_hash(),
            session.state_hash()
        );
    }

    #[test]
    fn builders_consume_charges_and_city_commands_validate_ownership() {
        let player = PlayerId::new(1);
        let other = PlayerId::new(2);
        let map = MapState::blank(FlatHex::new(3, 3, false, false).unwrap());
        let mut session = Session::with_map([player, other], map);
        session
            .dispatch(Command::UnitSpawn {
                player,
                unit: UnitId::new(0),
                kind: UnitKind::Builder,
                tile: super::TileId::new(0),
            })
            .unwrap();
        for _ in 0..3 {
            session
                .dispatch(Command::UnitBuildRoad {
                    player,
                    unit: UnitId::new(0),
                })
                .unwrap();
        }
        assert_eq!(session.observe(player).unwrap().units()[0].charges, 0);
        assert_eq!(
            session.dispatch(Command::UnitBuildRoad {
                player,
                unit: UnitId::new(0),
            }),
            Err(super::DispatchError::NoBuilderCharges)
        );
        assert_eq!(
            session.dispatch(Command::CitySetProduction {
                player: other,
                city: CityId::new(0),
                item: ProductionItem::Unit(UnitKind::Warrior),
            }),
            Err(super::DispatchError::UnknownCity(CityId::new(0)))
        );

        let map = MapState::blank(FlatHex::new(3, 3, false, false).unwrap());
        let mut queued_session = Session::with_map([player], map);
        queued_session
            .dispatch(Command::UnitSpawn {
                player,
                unit: UnitId::new(2),
                kind: UnitKind::Settler,
                tile: super::TileId::new(0),
            })
            .unwrap();
        queued_session
            .dispatch(Command::CityFound {
                player,
                city: CityId::new(0),
                unit: UnitId::new(2),
            })
            .unwrap();
        queued_session
            .dispatch(Command::CityBuildDistrict {
                player,
                city: CityId::new(0),
                kind: DistrictKind::Campus,
                tile: super::TileId::new(2),
            })
            .unwrap();
        queued_session
            .dispatch(Command::UnitSpawn {
                player,
                unit: UnitId::new(3),
                kind: UnitKind::Settler,
                tile: super::TileId::new(2),
            })
            .unwrap();
        assert_eq!(
            queued_session.dispatch(Command::CityFound {
                player,
                city: CityId::new(1),
                unit: UnitId::new(3),
            }),
            Err(super::DispatchError::Occupied(super::TileId::new(2)))
        );
    }

    #[test]
    fn research_civics_policies_and_victories_are_deterministic() {
        let player = PlayerId::new(1);
        let other = PlayerId::new(2);
        let map = MapState::blank(FlatHex::new(5, 5, false, false).unwrap());
        let mut session = Session::with_map([player, other], map);
        for (unit, owner, tile) in [
            (UnitId::new(0), player, super::TileId::new(0)),
            (UnitId::new(1), other, super::TileId::new(4)),
        ] {
            session
                .dispatch(Command::UnitSpawn {
                    player: owner,
                    unit,
                    kind: UnitKind::Settler,
                    tile,
                })
                .unwrap();
        }
        session
            .dispatch(Command::CityFound {
                player,
                city: CityId::new(0),
                unit: UnitId::new(0),
            })
            .unwrap();
        session
            .dispatch(Command::CityFound {
                player: other,
                city: CityId::new(1),
                unit: UnitId::new(1),
            })
            .unwrap();
        assert_eq!(
            session.dispatch(Command::PolicyAdopt {
                player,
                slot: 0,
                policy: "policy_growth".to_owned(),
            }),
            Err(super::DispatchError::MissingPrerequisite)
        );
        session
            .dispatch(Command::ResearchTechnology {
                player,
                technology: "tech_01".to_owned(),
            })
            .unwrap();
        session
            .dispatch(Command::ResearchCivic {
                player,
                civic: "civic_01".to_owned(),
            })
            .unwrap();
        session
            .state
            .progress
            .get_mut(&player)
            .unwrap()
            .technology_progress = 10;
        for _ in 0..10 {
            session.dispatch(Command::TurnEnd { player }).unwrap();
            session
                .dispatch(Command::TurnEnd { player: other })
                .unwrap();
        }
        let observation = session.observe(player).unwrap();
        assert!(
            observation
                .progress()
                .technologies
                .contains(&"tech_01".to_owned())
        );
        assert!(
            observation
                .progress()
                .civics
                .contains(&"civic_01".to_owned())
        );
        session
            .state
            .progress
            .get_mut(&player)
            .unwrap()
            .technology_progress = 100;
        session
            .dispatch(Command::ResearchTechnology {
                player,
                technology: "tech_02".to_owned(),
            })
            .unwrap();
        assert!(
            session
                .observe(player)
                .unwrap()
                .progress()
                .technologies
                .contains(&"tech_02".to_owned())
        );
        session
            .dispatch(Command::ResearchCivic {
                player,
                civic: "civic_02".to_owned(),
            })
            .unwrap();
        for _ in 0..12 {
            session.dispatch(Command::TurnEnd { player }).unwrap();
            session
                .dispatch(Command::TurnEnd { player: other })
                .unwrap();
        }
        {
            let progress = session.state.progress.get_mut(&player).unwrap();
            progress.civics.insert("civic_03".to_owned());
            progress.civics.insert("civic_04".to_owned());
            progress.civic_progress = 38;
        }
        session
            .dispatch(Command::ResearchCivic {
                player,
                civic: "civic_05".to_owned(),
            })
            .unwrap();
        assert_eq!(session.observe(player).unwrap().progress().policy_slots, 3);
        session
            .dispatch(Command::PolicyAdopt {
                player,
                slot: 2,
                policy: "policy_defense".to_owned(),
            })
            .unwrap();
        session
            .dispatch(Command::PolicyAdopt {
                player,
                slot: 0,
                policy: "policy_growth".to_owned(),
            })
            .unwrap();
        assert_eq!(
            session.observe(player).unwrap().progress().policies.len(),
            2
        );
        session
            .state
            .progress
            .get_mut(&player)
            .unwrap()
            .policies
            .insert(1, "policy_growth".to_owned());
        assert_eq!(
            SimulationState::decode(&session.state.encode()),
            Err(SerializationError::InvalidProgressState)
        );
        session
            .state
            .progress
            .get_mut(&player)
            .unwrap()
            .policies
            .remove(&1);
        assert_eq!(
            session.dispatch(Command::PolicyAdopt {
                player,
                slot: 3,
                policy: "policy_growth".to_owned(),
            }),
            Err(super::DispatchError::InvalidPolicySlot)
        );
        let all_technology_ids: Vec<_> = super::city::base_ruleset()
            .technologies()
            .map(|technology| technology.id().to_owned())
            .collect();
        let final_technology = all_technology_ids.last().cloned().unwrap();
        session
            .state
            .progress
            .get_mut(&player)
            .unwrap()
            .technologies
            .extend(
                all_technology_ids
                    .into_iter()
                    .filter(|id| id != &final_technology),
            );
        session
            .state
            .progress
            .get_mut(&player)
            .unwrap()
            .technology_progress = 58;
        session
            .dispatch(Command::ResearchTechnology {
                player,
                technology: final_technology,
            })
            .unwrap();
        assert_eq!(
            session.observe(player).unwrap().victory(),
            Some(VictoryKind::Science)
        );
        assert_eq!(
            session.dispatch(Command::ResearchTechnology {
                player,
                technology: "tech_02".to_owned(),
            }),
            Err(super::DispatchError::VictoryAlreadyReached)
        );
        let roundtrip = SimulationState::decode(&session.state.encode()).unwrap();
        assert_eq!(roundtrip.state_hash(), session.state_hash());
    }

    #[test]
    fn a_single_city_reaches_domination_after_a_completed_turn() {
        let player = PlayerId::new(1);
        let map = MapState::blank(FlatHex::new(3, 3, false, false).unwrap());
        let mut session = Session::with_map([player], map);
        session
            .dispatch(Command::UnitSpawn {
                player,
                unit: UnitId::new(0),
                kind: UnitKind::Settler,
                tile: super::TileId::new(0),
            })
            .unwrap();
        session
            .dispatch(Command::CityFound {
                player,
                city: CityId::new(0),
                unit: UnitId::new(0),
            })
            .unwrap();
        session.dispatch(Command::TurnEnd { player }).unwrap();
        assert_eq!(
            session.observe(player).unwrap().victory(),
            Some(VictoryKind::Domination)
        );
    }

    #[test]
    fn malformed_documents_are_rejected() {
        assert_eq!(Command::decode(b"bad"), Err(SerializationError::Truncated));
        let mut command = Command::TurnEnd {
            player: PlayerId::new(1),
        }
        .encode();
        command[4..6].copy_from_slice(&1_u16.to_le_bytes());
        assert_eq!(
            Command::decode(&command),
            Err(SerializationError::UnsupportedVersion(1))
        );
        let mut command = Command::TurnEnd {
            player: PlayerId::new(1),
        }
        .encode();
        command[6] = 255;
        assert_eq!(
            Command::decode(&command),
            Err(SerializationError::InvalidTag(255))
        );

        let mut bytes = SimulationState::new([PlayerId::new(1)]).encode();
        bytes.push(0);
        assert_eq!(
            SimulationState::decode(&bytes),
            Err(SerializationError::TrailingBytes)
        );

        let mut old_state = SimulationState::new([PlayerId::new(1)]).encode();
        old_state[4..6].copy_from_slice(&1_u16.to_le_bytes());
        assert_eq!(
            SimulationState::decode(&old_state),
            Err(SerializationError::UnsupportedVersion(1))
        );
        let mut old_replay = ReplayLog::new(SimulationState::new([PlayerId::new(1)])).encode();
        old_replay[4..6].copy_from_slice(&1_u16.to_le_bytes());
        assert_eq!(
            ReplayLog::decode(&old_replay),
            Err(SerializationError::UnsupportedVersion(1))
        );

        let original = SimulationState::new([PlayerId::new(1)]).encode();
        let mut duplicate = original[..26].to_vec();
        duplicate[10..14].copy_from_slice(&2_u32.to_le_bytes());
        duplicate.extend_from_slice(&1_u32.to_le_bytes());
        duplicate.extend_from_slice(&0_i64.to_le_bytes());
        duplicate.extend_from_slice(&0_u32.to_le_bytes());
        assert_eq!(
            SimulationState::decode(&duplicate),
            Err(SerializationError::DuplicateKey)
        );

        let mut too_many = SimulationState::new([PlayerId::new(1)]).encode();
        too_many[10..14].copy_from_slice(&1_000_001_u32.to_le_bytes());
        assert_eq!(
            SimulationState::decode(&too_many),
            Err(SerializationError::InvalidLength)
        );

        let mut unsorted = SimulationState::new([PlayerId::new(1), PlayerId::new(2)]).encode();
        unsorted[14..18].copy_from_slice(&2_u32.to_le_bytes());
        unsorted[26..30].copy_from_slice(&1_u32.to_le_bytes());
        assert_eq!(
            SimulationState::decode(&unsorted),
            Err(SerializationError::NonCanonicalOrder)
        );

        let mut city_document = Vec::new();
        super::put_u32(&mut city_document, 0);
        super::put_u32(&mut city_document, 1);
        super::put_u32(&mut city_document, 0);
        super::put_u32(&mut city_document, 1);
        super::put_i64(&mut city_document, 0);
        super::put_u32(&mut city_document, 2);
        super::put_i64(&mut city_document, 0);
        super::put_u8(&mut city_document, 255);
        super::put_u32(&mut city_document, 2);
        super::put_u32(&mut city_document, 2);
        super::put_u8(&mut city_document, DistrictKind::Campus as u8);
        super::put_u32(&mut city_document, 1);
        super::put_u8(&mut city_document, DistrictKind::Harbor as u8);
        super::put_u32(&mut city_document, 0);
        assert_eq!(
            super::decode_city(&mut super::Reader::new(&city_document)),
            Err(SerializationError::NonCanonicalOrder)
        );

        let player = PlayerId::new(1);
        let mut invalid_map = MapState::blank(FlatHex::new(3, 3, false, false).unwrap());
        invalid_map
            .set_terrain(super::TileId::new(2), Terrain::Mountain)
            .unwrap();
        let mut invalid_district = SimulationState::with_map([player], invalid_map);
        let mut city = City::new(CityId::new(0), player, super::TileId::new(0));
        city.production = Some(ProductionItem::District {
            kind: DistrictKind::Campus,
            tile: super::TileId::new(2),
        });
        invalid_district.cities.insert(CityId::new(0), city);
        assert_eq!(
            SimulationState::decode(&invalid_district.encode()),
            Err(SerializationError::InvalidCityState)
        );

        let mut invalid_building = SimulationState::with_map(
            [player],
            MapState::blank(FlatHex::new(3, 3, false, false).unwrap()),
        );
        let mut city = City::new(CityId::new(0), player, super::TileId::new(0));
        city.production = Some(ProductionItem::Building(BuildingKind::Library));
        invalid_building.cities.insert(CityId::new(0), city);
        assert_eq!(
            SimulationState::decode(&invalid_building.encode()),
            Err(SerializationError::InvalidCityState)
        );

        let mut replay = ReplayLog::new(SimulationState::new([PlayerId::new(1)])).encode();
        replay.push(0);
        assert_eq!(
            ReplayLog::decode(&replay),
            Err(SerializationError::TrailingBytes)
        );

        let mut session = Session::new([PlayerId::new(1)]);
        assert_eq!(
            session.dispatch(Command::TurnEnd {
                player: PlayerId::new(2),
            }),
            Err(super::DispatchError::UnknownPlayer(PlayerId::new(2)))
        );
    }

    #[test]
    fn hash_field_boundaries_remain_ordered() {
        let mut first = StateHasher::new();
        first.write_bytes(b"ab");
        first.write_bytes(b"c");
        let mut second = StateHasher::new();
        second.write_bytes(b"a");
        second.write_bytes(b"bc");
        assert_ne!(first.finish(), second.finish());
    }

    #[test]
    fn turn_overflow_rejects_without_mutating_state() {
        let player = PlayerId::new(1);
        let mut state = SimulationState::new([player]);
        state.turn = u32::MAX;
        let before = state.clone();
        assert_eq!(
            state.apply(&Command::TurnEnd { player }),
            Err(super::DispatchError::TurnOverflow)
        );
        assert_eq!(state, before);
    }

    #[test]
    fn final_seat_turn_overflow_preserves_previous_endings() {
        let player = PlayerId::new(1);
        let other = PlayerId::new(2);
        let mut state = SimulationState::new([player, other]);
        state.turn = u32::MAX;
        state.apply(&Command::TurnEnd { player }).unwrap();
        let before = state.clone();
        assert_eq!(
            state.apply(&Command::TurnEnd { player: other }),
            Err(super::DispatchError::TurnOverflow)
        );
        assert_eq!(state, before);
    }
}
