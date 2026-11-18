//! Nightly live-heap comparison with an independent reload process.

use std::{
    env,
    io::{self, Read, Write},
    process::{Command, Stdio},
};

use histeries_sim::{Session, SimulationState};

use crate::{
    REPLAY_SEGMENT_TURNS, flag_string, flag_u32, flag_u64, release_round_history, selfplay::Game,
    validate_flags,
};

#[global_allocator]
static ALLOCATOR: dhat::Alloc = dhat::Alloc;

const BASELINE_TURN: u32 = 100;
const MAX_BYTES_PER_ENTITY: u128 = 512;
const MAX_EXTRA_BYTES_PER_ENTITY_PER_TURN: u128 = 1;

#[derive(Clone, Copy, Debug)]
struct Reading {
    hash: u64,
    entities: usize,
    bytes: usize,
}

impl Reading {
    fn sample(game: &Game) -> Self {
        Self {
            hash: game.session.state_hash().value(),
            entities: game.session.live_entity_count(),
            bytes: dhat::HeapStats::get().curr_bytes,
        }
    }

    fn compare(self, fresh: Self, turn: u32) -> Result<(), String> {
        if (self.hash, self.entities) != (fresh.hash, fresh.entities) {
            return Err(format!(
                "reload mismatch at turn {turn}: live {self:?}, fresh {fresh:?}"
            ));
        }
        if self.bytes > fresh.bytes {
            return Err(format!(
                "excess retained heap at turn {turn}: live {self:?}, fresh {fresh:?}"
            ));
        }
        Ok(())
    }

    fn check_density(self, baseline: Self, turn: u32) -> Result<(), String> {
        let entities = self.entities as u128;
        if entities == 0 || self.bytes as u128 > entities * MAX_BYTES_PER_ENTITY {
            return Err(format!(
                "live heap exceeds {MAX_BYTES_PER_ENTITY} bytes/entity at turn {turn}: {self:?}"
            ));
        }
        // Compare two densities as exact ratios. Subtract the allowed increase
        // before cross multiplication so both products fit in u128.
        let allowance = u128::from(turn - BASELINE_TURN) * MAX_EXTRA_BYTES_PER_ENTITY_PER_TURN;
        let adjusted = (self.bytes as u128).saturating_sub(entities * allowance);
        if adjusted * baseline.entities as u128 > baseline.bytes as u128 * entities {
            return Err(format!(
                "live bytes/entity grew faster than {MAX_EXTRA_BYTES_PER_ENTITY_PER_TURN} byte/entity/turn at turn {turn}: baseline {baseline:?}, live {self:?}"
            ));
        }
        Ok(())
    }
}

fn warm_up() -> Result<(), String> {
    // Initialize the immutable process-wide ruleset and standard streams before
    // measuring per-game ownership. Both processes run the same warm-up fixture.
    let _ = (io::stdin(), io::stdout());
    Game::new(0, 2, "small")?.round(1, true)?;
    Ok(())
}

fn profile() -> dhat::Profiler {
    dhat::Profiler::builder()
        .testing()
        .trim_backtraces(Some(4))
        .build()
}

fn load(save: &[u8], objectives: &[u8]) -> Result<Game, String> {
    let state = SimulationState::decode(save).map_err(|error| error.to_string())?;
    Game::from_session(Session::from_state(state), objectives)
}

fn reload_worker(args: &[String]) -> Result<(), String> {
    let objectives = args
        .iter()
        .map(|arg| arg.parse::<u8>().map_err(|error| error.to_string()))
        .collect::<Result<Vec<_>, _>>()?;
    let mut save = Vec::new();
    io::stdin()
        .read_to_end(&mut save)
        .map_err(|error| error.to_string())?;
    warm_up()?;
    let _profiler = profile();
    let game = load(&save, &objectives)?;
    if game.objectives() != objectives {
        return Err("heap reload changed a seat's planning objective".into());
    }
    let sample = Reading::sample(&game);
    drop(game);
    if dhat::HeapStats::get().curr_bytes != 0 {
        return Err("freshly loaded game retained allocations after drop".into());
    }
    println!("{} {} {}", sample.hash, sample.entities, sample.bytes);
    Ok(())
}

fn fresh_reading(game: &Game) -> Result<Reading, String> {
    let mut child = Command::new(env::current_exe().map_err(|error| error.to_string())?)
        .args(["memgate", "reload"])
        .args(game.objectives().iter().map(u8::to_string))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .map_err(|error| format!("starting heap reload worker: {error}"))?;
    let write = child
        .stdin
        .take()
        .expect("worker stdin is piped")
        .write_all(&game.session.state_snapshot().encode());
    let output = child
        .wait_with_output()
        .map_err(|error| error.to_string())?;
    write.map_err(|error| format!("sending saved game: {error}"))?;
    if !output.status.success() {
        return Err(format!("heap reload worker exited {}", output.status));
    }
    let line = std::str::from_utf8(&output.stdout).map_err(|error| error.to_string())?;
    let fields: Vec<_> = line.split_whitespace().collect();
    let [hash, entities, bytes] = fields[..] else {
        return Err(format!("invalid heap reload result {line:?}"));
    };
    Ok(Reading {
        hash: hash.parse().map_err(|_| format!("invalid hash {hash}"))?,
        entities: entities
            .parse()
            .map_err(|_| format!("invalid entity count {entities}"))?,
        bytes: bytes
            .parse()
            .map_err(|_| format!("invalid heap size {bytes}"))?,
    })
}

