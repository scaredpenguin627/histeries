//! Tactical search and projection checks through live command dispatch.

use histeries_ai::{Operation, tactical};
use histeries_hex::{FlatHex, TileId};
use histeries_rules_core::{PlayerId, UnitId};
use histeries_sim::{Command, DispatchError, MapState, PromotionKind, Session, UnitKind};

fn battlefield() -> Session {
    Session::with_map(
        [PlayerId::new(1), PlayerId::new(2)],
        MapState::blank(FlatHex::new(5, 1, false, false).unwrap()),
    )
}

fn spawn(session: &mut Session, owner: u32, id: u32, tile: u32, kind: UnitKind) {
    session
        .dispatch(Command::UnitSpawn {
            player: PlayerId::new(owner),
            unit: UnitId::new(id),
            kind,
            tile: TileId::new(tile),
        })
        .unwrap();
}

fn next_turn(session: &mut Session) {
    for seat in [1, 2] {
        session
            .dispatch(Command::TurnEnd {
                player: PlayerId::new(seat),
            })
            .unwrap();
    }
}

fn attack(session: &mut Session, owner: u32, from: u32, target: u32) {
    session
        .dispatch(Command::UnitAttack {
            player: PlayerId::new(owner),
            attacker: UnitId::new(from),
            defender: UnitId::new(target),
        })
        .unwrap();
}

#[test]
fn beam_leaves_the_shared_target_for_the_shorter_reaching_ally() {
    let player = PlayerId::new(1);
    let mut session = battlefield();
    spawn(&mut session, 1, 1, 2, UnitKind::Archer);
    spawn(&mut session, 1, 256, 1, UnitKind::Archer);
    spawn(&mut session, 2, 20, 0, UnitKind::Archer);
    spawn(&mut session, 2, 30, 4, UnitKind::Archer);
    for defender in [20, 30] {
        for _ in 0..7 {
            attack(&mut session, 1, 1, defender);
            next_turn(&mut session);
        }
    }
    let observation = session.observe(player).unwrap();
    let mut operations = Vec::new();
    let commands = tactical::plan(&observation, &mut operations);
    assert_eq!(
        operations,
        [
            Operation::Attack(UnitId::new(1), UnitId::new(30)),
            Operation::Attack(UnitId::new(256), UnitId::new(20)),
        ]
    );
    assert_ne!(commands, tactical::greedy(&observation, &mut Vec::new()));
    for command in commands {
        session.dispatch(command).unwrap();
    }
    assert!(
        session
            .observe(PlayerId::new(2))
            .unwrap()
            .units()
            .is_empty()
    );
}

#[test]
fn exhausted_contact_receives_an_authorized_replayable_hold() {
    let player = PlayerId::new(1);
    let unit = UnitId::new(7);
    let mut session = battlefield();
    spawn(&mut session, 1, 7, 0, UnitKind::Warrior);
    spawn(&mut session, 2, 8, 1, UnitKind::Warrior);
    let hold = Command::UnitHold { player, unit };
    assert_eq!(Command::decode(&hold.encode()), Ok(hold.clone()));
    assert_eq!(
        session.dispatch(Command::UnitHold {
            player: PlayerId::new(2),
            unit
        }),
        Err(DispatchError::UnauthorizedUnit(unit))
    );
    session.dispatch(hold.clone()).unwrap();
    let obs = session.observe(player).unwrap();
    assert_eq!(obs.units()[0].movement, 0);
    assert_eq!(tactical::plan(&obs, &mut Vec::new()), vec![hold.clone()]);
    session.dispatch(hold).unwrap();
    assert_eq!(
        session.replay().replay().unwrap().state_hash(),
        session.state_hash()
    );
}

#[test]
fn preview_matches_dispatch_after_shock_and_drill() {
    let mut session = battlefield();
    spawn(&mut session, 1, 7, 0, UnitKind::Warrior);
    spawn(&mut session, 2, 8, 1, UnitKind::Warrior);
    attack(&mut session, 1, 7, 8);
    attack(&mut session, 2, 8, 7);
    for (player, unit, promotion) in [(1, 7, PromotionKind::Shock), (2, 8, PromotionKind::Drill)] {
        session
            .dispatch(Command::UnitPromote {
                player: PlayerId::new(player),
                unit: UnitId::new(unit),
                promotion,
            })
            .unwrap();
    }
    next_turn(&mut session);
    let observation = session.observe(PlayerId::new(1)).unwrap();
    let [attacker, defender] = observation.visible_units() else {
        panic!("expected two visible units")
    };
    let forecast = attacker.forecast_attack(*defender, 1).unwrap();
    assert_eq!((forecast.to_defender, forecast.to_attacker), (13, 10));
    attack(&mut session, 1, 7, 8);
    let result = session.observe(PlayerId::new(1)).unwrap();
    assert_eq!(
        result.visible_units()[0].hp,
        attacker.hp - forecast.to_attacker
    );
    assert_eq!(
        result.visible_units()[1].hp,
        defender.hp - forecast.to_defender
    );
}

