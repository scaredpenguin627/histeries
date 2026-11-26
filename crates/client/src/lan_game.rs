//! A seat-scoped desktop view of a LAN match. The worker owns the replica and all sockets.

use std::{
    net::SocketAddr,
    sync::mpsc::{Receiver, SyncSender, TryRecvError, sync_channel},
    thread,
    time::{Duration, Instant},
};

use histeries_net::{
    Position,
    lan::{LanClient, LanError, Refusal, SubmissionOutcome, SyncOutcome},
};
use histeries_rules_core::PlayerId;
use histeries_sim::{Command, Observation, VictoryKind};

const POLL_INTERVAL: Duration = Duration::from_millis(600);

struct View {
    observation: Observation,
    position: Position,
    active: Option<PlayerId>,
    winner: Option<(PlayerId, VictoryKind)>,
}

impl View {
    fn project(client: &LanClient, seat: PlayerId) -> Result<Self, String> {
        let replica = client.replica();
        let observation = replica.session().observe(seat).map_err(|e| e.to_string())?;
        Ok(Self {
            observation,
            position: replica.position(),
            active: replica.active_seat(),
            winner: replica.session().winner(),
        })
    }
}

enum Request {
    Sync(Position),
    Submit(Command),
}

enum Update {
    Verified {
        view: Option<Box<View>>,
        notice: Option<String>,
    },
    Rejected(String),
    NeedsSync(String),
    Stopped(String),
}

impl Update {
    fn verified(
        client: &LanClient,
        seat: PlayerId,
        previous: Option<Position>,
        notice: Option<String>,
    ) -> Self {
        let view = if previous == Some(client.replica().position()) {
            None
        } else {
            match View::project(client, seat) {
                Ok(view) => Some(Box::new(view)),
                Err(reason) => return Self::Stopped(reason),
            }
        };
        Self::Verified { view, notice }
    }

    fn failed(error: LanError) -> Self {
        let message = error.to_string();
        match error {
            LanError::Refused(Refusal::Rejected(_)) => Self::Rejected(message),
            LanError::Refused(Refusal::RecoveryExpired) => Self::Stopped(format!(
                "{message}. Rejoin to inspect the current seat state."
            )),
            LanError::Io(_)
            | LanError::Protocol(_)
            | LanError::SubmissionPending
            | LanError::Refused(Refusal::Superseded | Refusal::Stale { .. }) => Self::NeedsSync(
                format!("{message}. Synchronize to verify the current position."),
            ),
            _ => Self::Stopped(message),
        }
    }
}

fn run(
    address: SocketAddr,
    seat: PlayerId,
    token: String,
    requests: Receiver<Request>,
    updates: SyncSender<Update>,
) {
    let mut client = match LanClient::connect(address, seat, token) {
        Ok(client) => client,
        Err(error) => {
            let _ = updates.send(Update::Stopped(error.to_string()));
            return;
        }
    };
    if updates
        .send(Update::verified(&client, seat, None, None))
        .is_err()
    {
        return;
    }
    while let Ok(request) = requests.recv() {
        let update = match request {
            Request::Submit(cmd) => match client.submit(cmd) {
                Ok(()) => Update::verified(&client, seat, None, None),
                Err(error) => Update::failed(error),
            },
            Request::Sync(previous) => match client.sync() {
                Ok(outcome) => {
                    let mut notice = match outcome {
                        SyncOutcome::Replayed { .. } => None,
                        SyncOutcome::Checkpoint { previous, current } => Some(format!(
                            "Restored a checkpoint from sequence {} to {}.",
                            previous.sequence, current.sequence
                        )),
                    };
                    if let Some(outcome) = client.submission_outcome() {
                        let recovery = match outcome {
                            SubmissionOutcome::Accepted => {
                                "The host accepted the interrupted order."
                            }
                            SubmissionOutcome::Canceled => {
                                "The host canceled the interrupted order."
                            }
                            SubmissionOutcome::Unknown => {
                                "The interrupted order's outcome is unknown because the host's receipt expired. Inspect your seat before issuing another order."
                            }
                        };
                        notice = Some(match notice {
                            Some(checkpoint) => format!("{recovery} {checkpoint}"),
                            None => recovery.into(),
                        });
                    }
                    Update::verified(&client, seat, Some(previous), notice)
                }
                // A failed synchronization can never authorize another order.
                Err(LanError::Refused(Refusal::Rejected(reason))) => Update::NeedsSync(reason),
                Err(error) => Update::failed(error),
            },
        };
        let stopped = matches!(update, Update::Stopped(_));
        if updates.send(update).is_err() || stopped {
            return;
        }
    }
}

