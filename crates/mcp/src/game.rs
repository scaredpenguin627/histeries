use histeries_ai::advance_greedy_opponents;
use histeries_rules_core::PlayerId;
use histeries_sim::{Command, MatchHost, MatchObservation, MatchView, Session, Viewpoint};
use serde_json::{Value, json};

use crate::wire;

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
    host: MatchHost,
    bots: Vec<PlayerId>,
}

impl Game {
    pub(crate) fn new(session: Session, bots: Vec<PlayerId>) -> Self {
        Self {
            host: MatchHost::new(session),
            bots,
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
            .host
            .session()
            .observe_match(role.viewpoint())
            .map_err(|e| e.to_string())?;
        let mut result = Self::render(view);
        if let Ok(seat) = role.player() {
            result["turn_ended"] = self.host.session().has_ended_turn(seat).into();
        }
        Ok(result)
    }

    pub(crate) fn replay(&self, role: Role, turn: u32) -> Result<Value, String> {
        self.host.replay(role.viewpoint(), turn).map(Self::render)
    }

    pub(crate) fn dispatch(&mut self, role: Role, id: &str, args: Value) -> Result<Value, String> {
        let player = role.player()?;
        if self.host.session().winner().is_some() {
            return Err("the game has ended".into());
        }
        let editor = matches!(role, Role::Editor(_));
        let command = wire::decode(id, args, player, editor)?;
        let ending = matches!(command, Command::TurnEnd { .. });
        if editor {
            self.host.dispatch_as_editor(player, command)?;
        } else {
            self.host.dispatch(player, command)?;
        }
        if ending {
            advance_greedy_opponents(&mut self.host, &self.bots)?;
        }
        self.observe(role)
    }

    pub(crate) fn resume(&mut self, role: Role) -> Result<(), String> {
        if role
            .player()
            .is_ok_and(|seat| self.host.session().has_ended_turn(seat))
        {
            advance_greedy_opponents(&mut self.host, &self.bots)?;
        }
        Ok(())
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
