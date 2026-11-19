//! Command-level conquest, including a guarded target and a terminal replay.
use super::*;
use histeries_hex::FlatHex;

#[test]
fn taking_the_last_rival_city_is_a_playable_domination_victory() {
    let first = PlayerId::new(1);
    let second = PlayerId::new(2);
    let city = CityId::new(2);
    let attacker = UnitId::new(8);
    let defender = UnitId::new(9);
    let mut session = Session::with_map(
        [first, second],
        MapState::blank(FlatHex::new(6, 1, false, false).unwrap()),
    );
    for (player, pos) in [(first, 0), (second, 3)] {
        let founder = UnitId::new(pos);
        session
            .dispatch(Command::UnitSpawn {
                player,
                unit: founder,
                tile: TileId::new(pos),
                kind: UnitKind::Settler,
            })
            .unwrap();
        session
            .dispatch(Command::CityFound {
                player,
                unit: founder,
                city: CityId::new(player.index()),
            })
            .unwrap();
    }
    for (player, id, pos) in [(first, attacker, 2), (second, defender, 4)] {
        session
            .dispatch(Command::UnitSpawn {
                player,
                unit: id,
                tile: TileId::new(pos),
                kind: UnitKind::Warrior,
            })
            .unwrap();
    }
    let capture = Command::UnitCaptureCity {
        player: first,
        unit: attacker,
        city,
    };
    let before = session.state_hash();
    assert!(session.dispatch(capture.clone()).is_err());
    assert_eq!(session.state_hash(), before);
    session
        .dispatch(Command::UnitMove {
            player: second,
            unit: defender,
            destination: TileId::new(5),
        })
        .unwrap();
    let survivor = UnitId::new(10);
    let rival_attacker = UnitId::new(11);
    for (unit, kind, tile) in [
        (survivor, UnitKind::Settler, 4),
        (rival_attacker, UnitKind::Warrior, 1),
    ] {
        session
            .dispatch(Command::UnitSpawn {
                player: second,
                unit,
                kind,
                tile: TileId::new(tile),
            })
            .unwrap();
    }
    assert_eq!(Command::decode(&capture.encode()), Ok(capture.clone()));
    session.dispatch(capture).unwrap();
    assert_eq!(session.winner(), Some((first, VictoryKind::Domination)));
    assert_eq!(session.observe(first).unwrap().cities().len(), 2);
    assert!(session.observe(second).unwrap().cities().is_empty());
    let snapshot = session.state_snapshot();
    let replay = session.replay().clone();
    let events = session.events().to_vec();
    for command in [
        Command::CitySettle {
            player: second,
            unit: survivor,
        },
        Command::CityFound {
            player: second,
            city: CityId::new(3),
            unit: survivor,
        },
        Command::UnitCaptureCity {
            player: second,
            unit: rival_attacker,
            city: CityId::new(first.index()),
        },
    ] {
        assert_eq!(
            session.dispatch(command),
            Err(DispatchError::VictoryAlreadyReached)
        );
        assert_eq!(session.state_snapshot(), snapshot);
        assert_eq!(session.replay(), &replay);
        assert_eq!(session.events(), events);
    }
    assert_eq!(
        SimulationState::decode(&session.state_snapshot().encode())
            .unwrap()
            .state_hash(),
        session.state_hash()
    );
    assert_eq!(
        ReplayLog::decode(&session.replay().encode())
            .unwrap()
            .replay()
            .unwrap()
            .state_hash(),
        session.state_hash()
    );
}

