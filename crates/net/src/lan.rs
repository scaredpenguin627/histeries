//! Authenticated command exchange between trusted LAN peers.
//!
//! Each request uses a fresh TCP connection. A client that loses a submit
//! response can synchronize from its previous position without resubmitting.
//! Seat credentials and the full deterministic world travel in plaintext.

mod wire;

use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    error::Error,
    fmt, io,
    net::{SocketAddr, TcpListener, TcpStream},
    sync::{
        Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::Duration,
};

use histeries_rules_core::PlayerId;
use histeries_sim::{Command, MatchView, VictoryKind, Viewpoint};
use serde::{Deserialize, Serialize};

use crate::{Commit, DesyncReport, LockstepHost, Opening, Position, Replica};
use wire::{Action, FrameStream, Permit, Reply, Request};

// Enough for ordinary interactive polling. Older clients restore a fresh checkpoint.
const HISTORY: usize = 128;
const CONNECTIONS: usize = 8;
const MAX_TOKEN: usize = 256;

/// Protocol errors returned by the LAN host.
#[derive(Debug, Serialize, Deserialize)]
pub enum Refusal {
    /// The requested protocol version is unsupported.
    Version {
        /// Version requested by the client.
        requested: u16,
        /// Version served by the host.
        supported: u16,
    },
    /// No credential matched the claimed seat.
    Credentials,
    /// A join or recovery replaced the submitter's write permit.
    Superseded,
    /// The supplied host or write permit does not identify this joined seat's session.
    RecoveryExpired,
    /// Another command already advanced the host.
    Stale {
        /// Client's submission position.
        submitted: Position,
        /// Current host position.
        current: Position,
    },
    /// The same sequence has different round or hash values.
    Desync {
        /// Position known to the host.
        expected: Position,
        /// Position sent by the replica.
        received: Position,
    },
    /// The submission failed simulation or seat validation.
    Rejected(String),
}

impl fmt::Display for Refusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Version {
                requested,
                supported,
            } => {
                write!(f, "requested protocol {requested}, host uses {supported}")
            }
            Self::Credentials => f.write_str("invalid seat credentials"),
            Self::Superseded => {
                f.write_str("seat write permit replaced; synchronize before submitting")
            }
            Self::RecoveryExpired => {
                f.write_str("the host does not recognize this joined seat session")
            }
            Self::Stale { submitted, current } => {
                write!(f, "stale position ({submitted}), host at {current}")
            }
            Self::Desync { expected, received } => {
                write!(f, "desync: host at {expected}, peer reported {received}")
            }
            Self::Rejected(reason) => f.write_str(reason),
        }
    }
}

/// A transport failure, command refusal or failed independent verification.
#[derive(Debug)]
pub enum LanError {
    /// A previous submission requires explicit synchronization before issuing another order.
    SubmissionPending,
    /// The original session can no longer verify this client's interrupted submission.
    InterruptedSubmissionRequiresRejoin,
    /// Network or framing I/O failed. A lost submit reply can be recovered with sync.
    Io(io::Error),
    /// Invalid configuration or wire data.
    Protocol(String),
    /// The host refused this request.
    Refused(Refusal),
    /// Local replay diverged from an authoritative commit.
    Replica(DesyncReport),
}

impl fmt::Display for LanError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SubmissionPending => {
                f.write_str("synchronize the previous submission before issuing another order")
            }
            Self::InterruptedSubmissionRequiresRejoin => f.write_str(
                "The interrupted order's outcome is unknown. Rejoin the match before issuing another order.",
            ),
            Self::Io(e) => write!(f, "LAN I/O: {e}"),
            Self::Protocol(e) => write!(f, "LAN protocol: {e}"),
            Self::Refused(reason) => write!(f, "LAN refusal: {reason}"),
            Self::Replica(report) => write!(f, "LAN replica: {report}"),
        }
    }
}

impl Error for LanError {}

impl From<io::Error> for LanError {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}

/// A listener serving a fixed set of authenticated seats in one match.
pub struct LanServer {
    listener: TcpListener,
    credentials: BTreeMap<PlayerId, String>,
    game: Mutex<Game>,
}

struct Game {
    host: LockstepHost,
    instance: [u8; 16],
    writers: BTreeMap<PlayerId, u64>,
    history: VecDeque<(Permit, Commit)>,
}

/// The host's retained public outcome and historical observation for the authenticated seat.
#[derive(Serialize, Deserialize)]
pub struct SeatReplay {
    /// The simulation's serialized fog-filtered seat observation.
    pub observation: serde_json::Value,
    /// Public victory at the requested boundary.
    pub winner: Option<(PlayerId, VictoryKind)>,
}