/// The window owns both channel ends. Dropping them retires an idle worker immediately.
/// An in-flight exchange exits at its socket deadline without blocking the window on join.
pub(crate) struct LanGame {
    seat: PlayerId,
    requests: SyncSender<Request>,
    updates: Receiver<Update>,
    view: Option<Box<View>>,
    busy: bool,
    needs_sync: bool,
    stopped: bool,
    next_poll: Instant,
    error: Option<String>,
    notice: Option<String>,
}

impl LanGame {
    pub(crate) fn connect(
        address: SocketAddr,
        seat: PlayerId,
        token: String,
    ) -> Result<Self, String> {
        let (requests, incoming) = sync_channel(1);
        let (outgoing, updates) = sync_channel(1);
        thread::Builder::new()
            .name("LAN seat".into())
            .spawn(move || run(address, seat, token, incoming, outgoing))
            .map_err(|e| e.to_string())?;
        Ok(Self {
            seat,
            requests,
            updates,
            view: None,
            busy: true,
            needs_sync: false,
            stopped: false,
            next_poll: Instant::now(),
            error: None,
            notice: None,
        })
    }

    pub(crate) fn seat(&self) -> PlayerId {
        self.seat
    }

    pub(crate) fn observation(&self) -> Option<&Observation> {
        self.view.as_ref().map(|view| &view.observation)
    }

    pub(crate) fn position(&self) -> Option<Position> {
        self.view.as_ref().map(|view| view.position)
    }

    pub(crate) fn active_seat(&self) -> Option<PlayerId> {
        self.view.as_ref().and_then(|view| view.active)
    }

    pub(crate) fn winner(&self) -> Option<(PlayerId, VictoryKind)> {
        self.view.as_ref().and_then(|view| view.winner)
    }

    pub(crate) fn ready(&self) -> bool {
        !self.busy && !self.stopped && !self.needs_sync && self.active_seat() == Some(self.seat)
    }

    pub(crate) fn busy(&self) -> bool {
        self.busy
    }

    pub(crate) fn can_sync(&self) -> bool {
        self.view.is_some() && !self.busy && !self.stopped
    }

    pub(crate) fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    pub(crate) fn status(&self) -> Option<&str> {
        self.notice.as_deref()
    }

    /// Consumes at most one response and, while waiting for another seat, starts a bounded poll.
    pub(crate) fn poll(&mut self) -> bool {
        if self.stopped {
            return false;
        }
        let update = match self.updates.try_recv() {
            Ok(update) => update,
            Err(TryRecvError::Empty) => {
                if !self.busy
                    && !self.needs_sync
                    && self.winner().is_none()
                    && self.active_seat() != Some(self.seat)
                    && Instant::now() >= self.next_poll
                {
                    let _ = self.sync();
                    return true;
                }
                return false;
            }
            Err(TryRecvError::Disconnected) => Update::Stopped("LAN worker disconnected.".into()),
        };
        self.busy = false;
        self.next_poll = Instant::now() + POLL_INTERVAL;
        match update {
            Update::Verified { view, notice } => {
                if let Some(view) = view {
                    self.view = Some(view);
                }
                self.error = None;
                if self.needs_sync && notice.is_none() {
                    self.notice = self.position().map(|position| {
                        format!("Synchronized at sequence {}. Check your seat's view before issuing another order.", position.sequence)
                    });
                } else if notice.is_some() {
                    self.notice = notice;
                }
                self.needs_sync = false;
            }
            Update::Stopped(reason) => {
                self.stopped = true;
                self.error = Some(reason);
            }
            Update::NeedsSync(reason) => {
                self.needs_sync = true;
                self.error = Some(reason);
            }
            Update::Rejected(reason) => self.error = Some(reason),
        }
        true
    }

    pub(crate) fn submit(&mut self, command: Command) -> Result<(), String> {
        if !self.ready() {
            return Err(
                "Wait for your seat and a verified position before issuing an order.".into(),
            );
        }
        self.send(Request::Submit(command))?;
        self.error = None;
        self.notice = None;
        Ok(())
    }

    pub(crate) fn sync(&mut self) -> Result<(), String> {
        let previous = self.position().ok_or("The LAN seat has not joined yet.")?;
        self.send(Request::Sync(previous))
    }

