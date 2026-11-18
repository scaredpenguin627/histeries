//! Paired head-to-head games on generated maps using the normal simulation registry.

use std::{
    collections::{BTreeMap, BTreeSet},
    panic,
    time::Instant,
};

use histeries_ai::{GreedyPlayer, Player, RandomPlayer, ThreeLayerPlayer, plan_seats};
use histeries_hex::{FlatHex, TileId, TileTopology};
use histeries_rules_core::{CityId, PlayerId, UnitId};
use histeries_sim::{
    Command, Session, SimulationState, Terrain, UnitKind, VictoryKind, generate_map,
};

use crate::{flag_string, flag_u32, flag_u64, seed_bytes, validate_flags};

const SEATS: [PlayerId; 2] = [PlayerId::new(1), PlayerId::new(2)];

#[derive(Clone, Copy)]
enum Opponent {
    Greedy,
    Random,
}

impl Opponent {
    fn player(self, seed: u64) -> Box<dyn Player + Send> {
        match self {
            Self::Greedy => Box::new(GreedyPlayer),
            Self::Random => Box::new(RandomPlayer::new(seed)),
        }
    }
}

fn scenario(seed: u64) -> Result<Session, String> {
    let topology = FlatHex::new(8, 8, false, false).map_err(|e| e.to_string())?;
    let mut session = Session::with_map(SEATS, generate_map(topology, seed_bytes(seed)));
    let corners = [TileId::new(0), TileId::new(63)];
    for (i, player) in SEATS.into_iter().enumerate() {
        let capital = corners[i];
        let garrison = topology.line(capital, corners[1 - i])[1];
        let settler = UnitId::new(2 * player.index());
        for command in [
            Command::EditorSetTerrain {
                player,
                tile: capital,
                terrain: Terrain::Plains,
            },
            Command::EditorSetTerrain {
                player,
                tile: garrison,
                terrain: Terrain::Plains,
            },
            Command::UnitSpawn {
                player,
                unit: settler,
                kind: UnitKind::Settler,
                tile: capital,
            },
            Command::CityFound {
                player,
                city: CityId::new(player.index()),
                unit: settler,
            },
            Command::UnitSpawn {
                player,
                unit: UnitId::new(settler.index() + 1),
                kind: UnitKind::Warrior,
                tile: garrison,
            },
        ] {
            session
                .dispatch(command)
                .map_err(|e| format!("setup: {e}"))?;
        }
    }
    Ok(session)
}