/// What the host could establish about an interrupted proposal after fencing its write permit.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SubmissionOutcome {
    /// The retained commit identifies this proposal.
    Accepted,
    /// The proposal did not commit and its permit has been retired.
    Canceled,
    /// The proposal's receipt expired or its original host session is gone.
    Unknown,
}

impl Game {
    fn issue_permit(&mut self, seat: PlayerId) -> Result<Permit, Refusal> {
        let generation = self.writers.entry(seat).or_default();
        *generation = generation
            .checked_add(1)
            .ok_or_else(|| Refusal::Rejected("seat permit sequence exhausted".into()))?;
        Ok(Permit {
            host: self.instance,
            seat,
            generation: *generation,
        })
    }

    fn submitted(&self, permit: Permit, from: Position) -> Result<SubmissionOutcome, Refusal> {
        let current = self.host.position();
        if from.sequence > current.sequence {
            return Err(Refusal::Stale {
                submitted: from,
                current,
            });
        }
        if from.sequence == current.sequence {
            same_position(current, from)?;
            return Ok(SubmissionOutcome::Canceled);
        }
        let Some((writer, commit)) = self
            .history
            .iter()
            .find(|(_, c)| c.before.sequence == from.sequence)
        else {
            return Ok(SubmissionOutcome::Unknown);
        };
        same_position(commit.before, from)?;
        Ok(if *writer == permit {
            SubmissionOutcome::Accepted
        } else {
            SubmissionOutcome::Canceled
        })
    }

    fn execute(&mut self, seat: PlayerId, action: Action) -> Result<Reply, Refusal> {
        let current = self.host.position();
        match action {
            Action::Join => Ok(Reply::Joined {
                permit: self.issue_permit(seat)?,
                opening: self.host.opening(),
            }),
            Action::Recover { previous, proposal } => {
                if previous.host != self.instance
                    || previous.seat != seat
                    || !self
                        .writers
                        .get(&seat)
                        .is_some_and(|&g| (1..=g).contains(&previous.generation))
                {
                    return Err(Refusal::RecoveryExpired);
                }
                let outcome = proposal
                    .map(|from| self.submitted(previous, from))
                    .transpose()?;
                // Settling the original proposal and fencing every older permit share the match lock.
                Ok(Reply::Recovered {
                    previous,
                    proposal,
                    outcome,
                    through: current,
                    permit: self.issue_permit(seat)?,
                })
            }
            Action::Replay { host, turn } => {
                if host != self.instance {
                    return Err(Refusal::RecoveryExpired);
                }
                let projected = self
                    .host
                    .host()
                    .replay(Viewpoint::Seat(seat), turn)
                    .map_err(Refusal::Rejected)?;
                let MatchView::Seat(view) = projected.view else {
                    return Err(Refusal::Rejected("expected a seat replay".into()));
                };
                Ok(Reply::Replay(SeatReplay {
                    observation: serde_json::to_value(view)
                        .map_err(|e| Refusal::Rejected(e.to_string()))?,
                    winner: projected.winner,
                }))
            }
            Action::Sync { host, from } => {
                if host != self.instance {
                    return Err(Refusal::RecoveryExpired);
                }
                if from.sequence > current.sequence {
                    return Err(Refusal::Stale {
                        submitted: from,
                        current,
                    });
                }
                let commits = if from.sequence == current.sequence {
                    same_position(current, from)?;
                    vec![]
                } else if let Some(index) = self
                    .history
                    .iter()
                    .position(|(_, c)| c.before.sequence == from.sequence)
                {
                    same_position(self.history[index].1.before, from)?;
                    self.history
                        .iter()
                        .skip(index)
                        .map(|(_, c)| c.clone())
                        .collect()
                } else {
                    return Ok(Reply::Opening(self.host.opening()));
                };
                Ok(Reply::Updates {
                    commits,
                    through: current,
                })
            }
            Action::Submit {
                permit,
                from,
                command,
            } => {
                if permit.host != self.instance
                    || permit.seat != seat
                    || self.writers.get(&seat) != Some(&permit.generation)
                {
                    return Err(Refusal::Superseded);
                }
                if from.sequence != current.sequence {
                    return Err(Refusal::Stale {
                        submitted: from,
                        current,
                    });
                }
                same_position(current, from)?;
                let cmd =
                    Command::decode(&command).map_err(|e| Refusal::Rejected(e.to_string()))?;
                let commit = self
                    .host
                    .submit(seat, from, cmd)
                    .map_err(Refusal::Rejected)?;
                if self.history.len() == HISTORY {
                    self.history.pop_front();
                }
                self.history.push_back((permit, commit.clone()));
                Ok(Reply::Accepted(commit))
            }
        }
    }
}

