//! A scenario draft keeps edits separate from the game being played.
use crate::{
    BoardObservation, Command, Feature, MapState, MatchHost, ResourceKind, RiverDirection, Session,
    Terrain, UnitKind,
};
use histeries_hex::{FlatHex, TileId};
use histeries_rules_core::{PlayerId, UnitId};
use std::collections::{BTreeSet, VecDeque};

#[derive(Clone, Copy, PartialEq)]
/// A local scenario-authoring operation translated into registered simulation commands.
pub enum ScenarioBrush {
    /// Select a tile without changing it.
    Inspect,
    /// Set tile terrain.
    Terrain(Terrain),
    /// Set a terrain feature.
    Feature(Feature),
    /// Place or clear a resource.
    Resource(Option<ResourceKind>),
    /// Set or clear a river edge.
    River(Option<RiverDirection>),
    /// Place a unit.
    Unit(UnitKind),
    /// Found a city for the selected seat.
    City,
    /// Order the selected seat’s builder to construct a road.
    Road,
}

const HISTORY_POSITIONS: usize = 33;
const HISTORY_BYTES: usize = 16 * 1024 * 1024;

/// A local authoring session with a full-board view and bounded snapshot history.
pub struct ScenarioDraft {
    history: VecDeque<Vec<u8>>,
    cursor: usize,
    board: BoardObservation,
    seats: Vec<PlayerId>,
}

impl ScenarioDraft {
    /// Creates an empty non-wrapping board and its participating seats.
    pub fn blank(width: u32, height: u32, seats: u32) -> Result<Self, String> {
        if !Self::PLAYERS.contains(&seats)
            || !Self::DIMENSIONS.contains(&width)
            || !Self::DIMENSIONS.contains(&height)
        {
            return Err("Use 1–128 tiles along each dimension and 2–8 players.".into());
        }
        let topology = FlatHex::new(width, height, false, false).map_err(|e| e.to_string())?;
        Self::open(
            &Session::with_map((1..=seats).map(PlayerId::new), MapState::blank(topology))
                .save_bytes(),
        )
    }

    /// Opens a saved position as a separate, local scenario.
    pub fn open(bytes: &[u8]) -> Result<Self, String> {
        let session = Session::from_save_bytes(bytes).map_err(|e| e.to_string())?;
        let seats: Vec<_> = session.seat_ids().collect();
        let player = *seats.first().ok_or("The scenario contains no players.")?;
        let bytes = session.save_bytes();
        let board = MatchHost::new(session)
            .observe_editor(player)
            .map_err(|e| e.to_string())?;
        Ok(Self {
            history: VecDeque::from([bytes]),
            cursor: 0,
            board,
            seats,
        })
    }

    /// Limits for the dimensions of a blank board.
    pub const DIMENSIONS: std::ops::RangeInclusive<u32> = 1..=128;
    /// Limits for the number of seats on a blank board.
    pub const PLAYERS: std::ops::RangeInclusive<u32> = 2..=8;

    /// Returns every tile in the local authoring view.
    pub fn board(&self) -> &BoardObservation {
        &self.board
    }

    /// Returns the scenario's seats in stable order.
    pub fn seats(&self) -> &[PlayerId] {
        &self.seats
    }

    /// Returns the canonical current snapshot.
    pub fn bytes(&self) -> &[u8] {
        &self.history[self.cursor]
    }
    /// Reports whether an earlier position is retained.
    pub fn can_undo(&self) -> bool {
        self.cursor > 0
    }
    /// Reports whether a later position is retained.
    pub fn can_redo(&self) -> bool {
        self.cursor + 1 < self.history.len()
    }

    /// Restores the previous or next retained snapshot.
    pub fn travel(&mut self, redo: bool) -> Result<(), String> {
        let target = if redo && self.can_redo() {
            self.cursor + 1
        } else if !redo && self.can_undo() {
            self.cursor - 1
        } else {
            return Ok(());
        };
        let session = Session::from_save_bytes(&self.history[target]).map_err(|e| e.to_string())?;
        self.board = MatchHost::new(session)
            .observe_editor(self.seats[0])
            .map_err(|e| e.to_string())?;
        self.cursor = target;
        Ok(())
    }

