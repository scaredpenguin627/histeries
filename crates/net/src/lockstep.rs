use std::fmt;

use histeries_rules_core::PlayerId;
use histeries_sim::{Command, MatchHost, Session};
use serde::{Deserialize, Serialize};

/// Wire version for opening snapshots and authoritative command records.
pub const PROTOCOL_VERSION: u16 = 1;

/// The number of accepted commands, the simulation round, and the canonical state hash.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Position {
    /// Number of authoritative commands since the match was hosted.
    pub sequence: u64,
    /// Number of completed simulation rounds.
    pub turn: u32,
    /// Numeric value of the simulation's StateHash.
    pub hash: u64,
}

impl Position {
    fn at(sequence: u64, session: &Session) -> Self {
        Self {
            sequence,
            turn: session.turn(),
            hash: session.state_hash().value(),
        }
    }
}

impl fmt::Display for Position {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "sequence {} turn {} hash {:016x}",
            self.sequence, self.turn, self.hash
        )
    }
}

/// Starts a trusted replica, including one joining partway through a round.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Opening {
    /// Lockstep wire version.
    pub version: u16,
    /// Authoritative position after replaying the supplied orders.
    pub position: Position,
    /// Versioned simulation save at the beginning of this round's replay segment.
    pub save: Vec<u8>,
    /// Canonical command documents from that save, preserving per-seat order budgets.
    pub orders: Vec<Vec<u8>>,
}

/// A single accepted player order and the two positions it connects.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Commit {
    /// Lockstep wire version.
    pub version: u16,
    /// Position the proposal was based on.
    pub before: Position,
    /// Verified position after one registry dispatch.
    pub after: Position,
    /// Seat that was authenticated to submit this order.
    pub seat: PlayerId,
    /// Canonical simulation command bytes.
    pub command: Vec<u8>,
}

/// Evidence for a rejected record. The replica retains its last verified position.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DesyncReport {
    /// The authority's position immediately before the disputed command.
    pub before: Position,
    /// Seat attributed to the disputed record.
    pub seat: PlayerId,
    /// Authority's claim at the point validation failed.
    pub expected: Position,
    /// Replica's corresponding position, including its tentative post-command hash if available.
    pub actual: Position,
    /// Difference encountered while checking or replaying the record.
    pub reason: String,
}

impl fmt::Display for DesyncReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "desync ({}) for seat {} from {}: expected {}, actual {}",
            self.reason,
            self.seat.index(),
            self.before,
            self.expected,
            self.actual
        )
    }
}

impl std::error::Error for DesyncReport {}

/// Orders regular player commands for a shared keyboard or a transport.
#[derive(Clone)]
pub struct LockstepHost {
    host: MatchHost,
    position: Position,
}

impl LockstepHost {
    /// Starts fresh protocol sequencing at a supplied simulation position. Hosting resumes
    /// with the standalone save rules even when the source session was playing an old replay.
    pub fn new(session: Session) -> Result<Self, String> {
        Self::from_save(&session.save_bytes())
    }

    fn from_save(save: &[u8]) -> Result<Self, String> {
        let session = Session::from_save_bytes(save).map_err(|e| e.to_string())?;
        if !(2..=8).contains(&session.seat_ids().count()) {
            return Err("lockstep requires 2 to 8 seats in the session".into());
        }
        Ok(Self {
            position: Position::at(0, &session),
            host: MatchHost::new(session),
        })
    }

    /// Shared interactive host for authorized observations and retained replay views.
    pub fn host(&self) -> &MatchHost {
        &self.host
    }

    /// The hosted session, for read-only observation and local saves.
    pub fn session(&self) -> &Session {
        self.host.session()
    }

    /// Latest accepted command boundary.
    pub fn position(&self) -> Position {
        self.position
    }

    /// The lowest actual seat ID that has yet to end this round, until victory.
    pub fn active_seat(&self) -> Option<PlayerId> {
        let session = self.session();
        if session.winner().is_some() {
            return None;
        }
        session
            .seat_ids()
            .find(|&seat| !session.has_ended_turn(seat))
    }

    /// Creates a snapshot and the bounded current-round command suffix.
    pub fn opening(&self) -> Opening {
        let segment = self.session().replay();
        Opening {
            version: PROTOCOL_VERSION,
            position: self.position,
            save: segment.playback_session().save_bytes(),
            orders: segment.commands().iter().map(Command::encode).collect(),
        }
    }