// Plans share a start-of-round snapshot. A later seat can lose a target or
// contested tile to the earlier seat. After a lost move, a settler is no
// longer at its intended position for any later move or founding order.
fn dispatch_plan(
    session: &mut Session,
    before: &SimulationState,
    seat: PlayerId,
    commands: &[Command],
) -> Result<u64, String> {
    let mut conflicts = 0;
    let mut lost_moves = BTreeSet::new();
    for (i, command) in commands.iter().enumerate() {
        let depends_on_lost_move = matches!(command,
            Command::UnitMove { unit, .. } | Command::CitySettle { unit, .. } | Command::CityFound { unit, .. }
            if lost_moves.contains(unit));
        // Validate skipped dependency chains on their original snapshot too;
        // an illegal plan must still fail even after losing its first move.
        let error = if depends_on_lost_move {
            Some("depends on a lost move".to_owned())
        } else {
            session
                .dispatch(command.clone())
                .err()
                .map(|e| e.to_string())
        };
        if let Some(error) = error {
            let conflict_possible = seat == SEATS[1]
                && matches!(
                    command,
                    Command::UnitMove { .. }
                        | Command::UnitAttack { .. }
                        | Command::UnitHeal { .. }
                        | Command::UnitPromote { .. }
                        | Command::UnitHold { .. }
                        | Command::CitySetProduction { .. }
                        | Command::CitySettle { .. }
                        | Command::CityFound { .. }
                );
            if conflict_possible {
                let mut isolated = Session::from_state(before.clone());
                if commands[..=i]
                    .iter()
                    .all(|c| isolated.dispatch(c.clone()).is_ok())
                {
                    if let Command::UnitMove { unit, .. } = command {
                        lost_moves.insert(*unit);
                    }
                    conflicts += 1;
                    continue;
                }
            }
            return Err(format!(
                "turn={} seat={} command={command:?}: {error}",
                before.turn(),
                seat.index()
            ));
        }
        if session.winner().is_some() {
            break;
        }
    }
    Ok(conflicts)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Game {
    victory: Option<(PlayerId, VictoryKind)>,
    turns: u32,
    rounds_planned: u32,
    hash: u64,
    conflicts: u64,
}

fn play(
    seed: u64,
    candidate: usize,
    opponent: Opponent,
    limit: u32,
    workers: usize,
    times: &mut RoundTimes,
) -> Result<Game, String> {
    let mut session = scenario(seed)?;
    let mut players: [Box<dyn Player + Send>; 2] =
        [Box::new(ThreeLayerPlayer::default()), opponent.player(seed)];
    players.swap(0, candidate);
    let [first, second] = &mut players;
    let mut conflicts = 0;
    let mut turns = 0;
    loop {
        let [one, two] = SEATS.map(|seat| session.observe(seat).map_err(|e| e.to_string()));
        let (one, two) = (one?, two?);
        let victory = session.winner();
        if victory.is_none() && one.turn() != turns {
            return Err(format!("plans did not complete round {turns}"));
        }
        if victory.is_some() || turns == limit {
            return Ok(Game {
                victory,
                turns: one.turn(),
                rounds_planned: turns,
                hash: session.state_hash().value(),
                conflicts,
            });
        }
        let snapshot = session.state_snapshot();
        let start = Instant::now();
        let plans = plan_seats(
            &mut [(first.as_mut(), one), (second.as_mut(), two)],
            workers,
        );
        times.record(start.elapsed().as_micros());
        for (seat, plan) in SEATS.into_iter().zip(plans) {
            conflicts += dispatch_plan(&mut session, &snapshot, seat, &plan.commands)?;
            if session.winner().is_some() {
                break;
            }
        }
        turns += 1;
    }
}

// Microsecond buckets below 1 ms, then 512 buckets per power of two.
// A Duration fits in at most 84 bits of microseconds, bounding the bucket count.
#[derive(Default)]
struct RoundTimes {
    buckets: BTreeMap<u128, u128>,
    count: u128,
}

impl RoundTimes {
    fn record(&mut self, micros: u128) {
        let width = 1_u128 << micros.checked_ilog2().unwrap_or(0).saturating_sub(9);
        let upper = micros.div_ceil(width) * width;
        *self.buckets.entry(upper).or_default() += 1;
        self.count += 1;
    }

    fn p95_upper_us(&self) -> u128 {
        let rank = self.count - self.count / 20;
        let mut total = 0;
        for (&upper, &count) in &self.buckets {
            total += count;
            if total >= rank {
                return upper;
            }
        }
        0
    }
}

/// One-sided exact sign test on seed pairs with unequal scores. Compute the
/// binomial masses relative to the mode to avoid overflow for large samples.
fn paired_p_value(better: u64, worse: u64) -> f64 {
    let n = better + worse;
    let mode = n / 2;
    let mut total = 1.;
    let mut tail = if mode >= better { 1. } else { 0. };
    let mut mass = 1.;
    for k in (1..=mode).rev() {
        mass *= k as f64 / (n - k + 1) as f64;
        total += mass;
        if k > better {
            tail += mass;
        }
    }
    mass = 1.;
    for k in mode..n {
        mass *= (n - k) as f64 / (k + 1) as f64;
        total += mass;
        if k + 1 >= better {
            tail += mass;
        }
    }
    tail / total
}

pub(super) fn run(args: &[String]) -> Result<(), String> {
    validate_flags(
        args,
        &["--opponent", "--seed", "--seeds", "--turns", "--workers"],
        0,
    )?;
    let name = flag_string(args, "--opponent")?.unwrap_or_else(|| "greedy".into());
    let opponent = match name.as_str() {
        "greedy" => Opponent::Greedy,
        "random" => Opponent::Random,
        _ => return Err("--opponent must be greedy or random".into()),
    };
    let first = flag_u64(args, "--seed")?.unwrap_or(0);
    let count = flag_u32(args, "--seeds")?.unwrap_or(32);
    let limit = flag_u32(args, "--turns")?.unwrap_or(500);
    let workers = flag_u32(args, "--workers")?.unwrap_or(1) as usize;
    if count == 0 || limit == 0 || workers == 0 {
        return Err("--seeds, --turns and --workers must be positive".into());
    }
    let last = first
        .checked_add(u64::from(count - 1))
        .ok_or("seed range overflows")?;
    println!(
        "candidate=three-layer opponent={name} seeds={first}..={last} turn_limit={limit} workers={workers}"
    );
    let games = u64::from(count) * 2;
    let (mut better, mut worse) = (0_u64, 0_u64);
    let (mut draws, mut errors, mut crashes, mut conflicts) = (0_u64, 0_u64, 0_u64, 0_u64);
    let mut wins = [[0_u64; 2]; 2];
    let mut times = RoundTimes::default();
    for seed in first..=last {
        let mut balance = 0_i32;
        for (candidate, candidate_seat) in SEATS.into_iter().enumerate() {
            print!("seed={seed} candidate_seat={} ", candidate_seat.index());
            let result = panic::catch_unwind(panic::AssertUnwindSafe(|| {
                play(seed, candidate, opponent, limit, workers, &mut times)
            }));
            match result {
                Ok(Ok(game)) => {
                    print!(
                        "turns={} rounds_planned={} hash={} conflicts={} ",
                        game.turns, game.rounds_planned, game.hash, game.conflicts
                    );
                    conflicts += game.conflicts;
                    if let Some((seat, kind)) = game.victory {
                        let lost = seat != candidate_seat;
                        wins[usize::from(lost)][match kind {
                            VictoryKind::Science => 0,
                            VictoryKind::Domination => 1,
                        }] += 1;
                        balance += if lost { -1 } else { 1 };
                        println!("winner={} victory={kind:?}", seat.index());
                    } else {
                        draws += 1;
                        println!("outcome=draw");
                    }
                }
                Ok(Err(error)) => {
                    errors += 1;
                    println!("outcome=error {error}");
                }
                Err(_) => {
                    crashes += 1;
                    println!("outcome=crash");
                }
            }
        }
        better += u64::from(balance > 0);
        worse += u64::from(balance < 0);
    }
    let [candidate_wins, opponent_wins] = wins.map(|[science, domination]| science + domination);
    let win_bp = |n: u64| 10_000 * n / games;
    let p95 = times.p95_upper_us();
    println!(
        "games={games} wins={candidate_wins} losses={opponent_wins} draws={draws} errors={errors} crashes={crashes} conflicts={conflicts} win_bp={} planning_round_p95_upper_us={p95}",
        win_bp(candidate_wins)
    );
    for (player, [science, domination]) in ["three-layer", &name].into_iter().zip(wins) {
        println!(
            "player={player} science_wins={science} science_win_bp={} domination_wins={domination} domination_win_bp={}",
            win_bp(science),
            win_bp(domination)
        );
    }
    if errors + crashes != 0 {
        return Err("tournament game failures; rating and significance unavailable".into());
    }
    // Half a win and half a loss keep the estimate finite at either extreme.
    let odds = (2 * candidate_wins + draws + 1) as f64 / (2 * opponent_wins + draws + 1) as f64;
    println!(
        "elo_delta_smoothed={:.0} paired_better={better} paired_worse={worse} paired_tied={} paired_sign_p_one_sided{}",
        400. * odds.log10(),
        u64::from(count) - better - worse,
        format_p(paired_p_value(better, worse))
    );
    Ok(())
}

fn format_p(p: f64) -> String {
    if p < 1e-300 {
        "<1e-300".into()
    } else {
        format!("={p:.8e}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_sign_tail_uses_independent_seeds() {
        assert_eq!(paired_p_value(0, 0), 1.);
        assert!((paired_p_value(5, 0) - 1. / 32.).abs() < 1e-12);
        assert!((paired_p_value(3, 2) - 0.5).abs() < 1e-12);
        assert!((paired_p_value(1, 4) - 31. / 32.).abs() < 1e-12);
        assert_eq!(format_p(paired_p_value(2048, 0)), "<1e-300");
    }

    #[test]
    fn histogram_reports_nearest_rank_with_bounded_buckets() {
        let mut times = RoundTimes::default();
        for micros in 1..=20 {
            times.record(micros);
        }
        assert_eq!(times.p95_upper_us(), 19);
        let mut long = RoundTimes::default();
        long.record(1025);
        assert_eq!(long.p95_upper_us(), 1026);
    }

    #[test]
    fn rejects_seed_overflow_and_empty_schedules() {
        let args = [
            "--seed".into(),
            u64::MAX.to_string(),
            "--seeds".into(),
            "2".into(),
        ];
        assert!(run(&args).is_err());
        for flag in ["--seeds", "--turns", "--workers"] {
            assert!(run(&[flag.into(), "0".into()]).is_err());
        }
    }

    #[test]
    fn contested_orders_are_distinguished_from_illegal_plans() {
        let topology = FlatHex::new(3, 1, false, false).unwrap();
        let mut session = Session::with_map(SEATS, histeries_sim::MapState::blank(topology));
        for (seat, tile) in SEATS.into_iter().zip([0, 2]) {
            session
                .dispatch(Command::UnitSpawn {
                    player: seat,
                    unit: UnitId::new(seat.index()),
                    kind: UnitKind::Warrior,
                    tile: TileId::new(tile),
                })
                .unwrap();
        }
        let before = session.state_snapshot();
        let move_to = |seat: PlayerId, tile| Command::UnitMove {
            player: seat,
            unit: UnitId::new(seat.index()),
            destination: TileId::new(tile),
        };
        session.dispatch(move_to(SEATS[0], 1)).unwrap();
        assert_eq!(
            dispatch_plan(&mut session, &before, SEATS[1], &[move_to(SEATS[1], 1)]),
            Ok(1)
        );
        assert!(dispatch_plan(&mut session, &before, SEATS[1], &[move_to(SEATS[1], 0)]).is_err());
        assert!(
            dispatch_plan(
                &mut session,
                &before,
                SEATS[1],
                &[move_to(SEATS[1], 1), move_to(SEATS[1], 99)]
            )
            .is_err()
        );
    }

    #[test]
    fn terminal_command_stops_the_rest_of_the_plan() {
        let player = SEATS[0];
        let topology = FlatHex::new(2, 2, false, false).unwrap();
        let mut session = Session::with_map([player], histeries_sim::MapState::blank(topology));
        let unit = UnitId::new(1);
        session
            .dispatch(Command::UnitSpawn {
                player,
                unit,
                kind: UnitKind::Settler,
                tile: TileId::new(0),
            })
            .unwrap();
        session
            .dispatch(Command::CityFound {
                player,
                unit,
                city: CityId::new(1),
            })
            .unwrap();
        let before = session.state_snapshot();
        let plan = [Command::TurnEnd { player }, Command::TurnEnd { player }];
        assert_eq!(dispatch_plan(&mut session, &before, player, &plan), Ok(0));
        assert_eq!(session.winner(), Some((player, VictoryKind::Domination)));
        assert_eq!(session.state_snapshot().turn(), 1);
    }

    #[test]
    fn losing_a_route_step_preserves_its_settler_and_unrelated_foundings() {
        let topology = FlatHex::new(4, 2, false, false).unwrap();
        let mut session = Session::with_map(SEATS, histeries_sim::MapState::blank(topology));
        let [first, second] = SEATS;
        for (player, id, kind, tile) in [
            (first, 1, UnitKind::Builder, 0),
            (second, 2, UnitKind::Settler, 2),
            (second, 3, UnitKind::Settler, 7),
        ] {
            session
                .dispatch(Command::UnitSpawn {
                    player,
                    unit: UnitId::new(id),
                    kind,
                    tile: TileId::new(tile),
                })
                .unwrap();
        }
        let original = session.state_snapshot();
        let movement = |player, unit, tile| Command::UnitMove {
            player,
            unit: UnitId::new(unit),
            destination: TileId::new(tile),
        };
        session.dispatch(movement(first, 1, 1)).unwrap();
        let settle = |unit| Command::CitySettle {
            player: second,
            unit: UnitId::new(unit),
        };
        let commands = [
            movement(second, 2, 1),
            settle(3),
            movement(second, 2, 4),
            settle(2),
        ];
        assert_eq!(
            dispatch_plan(&mut session, &original, second, &commands),
            Ok(3)
        );
        let observation = session.observe(second).unwrap();
        assert_eq!(observation.cities().len(), 1);
        assert_eq!(observation.cities()[0].tile, TileId::new(7));
        assert_eq!(observation.units().len(), 1);
        assert_eq!(observation.units()[0].tile, TileId::new(2));
    }

    #[test]
    fn timeout_is_reproducible_and_seats_start_equally() {
        let session = scenario(7).unwrap();
        for seat in SEATS {
            let o = session.observe(seat).unwrap();
            assert_eq!((o.cities().len(), o.units().len()), (1, 1));
            assert_eq!(o.units()[0].kind, UnitKind::Warrior);
        }
        for seat in 0..2 {
            let game = play(7, seat, Opponent::Random, 2, 1, &mut RoundTimes::default()).unwrap();
            assert_eq!(game.victory, None);
            assert_eq!(game.turns, 2);
            assert_eq!(
                game,
                play(7, seat, Opponent::Random, 2, 8, &mut RoundTimes::default()).unwrap()
            );
        }
    }

    #[test]
    fn default_horizon_reaches_a_simulation_victory() {
        let mut timings = RoundTimes::default();
        let game = play(7, 0, Opponent::Greedy, 500, 1, &mut timings).unwrap();
        assert!(game.victory.is_some());
        assert_eq!(u128::from(game.rounds_planned), timings.count);
    }
}
