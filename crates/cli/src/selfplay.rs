//! Multi-seat games and independent worker, checkpoint and replay comparisons.

mod journal;

use std::{
    cmp::Reverse,
    collections::BTreeSet,
    path::Path,
    time::{Duration, Instant},
};

use histeries_ai::{Player, ThreeLayerPlayer, VictoryObjective, plan_seats};
use histeries_hex::{FlatHex, TileId, TileTopology};
use histeries_rules_core::PlayerId;
use histeries_sim::{
    Command, Observation, ReplayLog, Session, SimulationState, VictoryKind, generate_map,
};

use serde::{Deserialize, Serialize};

use crate::{flag_string, flag_u32, flag_u64, seed_bytes, tournament, validate_flags};

fn scenario(seed: u64, players: u32, size: &str) -> Result<Session, String> {
    let side = match size {
        "small" => 8,
        "large" => 16,
        "huge" => 32,
        _ => return Err("--size must be small, large or huge".into()),
    };
    if !((2..=8).contains(&players) || (players == 16 && size == "huge")) {
        return Err("expected 2 to 8 players, or 16 on a huge map".into());
    }
    let topology = FlatHex::new(side, side, false, false).map_err(|e| e.to_string())?;
    let mut session = Session::with_map(
        (1..=players).map(PlayerId::new),
        generate_map(topology, seed_bytes(seed)),
    );
    // Maximize minimum separation and resolve ties by ascending tile ID.
    let mut capitals = vec![TileId::new(0)];
    while capitals.len() < players as usize {
        let next = (0..side * side)
            .map(TileId::new)
            .max_by_key(|&tile| {
                (
                    capitals
                        .iter()
                        .map(|&other| topology.distance(tile, other))
                        .min(),
                    Reverse(tile),
                )
            })
            .ok_or("empty map")?;
        capitals.push(next);
    }
    let mut used: BTreeSet<_> = capitals.iter().copied().collect();
    for (i, &capital) in capitals.iter().enumerate() {
        let garrison = topology
            .neighbors(capital)
            .into_iter()
            .find(|tile| !used.contains(tile))
            .ok_or("no free starting garrison tile")?;
        used.insert(garrison);
        tournament::opening(&mut session, PlayerId::new(i as u32 + 1), capital, garrison)?;
    }
    Ok(session)
}

pub(super) struct Game {
    pub(super) session: Session,
    seats: Vec<(PlayerId, ThreeLayerPlayer)>,
    contact_orders: u64,
}

impl Game {
    pub(super) fn new(seed: u64, players: u32, size: &str) -> Result<Self, String> {
        let session = scenario(seed, players, size)?;
        let seats = (1..=players)
            .map(|n| (PlayerId::new(n), ThreeLayerPlayer::default()))
            .collect();
        Ok(Self {
            session,
            seats,
            contact_orders: 0,
        })
    }

    pub(super) fn objectives(&self) -> Vec<u8> {
        self.seats
            .iter()
            .map(
                |(_, controller)| match controller.incumbent().map(|(_, o)| o) {
                    None => 0,
                    Some(VictoryObjective::Science) => 1,
                    Some(VictoryObjective::Domination) => 2,
                    Some(VictoryObjective::Adaptive) => 3,
                },
            )
            .collect()
    }

    pub(super) fn from_session(session: Session, objectives: &[u8]) -> Result<Self, String> {
        let state = session.state_snapshot();
        if state.players().count() != objectives.len() {
            return Err("checkpoint controller count differs from seats".into());
        }
        let mut seats = Vec::new();
        for (seat, objective) in state.players().zip(objectives) {
            let controller = match objective {
                0 => ThreeLayerPlayer::default(),
                1 => ThreeLayerPlayer::resume(seat, VictoryObjective::Science),
                2 => ThreeLayerPlayer::resume(seat, VictoryObjective::Domination),
                3 => ThreeLayerPlayer::resume(seat, VictoryObjective::Adaptive),
                _ => return Err("invalid checkpoint controller objective".into()),
            };
            seats.push((seat, controller));
        }
        Ok(Self {
            session,
            seats,
            contact_orders: 0,
        })
    }

