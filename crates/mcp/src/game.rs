use histeries_ai::advance_greedy_opponents;
use histeries_net::lan::{LanClient, LanError, Refusal, SubmissionOutcome};
use histeries_rules_core::PlayerId;
use histeries_sim::{
    Command, MatchHost, MatchObservation, MatchView, Session, VictoryKind, Viewpoint,
};
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

pub(crate) trait MatchTools: Send {
    fn observe(&mut self, role: Role) -> Result<Value, String>;
    fn replay(&self, role: Role, turn: u32) -> Result<Value, String>;
    fn dispatch(&mut self, role: Role, id: &str, args: Value) -> Result<Value, String>;
    fn apply_package(
        &mut self,
        _role: Role,
        _manifest: &str,
        _module: &str,
    ) -> Result<Value, String> {
        Err("an editor credential is required".into())
    }
}

fn with_winner(mut view: Value, winner: Option<(PlayerId, VictoryKind)>) -> Value {
    view["winner"] = json!(winner.map(|(seat, kind)| json!({"seat":seat.index(),"kind":kind})));
    view
}

fn render(projected: MatchObservation) -> Value {
    let view = match projected.view {
        MatchView::Seat(seat) => wire::observation(&seat),
        MatchView::Spectator { turn, seats } => json!({"turn":turn,"seats":seats}),
    };
    with_winner(view, projected.winner)
}

fn live(session: &Session, role: Role) -> Result<Value, String> {
    let view = session
        .observe_match(role.viewpoint())
        .map_err(|e| e.to_string())?;
    let mut result = render(view);
    if let Ok(seat) = role.player() {
        result["turn_ended"] = session.has_ended_turn(seat).into();
    }
    Ok(result)
}

