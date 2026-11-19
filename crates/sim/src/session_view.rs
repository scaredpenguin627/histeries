//! Live and historical observations from the same simulation projection.

use super::{
    BTreeMap, BTreeSet, CityObservation, DispatchError, Observation, PlayerId, ProductionItem,
    ProgressObservation, ReplayLog, SerializationError, Session, SimulationState, TileId,
    TileTopology, UnitObservation, UnitSlots, VictoryKind, VisibleCityObservation,
    VisibleTileObservation, VisibleUnitObservation,
};

/// The viewpoint authorized by a match host.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Viewpoint {
    /// A player's own private data and visible board.
    Seat(PlayerId),
    /// Every seat's observation in canonical order.
    Spectator,
}

/// The board returned for the requested viewpoint.
#[derive(Debug, Eq, PartialEq)]
pub enum MatchView {
    /// A single player's fog-filtered board.
    Seat(Box<Observation>),
    /// The read-only spectator view.
    Spectator {
        /// The projected round.
        turn: u32,
        /// Individual seat views in stable player order.
        seats: Vec<Observation>,
    },
}

/// A board projection and the public outcome at the same point in the match.
#[derive(Debug, Eq, PartialEq)]
pub struct MatchObservation {
    /// Board filtered for the requested viewpoint.
    pub view: MatchView,
    /// The publicly declared winner, if any.
    pub winner: Option<(PlayerId, VictoryKind)>,
}

impl Session {
    /// Opens a session from a strict, versioned save document.
    pub fn from_save_bytes(bytes: &[u8]) -> Result<Self, SerializationError> {
        SimulationState::decode(bytes).map(Self::from_state)
    }

    /// Lists seat IDs in canonical order for host credential binding.
    pub fn seat_ids(&self) -> impl Iterator<Item = PlayerId> + '_ {
        self.state.players()
    }

    /// Whether this seat has already ended the current round.
    pub fn has_ended_turn(&self, seat: PlayerId) -> bool {
        self.state.ended_players.contains(&seat)
    }

    /// Projects the live match using the same visibility rules as replay.
    pub fn observe_match(&self, viewpoint: Viewpoint) -> Result<MatchObservation, DispatchError> {
        self.state.observe_match(viewpoint)
    }
}

impl ReplayLog {
    /// Returns the round recorded in the opening snapshot.
    pub fn opening_turn(&self) -> u32 {
        self.initial.turn()
    }

    /// Projects the opening snapshot before applying the segment's commands.
    pub fn observe_opening(&self, viewpoint: Viewpoint) -> Result<MatchObservation, DispatchError> {
        self.initial.observe_match(viewpoint)
    }

    /// Replays this segment using its recorded rules, then projects its final position.
    pub fn observe_end(&self, viewpoint: Viewpoint) -> Result<MatchObservation, DispatchError> {
        self.replay()?.observe_match(viewpoint)
    }
}

impl SimulationState {
    pub(super) fn winner(&self) -> Option<(PlayerId, VictoryKind)> {
        self.victories
            .first_key_value()
            .map(|(&seat, &kind)| (seat, kind))
    }

    fn observe_match(&self, viewpoint: Viewpoint) -> Result<MatchObservation, DispatchError> {
        let view = match viewpoint {
            Viewpoint::Seat(seat) => MatchView::Seat(Box::new(self.observe(seat)?)),
            Viewpoint::Spectator => MatchView::Spectator {
                turn: self.turn(),
                seats: self
                    .players()
                    .map(|seat| self.observe(seat))
                    .collect::<Result<_, _>>()?,
            },
        };
        Ok(MatchObservation {
            view,
            winner: self.winner(),
        })
    }

    /// Projects private seat data and the board within two steps of its cities and units.
    pub(super) fn observe(&self, seat: PlayerId) -> Result<Observation, DispatchError> {
        if !self.contains_player(seat) {
            return Err(DispatchError::UnknownPlayer(seat));
        }
        let units = self
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
                waiting: city.waiting.clone(),
                districts: city.districts().collect(),
                buildings: city.buildings().collect(),
            })
            .collect();
        let progress = self
            .progress
            .get(&seat)
            .ok_or(DispatchError::UnknownPlayer(seat))?;
        let visible_tiles = self.visible_tiles(seat);
        let mut slots = BTreeMap::<TileId, UnitSlots>::new();
        for unit in self.units.values() {
            slots.entry(unit.tile).or_default().insert(unit);
        }
        let mut occupied: BTreeSet<_> = slots.keys().copied().collect();
        let mut construction_sites = BTreeSet::new();
        for city in self.cities.values() {
            occupied.insert(city.tile);
            occupied.extend(city.districts.keys().copied());
            for item in city.orders() {
                if let ProductionItem::District { tile, .. } = item {
                    construction_sites.insert(tile);
                }
            }
        }
        let visible_tiles_observation = self
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
                        road: self.roads.contains(tile),
                        occupied: occupied.contains(tile),
                        units: slots.get(tile).copied().unwrap_or_default(),
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
            .units
            .values()
            .filter(|unit| visible_tiles.contains(&unit.tile))
            .map(VisibleUnitObservation::from)
            .collect();
        let visible_cities = self
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
            turn: self.turn,
            own_marker: self
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
            victory: self.victories.get(&seat).copied(),
            visible_tiles: visible_tiles_observation,
            visible_units,
            visible_cities,
        })
    }

    fn visible_tiles(&self, seat: PlayerId) -> BTreeSet<TileId> {
        let Some(map) = &self.map else {
            return BTreeSet::new();
        };
        let sources = self
            .units
            .values()
            .filter(|unit| unit.owner == Some(seat))
            .map(|unit| unit.tile)
            .chain(
                self.cities
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