fn same_position(expected: Position, received: Position) -> Result<(), Refusal> {
    if expected == received {
        Ok(())
    } else {
        Err(Refusal::Desync { expected, received })
    }
}

impl LanServer {
    /// Binds the specified TCP address. Every player must have a unique nonempty token.
    pub fn bind(
        address: SocketAddr,
        host: LockstepHost,
        credentials: BTreeMap<PlayerId, String>,
    ) -> Result<Self, LanError> {
        let seats = host.session().seat_ids().collect::<BTreeSet<_>>();
        let unique = credentials.values().collect::<BTreeSet<_>>();
        if !seats.iter().eq(credentials.keys())
            || unique.len() != seats.len()
            || unique
                .iter()
                .any(|token| token.is_empty() || token.len() > MAX_TOKEN)
        {
            return Err(LanError::Protocol(
                "supply one distinct 1–256 byte token for every seat".into(),
            ));
        }
        let listener = TcpListener::bind(address)?;
        listener.set_nonblocking(true)?;
        let mut instance = [0; 16];
        getrandom::fill(&mut instance)
            .map_err(|e| LanError::Protocol(format!("could not identify LAN host: {e}")))?;
        Ok(Self {
            listener,
            credentials,
            game: Mutex::new(Game {
                host,
                instance,
                writers: BTreeMap::new(),
                history: VecDeque::new(),
            }),
        })
    }

    /// Returns the bound address, including a port assigned by the OS.
    pub fn local_addr(&self) -> Result<SocketAddr, LanError> {
        Ok(self.listener.local_addr()?)
    }

    /// Serves the match until interrupted by the process owner.
    pub fn serve(&self) -> Result<(), LanError> {
        self.serve_until(&AtomicBool::new(false))
    }

    /// Serves the match until shutdown is requested, then joins any in-flight workers.
    pub fn serve_until(&self, stop: &AtomicBool) -> Result<(), LanError> {
        thread::scope(|scope| {
            let mut workers = Vec::<thread::ScopedJoinHandle<'_, ()>>::new();
            while !stop.load(Ordering::Acquire) {
                workers.retain(|worker| !worker.is_finished());
                match self.listener.accept() {
                    Ok((socket, _)) if workers.len() < CONNECTIONS => {
                        workers.push(thread::Builder::new().spawn_scoped(scope, move || {
                            // A broken request or disconnected peer affects only its own socket.
                            let _ = self.handle(socket);
                        })?);
                    }
                    Ok(_) => {}
                    Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(5))
                    }
                    Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
                    Err(e) => return Err(e.into()),
                }
            }
            Ok(())
        })
    }

    fn handle(&self, socket: TcpStream) -> Result<(), LanError> {
        let mut stream = FrameStream::new(socket)?;
        let request = stream.read_request()?;
        let reply = if request.version != wire::VERSION {
            Reply::Refused(Refusal::Version {
                requested: request.version,
                supported: wire::VERSION,
            })
        } else if self.credentials.get(&request.seat) != Some(&request.token) {
            Reply::Refused(Refusal::Credentials)
        } else {
            self.game
                .lock()
                .map_err(|_| LanError::Protocol("host unavailable".into()))?
                .execute(request.seat, request.action)
                .unwrap_or_else(Reply::Refused)
        };
        // The simulation mutex is released before any response is written to the network.
        stream.write_reply(reply)
    }
}

/// How the peer reached the latest authoritative position.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SyncOutcome {
    /// Independently replayed the retained commands.
    Replayed {
        /// Number of contiguous command commits applied.
        commands: usize,
    },
    /// The previous cursor aged out. The peer verified the current checkpoint and its round orders.
    Checkpoint {
        /// Last independently verified position before the gap.
        previous: Position,
        /// Restored position after loading the checkpoint.
        current: Position,
    },
}

/// A seat-bound client with its own verified local simulation.
pub struct LanClient {
    address: SocketAddr,
    seat: PlayerId,
    token: String,
    replica: Replica,
    permit: Permit,
    recovery: Recovery,
    recovered_submission: Option<SubmissionOutcome>,
}