    /// Applies a brush atomically and returns its ordered, replayable commands.
    pub fn apply(
        &mut self,
        brush: ScenarioBrush,
        tile: TileId,
        player: PlayerId,
    ) -> Result<Vec<Command>, String> {
        let mut commands = Vec::with_capacity(2);
        let occupants = || {
            self.board
                .visible_units()
                .iter()
                .filter(|unit| unit.tile == tile && unit.owner == Some(player))
        };
        let next_id = || {
            let mut next = 0u32;
            let ids: BTreeSet<_> = self
                .board
                .visible_units()
                .iter()
                .map(|u| u.id.index())
                .collect();
            for id in ids {
                if id != next {
                    break;
                }
                next = next.checked_add(1).ok_or("No unit IDs available.")?;
            }
            Ok::<_, String>(UnitId::new(next))
        };
        let command = match brush {
            ScenarioBrush::Inspect => return Ok(commands),
            ScenarioBrush::Terrain(terrain) => Command::EditorSetTerrain {
                player,
                tile,
                terrain,
            },
            ScenarioBrush::Feature(feature) => Command::EditorSetFeature {
                player,
                tile,
                feature,
            },
            ScenarioBrush::Resource(resource) => Command::EditorSetResource {
                player,
                tile,
                resource,
            },
            ScenarioBrush::River(river) => Command::EditorSetRiver {
                player,
                tile,
                river,
            },
            ScenarioBrush::Unit(UnitKind::Barbarian) => Command::BarbarianSpawn {
                tile,
                kind: UnitKind::Barbarian,
            },
            ScenarioBrush::Unit(kind) => Command::UnitSpawn {
                player,
                tile,
                kind,
                unit: next_id()?,
            },
            ScenarioBrush::Road => {
                let builder = occupants()
                    .find(|u| u.kind == UnitKind::Builder)
                    .ok_or("Place the selected seat’s builder on the tile to build a road.")?;
                Command::UnitBuildRoad {
                    player,
                    unit: builder.id,
                }
            }
            ScenarioBrush::City => {
                let unit = if let Some(founder) = occupants().find(|u| u.kind == UnitKind::Settler)
                {
                    founder.id
                } else {
                    let unit = next_id()?;
                    commands.push(Command::UnitSpawn {
                        player,
                        unit,
                        tile,
                        kind: UnitKind::Settler,
                    });
                    unit
                };
                Command::CitySettle { player, unit }
            }
        };
        commands.push(command);
        self.edit(|host| host.dispatch_editor_batch(player, commands.iter().cloned()))?;
        Ok(commands)
    }

    fn edit(
        &mut self,
        operation: impl FnOnce(&mut MatchHost) -> Result<Vec<u8>, String>,
    ) -> Result<(), String> {
        let session = Session::from_save_bytes(self.bytes()).map_err(|e| e.to_string())?;
        let mut host = MatchHost::new(session);
        let bytes = operation(&mut host)?;
        if bytes == self.bytes() {
            return Ok(());
        }
        let board = host
            .observe_editor(self.seats[0])
            .map_err(|e| e.to_string())?;
        self.remember(bytes);
        self.board = board;
        Ok(())
    }