#[test]
fn seeded_orders_respect_hidden_units_and_enemy_zoc() {
    let player = PlayerId::new(1);
    let mut session = battlefield();
    spawn(&mut session, 1, 7, 1, UnitKind::Warrior);
    spawn(&mut session, 2, 9, 2, UnitKind::Warrior);
    let before = session.observe(player).unwrap();
    spawn(&mut session, 2, u32::MAX, 4, UnitKind::Archer);
    assert_eq!(
        session.dispatch(Command::UnitSpawn {
            player,
            unit: UnitId::new(12),
            kind: UnitKind::Warrior,
            tile: TileId::new(4),
        }),
        Err(DispatchError::Occupied(TileId::new(4)))
    );
    let after = session.observe(player).unwrap();
    assert_eq!(before, after);
    let mut attacks = 0;
    let mut holds = 0;
    for seed in 0..64 {
        let choices = tactical::random(&before, &mut Vec::new(), seed);
        assert_eq!(choices, tactical::random(&after, &mut Vec::new(), seed));
        match choices.as_slice() {
            [Command::UnitAttack { defender, .. }] if *defender == UnitId::new(9) => attacks += 1,
            [Command::UnitHold { .. }] => holds += 1,
            _ => panic!("an adjacent enemy blocks retreat and healing"),
        }
    }
    assert!(attacks > 0 && holds > 0);
    assert_eq!(
        tactical::plan(&before, &mut Vec::new()),
        tactical::plan(&after, &mut Vec::new())
    );
}

#[test]
fn later_unit_can_enter_a_tile_vacated_earlier_in_the_joint_plan() {
    use histeries_ai::{Player, ThreeLayerPlayer};
    let player = PlayerId::new(1);
    let mut session = battlefield();
    spawn(&mut session, 1, 1, 1, UnitKind::Warrior);
    spawn(&mut session, 1, 256, 0, UnitKind::Warrior);
    let observation = session.observe(player).unwrap();
    let plan = ThreeLayerPlayer::default().plan(&observation);
    for command in plan.commands {
        session.dispatch(command).unwrap();
    }
    let result = session.observe(player).unwrap();
    assert_eq!(result.units()[0].tile, TileId::new(2));
    assert_eq!(result.units()[1].tile, TileId::new(1));
}

#[test]
fn tactical_search_targets_military_in_either_slot_order() {
    for (military, civilian) in [(5, 6), (6, 5)] {
        let mut session = battlefield();
        spawn(&mut session, 1, 1, 0, UnitKind::Archer);
        spawn(&mut session, 1, 2, 0, UnitKind::Builder);
        spawn(&mut session, 2, military, 2, UnitKind::Archer);
        spawn(&mut session, 2, civilian, 2, UnitKind::Settler);
        let player = PlayerId::new(1);
        let command = Command::UnitAttack {
            player,
            attacker: UnitId::new(1),
            defender: UnitId::new(military),
        };
        let planned = tactical::plan(&session.observe(player).unwrap(), &mut Vec::new());
        assert_eq!(planned, [command]);
        session.dispatch(planned[0].clone()).unwrap();
        let observation = session.observe(player).unwrap();
        let target = observation
            .visible_units()
            .iter()
            .find(|unit| unit.id == UnitId::new(civilian))
            .unwrap();
        assert_eq!(target.hp, UnitKind::Settler.max_hp());
    }
}

#[test]
fn archer_uses_a_scouts_sight_to_step_into_range_and_fire() {
    use histeries_ai::{Player, ThreeLayerV1Player};
    let seat = PlayerId::new(1);
    let mut session = battlefield();
    spawn(&mut session, 1, 1, 0, UnitKind::Archer);
    spawn(&mut session, 1, 2, 2, UnitKind::Builder);
    spawn(&mut session, 2, 3, 3, UnitKind::Warrior);
    let obs = session.observe(seat).unwrap();
    let attack = Command::UnitAttack {
        player: seat,
        attacker: UnitId::new(1),
        defender: UnitId::new(3),
    };
    assert!(
        !ThreeLayerV1Player::default()
            .plan(&obs)
            .commands
            .contains(&attack)
    );
    let commands = tactical::plan(&obs, &mut Vec::new());
    assert_eq!(
        commands,
        [
            Command::UnitMove {
                player: seat,
                unit: UnitId::new(1),
                destination: TileId::new(1)
            },
            attack
        ]
    );
    for order in commands {
        session.dispatch(order).unwrap();
    }
    assert!(session.observe(PlayerId::new(2)).unwrap().units()[0].hp < UnitKind::Warrior.max_hp());
}