#[test]
fn finishing_another_seats_science_after_victory_preserves_the_first_winner() {
    let first = PlayerId::new(1);
    let second = PlayerId::new(2);
    let map = MapState::blank(FlatHex::new(2, 1, false, false).unwrap());
    let mut checkpoint = SimulationState::with_map([first, second], map);
    let ruleset = city::base_ruleset();
    for (owner, pos) in [(first, 0), (second, 1)] {
        let id = CityId::new(pos);
        checkpoint
            .cities
            .insert(id, City::new(id, owner, TileId::new(pos)));
        checkpoint.progress.get_mut(&owner).unwrap().technologies = ruleset
            .technologies()
            .map(|node| node.id().to_owned())
            .collect();
    }
    checkpoint.victories.insert(first, VictoryKind::Science);
    let final_id = "tech_25";
    let progress = checkpoint.progress.get_mut(&second).unwrap();
    assert!(progress.technologies.remove(final_id));
    progress.current_technology = Some(final_id.to_owned());
    progress.technology_progress = i64::from(ruleset.technology(final_id).unwrap().cost()) - 1;
    let mut session = Session::from_state(SimulationState::decode(&checkpoint.encode()).unwrap());
    for player in [second, first] {
        session.dispatch(Command::TurnEnd { player }).unwrap();
    }
    let result = session.state_snapshot();
    assert!(result.progress[&second].has_technology(final_id));
    assert_eq!(result.victories, checkpoint.victories);
    assert_eq!(SimulationState::decode(&result.encode()).unwrap(), result);
    assert_eq!(
        ReplayLog::decode(&session.replay().encode())
            .unwrap()
            .replay()
            .unwrap(),
        result
    );
}

#[test]
fn version_four_replays_keep_the_founding_rule_after_domination() {
    let first = PlayerId::new(1);
    let second = PlayerId::new(2);
    let mut legacy = Session::with_map(
        [first, second],
        MapState::blank(FlatHex::new(2, 1, false, false).unwrap()),
    );
    legacy.replay.rules = ReplayRules::LegacyV4;
    for (player, pos) in [(first, 0), (second, 1)] {
        legacy
            .dispatch(Command::UnitSpawn {
                player,
                unit: UnitId::new(pos),
                kind: UnitKind::Settler,
                tile: TileId::new(pos),
            })
            .unwrap();
    }
    legacy
        .dispatch(Command::CitySettle {
            player: first,
            unit: UnitId::new(0),
        })
        .unwrap();
    for player in [first, second] {
        legacy.dispatch(Command::TurnEnd { player }).unwrap();
    }
    assert_eq!(legacy.winner(), Some((first, VictoryKind::Domination)));
    legacy
        .dispatch(Command::CitySettle {
            player: second,
            unit: UnitId::new(1),
        })
        .unwrap();
    assert_eq!(legacy.observe(second).unwrap().cities().len(), 1);
    let original = legacy.replay().encode();
    let decoded = ReplayLog::decode(&original).unwrap();
    assert_eq!(decoded.encode(), original);
    assert_eq!(decoded.replay().unwrap(), legacy.state_snapshot());
}

#[test]
fn neutral_raider_closes_and_fights_through_the_round_transition() {
    let seat = PlayerId::new(1);
    let mut game = Session::with_map(
        [seat],
        MapState::blank(FlatHex::new(4, 1, false, false).unwrap()),
    );
    game.dispatch(Command::UnitSpawn {
        player: seat,
        unit: UnitId::new(1),
        kind: UnitKind::Warrior,
        tile: TileId::new(0),
    })
    .unwrap();
    game.dispatch(Command::BarbarianSpawn {
        kind: UnitKind::Barbarian,
        tile: TileId::new(2),
    })
    .unwrap();
    game.dispatch(Command::TurnEnd { player: seat }).unwrap();
    assert!(
        game.observe(seat)
            .unwrap()
            .visible_units()
            .iter()
            .any(|u| u.owner.is_none() && u.tile == TileId::new(1))
    );
    game.dispatch(Command::TurnEnd { player: seat }).unwrap();
    assert!(game.observe(seat).unwrap().units()[0].hp < UnitKind::Warrior.max_hp());
    assert_eq!(
        game.replay().replay().unwrap().state_hash(),
        game.state_hash()
    );
}
