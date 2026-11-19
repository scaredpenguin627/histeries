//! Command authorization and bounded history for interactive matches.
use crate::{
    BoardObservation, Command, DispatchError, MatchObservation, Observation, PlayerId,
    ProductionItem, ReplayLog, Session, Viewpoint,
};
use std::collections::{BTreeMap, VecDeque};

const HISTORY: usize = 32;
const ORDERS_PER_SEAT: usize = 512;

/// A shared interactive match, with one bounded replay window for all seats.
#[derive(Clone)]
pub struct MatchHost {
    session: Session,
    orders: BTreeMap<PlayerId, usize>,
    history: VecDeque<(u32, ReplayLog)>,
}

impl MatchHost {
    /// Hosts a live session starting its replay history at the supplied position.
    pub fn new(mut session: Session) -> Self {
        session.take_replay_segment();
        session.drain_events();
        Self {
            session,
            orders: BTreeMap::new(),
            history: VecDeque::new(),
        }
    }

    /// Reads the hosted session through its public interface.
    pub fn session(&self) -> &Session {
        &self.session
    }

    /// Projects every tile of the locally owned scenario. Remote seat and spectator
    /// readers receive their authorized match views through the session instead.
    pub(crate) fn observe_editor(&self, seat: PlayerId) -> Result<BoardObservation, DispatchError> {
        let map = self.session.editor_map(seat)?;
        Ok(self
            .session
            .state
            .project_board(&map.tiles().map(|(id, _)| id).collect()))
    }

    /// Reconstructs the authorized view at a retained round boundary.
    pub fn replay(&self, viewpoint: Viewpoint, turn: u32) -> Result<MatchObservation, String> {
        // Each entry replays the commands leading from one round boundary to the next.
        let round = self.history.iter().find(|(end, _)| *end == turn);
        let segment = self.session.replay();
        let projected = if turn == self.session.turn() && self.session.winner().is_some() {
            segment.observe_end(viewpoint)
        } else if let Some((_, log)) = round {
            log.observe_end(viewpoint)
        } else if turn == segment.opening_turn() {
            segment.observe_opening(viewpoint)
        } else if let Some((_, first)) = self
            .history
            .front()
            .filter(|(_, log)| log.opening_turn() == turn)
        {
            first.observe_opening(viewpoint)
        } else {
            return Err("turn is outside the retained replay window".into());
        };
        projected.map_err(|e| e.to_string())
    }

    /// Verifies seat authority and target visibility, then dispatches a regular player order.
    pub fn dispatch(&mut self, seat: PlayerId, command: Command) -> Result<(), String> {
        let view = self.session.observe(seat).map_err(|e| e.to_string())?;
        if !visible_targets(&view, &command) {
            return Err("target is unavailable to this seat".into());
        }
        self.dispatch_as_editor(seat, command)
    }

    /// Applies an editor batch atomically. Invalid commands and invalid resulting saves
    /// discard the entire candidate, including its events, order counts and replay.
    pub(crate) fn dispatch_editor_batch(
        &mut self,
        player: PlayerId,
        commands: impl IntoIterator<Item = Command>,
    ) -> Result<Vec<u8>, String> {
        self.session.editor_map(player).map_err(|e| e.to_string())?;
        let mut staged = self.clone();
        for command in commands {
            staged.dispatch_as_editor(player, command)?;
        }
        let bytes = staged.session.save_bytes();
        Session::from_save_bytes(&bytes).map_err(|e| e.to_string())?;
        *self = staged;
        Ok(bytes)
    }

    /// Dispatches an order bound to an authorized host editor, including scenario edits.
    pub fn dispatch_as_editor(&mut self, player: PlayerId, command: Command) -> Result<(), String> {
        if self.session.winner().is_some() {
            return Err("the game has ended".into());
        }
        if command.issuer().is_some_and(|issuer| issuer != player) {
            return Err("order belongs to another seat".into());
        }
        if self.session.has_ended_turn(player) {
            return Err("seat has already ended this turn".into());
        }
        let ending = matches!(command, Command::TurnEnd { .. });
        if !ending && self.orders.get(&player).copied().unwrap_or(0) >= ORDERS_PER_SEAT {
            return Err("turn order limit reached; end the turn to continue".into());
        }
        let before = self.session.turn();
        self.session.dispatch(command).map_err(|e| e.to_string())?;
        self.session.drain_events();
        if self.session.turn() != before {
            if self.history.len() == HISTORY {
                self.history.pop_front();
            }
            self.history
                .push_back((self.session.turn(), self.session.take_replay_segment()));
            self.orders.clear();
        } else if !ending {
            *self.orders.entry(player).or_default() += 1;
        }
        Ok(())
    }
}

