//! Command authorization and bounded history for interactive matches.
use crate::{
    Command, MatchObservation, Observation, PlayerId, ProductionItem, ReplayLog, Session, Viewpoint,
};
use std::collections::{BTreeMap, VecDeque};

const HISTORY: usize = 32;
const ORDERS_PER_SEAT: usize = 512;

/// A shared interactive match, with one bounded replay window for all seats.
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