#[derive(Clone, Copy)]
enum Recovery {
    Ready,
    RequiresRejoin,
    // Some(position) identifies an interrupted order. None reclaims a superseded permit.
    NeedsFence(Option<Position>),
    // A fence receipt is kept across failed attempts to verify the latest position.
    NeedsSync {
        minimum: Position,
        outcome: Option<SubmissionOutcome>,
    },
}

impl Recovery {
    fn verify_target(self, target: Position) -> Result<(), LanError> {
        if let Self::NeedsSync { minimum, .. } = self {
            if target.sequence < minimum.sequence {
                return Err(LanError::Protocol(
                    "synchronization predates the recovery boundary".into(),
                ));
            }
            if target.sequence == minimum.sequence {
                same_position(minimum, target).map_err(LanError::Refused)?;
            }
        }
        Ok(())
    }
}

impl LanClient {
    /// Authenticates and reconstructs the current world, including orders in progress.
    pub fn connect(address: SocketAddr, seat: PlayerId, token: String) -> Result<Self, LanError> {
        let Reply::Joined { opening, permit } = exchange(address, seat, &token, Action::Join)?
        else {
            return Err(LanError::Protocol("expected match opening".into()));
        };
        if permit.seat != seat || permit.generation == 0 {
            return Err(LanError::Protocol(
                "invalid write permit in match opening".into(),
            ));
        }
        let replica = restore(&opening, seat)?;
        Ok(Self {
            address,
            seat,
            token,
            replica,
            permit,
            recovery: Recovery::Ready,
            recovered_submission: None,
        })
    }

    /// Returns this peer's local simulation.
    pub fn replica(&self) -> &Replica {
        &self.replica
    }

    /// Seat bound to this credential at connection time.
    pub fn seat(&self) -> PlayerId {
        self.seat
    }

    /// The interrupted submission's outcome after verification, or unknown if its session is gone.
    pub fn submission_outcome(&self) -> Option<SubmissionOutcome> {
        self.recovered_submission
    }

    /// Whether submissions are blocked pending synchronization of this joined session.
    pub fn needs_recovery(&self) -> bool {
        matches!(
            self.recovery,
            Recovery::NeedsFence(_) | Recovery::NeedsSync { .. }
        )
    }

    /// Reads this seat's projection at a completed turn retained by the host.
    pub fn replay(&self, turn: u32) -> Result<SeatReplay, LanError> {
        match exchange(
            self.address,
            self.seat,
            &self.token,
            Action::Replay {
                host: self.permit.host,
                turn,
            },
        )? {
            Reply::Replay(replay)
                if replay.observation["seat"] == self.seat.index()
                    && replay.observation["turn"] == turn =>
            {
                Ok(replay)
            }
            _ => Err(LanError::Protocol(
                "unexpected seat or turn in replay response".into(),
            )),
        }
    }

    /// Fetches and verifies other seats' orders, including while waiting for a turn.
    pub fn sync(&mut self) -> Result<SyncOutcome, LanError> {
        if matches!(self.recovery, Recovery::RequiresRejoin) {
            return Err(LanError::InterruptedSubmissionRequiresRejoin);
        }
        self.recovered_submission = None;
        let result = self.synchronize();
        if matches!(
            result,
            Err(LanError::Refused(
                Refusal::RecoveryExpired | Refusal::Credentials | Refusal::Version { .. }
            ))
        ) && matches!(
            self.recovery,
            Recovery::NeedsFence(Some(_))
                | Recovery::NeedsSync {
                    outcome: Some(_),
                    ..
                }
        ) {
            self.recovery = Recovery::RequiresRejoin;
            self.recovered_submission = Some(SubmissionOutcome::Unknown);
            return Err(LanError::InterruptedSubmissionRequiresRejoin);
        }
        result
    }

