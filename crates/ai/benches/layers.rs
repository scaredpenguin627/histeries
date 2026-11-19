//! Repeatable per-layer timings for one contested seat observation.
use histeries_ai::{VictoryObjective, operational, strategic, tactical};
use histeries_hex::{FlatHex, TileId};
use histeries_rules_core::{PlayerId, UnitId};
use histeries_sim::{Command, MapState, Session, UnitKind};
use std::{hint::black_box, time::Instant};

fn measure<T>(name: &str, mut evaluate: impl FnMut() -> T) {
    const SAMPLES: usize = 1000;
    black_box(evaluate());
    let mut timings = Vec::with_capacity(SAMPLES);
    for _ in 0..SAMPLES {
        let start = Instant::now();
        black_box(evaluate());
        timings.push(start.elapsed().as_nanos());
    }
    timings.sort_unstable();
    println!(
        "{name} samples={SAMPLES} median_ns={} p95_ns={}",
        timings[499], timings[949]
    );
}

fn main() {
    let seat = PlayerId::new(1);
    let rival = PlayerId::new(2);
    let mut session = Session::with_map(
        [seat, rival],
        MapState::blank(FlatHex::new(8, 8, false, false).unwrap()),
    );
    for (id, player, kind, location) in [
        (0, seat, UnitKind::Settler, 9),
        (1, seat, UnitKind::Archer, 18),
        (2, seat, UnitKind::Warrior, 19),
        (3, rival, UnitKind::Warrior, 20),
        (4, rival, UnitKind::Archer, 27),
    ] {
        session
            .dispatch(Command::UnitSpawn {
                player,
                unit: UnitId::new(id),
                kind,
                tile: TileId::new(location),
            })
            .unwrap();
    }
    session
        .dispatch(Command::CitySettle {
            player: seat,
            unit: UnitId::new(0),
        })
        .unwrap();
    let view = session.observe(seat).unwrap();
    measure("strategic", || {
        strategic::choose_objective(black_box(&view), Some(VictoryObjective::Science))
    });
    measure("operational", || {
        operational::plan(black_box(&view), VictoryObjective::Science, &mut Vec::new())
    });
    measure("tactical", || {
        tactical::plan(black_box(&view), &mut Vec::new())
    });
}
