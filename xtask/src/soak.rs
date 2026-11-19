//! Run release-mode gate games and collect replayable JSON Lines evidence.

use std::{
    fs,
    hash::{DefaultHasher, Hash, Hasher},
    io::{BufWriter, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        mpsc,
    },
    thread,
    time::Instant,
};

use serde_json::{Value, json};

#[derive(Clone, Copy, PartialEq)]
enum Tier {
    Fast,
    Nightly,
    Weekly,
    Soak,
}

#[derive(Clone, Copy)]
struct Game {
    seed: u64,
    players: usize,
    turns: u32,
    size: &'static str,
}

struct Batch {
    tier: Tier,
    games: usize,
    offset: usize,
    base: u64,
    turns: u32,
    jobs: usize,
    output: PathBuf,
    checkpoint_dir: Option<PathBuf>,
    stop_after_round: Option<u32>,
}

fn positive<T>(value: &str, option: &str) -> Result<T, String>
where
    T: std::str::FromStr + Default + PartialOrd,
{
    value
        .parse()
        .ok()
        .filter(|n| *n > T::default())
        .ok_or_else(|| format!("{option} requires a positive integer"))
}

impl Batch {
    fn parse(root: &Path, command: &str, args: &[String]) -> Result<Self, String> {
        let (profile, args) = if command == "fixture" {
            let (name, rest) = args
                .split_first()
                .ok_or("fixture requires fast, nightly or weekly")?;
            (name.as_str(), rest)
        } else {
            (command, args)
        };
        let (tier, games, turns) = match profile {
            "fast" => (Tier::Fast, 6, 200),
            "nightly" => (Tier::Nightly, 5, 500),
            "weekly" => (Tier::Weekly, 2, 1_000),
            "soak" if command == "soak" => (Tier::Soak, 1_000, 500),
            _ => return Err(format!("unknown fixture {profile}")),
        };
        let cpus = thread::available_parallelism().map_or(1, usize::from);
        let mut batch = Self {
            tier,
            games,
            offset: 0,
            base: 0,
            turns,
            jobs: if tier == Tier::Soak {
                (cpus / 8).max(1)
            } else {
                1
            },
            output: root.join(format!("target/gates/{profile}.jsonl")),
            checkpoint_dir: None,
            stop_after_round: None,
        };
        let (pairs, remainder) = args.as_chunks::<2>();
        if !remainder.is_empty() {
            return Err("every option requires a value".into());
        }
        let mut seen = std::collections::BTreeSet::new();
        for [key, value] in pairs {
            let key = key.as_str();
            let value = value.as_str();
            if !seen.insert(key) {
                return Err(format!("duplicate option {key}"));
            }
            match key {
                "--output" => batch.output = root.join(value),
                "--checkpoint-dir" if tier == Tier::Weekly => {
                    batch.checkpoint_dir = Some(root.join(value));
                }
                "--stop-after-round" if tier == Tier::Weekly => {
                    batch.stop_after_round = Some(positive(value, key)?);
                }
                "--seed" => {
                    batch.base = value.parse().map_err(|_| "invalid --seed")?;
                    if tier != Tier::Soak {
                        batch.games = if tier == Tier::Fast { 2 } else { 1 };
                    }
                }
                "--games" if tier == Tier::Soak => batch.games = positive(value, key)?,
                "--turns" if tier == Tier::Soak => batch.turns = positive(value, key)?,
                "--jobs" if tier == Tier::Soak => batch.jobs = positive(value, key)?,
                "--offset" if tier == Tier::Soak => {
                    batch.offset = value.parse().map_err(|_| "invalid --offset")?;
                }
                _ => return Err(format!("unknown {profile} option {key}")),
            }
        }
        if let Some(round) = batch.stop_after_round
            && (batch.checkpoint_dir.is_none() || batch.games != 1 || round > batch.turns)
        {
            return Err("--stop-after-round requires --checkpoint-dir, one --seed, and a round within the weekly fixture".into());
        }
        batch
            .offset
            .checked_add(batch.games)
            .ok_or("game index overflow")?;
        Ok(batch)
    }