    /// Accepts one proposal against its exact position. Retrying a stale proposal cannot
    /// apply the order twice, including orders that leave the simulation hash unchanged.
    pub fn submit(
        &mut self,
        seat: PlayerId,
        expected: Position,
        command: Command,
    ) -> Result<Commit, String> {
        if expected != self.position {
            return Err(format!(
                "stale proposal: expected {expected}, host at {}",
                self.position
            ));
        }
        let bytes = command.encode();
        self.advance(seat, command)?;
        Ok(Commit {
            version: PROTOCOL_VERSION,
            before: expected,
            after: self.position,
            seat,
            command: bytes,
        })
    }

    fn advance(&mut self, seat: PlayerId, command: Command) -> Result<(), String> {
        if self.active_seat() != Some(seat) {
            return Err("this seat is not active".into());
        }
        let next = self
            .position
            .sequence
            .checked_add(1)
            .ok_or("command sequence exhausted")?;
        self.host.dispatch(seat, command)?;
        self.position = Position::at(next, self.session());
        Ok(())
    }
}

/// An independent simulation that accepts only a contiguous, verified record stream.
pub struct Replica {
    local: LockstepHost,
}

impl Replica {
    /// Validates the snapshot and replays the remainder of the current round.
    pub fn from_opening(opening: &Opening) -> Result<Self, String> {
        if opening.version != PROTOCOL_VERSION {
            return Err(format!("unsupported lockstep version {}", opening.version));
        }
        let mut local = LockstepHost::from_save(&opening.save)?;
        if local.position.turn != opening.position.turn {
            return Err("opening save and position refer to different rounds".into());
        }
        local.position.sequence = opening
            .position
            .sequence
            .checked_sub(opening.orders.len() as u64)
            .ok_or("opening has more orders than its sequence")?;
        for document in &opening.orders {
            let command = Command::decode(document).map_err(|e| e.to_string())?;
            let seat = command.issuer().ok_or("opening order has no player")?;
            local.advance(seat, command)?;
            if local.position.turn != opening.position.turn {
                return Err("opening orders cross a round boundary".into());
            }
        }
        if local.position != opening.position {
            return Err(format!(
                "opening desync: expected {}, actual {}",
                opening.position, local.position
            ));
        }
        Ok(Self { local })
    }

    /// Simulation at the last verified boundary.
    pub fn session(&self) -> &Session {
        self.local.session()
    }

    /// Position of the last verified record.
    pub fn position(&self) -> Position {
        self.local.position
    }

    /// Seat permitted to act at the last verified position.
    pub fn active_seat(&self) -> Option<PlayerId> {
        self.local.active_seat()
    }

