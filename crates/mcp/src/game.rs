use std::collections::{BTreeMap, VecDeque};

use histeries_ai::{GreedyPlayer, Player, plan_seats};
use histeries_rules_core::PlayerId;
use histeries_sim::{
    Command, MatchObservation, MatchView, Observation, ProductionItem, ReplayLog, Session,
    Viewpoint,
};
use serde_json::{Value, json};

use crate::wire;

const HISTORY: usize = 32;
const ORDERS_PER_SEAT: usize = 512;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Role {
    Seat(PlayerId),
    Editor(PlayerId),
    Spectator,
}

impl Role {
    fn viewpoint(self) -> Viewpoint {
        match self {
            Self::Seat(seat) | Self::Editor(seat) => Viewpoint::Seat(seat),
            Self::Spectator => Viewpoint::Spectator,
        }
    }

    pub(crate) fn player(self) -> Result<PlayerId, String> {
        match self {
            Self::Seat(seat) | Self::Editor(seat) => Ok(seat),
            Self::Spectator => Err("spectators have read-only access".into()),
        }
    }
}

pub(crate) struct Game {
    session: Session,
    bots: Vec<PlayerId>,
    orders: BTreeMap<PlayerId, usize>,
    history: VecDeque<(u32, ReplayLog)>,
}

impl Game {
    pub(crate) fn new(mut session: Session, bots: Vec<PlayerId>) -> Self {
        session.take_replay_segment();
        session.drain_events();
        Self {
            session,
            bots,
            orders: BTreeMap::new(),
            history: VecDeque::new(),
        }
    }

    fn render(projected: MatchObservation) -> Value {
        let mut view = match projected.view {
            MatchView::Seat(seat) => wire::observation(&seat),
            MatchView::Spectator { turn, seats } => json!({"turn":turn, "seats":seats}),
        };
        view["winner"] = json!(
            projected
                .winner
                .map(|(seat, kind)| json!({"seat":seat.index(),"kind":kind}))
        );
        view
    }

    pub(crate) fn observe(&self, role: Role) -> Result<Value, String> {
        let view = self
            .session
            .observe_match(role.viewpoint())
            .map_err(|e| e.to_string())?;
        let mut result = Self::render(view);
        if let Ok(seat) = role.player() {
            result["turn_ended"] = self.session.has_ended_turn(seat).into();
        }
        Ok(result)
    }

    pub(crate) fn replay(&self, role: Role, turn: u32) -> Result<Value, String> {
        // Each entry replays the commands leading from one round boundary to the next.
        let round = self.history.iter().find(|(end, _)| *end == turn);
        let viewpoint = role.viewpoint();
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
        projected.map(Self::render).map_err(|e| e.to_string())
    }

    fn apply(&mut self, player: PlayerId, command: Command) -> Result<(), String> {
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

    pub(crate) fn dispatch(&mut self, role: Role, id: &str, args: Value) -> Result<Value, String> {
        let player = role.player()?;
        if self.session.winner().is_some() {
            return Err("the game has ended".into());
        }
        let editor = matches!(role, Role::Editor(_));
        let command = wire::decode(id, args, player, editor)?;
        if !editor
            && !visible_targets(
                &self.session.observe(player).map_err(|e| e.to_string())?,
                &command,
            )
        {
            return Err("target is unavailable to this seat".into());
        }
        let ending = matches!(command, Command::TurnEnd { .. });
        self.apply(player, command)?;
        if ending {
            self.advance_bots()?;
        }
        self.observe(role)
    }

    pub(crate) fn resume(&mut self, role: Role) -> Result<(), String> {
        if role
            .player()
            .is_ok_and(|seat| self.session.has_ended_turn(seat))
        {
            self.advance_bots()?;
        }
        Ok(())
    }

    fn advance_bots(&mut self) -> Result<(), String> {
        if self.bots.is_empty() || self.session.winner().is_some() {
            return Ok(());
        }
        let round = self.session.turn();
        let mut controllers = vec![GreedyPlayer; self.bots.len()];
        let mut seats: Vec<(&mut (dyn Player + Send), Observation)> = Vec::new();
        for (&seat, controller) in self.bots.iter().zip(&mut controllers) {
            if !self.session.has_ended_turn(seat) {
                let view = self.session.observe(seat).map_err(|e| e.to_string())?;
                seats.push((controller, view));
            }
        }
        // plan_seats sorts the seat/observation pairs and returns the plans in that order.
        let plans = plan_seats(&mut seats, self.bots.len());
        debug_assert_eq!(seats.len(), plans.len());
        for ((_, view), plan) in seats.into_iter().zip(plans) {
            if self.session.turn() != round {
                break;
            }
            if self.session.has_ended_turn(view.seat()) {
                continue;
            }
            for order in plan.commands {
                if self.session.winner().is_some() {
                    return Ok(());
                }
                // Earlier orders may have invalidated the remaining plan. A rejected
                // tactical order never prevents the opponent handing the turn back.
                let ending = matches!(order, Command::TurnEnd { .. });
                let applied = self.apply(view.seat(), order);
                if ending {
                    applied?;
                }
            }
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
mod tests {
    use super::*;

    #[test]
    fn replay_exposes_only_the_authorized_observation_and_public_winner() {
        let [one, two] = [9, 27].map(PlayerId::new);
        let session = Session::new([one, two]);
        let mut expected = wire::observation(&session.observe(two).unwrap());
        expected["winner"] = Value::Null;
        let mut game = Game::new(session, vec![]);
        game.dispatch(Role::Editor(one), "editor.set_marker", json!({"value":791}))
            .unwrap();
        game.dispatch(Role::Seat(one), "turn.end", json!({}))
            .unwrap();
        game.dispatch(Role::Seat(two), "turn.end", json!({}))
            .unwrap();

        assert_eq!(game.replay(Role::Seat(two), 0).unwrap(), expected);
        expected["turn"] = 1.into();
        assert_eq!(game.replay(Role::Seat(two), 1).unwrap(), expected);
        expected["turn_ended"] = false.into();
        assert_eq!(game.observe(Role::Seat(two)).unwrap(), expected);
        assert_eq!(game.replay(Role::Seat(one), 1).unwrap()["own_marker"], 791);
        let spectator = game.replay(Role::Spectator, 1).unwrap();
        assert_eq!(spectator["seats"][0]["own_marker"], 791);
        assert_eq!(spectator["seats"][1]["own_marker"], 0);
    }
}