    fn synchronize(&mut self) -> Result<SyncOutcome, LanError> {
        if let Recovery::NeedsFence(proposal) = self.recovery {
            let previous = self.permit;
            match exchange(
                self.address,
                self.seat,
                &self.token,
                Action::Recover { previous, proposal },
            )? {
                Reply::Recovered {
                    previous: echoed,
                    proposal: resolved,
                    permit,
                    outcome,
                    through,
                } if echoed == previous
                    && resolved == proposal
                    && permit.host == previous.host
                    && permit.seat == self.seat
                    && permit.generation > previous.generation
                    && outcome.is_some() == proposal.is_some() =>
                {
                    if let Some(from) = proposal {
                        if through.sequence < from.sequence
                            || (through.sequence == from.sequence
                                && outcome != Some(SubmissionOutcome::Canceled))
                        {
                            return Err(LanError::Protocol(
                                "invalid proposal boundary in recovery receipt".into(),
                            ));
                        }
                        if through.sequence == from.sequence {
                            same_position(from, through).map_err(LanError::Refused)?;
                        }
                    }
                    self.permit = permit;
                    self.recovery = Recovery::NeedsSync {
                        minimum: through,
                        outcome,
                    };
                }
                _ => {
                    return Err(LanError::Protocol(
                        "unexpected submission recovery receipt".into(),
                    ));
                }
            }
        }
        let from = self.replica.position();
        let outcome = match exchange(
            self.address,
            self.seat,
            &self.token,
            Action::Sync {
                host: self.permit.host,
                from,
            },
        )? {
            Reply::Updates { commits, through } => {
                if matches!(
                    self.recovery,
                    Recovery::NeedsSync {
                        outcome: Some(SubmissionOutcome::Unknown),
                        ..
                    }
                ) {
                    return Err(LanError::Protocol(
                        "the expired submission requires a checkpoint".into(),
                    ));
                }
                self.recovery.verify_target(through)?;
                let commands = commits.len();
                for commit in commits {
                    self.replica.apply(&commit).map_err(LanError::Replica)?;
                }
                same_position(through, self.replica.position()).map_err(LanError::Refused)?;
                Ok(SyncOutcome::Replayed { commands })
            }
            Reply::Opening(opening) if opening.position.sequence > from.sequence => {
                self.recovery.verify_target(opening.position)?;
                self.replica = restore(&opening, self.seat)?;
                Ok(SyncOutcome::Checkpoint {
                    previous: from,
                    current: self.replica.position(),
                })
            }
            _ => Err(LanError::Protocol(
                "unexpected synchronization reply".into(),
            )),
        }?;
        if let Recovery::NeedsSync { outcome, .. } = self.recovery {
            self.recovered_submission = outcome;
        }
        self.recovery = Recovery::Ready;
        Ok(outcome)
    }

    /// Submits exactly once at the replica's current position and independently applies the commit.
    pub fn submit(&mut self, command: Command) -> Result<(), LanError> {
        match self.recovery {
            Recovery::Ready => {}
            Recovery::RequiresRejoin => return Err(LanError::InterruptedSubmissionRequiresRejoin),
            _ => return Err(LanError::SubmissionPending),
        }
        self.recovered_submission = None;
        let encoded = command.encode();
        let from = self.replica.position();
        let action = Action::Submit {
            permit: self.permit,
            from,
            command: encoded.clone(),
        };
        self.recovery = Recovery::NeedsFence(Some(from));
        match exchange(self.address, self.seat, &self.token, action) {
            Ok(Reply::Accepted(commit))
                if commit.seat == self.seat && commit.command == encoded =>
            {
                self.replica.apply(&commit).map_err(LanError::Replica)?;
                self.recovery = Recovery::Ready;
                Ok(())
            }
            Ok(_) => Err(LanError::Protocol(
                "unexpected acknowledgement for submitted command".into(),
            )),
            Err(error) => {
                if let LanError::Refused(reason) = &error {
                    self.recovery = match reason {
                        Refusal::Superseded => Recovery::NeedsFence(None),
                        Refusal::Stale { current, .. } => Recovery::NeedsSync {
                            minimum: *current,
                            outcome: None,
                        },
                        _ => Recovery::Ready,
                    };
                }
                Err(error)
            }
        }
    }
}

fn restore(opening: &Opening, seat: PlayerId) -> Result<Replica, LanError> {
    let replica = Replica::from_opening(opening).map_err(LanError::Protocol)?;
    if !replica.session().seat_ids().any(|id| id == seat) {
        return Err(LanError::Protocol(
            "seat is absent from match opening".into(),
        ));
    }
    Ok(replica)
}

fn exchange(
    address: SocketAddr,
    seat: PlayerId,
    token: &str,
    action: Action,
) -> Result<Reply, LanError> {
    let mut stream = FrameStream::new(TcpStream::connect_timeout(&address, wire::TIMEOUT)?)?;
    stream.write_request(&Request {
        version: wire::VERSION,
        seat,
        token: token.into(),
        action,
    })?;
    match stream.read_reply()? {
        Reply::Refused(reason) => Err(LanError::Refused(reason)),
        reply => Ok(reply),
    }
}

#[cfg(test)]
mod tests;