    pub(super) fn round(&mut self, workers: usize, stress: bool) -> Result<Duration, String> {
        if self.session.winner().is_some() {
            return Err("attempted to plan a terminal game".into());
        }
        let before = self.session.state_snapshot();
        let mut inputs = Vec::with_capacity(self.seats.len());
        for (seat, controller) in &mut self.seats {
            let observation = self.session.observe(*seat).map_err(|e| e.to_string())?;
            inputs.push((controller as &mut (dyn Player + Send), observation));
        }
        let start = Instant::now();
        let plans = plan_seats(&mut inputs, workers);
        let elapsed = start.elapsed();
        let topology = before.map().ok_or("missing self-play map")?.topology();
        for ((_, obs), plan) in inputs.iter().zip(&plans) {
            self.contact_orders += audit_contact_orders(&topology, obs, &plan.commands)?;
        }
        for ((seat, _), mut plan) in self.seats.iter().zip(plans) {
            if stress {
                // The base tree's terminal leaf has no dependants. Keeping it
                // open prevents science victory while all seats continue to play.
                plan.commands.retain(|c| !matches!(c, Command::ResearchTechnology {technology, ..} if technology == "tech_25"));
            }
            tournament::dispatch_plan(&mut self.session, &before, *seat, &plan.commands)?;
            if self.session.winner().is_some() {
                if stress {
                    return Err("endurance fixture terminated early".into());
                }
                break;
            }
        }
        if self.session.winner().is_none() && self.session.turn() != before.turn() + 1 {
            return Err(format!("incomplete nonterminal round {}", before.turn()));
        }
        self.session.drain_events();
        Ok(elapsed)
    }
}

