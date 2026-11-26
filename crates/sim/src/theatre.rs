//! Seat-scoped expedition orders and surveys on secondary boards.

use histeries_hex::{Embedding, FlatHex};
use histeries_ruleset::{TheatreDefinition, base_ruleset};

use super::{
    BTreeMap, BTreeSet, CityId, DispatchError, EventKind, PlayerId, PlayerProgress, Reader,
    SerializationError, SimulationState, TileId, TileTopology, put_string, put_u8, put_u32,
};

/// One seat's chart of an unlocked secondary board.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
pub struct TheatreObservation {
    /// Stable theatre identifier.
    pub id: String,
    /// Ruleset display name.
    pub name: String,
    /// Shared entry and recall site, known before launch.
    pub entry: TileId,
    /// Current position of this seat's expedition.
    pub expedition: Option<TileId>,
    /// Whether this seat may launch from one of its cities this round.
    pub can_launch: bool,
    /// Whether this expedition may travel to an adjacent site this round.
    pub can_move: bool,
    /// Whether this expedition can return home this round.
    pub can_recall: bool,
    /// Charted sites in ascending board-local tile order.
    pub tiles: Vec<TheatreTileObservation>,
}

/// Surveyed geography and current visibility of one secondary-board site.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
pub struct TheatreTileObservation {
    /// Identifier within this theatre.
    pub tile: TileId,
    /// Topology-supplied position for presentation.
    #[serde(with = "DisplayPosition")]
    pub embedding: Embedding,
    /// Adjacent charted sites in this theatre.
    pub neighbors: Vec<TileId>,
    /// Whether the expedition has ever occupied this site.
    pub visited: bool,
    /// Whether the site is within one step of the active expedition.
    pub visible: bool,
}

#[derive(serde::Serialize)]
#[serde(remote = "Embedding")]
struct DisplayPosition {
    x: i32,
    y: i32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct Survey {
    position: Option<TileId>,
    last_order: u32,
    visited: BTreeSet<TileId>,
}

pub(super) type Surveys = BTreeMap<(PlayerId, String), Survey>;

pub(super) enum Order {
    Launch(CityId),
    Move(TileId),
    Recall,
}

fn topology(definition: &TheatreDefinition) -> FlatHex {
    FlatHex::new(
        definition.width(),
        definition.height(),
        definition.wrap_x(),
        definition.wrap_y(),
    )
    .expect("ruleset validates theatre dimensions")
}

impl SimulationState {
    pub(super) fn theatre_order(
        &mut self,
        player: PlayerId,
        id: &str,
        order: Order,
    ) -> Result<EventKind, DispatchError> {
        self.require_player(player)?;
        if self.ended_players.contains(&player) {
            return Err(DispatchError::TurnAlreadyEnded);
        }
        if !self.victories.is_empty() {
            return Err(DispatchError::VictoryAlreadyReached);
        }

        let rules = base_ruleset();
        let definition = rules
            .theatre(id)
            .filter(|def| self.progress[&player].has_age_unlock(rules, def.required_unlock()))
            .ok_or(DispatchError::TheatreUnavailable)?;
        let key = (player, id.to_owned());
        let survey = self.surveys.get(&key);
        if survey.is_some_and(|s| s.last_order == self.turn) {
            return Err(DispatchError::InvalidExpeditionOrder);
        }
        let entry = TileId::new(definition.entry_tile());
        let position = match (order, survey.and_then(|s| s.position)) {
            (Order::Launch(city), None) => {
                self.require_city_owner(city, player)?;
                Some(entry)
            }
            (Order::Move(tile), Some(from))
                if topology(definition).neighbors(from).contains(&tile) =>
            {
                Some(tile)
            }
            (Order::Recall, Some(from)) if from == entry => None,
            _ => return Err(DispatchError::InvalidExpeditionOrder),
        };

        let survey = self.surveys.entry(key).or_insert_with(|| Survey {
            position: None,
            last_order: self.turn,
            visited: BTreeSet::new(),
        });
        survey.position = position;
        survey.last_order = self.turn;
        if let Some(site) = position {
            survey.visited.insert(site);
        }
        Ok(EventKind::ExpeditionChanged {
            player,
            theatre: id.to_owned(),
            tile: position,
        })
    }

