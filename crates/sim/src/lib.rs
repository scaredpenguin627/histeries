//! Deterministic world-simulation state, commands, observations, events, and replay.

#![forbid(unsafe_code)]

use std::{collections::BTreeMap, fmt};

use histeries_hex::TileId;
use histeries_rules_core::PlayerId;

pub mod map;
pub use map::{Feature, MapError, MapState, ResourceKind, RiverDirection, Terrain, generate_map};

/// The versioned algorithm identifier for per-turn state hashes.
pub const STATE_HASH_ALGORITHM: &str = "fnv1a64-v1-little-endian-length-prefixed";
const STATE_MAGIC: &[u8; 4] = b"HST1";
const COMMAND_MAGIC: &[u8; 4] = b"HCM1";
const REPLAY_MAGIC: &[u8; 4] = b"HRP1";
const FORMAT_VERSION: u16 = 1;

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
        }
    }

    /// Encodes this command using the versioned custom binary format.
    pub fn encode(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(25);
        bytes.extend_from_slice(COMMAND_MAGIC);
        put_u16(&mut bytes, FORMAT_VERSION);
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
        }
        bytes
    }

    /// Decodes one command and rejects trailing or malformed bytes.
    pub fn decode(bytes: &[u8]) -> Result<Self, SerializationError> {
        let mut reader = Reader::new(bytes);
        reader.expect_magic(COMMAND_MAGIC)?;
        reader.expect_version()?;
        let kind = reader.u8()?;
        let player = PlayerId::new(reader.u32()?);
        let command = match kind {
            0 => Self::TurnEnd { player },
            1 => Self::EditorSetMarker {
                player,
                value: reader.i64()?,
            },
            2 => Self::EditorSetTerrain {
                player,
                tile: TileId::new(reader.u32()?),
                terrain: decode_terrain(&mut reader)?,
            },
            3 => Self::EditorSetFeature {
                player,
                tile: TileId::new(reader.u32()?),
                feature: decode_feature(&mut reader)?,
            },
            4 => Self::EditorSetRiver {
                player,
                tile: TileId::new(reader.u32()?),
                river: decode_river(&mut reader)?,
            },
            5 => Self::EditorSetResource {
                player,
                tile: TileId::new(reader.u32()?),
                resource: decode_resource(&mut reader)?,
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

static COMMAND_SPECS: [CommandSpec; 6] = [
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
];

/// Returns the process-wide command registry.
pub const fn command_registry() -> CommandRegistry {
    CommandRegistry {
        specs: &COMMAND_SPECS,
    }
}

/// Canonical world state for the Phase 1 session boundary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SimulationState {
    turn: u32,
    markers: BTreeMap<PlayerId, i64>,
    map: Option<MapState>,
}

impl SimulationState {
    /// Creates a state with the supplied seats and zero markers.
    pub fn new(players: impl IntoIterator<Item = PlayerId>) -> Self {
        let markers = players.into_iter().map(|player| (player, 0)).collect();
        Self {
            turn: 0,
            markers,
            map: None,
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
        hasher.finish()
    }

    /// Encodes state with a magic, format version, and canonical map ordering.
    pub fn encode(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(18 + self.markers.len() * 12);
        bytes.extend_from_slice(STATE_MAGIC);
        put_u16(&mut bytes, FORMAT_VERSION);
        put_u32(&mut bytes, self.turn);
        put_u32(&mut bytes, self.markers.len() as u32);
        for (player, marker) in &self.markers {
            put_u32(&mut bytes, player.index());
            put_i64(&mut bytes, *marker);
        }
        if let Some(map) = &self.map {
            put_u32(&mut bytes, map.encode().len() as u32);
            bytes.extend_from_slice(&map.encode());
        } else {
            put_u32(&mut bytes, 0);
        }
        bytes
    }

    /// Decodes state and rejects duplicate players, truncation, or trailing bytes.
    pub fn decode(bytes: &[u8]) -> Result<Self, SerializationError> {
        let mut reader = Reader::new(bytes);
        reader.expect_magic(STATE_MAGIC)?;
        reader.expect_version()?;
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
        reader.finish()?;
        Ok(Self { turn, markers, map })
    }

    fn apply(&mut self, command: &Command) -> Result<EventKind, DispatchError> {
        match command {
            Command::TurnEnd { player } => {
                self.require_player(*player)?;
                self.turn = self
                    .turn
                    .checked_add(1)
                    .ok_or(DispatchError::TurnOverflow)?;
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
        }
    }

    fn require_player(&self, player: PlayerId) -> Result<(), DispatchError> {
        if self.contains_player(player) {
            Ok(())
        } else {
            Err(DispatchError::UnknownPlayer(player))
        }
    }
}

/// A seat-scoped observation with fog-of-war applied.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Observation {
    seat: PlayerId,
    turn: u32,
    own_marker: i64,
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
    /// A map tile was edited.
    MapEdited {
        /// The edited tile ID.
        tile: TileId,
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
        put_u16(&mut bytes, FORMAT_VERSION);
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
        reader.expect_version()?;
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

    /// Returns an observation restricted to one seat's own marker.
    pub fn observe(&self, seat: PlayerId) -> Result<Observation, DispatchError> {
        Ok(Observation {
            seat,
            turn: self.state.turn,
            own_marker: self
                .state
                .marker(seat)
                .ok_or(DispatchError::UnknownPlayer(seat))?,
        })
    }

    /// Returns the current state hash.
    pub fn state_hash(&self) -> StateHash {
        self.state.state_hash()
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
}

/// Errors raised while applying commands.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DispatchError {
    /// A command named a seat absent from the state.
    UnknownPlayer(PlayerId),
    /// The turn counter would overflow.
    TurnOverflow,
    /// The command ID was absent from the registry.
    UnregisteredCommand(&'static str),
    /// A map was required by an editor command.
    NoMap,
    /// A map edit failed validation.
    Map(MapError),
    /// The event sequence would overflow.
    EventSequenceOverflow,
}

impl fmt::Display for DispatchError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownPlayer(player) => write!(formatter, "unknown player {}", player.index()),
            Self::TurnOverflow => formatter.write_str("turn counter overflow"),
            Self::UnregisteredCommand(id) => write!(formatter, "unregistered command {id}"),
            Self::NoMap => formatter.write_str("command requires a map"),
            Self::Map(error) => write!(formatter, "map command rejected: {error}"),
            Self::EventSequenceOverflow => formatter.write_str("event sequence overflow"),
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
    /// A table contained a duplicate key.
    DuplicateKey,
    /// Canonical map keys were not strictly ascending.
    NonCanonicalOrder,
    /// Embedded map data failed map validation.
    InvalidMap,
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
            Self::DuplicateKey => formatter.write_str("duplicate key in binary document"),
            Self::NonCanonicalOrder => {
                formatter.write_str("non-canonical key order in binary document")
            }
            Self::InvalidMap => formatter.write_str("invalid embedded map document"),
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

    fn expect_version(&mut self) -> Result<(), SerializationError> {
        let version = self.u16()?;
        if version == FORMAT_VERSION {
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

fn put_u32(bytes: &mut Vec<u8>, value: u32) {
    bytes.extend_from_slice(&value.to_le_bytes());
}

fn put_i64(bytes: &mut Vec<u8>, value: i64) {
    bytes.extend_from_slice(&value.to_le_bytes());
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
    use histeries_hex::FlatHex;
    use histeries_rules_core::PlayerId;

    use super::{
        Command, EventKind, ReplayLog, STATE_HASH_ALGORITHM, SerializationError, Session,
        SimulationState, StateHasher, Terrain, command_registry, generate_map,
    };

    #[test]
    fn registry_has_stable_command_ids() {
        let registry = command_registry();
        assert_eq!(registry.specs()[0].id, "editor.set_feature");
        assert_eq!(registry.find("turn.end").unwrap().schema, "player:u32");
        assert!(
            registry
                .validate(&Command::TurnEnd {
                    player: PlayerId::new(1),
                })
                .is_ok()
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
        assert_eq!(
            session.events()[1].kind(),
            &EventKind::TurnAdvanced { turn: 1 }
        );
        assert_eq!(session.observe(player).unwrap().own_marker(), 42);
        assert_eq!(session.observe(other).unwrap().own_marker(), 0);
        let replay = ReplayLog::decode(&session.replay().encode()).unwrap();
        assert_eq!(replay.replay().unwrap().state_hash(), session.state_hash());
        assert_eq!(session.drain_events().len(), 2);
        session.dispatch(Command::TurnEnd { player }).unwrap();
        assert_eq!(session.events()[0].sequence(), 2);
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
    fn malformed_documents_are_rejected() {
        assert_eq!(Command::decode(b"bad"), Err(SerializationError::Truncated));
        let mut command = Command::TurnEnd {
            player: PlayerId::new(1),
        }
        .encode();
        command[4..6].copy_from_slice(&2_u16.to_le_bytes());
        assert_eq!(
            Command::decode(&command),
            Err(SerializationError::UnsupportedVersion(2))
        );
        let mut command = Command::TurnEnd {
            player: PlayerId::new(1),
        }
        .encode();
        command[6] = 9;
        assert_eq!(
            Command::decode(&command),
            Err(SerializationError::InvalidTag(9))
        );

        let mut bytes = SimulationState::new([PlayerId::new(1)]).encode();
        bytes.push(0);
        assert_eq!(
            SimulationState::decode(&bytes),
            Err(SerializationError::TrailingBytes)
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
}