pub(super) fn run(args: &[String]) -> Result<(), String> {
    if args.first().is_some_and(|arg| arg == "reload") {
        return reload_worker(&args[1..]);
    }
    validate_flags(args, &["--seed", "--players", "--turns", "--size"], 0)?;
    let seed = flag_u64(args, "--seed")?.unwrap_or(0);
    let seats = flag_u32(args, "--players")?.unwrap_or(8);
    let turns = flag_u32(args, "--turns")?.unwrap_or(500);
    let size = flag_string(args, "--size")?.unwrap_or_else(|| "large".to_owned());
    if turns <= BASELINE_TURN {
        return Err("memgate needs more than 100 full simulation turns".into());
    }
    warm_up()?;
    let opening = Game::new(seed, seats, &size)?;
    let save = opening.session.state_snapshot().encode();
    let objectives = opening.objectives();
    drop(opening);

    let _profiler = profile();
    // Start from a save in both processes so the immutable map's B-tree has
    // identical packing. The long-running game is never reloaded after turn 0.
    let mut game = load(&save, &objectives)?;
    let mut baseline = None;
    let mut samples = 0;
    let mut last = None;
    let mut max_difference = i128::MIN;
    let mut peak_density = 0.0_f64;
    let mut max_density_growth = f64::NEG_INFINITY;
    for turn in 1..=turns {
        game.round(1, true)?;
        release_round_history(&mut game.session, false);
        if !turn.is_multiple_of(REPLAY_SEGMENT_TURNS) {
            if turn != turns {
                continue;
            }
            drop(game.session.take_replay_segment());
        }
        if turn < BASELINE_TURN {
            continue;
        }
        if game.session.turn() != turn {
            return Err(format!(
                "expected checkpoint turn {turn}, got {}",
                game.session.turn()
            ));
        }
        let live = Reading::sample(&game);
        let fresh = fresh_reading(&game)?;
        live.compare(fresh, turn)?;
        let (start, _) = baseline.get_or_insert((live, fresh.bytes));
        live.check_density(*start, turn)?;
        max_difference = max_difference.max(live.bytes as i128 - fresh.bytes as i128);
        let density = live.bytes as f64 / live.entities as f64;
        peak_density = peak_density.max(density);
        if turn > BASELINE_TURN {
            let baseline_density = start.bytes as f64 / start.entities as f64;
            let growth = (density - baseline_density) / f64::from(turn - BASELINE_TURN);
            max_density_growth = max_density_growth.max(growth);
        }
        samples += 1;
        last = Some((live, fresh.bytes));
    }
    drop(game);
    let retained = dhat::HeapStats::get().curr_bytes;
    if retained != 0 {
        return Err(format!(
            "long-running game retained {retained} bytes after drop"
        ));
    }
    let (baseline, baseline_fresh_bytes) = baseline.expect("turn 100 is always sampled");
    let (end, final_fresh_bytes) = last.expect("the final turn is always sampled");
    let bytes_per_entity = baseline.bytes as f64 / baseline.entities as f64;
    let span = f64::from(turns - BASELINE_TURN);
    let slope = (end.bytes as f64 - baseline.bytes as f64) / span;
    let reload_slope = (final_fresh_bytes as f64 - baseline_fresh_bytes as f64) / span;
    let relative_slope = slope - reload_slope;
    println!(
        "{{\"seed\":{seed},\"players\":{seats},\"size\":\"{size}\",\"turns\":{turns},\
        \"samples\":{samples},\"baseline_entities\":{},\"baseline_live_bytes\":{},\
        \"baseline_reload_bytes\":{baseline_fresh_bytes},\
        \"peak_bytes_per_entity\":{peak_density:.3},\
        \"max_growth_bytes_per_entity_per_turn\":{max_density_growth:.3},\
        \"baseline_bytes_per_entity\":{bytes_per_entity:.3},\"final_entities\":{},\
        \"final_live_bytes\":{},\"final_reload_bytes\":{final_fresh_bytes},\
        \"live_bytes_per_turn_after_100\":{slope:.3},\
        \"max_live_minus_reload_bytes\":{max_difference},\
        \"live_minus_reload_bytes_per_turn\":{relative_slope:.3},\"retained_after_drop\":{retained},\
        \"final_hash\":{}}}",
        baseline.entities, baseline.bytes, end.entities, end.bytes, end.hash
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{BASELINE_TURN, Reading};

    #[test]
    fn density_caps_allow_entity_growth_but_reject_one_byte_over_budget() {
        let base = Reading {
            hash: 0,
            entities: 10,
            bytes: 5_000,
        };
        let mut next = Reading {
            entities: 20,
            bytes: 10_040,
            ..base
        };
        assert!(next.check_density(base, BASELINE_TURN + 2).is_ok());
        next.bytes += 1;
        assert!(next.check_density(base, BASELINE_TURN + 2).is_err());
        next.bytes = 20 * 512;
        assert!(next.check_density(base, BASELINE_TURN + 200).is_ok());
        next.bytes += 1;
        assert!(next.check_density(base, BASELINE_TURN + 200).is_err());
    }

    #[test]
    fn excess_live_bytes_and_mismatched_worlds_fail_independently() {
        let fresh = Reading {
            hash: 7,
            entities: 12,
            bytes: 100,
        };
        assert!(fresh.compare(fresh, 100).is_ok());
        assert!(
            Reading {
                bytes: 101,
                ..fresh
            }
            .compare(fresh, 100)
            .is_err()
        );
        assert!(Reading { hash: 8, ..fresh }.compare(fresh, 100).is_err());
        assert!(
            Reading {
                entities: 13,
                ..fresh
            }
            .compare(fresh, 100)
            .is_err()
        );
    }
}