// One check per observed military unit and round. Contact includes a visible
// enemy within attack range and an enemy military unit that can reach back.
fn audit_contact_orders(
    topology: &impl TileTopology,
    obs: &Observation,
    commands: &[Command],
) -> Result<u64, String> {
    let ordered: BTreeSet<_> = commands
        .iter()
        .filter_map(|command| match command {
            Command::UnitMove { player, unit, .. }
            | Command::UnitAttack {
                player,
                attacker: unit,
                ..
            }
            | Command::UnitHeal { player, unit }
            | Command::UnitHold { player, unit }
            | Command::UnitPromote { player, unit, .. }
                if *player == obs.seat() =>
            {
                Some(*unit)
            }
            _ => None,
        })
        .collect();
    let mut contacts = 0;
    for unit in obs.units().iter().filter(|unit| unit.kind.is_military()) {
        let in_contact = obs.visible_units().iter().any(|other| {
            if other.owner == Some(obs.seat()) {
                return false;
            }
            let distance = topology.distance(unit.tile, other.tile);
            distance <= unit.kind.range()
                || (other.kind.is_military() && distance <= other.kind.range())
        });
        if in_contact {
            if !ordered.contains(&unit.id) {
                return Err(format!(
                    "turn={} seat={} military unit {} in contact lacks an explicit order",
                    obs.turn(),
                    obs.seat().index(),
                    unit.id.index()
                ));
            }
            contacts += 1;
        }
    }
    Ok(contacts)
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct Fixture {
    seed: u64,
    players: u32,
    size: String,
    workers: usize,
    limit: u32,
    stress: bool,
}

#[derive(Clone, Serialize, Deserialize)]
struct Boundary {
    hash: u64,
    commands_end: usize,
    planning_us: u64,
}

#[derive(Clone, Serialize, Deserialize)]
struct Segment {
    rounds: Vec<Boundary>,
    contact_orders: u64,
    save_bytes: usize,
    controller_checkpoint: Option<u32>,
}

struct Workers {
    primary: Game,
    comparison: Game,
}

impl Workers {
    fn checked_segment(
        &mut self,
        fixture: &Fixture,
        start: u32,
        end: u32,
    ) -> Result<(Segment, Vec<u8>), String> {
        let game = &mut self.primary;
        let parallel = &mut self.comparison;
        let mut checkpoint = game.session.state_snapshot().encode();
        let mut checkpoint_hash = game.session.state_hash().value();
        let mut objectives = game.objectives();
        let mut segment = Segment {
            rounds: Vec::new(),
            contact_orders: 0,
            save_bytes: checkpoint.len(),
            controller_checkpoint: None,
        };
        let initial_contacts = game.contact_orders;
        let other_workers = if fixture.workers == 1 { 8 } else { 1 };
        for round in start + 1..=end {
            let first = game.round(fixture.workers, fixture.stress)?;
            let second = parallel.round(other_workers, fixture.stress)?;
            let hash = game.session.state_hash().value();
            if hash != parallel.session.state_hash().value() {
                return Err(format!(
                    "worker divergence after round {round}: {} vs {other_workers}",
                    fixture.workers
                ));
            }
            let saved = game.session.state_snapshot().encode();
            segment.save_bytes = segment.save_bytes.max(saved.len());
            let decoded = SimulationState::decode(&saved)
                .map_err(|e| format!("decode after round {round}: {e}"))?;
            if decoded.state_hash().value() != hash {
                return Err(format!("save/load divergence after round {round}"));
            }
            segment.rounds.push(Boundary {
                hash,
                commands_end: game.session.replay().commands().len(),
                planning_us: first
                    .max(second)
                    .as_micros()
                    .try_into()
                    .map_err(|_| "planning sample overflow")?,
            });
            if game.session.winner().is_some() {
                break;
            }
            if round.is_power_of_two() && round < fixture.limit {
                checkpoint = saved;
                checkpoint_hash = hash;
                objectives = game.objectives();
                segment.controller_checkpoint = Some(round);
            }
        }
        segment.contact_orders = game.contact_orders - initial_contacts;
        let mut resumed = restore(&checkpoint, &objectives, checkpoint_hash)?;
        let skip = (segment.controller_checkpoint.unwrap_or(start) - start) as usize;
        for (offset, boundary) in segment.rounds.iter().enumerate().skip(skip) {
            resumed.round(fixture.workers, fixture.stress)?;
            if resumed.session.state_hash().value() != boundary.hash {
                return Err(format!(
                    "resumed controller divergence after round {}",
                    start as usize + offset + 1
                ));
            }
        }
        if resumed.objectives() != game.objectives() || parallel.objectives() != game.objectives() {
            return Err(format!(
                "controller objectives diverged after round {}",
                start as usize + segment.rounds.len()
            ));
        }
        let replay = game.session.take_replay_segment().encode();
        drop(parallel.session.take_replay_segment());
        verify_replay(&replay, &segment.rounds, start)?;
        Ok((segment, replay))
    }
}

fn restore(bytes: &[u8], objectives: &[u8], expected: u64) -> Result<Game, String> {
    let state = SimulationState::decode(bytes).map_err(|e| format!("decode checkpoint: {e}"))?;
    if state.state_hash().value() != expected {
        return Err("checkpoint state hash mismatch".into());
    }
    Game::from_session(Session::from_state(state), objectives)
}

fn verify_replay(bytes: &[u8], rounds: &[Boundary], start: u32) -> Result<(), String> {
    let replay = ReplayLog::decode(bytes).map_err(|e| e.to_string())?;
    let mut replayed = Session::from_state(replay.initial_state().clone());
    let mut begin = 0;
    for (offset, boundary) in rounds.iter().enumerate() {
        let end = boundary.commands_end;
        if begin >= end {
            return Err("empty checkpoint round".into());
        }
        for command in replay
            .commands()
            .get(begin..end)
            .ok_or("short decoded replay")?
        {
            replayed
                .dispatch(command.clone())
                .map_err(|e| e.to_string())?;
        }
        if replayed.state_hash().value() != boundary.hash {
            return Err(format!(
                "replay divergence after round {}",
                start as usize + offset + 1
            ));
        }
        begin = end;
    }
    if begin != replay.commands().len() {
        return Err("unconsumed replay commands".into());
    }
    Ok(())
}

#[derive(Default)]
struct Measurements {
    hashes: Vec<u64>,
    timings: Vec<u64>,
    checkpoint_round: usize,
    contact_orders: u64,
    save_bytes: usize,
}

impl Measurements {
    fn append(&mut self, segment: Segment) {
        for boundary in segment.rounds {
            self.hashes.push(boundary.hash);
            self.timings.push(boundary.planning_us);
        }
        if let Some(round) = segment.controller_checkpoint {
            self.checkpoint_round = round as usize;
        }
        self.contact_orders += segment.contact_orders;
        self.save_bytes = self.save_bytes.max(segment.save_bytes);
    }
}

struct Report {
    hashes: Vec<u64>,
    winner: Option<(PlayerId, VictoryKind)>,
    checkpoint_round: usize,
    contact_orders: u64,
    p95_us: u64,
    max_us: u64,
    save_bytes: usize,
    resumed_from_round: u32,
}

fn checked_game(fixture: &Fixture, directory: Option<&Path>, stop: u32) -> Result<Report, String> {
    let mut workers = Workers {
        primary: Game::new(fixture.seed, fixture.players, &fixture.size)?,
        comparison: Game::new(fixture.seed, fixture.players, &fixture.size)?,
    };
    let mut measurements = Measurements::default();
    let journal = directory
        .map(|path| journal::Journal::open(path, fixture))
        .transpose()?;
    if let Some(journal) = &journal {
        journal.load(fixture, &mut workers, &mut measurements)?;
    }
    let resumed_from_round = measurements.hashes.len() as u32;
    let mut start = resumed_from_round;
    while start < stop {
        let end = if journal.is_some() {
            stop.min(start.saturating_add(100))
        } else {
            stop
        };
        let (segment, replay) = workers.checked_segment(fixture, start, end)?;
        if let Some(journal) = &journal {
            journal.publish(fixture, start, &segment, &workers, &replay)?;
        }
        measurements.append(segment);
        start = measurements.hashes.len() as u32;
        if workers.primary.session.winner().is_some() {
            break;
        }
    }
    measurements.timings.sort_unstable();
    let count = measurements.timings.len();
    Ok(Report {
        hashes: measurements.hashes,
        winner: workers.primary.session.winner(),
        checkpoint_round: measurements.checkpoint_round,
        contact_orders: measurements.contact_orders,
        p95_us: measurements.timings[count - count / 20 - 1],
        max_us: measurements.timings[count - 1],
        save_bytes: measurements.save_bytes,
        resumed_from_round,
    })
}

pub(super) fn run(args: &[String]) -> Result<(), String> {
    validate_flags(
        args,
        &[
            "--seed",
            "--players",
            "--size",
            "--turns",
            "--workers",
            "--games",
            "--mode",
            "--checkpoint-dir",
            "--stop-after-round",
        ],
        0,
    )?;
    let mut seen = BTreeSet::new();
    if let Some(flag) = args
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| &pair[0])
        .find(|flag| !seen.insert(*flag))
    {
        return Err(format!("duplicate flag {flag}"));
    }
    let first = flag_u64(args, "--seed")?.unwrap_or(0);
    let games = flag_u32(args, "--games")?.unwrap_or(1);
    let workers = flag_u32(args, "--workers")?.unwrap_or(1) as usize;
    let players = flag_u32(args, "--players")?.unwrap_or(2);
    let limit = flag_u32(args, "--turns")?.unwrap_or(500);
    let size = flag_string(args, "--size")?.unwrap_or_else(|| "small".into());
    let mode = flag_string(args, "--mode")?.unwrap_or_else(|| "game".into());
    let stress = match mode.as_str() {
        "game" => false,
        "stress" => true,
        _ => return Err("--mode must be game or stress".into()),
    };
    if games == 0 || workers == 0 || limit == 0 {
        return Err("--games, --workers and --turns must be positive".into());
    }
    let directory = flag_string(args, "--checkpoint-dir")?;
    let stop_after = flag_u32(args, "--stop-after-round")?;
    if directory.is_some() && !stress {
        return Err("--checkpoint-dir requires --mode stress".into());
    }
    if stop_after.is_some() && directory.is_none() {
        return Err("--stop-after-round requires --checkpoint-dir".into());
    }
    let stop = stop_after.unwrap_or(limit);
    if !(1..=limit).contains(&stop) {
        return Err("--stop-after-round must be within the requested turn limit".into());
    }
    let last = first
        .checked_add(u64::from(games - 1))
        .ok_or("seed range overflows")?;
    for seed in first..=last {
        let fixture = Fixture {
            seed,
            players,
            size: size.clone(),
            workers,
            limit,
            stress,
        };
        let r = checked_game(&fixture, directory.as_deref().map(Path::new), stop)
            .map_err(|e| format!("selfplay seed={seed} players={players}: {e}"))?;
        let resumed = r.resumed_from_round;
        let completed = r.hashes.len();
        if stress && completed < limit as usize {
            println!(
                r#"{{"seed":{seed},"turns_completed":{completed},"original_turns":{limit},"resumed_from_round":{resumed},"complete":false}}"#
            );
            continue;
        }
        let winner = r
            .winner
            .map_or_else(|| "null".into(), |(p, _)| p.index().to_string());
        let victory = r
            .winner
            .map_or_else(|| "null".into(), |(_, kind)| format!("\"{kind:?}\""));
        let checks = r.hashes.len();
        let contacts = r.contact_orders;
        println!(
            "{{\"seed\":{seed},\"players\":{players},\"mode\":\"{mode}\",\"complete\":true,\"resumed_from_round\":{resumed},\"turns_checked\":{},\"terminal\":{},\"winner\":{winner},\"victory\":{victory},\"contact_units\":{contacts},\"contact_orders\":{contacts},\"worker_comparisons\":{checks},\"replay_comparisons\":{checks},\"save_comparisons\":{checks},\"checkpoint_round\":{},\"resume_comparisons\":{},\"planning_p95_us\":{},\"planning_max_us\":{},\"save_bytes\":{},\"final_hash\":{},\"round_hashes\":{:?}}}",
            r.hashes.len(),
            r.winner.is_some(),
            r.checkpoint_round,
            checks - r.checkpoint_round,
            r.p95_us,
            r.max_us,
            r.save_bytes,
            r.hashes[r.hashes.len() - 1],
            r.hashes
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_game(
        seed: u64,
        players: u32,
        size: &str,
        workers: usize,
        limit: u32,
        stress: bool,
    ) -> Result<Report, String> {
        checked_game(
            &Fixture {
                seed,
                players,
                size: size.into(),
                workers,
                limit,
                stress,
            },
            None,
            limit,
        )
    }

    #[test]
    fn eight_seats_restore_and_replay_through_real_rounds() {
        let r = run_game(72, 8, "small", 1, 4, false).unwrap();
        assert_eq!(r.hashes.len(), 4);
        assert_ne!(r.hashes[0], r.hashes[3]);
        assert!(r.winner.is_none());
    }

    #[test]
    fn contact_includes_an_enemy_archers_range_and_requires_an_owner_order() {
        use histeries_rules_core::UnitId;
        use histeries_sim::{MapState, UnitKind};

        let topology = FlatHex::new(3, 1, false, false).unwrap();
        let seats = [PlayerId::new(1), PlayerId::new(2)];
        let mut session = Session::with_map(seats, MapState::blank(topology));
        for (player, tile, kind) in [
            (seats[0], 0, UnitKind::Warrior),
            (seats[1], 2, UnitKind::Archer),
        ] {
            session
                .dispatch(Command::UnitSpawn {
                    player,
                    unit: UnitId::new(player.index()),
                    kind,
                    tile: TileId::new(tile),
                })
                .unwrap();
        }
        let obs = session.observe(seats[0]).unwrap();
        let hold = |player| Command::UnitHold {
            player,
            unit: UnitId::new(1),
        };
        assert!(audit_contact_orders(&topology, &obs, &[]).is_err());
        assert!(audit_contact_orders(&topology, &obs, &[hold(seats[1])]).is_err());
        assert_eq!(
            audit_contact_orders(&topology, &obs, &[hold(seats[0])]),
            Ok(1)
        );
    }
    #[test]
    fn ordinary_games_end_at_victory_and_stress_keeps_planning() {
        let normal = run_game(9, 2, "small", 1, 500, false).unwrap();
        assert!(normal.winner.is_some());
        assert!(normal.checkpoint_round > 0 && normal.checkpoint_round < normal.hashes.len());
        let limit = normal.hashes.len() as u32 + 2;
        let stress = run_game(9, 2, "small", 1, limit, true).unwrap();
        assert!(stress.winner.is_none());
        assert_eq!(stress.hashes.len(), limit as usize);
        assert_ne!(
            stress.hashes[limit as usize - 2],
            stress.hashes[limit as usize - 1]
        );
    }
    #[test]
    fn stress_restart_preserves_full_evidence_and_rejects_corrupt_or_mismatched_segments() {
        use std::{fs, time::SystemTime};

        let nonce = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root =
            std::env::temp_dir().join(format!("histeries-resume-{}-{nonce}", std::process::id()));
        let fixture = Fixture {
            seed: 9,
            players: 2,
            size: "small".into(),
            workers: 1,
            limit: 12,
            stress: true,
        };
        let uninterrupted = checked_game(&fixture, None, fixture.limit).unwrap();
        let first = checked_game(&fixture, Some(&root), 9).unwrap();
        let directory = root.join("seed-9");
        // A killed writer may leave a truncated temporary file. The last
        // committed record must still be usable by a new writer.
        fs::write(directory.join("segment.pending"), b"partial record").unwrap();
        assert_eq!(first.resumed_from_round, 0);
        assert_eq!(first.hashes, uninterrupted.hashes[..9]);

        let mismatched = Fixture {
            workers: 8,
            ..fixture.clone()
        };
        assert!(
            checked_game(&mismatched, Some(&root), 12)
                .is_err_and(|error| error.contains("fixture mismatch"))
        );
        let resumed = checked_game(&fixture, Some(&root), 12).unwrap();
        assert_eq!(resumed.resumed_from_round, 9);
        assert_eq!(resumed.hashes, uninterrupted.hashes);
        assert_eq!(resumed.winner, uninterrupted.winner);
        assert_eq!(resumed.contact_orders, uninterrupted.contact_orders);
        assert_eq!(resumed.checkpoint_round, uninterrupted.checkpoint_round);
        assert_eq!(resumed.save_bytes, uninterrupted.save_bytes);
        assert!(resumed.max_us >= first.max_us);
        let done = checked_game(&fixture, Some(&root), 12).unwrap();
        assert_eq!(done.resumed_from_round, 12);
        assert_eq!(
            (done.hashes, done.p95_us, done.max_us),
            (resumed.hashes, resumed.p95_us, resumed.max_us)
        );

        let first_path = directory.join("from-0000000000.chk");
        let mut bytes = fs::read(&first_path).unwrap();
        *bytes.last_mut().unwrap() ^= 1;
        fs::write(&first_path, bytes).unwrap();
        assert!(
            checked_game(&fixture, Some(&root), 12)
                .is_err_and(|error| error.contains("checksum mismatch"))
        );
        fs::remove_file(first_path).unwrap();
        assert!(
            checked_game(&fixture, Some(&root), 12)
                .is_err_and(|error| error.contains("gap or overlap"))
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn rejects_malformed_schedules() {
        for args in [
            "--players 1",
            "--checkpoint-dir unused",
            "--mode stress --stop-after-round 1",
            "--mode stress --turns 2 --stop-after-round 3 --checkpoint-dir unused",
            "--mode stress --turns 2 --stop-after-round 0 --checkpoint-dir unused",
            "--players 9",
            "--players 16 --size small",
            "--games 0",
            "--mode silent",
            "--size unknown",
            "--turns 0",
            "--workers 0",
            "--seed 2 --seed 2",
            "--seed 18446744073709551615 --games 2",
        ] {
            assert!(
                run(&args
                    .split_whitespace()
                    .map(str::to_owned)
                    .collect::<Vec<_>>())
                .is_err(),
                "{args}"
            );
        }
    }
}