fn visible_targets(view: &Observation, command: &Command) -> bool {
    let unit = |id| view.units().iter().any(|u| u.id == id);
    let city = |id| view.cities().iter().any(|c| c.id == id);
    let tile = |id| view.visible_tiles().iter().any(|t| t.tile == id);
    match command {
        Command::UnitMove {
            unit: id,
            destination,
            ..
        } => unit(*id) && tile(*destination),
        Command::UnitBuildRoad { unit: id, .. }
        | Command::UnitHeal { unit: id, .. }
        | Command::UnitHold { unit: id, .. }
        | Command::UnitPromote { unit: id, .. }
        | Command::CitySettle { unit: id, .. } => unit(*id),
        Command::UnitCaptureCity {
            unit: id,
            city: target,
            ..
        } => {
            unit(*id)
                && view
                    .visible_cities()
                    .iter()
                    .any(|c| c.id == *target && c.owner != view.seat())
        }
        Command::UnitAttack {
            attacker, defender, ..
        } => unit(*attacker) && view.visible_units().iter().any(|u| u.id == *defender),
        Command::CityBuildDistrict {
            city: id,
            tile: site,
            ..
        }
        | Command::CityEnqueueProduction {
            city: id,
            item: ProductionItem::District { tile: site, .. },
            ..
        }
        | Command::CitySetProduction {
            city: id,
            item: ProductionItem::District { tile: site, .. },
            ..
        } => city(*id) && tile(*site),
        Command::CityEnqueueProduction { city: id, .. }
        | Command::CitySetProduction { city: id, .. }
        | Command::CityBuildBuilding { city: id, .. } => city(*id),
        Command::ResearchCivic { .. }
        | Command::ResearchTechnology { .. }
        | Command::PolicyAdopt { .. }
        | Command::PolicyRemove { .. }
        | Command::TurnEnd { .. } => true,
        Command::CityFound { .. }
        | Command::UnitSpawn { .. }
        | Command::BarbarianSpawn { .. }
        | Command::EditorSetFeature { .. }
        | Command::EditorSetTerrain { .. }
        | Command::EditorSetRiver { .. }
        | Command::EditorSetResource { .. }
        | Command::EditorSetMarker { .. } => false,
    }
}

#[cfg(test)]
mod editor_tests {
    use super::*;
    use crate::{MapState, Terrain};
    use histeries_hex::{FlatHex, TileId};

    #[test]
    fn only_the_host_editor_projects_unseen_tiles() {
        let seat = PlayerId::new(1);
        let map = MapState::blank(FlatHex::new(5, 4, false, false).unwrap());
        let host = MatchHost::new(Session::with_map([seat], map));
        let private = host.session().observe(seat).unwrap();
        assert!(private.visible_tiles().is_empty());
        let board = host.observe_editor(seat).unwrap();
        assert_eq!(board.visible_tiles().len(), 20);
        assert_eq!(host.session().observe(seat).unwrap(), private);
        let stranger = PlayerId::new(99);
        assert_eq!(
            host.observe_editor(stranger),
            Err(DispatchError::UnknownPlayer(stranger))
        );
        assert_eq!(
            host.session().observe(stranger),
            Err(DispatchError::UnknownPlayer(stranger))
        );
        let empty = MatchHost::new(Session::new([seat]));
        assert_eq!(empty.observe_editor(seat), Err(DispatchError::NoMap));
    }

    #[test]
    fn accepted_editor_replay_starts_at_the_pre_edit_save_and_reproduces_the_result() {
        let seat = PlayerId::new(7);
        let map = MapState::blank(FlatHex::new(3, 2, false, false).unwrap());
        let mut session = Session::with_map([seat], map);
        session
            .dispatch(Command::EditorSetMarker {
                player: seat,
                value: 9,
            })
            .unwrap();
        let before = session.save_bytes();
        let mut host = MatchHost::new(session);
        let tile = TileId::new(2);
        let batch = [
            Command::EditorSetTerrain {
                player: seat,
                tile,
                terrain: Terrain::Hill,
            },
            Command::EditorSetMarker {
                player: seat,
                value: 13,
            },
        ];
        let after = host.dispatch_editor_batch(seat, batch.clone()).unwrap();
        let log = ReplayLog::decode(&host.session().replay().encode()).unwrap();
        assert_eq!(log.commands(), batch);
        let mut playback = log.playback_session();
        assert_eq!(playback.save_bytes(), before);
        for command in log.commands() {
            playback.dispatch(command.clone()).unwrap();
        }
        assert_eq!(playback.save_bytes(), after);
        assert_eq!(log.replay().unwrap().encode(), after);
    }

    #[test]
    fn rejected_editor_batch_preserves_the_hosts_state_order_counts_and_replay() {
        let one = PlayerId::new(1);
        let two = PlayerId::new(2);
        let mut host = MatchHost::new(Session::with_map(
            [one, two],
            MapState::blank(FlatHex::new(3, 2, false, false).unwrap()),
        ));
        host.dispatch_editor_batch(
            one,
            [Command::EditorSetMarker {
                player: one,
                value: 8,
            }],
        )
        .unwrap();
        host.dispatch(two, Command::TurnEnd { player: two })
            .unwrap();
        let before = host.session().save_bytes();
        let replay = host.session().replay().encode();
        let orders = host.orders.clone();
        let batch = [
            Command::TurnEnd { player: one },
            Command::EditorSetTerrain {
                player: one,
                tile: TileId::new(99),
                terrain: Terrain::Hill,
            },
        ];
        assert!(host.dispatch_editor_batch(one, batch).is_err());
        assert_eq!(host.session().save_bytes(), before);
        assert_eq!(host.session().replay().encode(), replay);
        assert_eq!(host.orders, orders);
        assert!(host.history.is_empty());
        host.dispatch(one, Command::TurnEnd { player: one })
            .unwrap();
        assert_eq!(host.history.len(), 1);
    }
}