    fn remember(&mut self, bytes: Vec<u8>) {
        self.history.truncate(self.cursor + 1);
        self.history.push_back(bytes);
        let mut retained: usize = self.history.iter().map(Vec::len).sum();
        while self.history.len() > 1
            && (self.history.len() > HISTORY_POSITIONS || retained > HISTORY_BYTES)
        {
            retained -= self
                .history
                .pop_front()
                .expect("history has prior positions")
                .len();
        }
        self.cursor = self.history.len() - 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn founding_batch_replays_from_the_pre_edit_snapshot() {
        let mut draft = ScenarioDraft::blank(4, 3, 2).unwrap();
        let seat = draft.seats[0];
        let before = draft.bytes().to_vec();
        let applied = draft
            .apply(ScenarioBrush::City, TileId::new(3), seat)
            .unwrap();
        assert!(matches!(
            &applied[..],
            [Command::UnitSpawn { .. }, Command::CitySettle { .. }]
        ));
        let mut replay = Session::from_save_bytes(&before).unwrap();
        for command in applied {
            replay.dispatch(command).unwrap();
        }
        assert_eq!(replay.save_bytes(), draft.bytes());
        assert_eq!(replay.replay().replay().unwrap().encode(), draft.bytes());
    }

    #[test]
    fn history_evicts_by_bytes_and_keeps_an_oversized_current_position() {
        let mut draft = ScenarioDraft::blank(1, 1, 2).unwrap();
        draft.remember(vec![0; HISTORY_BYTES / 2 + 1]);
        draft.remember(vec![1; HISTORY_BYTES / 2]);
        assert!(!draft.can_undo());
        assert_eq!(draft.bytes().len(), HISTORY_BYTES / 2);
        draft.remember(vec![2; HISTORY_BYTES + 1]);
        assert!(!draft.can_undo());
        assert_eq!(draft.bytes().len(), HISTORY_BYTES + 1);
    }

    #[test]
    fn neutral_barbarian_and_seat_placements_share_the_unit_id_space() {
        let camp = TileId::new(9);
        let garrison = TileId::new(5);
        let settlement = TileId::new(0);
        let neutral = (ScenarioBrush::Unit(UnitKind::Barbarian), camp);
        let warrior = (ScenarioBrush::Unit(UnitKind::Warrior), garrison);
        let city = (ScenarioBrush::City, settlement);
        for order in [[neutral, warrior, city], [warrior, city, neutral]] {
            let mut draft = ScenarioDraft::blank(6, 3, 2).unwrap();
            let seat = draft.seats[0];
            for (brush, tile) in order {
                draft.apply(brush, tile, seat).unwrap();
            }
            let units = draft.board.visible_units();
            assert_eq!(units.len(), 2);
            let barbarian = units.iter().find(|u| u.tile == camp).unwrap();
            let defender = units.iter().find(|u| u.tile == garrison).unwrap();
            assert_eq!(
                (barbarian.kind, barbarian.owner),
                (UnitKind::Barbarian, None)
            );
            assert_eq!(
                (defender.kind, defender.owner),
                (UnitKind::Warrior, Some(seat))
            );
            assert_ne!(barbarian.id, defender.id);
            let cities = draft.board.visible_cities();
            assert_eq!(cities.len(), 1);
            assert_eq!((cities[0].tile, cities[0].owner), (settlement, seat));
        }
    }

    #[test]
    fn placement_fills_unit_id_gaps_independently_of_tile_order() {
        let mut draft = ScenarioDraft::blank(7, 2, 2).unwrap();
        let player = draft.seats[0];
        let occupied = [(4, 0), (0, 5), (2, 3)];
        draft
            .edit(|host| {
                host.dispatch_editor_batch(
                    player,
                    occupied.map(|(id, tile)| Command::UnitSpawn {
                        player,
                        unit: UnitId::new(id),
                        tile: TileId::new(tile),
                        kind: UnitKind::Warrior,
                    }),
                )
            })
            .unwrap();
        draft
            .apply(
                ScenarioBrush::Unit(UnitKind::Warrior),
                TileId::new(1),
                player,
            )
            .unwrap();
        let units = draft.board.visible_units();
        assert_eq!(units.len(), 4);
        assert!(
            units
                .iter()
                .any(|unit| unit.tile == TileId::new(1) && unit.id == UnitId::new(1))
        );
        draft
            .apply(ScenarioBrush::City, TileId::new(6), player)
            .unwrap();
        assert_eq!(draft.board.visible_cities().len(), 1);
    }

    #[test]
    fn edits_are_atomic_and_undo_redo_are_bounded() {
        let mut draft = ScenarioDraft::blank(5, 4, 2).unwrap();
        let tile = TileId::new(2);
        let seat = draft.seats[0];
        draft
            .apply(ScenarioBrush::Unit(UnitKind::Warrior), tile, seat)
            .unwrap();
        let armed = draft.bytes().to_vec();
        // Spawning the intermediate settler succeeds. Founding on a warrior fails.
        assert_eq!(
            draft.apply(ScenarioBrush::City, tile, seat).unwrap_err(),
            crate::DispatchError::Occupied(tile).to_string()
        );
        assert_eq!(draft.bytes(), armed);
        assert_eq!(draft.board.visible_units().len(), 1);
        draft.travel(false).unwrap();
        assert!(draft.board.visible_units().is_empty());
        draft.travel(true).unwrap();
        assert_eq!(draft.bytes(), armed);
        draft.travel(false).unwrap();
        draft
            .apply(ScenarioBrush::Terrain(Terrain::Ocean), tile, seat)
            .unwrap();
        assert!(!draft.can_redo());
        assert!(
            draft
                .apply(ScenarioBrush::Feature(Feature::Forest), tile, seat)
                .is_err()
        );
        for i in 0..64 {
            let terrain = if i % 2 == 0 {
                Terrain::Plains
            } else {
                Terrain::Hill
            };
            draft
                .apply(ScenarioBrush::Terrain(terrain), tile, seat)
                .unwrap();
        }
        assert_eq!(draft.history.len(), HISTORY_POSITIONS);
        for _ in 1..HISTORY_POSITIONS {
            draft.travel(false).unwrap();
        }
        assert!(!draft.can_undo());
    }
}