    pub(super) fn theatre_observations(
        &self,
        seat: PlayerId,
        progress: &PlayerProgress,
    ) -> Vec<TheatreObservation> {
        let rules = base_ruleset();
        rules
            .theatres()
            .iter()
            .filter(|def| progress.has_age_unlock(rules, def.required_unlock()))
            .map(|def| {
                let board = topology(def);
                let entry = TileId::new(def.entry_tile());
                let survey = self.surveys.get(&(seat, def.id().to_owned()));
                let expedition = survey.and_then(|s| s.position);
                let mut known = BTreeSet::from([entry]);
                if let Some(survey) = survey {
                    for &site in &survey.visited {
                        known.insert(site);
                        known.extend(board.neighbors(site));
                    }
                }
                let mut visible = BTreeSet::new();
                if let Some(site) = expedition {
                    visible.insert(site);
                    visible.extend(board.neighbors(site));
                }
                let tiles = known
                    .iter()
                    .map(|&tile| TheatreTileObservation {
                        tile,
                        embedding: board
                            .embedding(tile)
                            .expect("charted site belongs to board"),
                        neighbors: board
                            .neighbors(tile)
                            .into_iter()
                            .filter(|n| known.contains(n))
                            .collect(),
                        visible: visible.contains(&tile),
                        visited: survey.is_some_and(|s| s.visited.contains(&tile)),
                    })
                    .collect();
                let available = !self.ended_players.contains(&seat)
                    && self.victories.is_empty()
                    && survey.is_none_or(|s| s.last_order < self.turn);
                TheatreObservation {
                    id: def.id().to_owned(),
                    name: def.name().to_owned(),
                    entry,
                    expedition,
                    can_launch: available
                        && expedition.is_none()
                        && self.cities.values().any(|city| city.owner == seat),
                    can_move: available
                        && expedition.is_some_and(|p| !board.neighbors(p).is_empty()),
                    can_recall: available && expedition == Some(entry),
                    tiles,
                }
            })
            .collect()
    }
}

pub(super) fn encode_surveys(surveys: &Surveys, bytes: &mut Vec<u8>) {
    put_u32(bytes, surveys.len() as u32);
    for ((seat, id), survey) in surveys {
        put_u32(bytes, seat.index());
        put_string(bytes, id);
        put_u32(bytes, survey.last_order);
        put_u8(bytes, u8::from(survey.position.is_some()));
        if let Some(position) = survey.position {
            put_u32(bytes, position.index());
        }
        put_u32(bytes, survey.visited.len() as u32);
        for tile in &survey.visited {
            put_u32(bytes, tile.index());
        }
    }
}

pub(super) fn decode_surveys(
    reader: &mut Reader<'_>,
    turn: u32,
    progress: &BTreeMap<PlayerId, PlayerProgress>,
) -> Result<Surveys, SerializationError> {
    let rules = base_ruleset();
    let invalid = SerializationError::InvalidTheatreState;
    let count = reader.count()?;
    if count == 0 || count > progress.len().saturating_mul(rules.theatres().len()) {
        return Err(invalid);
    }
    let mut surveys = Surveys::new();
    for _ in 0..count {
        let key = (PlayerId::new(reader.u32()?), reader.string()?);
        if surveys
            .last_key_value()
            .is_some_and(|(previous, _)| previous >= &key)
        {
            return Err(SerializationError::NonCanonicalOrder);
        }
        let definition = rules
            .theatre(&key.1)
            .filter(|def| {
                progress
                    .get(&key.0)
                    .is_some_and(|p| p.has_age_unlock(rules, def.required_unlock()))
            })
            .ok_or(invalid)?;
        let board = topology(definition);
        let entry = TileId::new(definition.entry_tile());
        let last_order = reader.u32()?;
        if last_order > turn {
            return Err(invalid);
        }
        let position = match reader.u8()? {
            0 => None,
            1 => Some(TileId::new(reader.u32()?)),
            tag => return Err(SerializationError::InvalidTag(tag)),
        };
        let n = reader.count()?;
        if n == 0
            || n > board.tile_count()
            || n as u64 + u64::from(position.is_none()) > u64::from(last_order) + 1
        {
            return Err(invalid);
        }
        let mut visited = BTreeSet::new();
        for _ in 0..n {
            let site = TileId::new(reader.u32()?);
            if !board.contains(site) {
                return Err(invalid);
            }
            if visited.last().is_some_and(|previous| *previous >= site) {
                return Err(SerializationError::NonCanonicalOrder);
            }
            visited.insert(site);
        }
        if !visited.contains(&entry) || position.is_some_and(|p| !visited.contains(&p)) {
            return Err(invalid);
        }
        // A continuous walk from the entry can only produce one connected survey.
        let mut connected = BTreeSet::from([entry]);
        let mut frontier = vec![entry];
        while let Some(site) = frontier.pop() {
            for neighbor in board.neighbors(site) {
                if visited.contains(&neighbor) && connected.insert(neighbor) {
                    frontier.push(neighbor);
                }
            }
        }
        if connected.len() != visited.len() {
            return Err(invalid);
        }
        surveys.insert(
            key,
            Survey {
                position,
                last_order,
                visited,
            },
        );
    }
    Ok(surveys)
}
