//! Seat-local planning memory and canonical ordering across native worker counts.
use histeries_ai::{Player, RandomPlayer, ThreeLayerPlayer, plan_seats};
use histeries_hex::{FlatHex, TileId};
use histeries_rules_core::{PlayerId, UnitId};
use histeries_sim::{Command, MapState, Session, UnitKind};

#[test]
fn independent_seats_plan_the_same_round_across_worker_counts() {
    let seats = [
        PlayerId::new(4),
        PlayerId::new(1),
        PlayerId::new(7),
        PlayerId::new(2),
    ];
    let map = MapState::blank(FlatHex::new(16, 2, false, false).unwrap());
    let mut session = Session::with_map(seats, map);
    for (index, &seat) in seats.iter().enumerate() {
        session
            .dispatch(Command::UnitSpawn {
                player: seat,
                unit: UnitId::new(seat.index()),
                kind: UnitKind::Settler,
                tile: TileId::new(index as u32 * 4),
            })
            .unwrap();
    }
    let observations = seats.map(|seat| session.observe(seat).unwrap());
    let plan = |workers| {
        let mut players: [ThreeLayerPlayer; 4] = Default::default();
        let mut inputs: Vec<_> = players
            .iter_mut()
            .zip(observations.clone())
            .map(|(player, obs)| (player as &mut (dyn Player + Send), obs))
            .collect();
        plan_seats(&mut inputs, workers)
    };
    let serial = plan(1);
    let parallel = plan(8);
    assert_eq!(serial, parallel);
    let mut sorted = seats;
    sorted.sort();
    for (plan, player) in serial.iter().zip(sorted) {
        assert_eq!(plan.commands.last(), Some(&Command::TurnEnd { player }));
    }
    let mut copy = Session::from_state(session.state_snapshot());
    for (plans, game) in [(serial, &mut session), (parallel, &mut copy)] {
        for plan in plans {
            for command in plan.commands {
                game.dispatch(command).unwrap();
            }
        }
    }
    assert_eq!(session.state_hash(), copy.state_hash());
}

#[test]
fn resuming_controller_memory_reproduces_the_next_plan() {
    let seat = PlayerId::new(3);
    let mut session = Session::with_map(
        [seat],
        MapState::blank(FlatHex::new(5, 5, false, false).unwrap()),
    );
    session
        .dispatch(Command::UnitSpawn {
            player: seat,
            unit: UnitId::new(7),
            kind: UnitKind::Archer,
            tile: TileId::new(2),
        })
        .unwrap();
    let observation = session.observe(seat).unwrap();
    let mut player = ThreeLayerPlayer::default();
    let first = player.plan(&observation);
    for command in first.commands {
        session.dispatch(command).unwrap();
    }
    let next = session.observe(seat).unwrap();
    assert_eq!(
        player.plan(&next),
        ThreeLayerPlayer::resume(seat, first.objective).plan(&next)
    );
    assert_eq!(
        RandomPlayer::new(123).plan(&next),
        RandomPlayer::new(123).plan(&next)
    );
}