#[test]
fn exposed_city_outweighs_incumbent_science_and_a_guard_removes_the_opportunity() {
    use histeries_ai::{ThreeLayerPlayer, VictoryObjective, strategic};
    let player = PlayerId::new(1);
    let mut session = battlefield();
    for (seat, id, tile) in [(1, 0, 0), (2, 3, 3)] {
        spawn(&mut session, seat, id, tile, UnitKind::Settler);
        session
            .dispatch(Command::CitySettle {
                player: PlayerId::new(seat),
                unit: UnitId::new(id),
            })
            .unwrap();
    }
    spawn(&mut session, 1, 2, 2, UnitKind::Warrior);
    let incumbent = VictoryObjective::Science;
    let seen = session.observe(player).unwrap();
    assert_eq!(
        ThreeLayerPlayer::resume(player, incumbent)
            .plan_v1(&seen)
            .objective,
        incumbent
    );
    assert_eq!(
        strategic::choose_objective(&seen, Some(incumbent)),
        VictoryObjective::Domination
    );
    spawn(&mut session, 2, 4, 4, UnitKind::Archer);
    assert_eq!(
        strategic::choose_objective(&session.observe(player).unwrap(), Some(incumbent)),
        incumbent
    );
}

#[test]
fn ranged_support_can_clear_a_perimeter_for_lower_id_melee_in_one_plan() {
    use histeries_ai::{ThreeLayerPlayer, VictoryObjective};
    let player = PlayerId::new(1);
    let rival = PlayerId::new(2);
    let mut session = Session::with_map(
        [player, rival],
        MapState::blank(FlatHex::new(7, 1, false, false).unwrap()),
    );
    for (seat, id, tile) in [(1, 10, 0), (2, 20, 3)] {
        spawn(&mut session, seat, id, tile, UnitKind::Settler);
        session
            .dispatch(Command::CitySettle {
                player: PlayerId::new(seat),
                unit: UnitId::new(id),
            })
            .unwrap();
    }
    spawn(&mut session, 1, 1, 2, UnitKind::Warrior);
    spawn(&mut session, 1, 2, 6, UnitKind::Archer);
    spawn(&mut session, 2, 3, 4, UnitKind::Archer);
    for _ in 0..7 {
        attack(&mut session, 1, 2, 3);
        next_turn(&mut session);
    }
    let observation = session.observe(player).unwrap();
    let v1 = ThreeLayerPlayer::resume(player, VictoryObjective::Domination).plan_v1(&observation);
    assert!(
        !v1.commands
            .iter()
            .any(|c| matches!(c, Command::UnitCaptureCity { .. }))
    );
    let commands = tactical::plan(&observation, &mut Vec::new());
    assert!(
        matches!(&commands[..], [Command::UnitAttack { attacker, .. }, Command::UnitCaptureCity { unit, .. }]
        if *attacker == UnitId::new(2) && *unit == UnitId::new(1))
    );
    for order in commands {
        session.dispatch(order).unwrap();
    }
    assert_eq!(
        session.winner(),
        Some((player, histeries_sim::VictoryKind::Domination))
    );
}

#[test]
fn archers_move_to_both_shores_and_fire_across_water() {
    let seat = PlayerId::new(1);
    let rival = PlayerId::new(2);
    let mut session = Session::with_map(
        [seat, rival],
        MapState::blank(FlatHex::new(7, 1, false, false).unwrap()),
    );
    for tile in [2, 4] {
        session
            .dispatch(Command::EditorSetTerrain {
                player: seat,
                tile: TileId::new(tile),
                terrain: histeries_sim::Terrain::Ocean,
            })
            .unwrap();
    }
    spawn(&mut session, 1, 1, 0, UnitKind::Archer);
    spawn(&mut session, 1, 2, 6, UnitKind::Archer);
    spawn(&mut session, 1, 3, 1, UnitKind::Builder);
    spawn(&mut session, 1, 4, 5, UnitKind::Builder);
    spawn(&mut session, 2, 5, 3, UnitKind::Warrior);
    let observation = session.observe(seat).unwrap();
    let commands = tactical::plan(&observation, &mut Vec::new());
    let mut expected = Vec::new();
    for (id, tile) in [(1, 1), (2, 5)] {
        expected.push(Command::UnitMove {
            player: seat,
            unit: UnitId::new(id),
            destination: TileId::new(tile),
        });
        expected.push(Command::UnitAttack {
            player: seat,
            attacker: UnitId::new(id),
            defender: UnitId::new(5),
        });
    }
    assert_eq!(commands, expected);
    for command in commands {
        session.dispatch(command).unwrap();
    }
    assert_eq!(session.observe(rival).unwrap().units()[0].hp, 90);
}