    /// Independently checks and dispatches a record. Rejection leaves every local field intact.
    pub fn apply(&mut self, record: &Commit) -> Result<(), DesyncReport> {
        let before = self.position();
        let report = |reason, expected, actual| DesyncReport {
            before: record.before,
            seat: record.seat,
            reason,
            expected,
            actual,
        };
        if record.version != PROTOCOL_VERSION {
            return Err(report(
                format!("unsupported lockstep version {}", record.version),
                record.before,
                before,
            ));
        }
        if record.before != before {
            return Err(report(
                "prior position differs".into(),
                record.before,
                before,
            ));
        }
        let command = Command::decode(&record.command)
            .map_err(|error| report(error.to_string(), record.before, before))?;
        let mut candidate = self.local.clone();
        candidate
            .advance(record.seat, command)
            .map_err(|reason| report(reason, record.before, before))?;
        if record.after != candidate.position {
            return Err(report(
                "result position differs".into(),
                record.after,
                candidate.position,
            ));
        }
        // Replicas never serve historical replay. Drop the completed round before staging
        // the next one so validation only ever copies the active segment and its order counts.
        if before.turn != candidate.position.turn {
            candidate.host = MatchHost::new(candidate.session().clone());
        }
        self.local = candidate;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn host(ids: &[u32]) -> LockstepHost {
        LockstepHost::new(Session::new(ids.iter().copied().map(PlayerId::new))).unwrap()
    }

    fn end(player: PlayerId) -> Command {
        Command::TurnEnd { player }
    }

    fn research(player: PlayerId) -> Command {
        Command::ResearchTechnology {
            player,
            technology: "tech_01".into(),
        }
    }

    #[test]
    fn hosting_a_historical_playback_resumes_with_the_replicas_live_rules() {
        use histeries_sim::ReplayLog;

        let save = include_bytes!("../../sim/tests/data/state-v4.hst");
        let mut archive = b"HRP1".to_vec();
        archive.extend_from_slice(&4_u16.to_le_bytes());
        archive.extend_from_slice(&(save.len() as u32).to_le_bytes());
        archive.extend_from_slice(save);
        archive.extend_from_slice(&0_u32.to_le_bytes());
        let playback = ReplayLog::decode(&archive).unwrap().playback_session();
        assert_eq!(playback.replay().encode(), archive);
        let authority = LockstepHost::new(playback).unwrap();
        let replica = Replica::from_opening(&authority.opening()).unwrap();
        assert_eq!(authority.session().replay(), replica.session().replay());
        assert_eq!(authority.position(), replica.position());
    }

    #[test]
    fn every_supported_seat_count_replays_in_canonical_order() {
        for count in 2..=8 {
            let ids: Vec<_> = (1..=count).map(|i| i * 13).rev().collect();
            let mut authority = host(&ids);
            let mut replica = Replica::from_opening(&authority.opening()).unwrap();
            for round in 0..3 {
                for &id in ids.iter().rev() {
                    let seat = PlayerId::new(id);
                    assert_eq!(authority.active_seat(), Some(seat));
                    let record = authority
                        .submit(seat, authority.position(), end(seat))
                        .unwrap();
                    assert_eq!(record.before.turn, round);
                    replica.apply(&record).unwrap();
                    assert_eq!(replica.position(), authority.position());
                    assert_eq!(replica.active_seat(), authority.active_seat());
                    assert_eq!(
                        replica.session().save_bytes(),
                        authority.session().save_bytes()
                    );
                }
                assert!(authority.opening().orders.is_empty());
            }
        }
        for count in [0, 1, 9] {
            assert!(LockstepHost::new(Session::new((0..count).map(PlayerId::new))).is_err());
        }
    }

    #[test]
    fn partial_save_handoff_rejects_wrong_seat_editor_and_duplicate_proposals() {
        let (one, two, three) = (PlayerId::new(3), PlayerId::new(8), PlayerId::new(13));
        let mut session = Session::new([three, one, two]);
        session.dispatch(end(one)).unwrap();
        let mut host =
            LockstepHost::new(Session::from_save_bytes(&session.save_bytes()).unwrap()).unwrap();
        assert_eq!(host.active_seat(), Some(two));
        let start = host.position();
        let editor = Command::EditorSetMarker {
            player: two,
            value: 42,
        };
        for (seat, command) in [
            (one, end(one)),
            (two, end(three)),
            (two, editor),
            (three, end(three)),
        ] {
            assert!(host.submit(seat, start, command).is_err());
            assert_eq!(host.position(), start);
            assert_eq!(host.session().save_bytes(), session.save_bytes());
        }
        host.submit(two, start, end(two)).unwrap();
        assert_eq!(host.active_seat(), Some(three));
        assert!(host.submit(two, start, end(two)).is_err());
        let join = Replica::from_opening(&host.opening()).unwrap();
        assert_eq!(join.active_seat(), Some(three));
        assert_eq!(join.position(), host.position());
    }

    #[test]
    fn corrupt_reordered_and_duplicate_records_leave_the_replica_at_its_verified_position() {
        let mut host = host(&[1, 2]);
        let one = PlayerId::new(1);
        let two = PlayerId::new(2);
        let mut replica = Replica::from_opening(&host.opening()).unwrap();
        let first = host.submit(one, host.position(), end(one)).unwrap();
        let second = host.submit(two, host.position(), end(two)).unwrap();
        let mut wrong_result = first.clone();
        wrong_result.after.hash ^= 1;
        let fault = replica.apply(&wrong_result).unwrap_err();
        assert_eq!(fault.expected, wrong_result.after);
        assert_eq!(fault.actual, first.after);
        assert_eq!(fault.before, first.before);
        let mut wrong_sequence = first.clone();
        wrong_sequence.after.sequence += 1;
        let mut wrong_round = first.clone();
        wrong_round.before.turn += 1;
        let mut wrong_version = first.clone();
        wrong_version.version += 1;
        let mut malformed = first.clone();
        malformed.command.push(0);
        let mut forged = first.clone();
        forged.command = Command::EditorSetMarker {
            player: one,
            value: 9,
        }
        .encode();
        let start = replica.session().save_bytes();
        for invalid in [
            &second,
            &wrong_sequence,
            &wrong_round,
            &wrong_version,
            &malformed,
            &forged,
        ] {
            assert!(replica.apply(invalid).is_err());
            assert_eq!(replica.position(), first.before);
            assert_eq!(replica.session().save_bytes(), start);
        }
        replica.apply(&first).unwrap();
        assert!(replica.apply(&first).is_err());
        replica.apply(&second).unwrap();
        assert_eq!(replica.session().save_bytes(), host.session().save_bytes());
    }

    #[test]
    fn mid_round_opening_preserves_order_budget_and_retries_of_unchanged_hashes() {
        let mut host = host(&[4, 8]);
        let seat = PlayerId::new(4);
        for _ in 0..511 {
            host.submit(seat, host.position(), research(seat)).unwrap();
        }
        let mut replica = Replica::from_opening(&host.opening()).unwrap();
        let last = host.submit(seat, host.position(), research(seat)).unwrap();
        assert_eq!(last.before.hash, last.after.hash);
        assert!(host.submit(seat, last.before, research(seat)).is_err());
        replica.apply(&last).unwrap();
        assert!(host.submit(seat, host.position(), research(seat)).is_err());
        let mut extra = last;
        extra.before = host.position();
        extra.after.sequence += 1;
        assert!(
            replica
                .apply(&extra)
                .unwrap_err()
                .reason
                .contains("order limit")
        );
        replica
            .apply(&host.submit(seat, host.position(), end(seat)).unwrap())
            .unwrap();
        assert_eq!(replica.position(), host.position());
    }

    #[test]
    fn conquest_replicates_the_winner_without_an_extra_turn() {
        use histeries_sim::{ScenarioBrush, ScenarioDraft, UnitKind, VictoryKind};

        let (one, two) = (PlayerId::new(1), PlayerId::new(2));
        let mut draft = ScenarioDraft::blank(6, 1, 2).unwrap();
        for (seat, index, brush) in [
            (one, 0, ScenarioBrush::City),
            (two, 3, ScenarioBrush::City),
            (one, 2, ScenarioBrush::Unit(UnitKind::Warrior)),
        ] {
            let tile = draft.board().visible_tiles()[index].tile;
            draft.apply(brush, tile, seat).unwrap();
        }
        let session = Session::from_save_bytes(draft.bytes()).unwrap();
        let capture = Command::UnitCaptureCity {
            player: one,
            unit: session.observe(one).unwrap().units()[0].id,
            city: session.observe(two).unwrap().cities()[0].id,
        };
        let mut authority = LockstepHost::new(session).unwrap();
        let mut replica = Replica::from_opening(&authority.opening()).unwrap();
        let commit = authority
            .submit(one, authority.position(), capture)
            .unwrap();
        replica.apply(&commit).unwrap();
        assert_eq!(
            replica.session().winner(),
            Some((one, VictoryKind::Domination))
        );
        assert_eq!(
            replica.session().save_bytes(),
            authority.session().save_bytes()
        );
        assert_eq!(replica.active_seat(), None);
        assert!(
            authority
                .submit(one, authority.position(), end(one))
                .is_err()
        );
        assert_eq!(
            Replica::from_opening(&authority.opening())
                .unwrap()
                .position(),
            replica.position()
        );
    }

    #[test]
    fn opening_rejects_corruption_and_inconsistent_metadata() {
        let mut host = host(&[1, 2]);
        let one = PlayerId::new(1);
        host.submit(one, host.position(), end(one)).unwrap();
        let opening = host.opening();
        let mut corrupt = opening.clone();
        corrupt.save.truncate(5);
        assert!(Replica::from_opening(&corrupt).is_err());
        let mut corrupt = opening.clone();
        corrupt.position.hash ^= 1;
        assert!(Replica::from_opening(&corrupt).is_err());
        let mut corrupt = opening.clone();
        corrupt.position.sequence = 0;
        assert!(Replica::from_opening(&corrupt).is_err());
        let mut corrupt = opening.clone();
        corrupt.version += 1;
        assert!(Replica::from_opening(&corrupt).is_err());
        let mut corrupt = opening;
        corrupt.orders[0].clear();
        assert!(Replica::from_opening(&corrupt).is_err());
    }
}