impl Game {
    pub(crate) fn new(session: Session, bots: Vec<PlayerId>) -> Self {
        Self {
            host: MatchHost::new(session),
            bots,
        }
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

impl MatchTools for Game {
    fn observe(&mut self, role: Role) -> Result<Value, String> {
        live(self.host.session(), role)
    }

    fn replay(&self, role: Role, turn: u32) -> Result<Value, String> {
        self.host.replay(role.viewpoint(), turn).map(render)
    }

    fn dispatch(&mut self, role: Role, id: &str, args: Value) -> Result<Value, String> {
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

    fn apply_package(&mut self, role: Role, manifest: &str, module: &str) -> Result<Value, String> {
        use base64::prelude::*;
        use histeries_sim::extensions::{MAX_MANIFEST_BYTES, MAX_MODULE_BYTES, Package};
        let Role::Editor(seat) = role else {
            return Err("an editor credential is required".into());
        };
        if manifest.len() > MAX_MANIFEST_BYTES || module.len() > MAX_MODULE_BYTES.div_ceil(3) * 4 {
            return Err("package exceeds size limit".into());
        }
        let wasm = BASE64_STANDARD
            .decode(module)
            .map_err(|_| "invalid Wasm base64")?;
        let package = Package::from_bytes(manifest.as_bytes(), &wasm)?;
        let commands = package.apply(&mut self.host, seat)?;
        Ok(json!({"id":package.id(),"commands":commands}))
    }
}

fn lan_view(client: &LanClient) -> Result<Value, String> {
    let replica = client.replica();
    let mut view = live(replica.session(), Role::Seat(client.seat()))?;
    view["active_seat"] = json!(replica.active_seat().map(PlayerId::index));
    Ok(view)
}

// A trusted replica's diagnostics can include both full-world hashes and errors from
// replaying another seat's orders. Only report the public failure category over MCP.
pub(crate) fn seat_lan_error(error: LanError) -> String {
    match error {
        LanError::Refused(Refusal::Credentials) => "invalid seat credentials",
        LanError::Refused(Refusal::Version { .. }) => "incompatible LAN protocol version",
        LanError::Refused(Refusal::Stale { .. }) => "stale position. Observe the match again",
        LanError::Refused(Refusal::Superseded) | LanError::SubmissionPending => {
            "observe the match before submitting another order"
        }
        LanError::Refused(Refusal::RecoveryExpired) => "the host cannot verify this match session",
        LanError::InterruptedSubmissionRequiresRejoin => {
            "the interrupted order's outcome is unknown. Rejoin the match with a fresh adapter"
        }
        LanError::Refused(Refusal::Desync { .. }) | LanError::Replica(_) => {
            "LAN replica verification failed"
        }
        LanError::Refused(Refusal::Rejected(_)) => "LAN host rejected the request",
        LanError::Protocol(_) => "invalid LAN protocol response",
        LanError::Io(_) => "LAN connection interrupted",
    }
    .into()
}

fn add_submission_outcome(view: &mut Value, submission: Option<SubmissionOutcome>) {
    if let Some(outcome) = submission {
        view["recovered_submission"] = json!(outcome);
    }
}

impl MatchTools for LanClient {
    fn observe(&mut self, _role: Role) -> Result<Value, String> {
        self.sync().map_err(seat_lan_error)?;
        let mut view = lan_view(self)?;
        add_submission_outcome(&mut view, self.submission_outcome());
        Ok(view)
    }

    fn replay(&self, _role: Role, turn: u32) -> Result<Value, String> {
        let replay = self.replay(turn).map_err(seat_lan_error)?;
        Ok(with_winner(replay.observation, replay.winner))
    }

    fn dispatch(&mut self, _role: Role, id: &str, args: Value) -> Result<Value, String> {
        let order = wire::decode(id, args, self.seat(), false)?;
        if self.replica().active_seat() != Some(self.seat()) {
            return Err("observe the match and wait for your seat's turn".into());
        }
        self.submit(order).map_err(seat_lan_error)?;
        lan_view(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use histeries_net::{DesyncReport, Position};

    #[test]
    fn sync_reports_only_the_seats_own_submission_outcome() {
        let mut ordinary = json!({});
        add_submission_outcome(&mut ordinary, None);
        assert_eq!(ordinary, json!({}));

        for (outcome, label) in [
            (SubmissionOutcome::Accepted, "accepted"),
            (SubmissionOutcome::Canceled, "canceled"),
            (SubmissionOutcome::Unknown, "unknown"),
        ] {
            let mut view = json!({});
            add_submission_outcome(&mut view, Some(outcome));
            assert_eq!(view, json!({"recovered_submission": label}));
        }

        assert_eq!(
            seat_lan_error(LanError::InterruptedSubmissionRequiresRejoin),
            "the interrupted order's outcome is unknown. Rejoin the match with a fresh adapter"
        );
    }

    #[test]
    fn lan_failures_do_not_expose_positions_or_other_seats_order_diagnostics() {
        let expected = Position {
            sequence: 12,
            turn: 1,
            hash: 0x1234_abcd,
        };
        let received = Position {
            hash: 0x5678_dcba,
            ..expected
        };
        let private_reason = "opponent order failed replay validation";
        for error in [
            LanError::Refused(Refusal::Desync { expected, received }),
            LanError::Replica(DesyncReport {
                before: received,
                seat: PlayerId::new(2),
                expected,
                actual: received,
                reason: private_reason.into(),
            }),
        ] {
            assert_eq!(seat_lan_error(error), "LAN replica verification failed");
        }
        assert_eq!(
            seat_lan_error(LanError::Protocol(format!("{private_reason}: {expected}"))),
            "invalid LAN protocol response"
        );
        assert_eq!(
            seat_lan_error(LanError::Refused(Refusal::Rejected(private_reason.into()))),
            "LAN host rejected the request"
        );
    }

    #[test]
    fn editor_and_player_tokens_receive_the_same_fogged_flat_json_observation() {
        let draft = histeries_sim::ScenarioDraft::blank(4, 3, 2).unwrap();
        assert_eq!(draft.board().visible_tiles().len(), 12);
        let seat = draft.seats()[0];
        let session = Session::from_save_bytes(draft.bytes()).unwrap();
        let value = wire::observation(&session.observe(seat).unwrap());
        let expected = [
            "cities",
            "own_marker",
            "progress",
            "seat",
            "turn",
            "units",
            "victory",
            "visible_cities",
            "visible_tiles",
            "visible_units",
        ];
        let mut keys: Vec<_> = value
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        keys.sort_unstable();
        assert_eq!(keys, expected);
        let mut game = Game::new(session, vec![]);
        let player = game.observe(Role::Seat(seat)).unwrap();
        assert_eq!(game.observe(Role::Editor(seat)).unwrap(), player);
        assert_eq!(player["visible_tiles"], json!([]));
    }

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