    fn game(&self, index: usize) -> Game {
        let index = index + self.offset;
        let (players, size, seed) = match self.tier {
            Tier::Fast => ([2, 4][index % 2], "small", self.base + (index / 2) as u64),
            Tier::Nightly => (8, "large", self.base + index as u64),
            Tier::Weekly => (16, "huge", self.base + index as u64),
            Tier::Soak => (
                2 + index % 7,
                "small",
                sample_seed(self.base.wrapping_add(index as u64)),
            ),
        };
        Game {
            players,
            size,
            seed,
            turns: self.turns,
        }
    }
}

// SplitMix64's bijective mixing function samples distinct, reproducible u64 seeds.
fn sample_seed(index: u64) -> u64 {
    let mut x = index.wrapping_add(0x9e37_79b9_7f4a_7c15);
    x = (x ^ (x >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    x ^ (x >> 31)
}

struct Executable(PathBuf);

impl Drop for Executable {
    fn drop(&mut self) {
        if let Err(error) = fs::remove_file(&self.0) {
            eprintln!("could not remove {}: {error}", self.0.display());
        }
    }
}

fn executable(root: &Path) -> Result<Executable, String> {
    // Cargo reports the actual executable, including target-dir and platform overrides.
    let output = Command::new("cargo")
        .current_dir(root)
        .args([
            "build",
            "--locked",
            "--release",
            "--target-dir",
            "target/gate-build",
            "-p",
            "histeries-cli",
            "--bin",
            "histeries",
            "--message-format=json-render-diagnostics",
        ])
        .stderr(Stdio::inherit())
        .output()
        .map_err(|e| e.to_string())?;
    if !output.status.success() {
        return Err(format!("building histeries failed: {}", output.status));
    }
    let built = String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .find_map(|v| {
            (v["target"]["name"] == "histeries")
                .then(|| v["executable"].as_str().map(PathBuf::from))
                .flatten()
        })
        .ok_or("cargo did not report the histeries executable")?;
    // A complete batch must use the same executable even if another gate
    // starts a build or a developer edits the CLI during a long soak.
    let snapshot = root.join("target/gate-build").join(format!(
        "worker-{}{}",
        std::process::id(),
        std::env::consts::EXE_SUFFIX
    ));
    fs::copy(&built, &snapshot).map_err(|e| format!("copy {}: {e}", built.display()))?;
    Ok(Executable(snapshot))
}

fn number(report: &Value, key: &str) -> Result<u64, String> {
    report[key]
        .as_u64()
        .ok_or_else(|| format!("missing or invalid metric {key}"))
}

fn check(report: &Value, game: Game, tier: Tier) -> Result<(), String> {
    let turns = number(report, "turns_checked")?;
    let hashes = report["round_hashes"]
        .as_array()
        .ok_or("missing round_hashes")?;
    if number(report, "seed")? != game.seed
        || number(report, "players")? != game.players as u64
        || turns == 0
        || turns > u64::from(game.turns)
        || hashes.len() as u64 != turns
        || !hashes.iter().all(Value::is_u64)
        || hashes.last() != report.get("final_hash")
    {
        return Err("inconsistent seed, seat count, round coverage or hash evidence".into());
    }
    let terminal = report["terminal"]
        .as_bool()
        .ok_or("missing terminal flag")?;
    if number(report, "contact_units")? != number(report, "contact_orders")? {
        return Err("military units in contact lack an explicit order".into());
    }
    let checkpoint = number(report, "checkpoint_round")?;
    if checkpoint >= turns
        || number(report, "worker_comparisons")? != turns
        || number(report, "replay_comparisons")? != turns
        || number(report, "save_comparisons")? != turns
        || number(report, "resume_comparisons")? != turns - checkpoint
    {
        return Err("incomplete worker, save, replay or resume comparisons".into());
    }
    if tier == Tier::Soak && !terminal {
        return Err(format!("no terminal outcome after {turns} rounds"));
    }
    if terminal
        && (!(1..=game.players as u64).contains(&number(report, "winner")?)
            || !matches!(report["victory"].as_str(), Some("Science" | "Domination")))
    {
        return Err("terminal result lacks a valid winner or victory type".into());
    }
    if tier != Tier::Soak && turns != u64::from(game.turns) {
        return Err(format!("only {turns}/{} stress rounds", game.turns));
    }
    let p95 = number(report, "planning_p95_us")?;
    let max = number(report, "planning_max_us")?;
    let size = number(report, "save_bytes")?;
    if p95 > max || size == 0 {
        return Err("inconsistent timing or save measurement".into());
    }
    if tier == Tier::Fast && p95 > 250_000 {
        return Err(format!("planning p95 {p95} us exceeds 250000 us"));
    }
    if tier == Tier::Nightly && size > 10 * 1024 * 1024 {
        return Err(format!("Large save {size} bytes exceeds 10 MiB"));
    }
    if tier == Tier::Weekly && size > 40 * 1024 * 1024 {
        return Err(format!("Huge save {size} bytes exceeds 40 MiB"));
    }
    if tier == Tier::Weekly && max > 2_000_000 {
        return Err(format!(
            "weekly maximum planning time {max} us exceeds 2000000 us"
        ));
    }
    Ok(())
}

fn play(root: &Path, program: &Path, game: Game, batch: &Batch) -> Result<Value, String> {
    let mode = if batch.tier == Tier::Soak {
        "game"
    } else {
        "stress"
    };
    let mut worker = Command::new(program);
    worker.current_dir(root).args([
        "selfplay",
        "--mode",
        mode,
        "--workers",
        "1",
        "--size",
        game.size,
        "--players",
        &game.players.to_string(),
        "--seed",
        &game.seed.to_string(),
        "--turns",
        &game.turns.to_string(),
    ]);
    if let Some(dir) = &batch.checkpoint_dir {
        worker.arg("--checkpoint-dir").arg(dir);
    }
    if let Some(round) = batch.stop_after_round {
        worker.arg("--stop-after-round").arg(round.to_string());
    }
    let result = worker.output().map_err(|e| e.to_string())?;
    if !result.status.success() {
        return Err(format!(
            "worker exited {}: {}",
            result.status,
            String::from_utf8_lossy(&result.stderr).trim()
        ));
    }
    let report: Value =
        serde_json::from_slice(&result.stdout).map_err(|e| format!("invalid worker JSON: {e}"))?;
    if let Some(stop) = batch.stop_after_round
        && stop < game.turns
        && report["complete"] == false
    {
        let round = number(&report, "turns_completed")?;
        if number(&report, "seed")? != game.seed
            || number(&report, "original_turns")? != u64::from(game.turns)
            || !(u64::from(stop)..u64::from(game.turns)).contains(&round)
            || number(&report, "resumed_from_round")? > round
        {
            return Err("invalid weekly checkpoint progress".into());
        }
    } else {
        check(&report, game, batch.tier)?;
    }
    Ok(report)
}

pub(super) fn run(root: &Path, command: &str, args: &[String]) -> Result<(), String> {
    let batch = Batch::parse(root, command, args)?;
    // Chunk steps persist checkpoints. Only a completed batch publishes a fixture report.
    if batch
        .stop_after_round
        .is_some_and(|stop| stop < batch.turns)
    {
        let program = executable(root)?;
        let report = play(root, &program.0, batch.game(0), &batch)?;
        println!("{report}");
        return Ok(());
    }
    if let Some(parent) = batch.output.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    // Protect concurrent writers without locking readers out of live JSONL evidence.
    let output = fs::File::options()
        .create(true)
        .write(true)
        .truncate(false)
        .open(&batch.output)
        .map_err(|e| format!("{}: {e}", batch.output.display()))?;
    let mut hash = DefaultHasher::new();
    batch
        .output
        .canonicalize()
        .map_err(|e| e.to_string())?
        .hash(&mut hash);
    let lock_dir = root.join("target/gate-build/report-locks");
    fs::create_dir_all(&lock_dir).map_err(|e| e.to_string())?;
    let writer_lock = fs::File::options()
        .write(true)
        .create(true)
        .truncate(false)
        .open(lock_dir.join(format!("{:016x}", hash.finish())))
        .map_err(|e| e.to_string())?;
    writer_lock
        .try_lock()
        .map_err(|e| format!("lock {}: {e}", batch.output.display()))?;
    output.set_len(0).map_err(|e| e.to_string())?;
    let mut file = BufWriter::new(output);
    let program = executable(root)?;
    let next = AtomicUsize::new(0);
    let cancelled = AtomicBool::new(false);
    let start = Instant::now();
    let mut passed = 0;
    let mut rounds = 0;
    let mut max_p95 = 0;
    let mut max_us = 0;
    let mut max_bytes = 0;
    let mut contact_orders = 0;
    let mut first_error = None;
    let jobs = batch.jobs.min(batch.games);
    eprintln!(
        "Running {} games with {jobs} concurrent workers; writing {}",
        batch.games,
        batch.output.display()
    );
    thread::scope(|scope| -> Result<(), String> {
        let (tx, rx) = mpsc::sync_channel(jobs);
        for _ in 0..jobs {
            let (tx, batch, program, next, cancelled) =
                (tx.clone(), &batch, &program, &next, &cancelled);
            scope.spawn(move || {
                while !cancelled.load(Ordering::Relaxed) {
                    let index = next.fetch_add(1, Ordering::Relaxed);
                    if index >= batch.games {
                        break;
                    }
                    let game = batch.game(index);
                    let result = play(root, &program.0, game, batch);
                    if result.is_err() {
                        cancelled.store(true, Ordering::Relaxed);
                    }
                    if tx.send((index + batch.offset, game, result)).is_err() {
                        break;
                    }
                }
            });
        }
        drop(tx);
        for (index, game, result) in rx {
            let line = match result {
                Ok(mut report) => {
                    passed += 1;
                    rounds += number(&report, "turns_checked")?;
                    max_p95 = max_p95.max(number(&report, "planning_p95_us")?);
                    max_us = max_us.max(number(&report, "planning_max_us")?);
                    max_bytes = max_bytes.max(number(&report, "save_bytes")?);
                    contact_orders += number(&report, "contact_orders")?;
                    report["index"] = json!(index);
                    report["size"] = json!(game.size);
                    report["turn_limit"] = json!(game.turns);
                    report
                }
                Err(error) => {
                    let message = format!(
                        "game {index}, seed {}, players {}, size {}, turns {}: {error}",
                        game.seed, game.players, game.size, game.turns
                    );
                    first_error.get_or_insert(message.clone());
                    json!({"failed": message, "index": index, "seed": game.seed, "players": game.players})
                }
            };
            writeln!(file, "{line}")
                .and_then(|_| file.flush())
                .map_err(|e| e.to_string())?;
        }
        Ok(())
    })?;
    let summary = json!({"games_requested": batch.games, "games_passed": passed,
        "rounds_checked": rounds, "worst_game_planning_p95_us": max_p95,
        "planning_max_us": max_us, "max_save_bytes": max_bytes, "contact_orders": contact_orders,
        "wall_ms": start.elapsed().as_millis(),
        "first_error": first_error});
    writeln!(file, "{summary}")
        .and_then(|_| file.flush())
        .map_err(|e| e.to_string())?;
    println!("{summary}");
    first_error.map_or(Ok(()), Err)
}

fn fast_traces(path: &str) -> Result<Vec<Vec<Value>>, String> {
    let batch = Batch::parse(Path::new("."), "fixture", &["fast".into()])?;
    let data = fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?;
    let mut lines = data.lines();
    let mut traces = Vec::new();
    for index in 0..batch.games {
        let report = serde_json::from_str(lines.next().ok_or("missing fixture")?)
            .map_err(|e| format!("{path}: invalid fixture {index}: {e}"))?;
        check(&report, batch.game(index), Tier::Fast).map_err(|e| format!("{path}: {e}"))?;
        traces.push(report["round_hashes"].as_array().unwrap().clone());
    }
    let summary: Value = serde_json::from_str(lines.next().ok_or("missing summary")?)
        .map_err(|e| format!("{path}: invalid summary: {e}"))?;
    if summary["games_passed"] != batch.games
        || !summary["first_error"].is_null()
        || lines.next().is_some()
    {
        return Err(format!("{path}: incomplete or failed batch"));
    }
    Ok(traces)
}

pub(super) fn compare_fast(paths: &[String]) -> Result<(), String> {
    if paths.len() < 2 {
        return Err("compare-fast requires at least two fast fixture reports".into());
    }
    let expected = fast_traces(&paths[0])?;
    for path in &paths[1..] {
        let actual = fast_traces(path)?;
        for (i, (reference, candidate)) in expected.iter().zip(actual).enumerate() {
            if let Some(round) = reference.iter().zip(candidate).position(|(a, b)| *a != b) {
                return Err(format!(
                    "{path} differs from {} at seed {}, players {}, round {}",
                    paths[0],
                    i / 2,
                    [2, 4][i % 2],
                    round + 1
                ));
            }
        }
    }
    println!("1,200 round hashes match on all {} platforms", paths.len());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn soak_shards_preserve_seeds_and_seat_cycle() {
        let all = Batch::parse(Path::new("."), "soak", &[]).unwrap();
        let shard =
            Batch::parse(Path::new("."), "soak", &["--offset".into(), "100".into()]).unwrap();
        let seats: Vec<_> = (0..7).map(|index| all.game(index).players).collect();
        assert_eq!(seats, [2, 3, 4, 5, 6, 7, 8]);
        assert_eq!(all.game(100).seed, shard.game(0).seed);
        assert_eq!(all.game(100).players, shard.game(0).players);
        let seeds: std::collections::BTreeSet<_> = (0..1000).map(|i| all.game(i).seed).collect();
        assert_eq!(seeds.len(), 1000);
        let overflow = [
            "--offset".into(),
            usize::MAX.to_string(),
            "--games".into(),
            "1".into(),
        ];
        assert!(Batch::parse(Path::new("."), "soak", &overflow).is_err());
    }

    #[test]
    fn truncated_rounds_or_exceeded_budgets_fail() {
        let game = Game {
            seed: 1,
            players: 2,
            turns: 1,
            size: "small",
        };
        let mut report = json!({"seed": 1, "players": 2, "turns_checked": 1, "round_hashes": [42],
            "final_hash": 42, "terminal": false, "planning_p95_us": 250000, "planning_max_us": 400000,
            "save_bytes": 300, "worker_comparisons": 1, "save_comparisons": 1, "replay_comparisons": 1,
            "checkpoint_round": 0, "resume_comparisons": 1, "contact_orders": 0, "contact_units": 0});
        assert!(check(&report, game, Tier::Fast).is_ok());
        assert!(check(&report, game, Tier::Soak).is_err());
        assert!(check(&report, Game { turns: 2, ..game }, Tier::Fast).is_err());
        report["planning_p95_us"] = json!(250001);
        assert!(check(&report, game, Tier::Fast).is_err());
        report["save_bytes"] = json!(10485761);
        assert!(check(&report, game, Tier::Nightly).is_err());
        report["save_bytes"] = json!(40 * 1024 * 1024);
        report["planning_max_us"] = json!(2_000_000);
        assert!(check(&report, game, Tier::Weekly).is_ok());
        report["planning_max_us"] = json!(2_000_001);
        assert!(check(&report, game, Tier::Weekly).is_err());
        report["planning_max_us"] = json!(2_000_000);
        report["save_bytes"] = json!(40 * 1024 * 1024 + 1);
        assert!(check(&report, game, Tier::Weekly).is_err());
    }
}