    fn send(&mut self, request: Request) -> Result<(), String> {
        if self.stopped {
            return Err("The LAN worker has stopped. Rejoin the match to reconnect.".into());
        }
        if self.busy {
            return Err("A LAN request is already in progress.".into());
        }
        self.requests
            .try_send(request)
            .map_err(|_| "LAN worker unavailable".to_owned())?;
        self.busy = true;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use histeries_net::{LockstepHost, lan::LanServer};
    use histeries_sim::Session;
    use std::{
        collections::BTreeMap,
        net::TcpListener,
        sync::atomic::{AtomicBool, Ordering},
    };

    fn wait(game: &mut LanGame, until: impl Fn(&LanGame) -> bool) {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            game.poll();
            if until(game) {
                return;
            }
            assert!(
                Instant::now() < deadline,
                "LAN seat did not settle: {:?}",
                game.error()
            );
            thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn lost_session_stops_the_worker_and_only_an_interrupted_order_reports_an_unknown_outcome() {
        let Update::Stopped(interrupted) =
            Update::failed(LanError::InterruptedSubmissionRequiresRejoin)
        else {
            panic!("a lost submission session must stop the worker");
        };
        assert!(interrupted.contains("outcome is unknown"));
        assert!(interrupted.contains("Rejoin the match"));
        let Update::Stopped(idle) = Update::failed(LanError::Refused(Refusal::RecoveryExpired))
        else {
            panic!("a replaced session requires a fresh join");
        };
        assert!(!idle.contains("interrupted"));
        assert!(idle.contains("Rejoin"));
    }

    #[test]
    fn a_silent_host_leaves_the_window_responsive_and_commands_disabled() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let seat = PlayerId::new(4);
        let mut game =
            LanGame::connect(listener.local_addr().unwrap(), seat, "four".into()).unwrap();
        let (socket, _) = listener.accept().unwrap();
        assert!(game.busy());
        assert!(!game.poll());
        assert!(!game.can_sync());
        assert!(!game.ready());
        assert!(game.observation().is_none());
        assert!(game.submit(Command::TurnEnd { player: seat }).is_err());
        drop(socket);
        wait(&mut game, |g| g.error().is_some());
        assert!(!game.busy());
        assert!(!game.ready());
    }

    #[test]
    fn a_stale_window_recovers_its_own_view_without_resending_its_order() {
        let one = PlayerId::new(4);
        let two = PlayerId::new(9);
        let mut session = Session::new([one, two]);
        for seat in [one, two] {
            session
                .dispatch(Command::EditorSetMarker {
                    player: seat,
                    value: i64::from(seat.index()),
                })
                .unwrap();
        }
        let server = LanServer::bind(
            "127.0.0.1:0".parse().unwrap(),
            LockstepHost::new(session).unwrap(),
            BTreeMap::from([(one, "four".into()), (two, "nine".into())]),
        )
        .unwrap();
        let address = server.local_addr().unwrap();
        let stop = AtomicBool::new(false);
        thread::scope(|scope| {
            scope.spawn(|| server.serve_until(&stop).unwrap());
            struct Stop<'a>(&'a AtomicBool);
            impl Drop for Stop<'_> {
                fn drop(&mut self) {
                    self.0.store(true, Ordering::Release);
                }
            }
            let _stop = Stop(&stop);
            let mut game = LanGame::connect(address, one, "four".into()).unwrap();
            wait(&mut game, LanGame::ready);
            assert_eq!(game.seat(), one);
            assert_eq!(game.observation().unwrap().own_marker(), 4);
            let mut peer = LanClient::connect(address, one, "four".into()).unwrap();
            peer.submit(Command::ResearchTechnology {
                player: one,
                technology: "tech_01".into(),
            })
            .unwrap();
            game.submit(Command::TurnEnd { player: one }).unwrap();
            assert!(game.busy());
            assert!(game.submit(Command::TurnEnd { player: one }).is_err());
            wait(&mut game, |g| g.error().is_some());
            assert!(!game.ready());
            assert!(game.can_sync());
            game.sync().unwrap();
            wait(&mut game, LanGame::ready);
            assert_eq!(game.position(), Some(peer.replica().position()));
            assert_eq!(game.position().unwrap().sequence, 1);
            assert!(game.status().unwrap().contains("sequence 1"));
            game.submit(Command::TurnEnd { player: one }).unwrap();
            wait(&mut game, |g| g.active_seat() == Some(two));
            assert_eq!(game.observation().unwrap().seat(), one);
            assert_eq!(game.observation().unwrap().own_marker(), 4);
            assert!(!game.ready());
            let mut second = LanClient::connect(address, two, "nine".into()).unwrap();
            second.submit(Command::TurnEnd { player: two }).unwrap();
            wait(&mut game, LanGame::ready);
            assert_eq!(game.position(), Some(second.replica().position()));
        });
    }
}
