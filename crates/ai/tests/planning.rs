//! Integration tests for deterministic three-layer planning.

use histeries_ai::{
    Operation, Player, ThreeLayerPlayer, VictoryObjective, operational, strategic, tactical,
};
use histeries_hex::{FlatHex, TileId, TileTopology};
use histeries_rules_core::{CityId, PlayerId, UnitId};
use histeries_sim::{Command, MapState, ProductionItem, Session, UnitKind};

fn session_with_map(players: &[PlayerId]) -> Session {
    let topology = FlatHex::new(4, 4, false, false).expect("test topology");
    Session::with_map(players.iter().copied(), MapState::blank(topology))
}

fn found_city(session: &mut Session, player: PlayerId, city: CityId, unit: UnitId, tile: TileId) {
    session
        .dispatch(Command::UnitSpawn {
            player,
            unit,
            kind: UnitKind::Settler,
            tile,
        })
        .expect("spawn settler");
    session
        .dispatch(Command::CityFound { player, city, unit })
        .expect("found city");
}

fn end_round(session: &mut Session, players: &[PlayerId]) {
    for player in players {
        session
            .dispatch(Command::TurnEnd { player: *player })
            .expect("end turn");
    }
}

#[test]
fn three_layer_plan_is_deterministic_and_dispatchable() {
    let player = PlayerId::new(1);
    let mut session = session_with_map(&[player]);
    session
        .dispatch(Command::UnitSpawn {
            player,
            unit: UnitId::new(1),
            kind: UnitKind::Warrior,
            tile: TileId::new(0),
        })
        .expect("spawn warrior");

    let observation = session.observe(player).expect("observation");
    let first = ThreeLayerPlayer::default().plan(&observation);
    let second = ThreeLayerPlayer::default().plan(&observation);

    assert_eq!(first, second);
    assert_eq!(first.objective, VictoryObjective::Domination);
    let command_ids: Vec<_> = first.commands.iter().map(Command::id).collect();
    assert_eq!(
        command_ids,
        vec![
            "research.technology",
            "research.civic",
            "unit.move",
            "turn.end"
        ]
    );
    for command in first.commands {
        session.dispatch(command).expect("planned command is legal");
    }
    assert_eq!(session.state_snapshot().turn(), 1);
}

#[test]
fn city_development_completes_prerequisites_then_building() {
    use histeries_sim::{BuildingKind, DistrictKind};

    let player = PlayerId::new(1);
    let opponent = PlayerId::new(2);
    let mut session = session_with_map(&[player, opponent]);
    // The city has no adjacent land for a new unit, but can place its
    // required research district on a visible site across the channel.
    let center = TileId::new(5);
    let topology = FlatHex::new(4, 4, false, false).unwrap();
    for tile in topology.neighbors(center) {
        session
            .dispatch(Command::EditorSetTerrain {
                player,
                tile,
                terrain: histeries_sim::Terrain::Ocean,
            })
            .unwrap();
    }
    found_city(&mut session, player, CityId::new(0), UnitId::new(0), center);
    found_city(
        &mut session,
        opponent,
        CityId::new(3),
        UnitId::new(3),
        TileId::new(15),
    );
    let mut district_queued = false;
    let mut building_queued = false;
    for turn in 0..160 {
        let obs = session.observe(player).unwrap();
        if obs.cities()[0].buildings.contains(&BuildingKind::Library) {
            assert!(district_queued && building_queued);
            return;
        }
        let mut operations = Vec::new();
        let commands = operational::plan(&obs, VictoryObjective::Science, &mut operations);
        if turn == 0 {
            assert!(operations.contains(&Operation::ResearchTechnology("tech_01".into())));
            assert!(operations.contains(&Operation::ResearchCivic("civic_01".into())));
        }
        for command in commands {
            if let Command::CitySetProduction { city, item, .. } = command
                && city == CityId::new(0)
            {
                match item {
                    ProductionItem::District {
                        kind: DistrictKind::Campus,
                        ..
                    } => district_queued = true,
                    ProductionItem::Building(BuildingKind::Library) => {
                        assert!(
                            obs.cities()[0]
                                .districts
                                .iter()
                                .any(|district| district.kind == DistrictKind::Campus)
                        );
                        building_queued = true;
                    }
                    _ => {}
                }
            }
            session
                .dispatch(command)
                .expect("valid operational command");
        }
        end_round(&mut session, &[player, opponent]);
    }
    panic!("the city failed to complete its development project");
}

#[test]
fn strategic_hysteresis_retains_the_previous_path_until_the_lead_grows() {
    let player = PlayerId::new(1);
    let opponent = PlayerId::new(2);
    let mut session = session_with_map(&[player, opponent]);
    found_city(
        &mut session,
        player,
        CityId::new(1),
        UnitId::new(1),
        TileId::new(0),
    );
    found_city(
        &mut session,
        opponent,
        CityId::new(2),
        UnitId::new(2),
        TileId::new(15),
    );
    for tile in [1, 4] {
        session
            .dispatch(Command::UnitSpawn {
                player,
                unit: UnitId::new(tile),
                kind: UnitKind::Warrior,
                tile: TileId::new(tile),
            })
            .unwrap();
    }
    let obs = session.observe(player).unwrap();
    assert_eq!(
        strategic::choose_objective(&obs, None),
        VictoryObjective::Science
    );
    assert_eq!(
        strategic::choose_objective(&obs, Some(VictoryObjective::Domination)),
        VictoryObjective::Domination
    );
    assert_eq!(
        strategic::choose_objective(&obs, Some(VictoryObjective::Science)),
        VictoryObjective::Science
    );
    session
        .dispatch(Command::ResearchTechnology {
            player,
            technology: "tech_01".into(),
        })
        .unwrap();
    for _ in 0..10 {
        end_round(&mut session, &[player, opponent]);
    }
    let obs = session.observe(player).unwrap();
    assert_eq!(
        strategic::choose_objective(&obs, Some(VictoryObjective::Domination)),
        VictoryObjective::Science
    );
}

#[test]
fn tactical_plan_attacks_adjacent_visible_enemy() {
    let player = PlayerId::new(1);
    let mut session = session_with_map(&[player]);
    let topology = session.state_snapshot().map().expect("test map").topology();
    let own_tile = TileId::new(0);
    let enemy_tile = topology
        .neighbors(own_tile)
        .into_iter()
        .next()
        .expect("neighbor");
    session
        .dispatch(Command::UnitSpawn {
            player,
            unit: UnitId::new(1),
            kind: UnitKind::Warrior,
            tile: own_tile,
        })
        .expect("spawn warrior");
    session
        .dispatch(Command::BarbarianSpawn {
            tile: enemy_tile,
            kind: UnitKind::Barbarian,
        })
        .expect("spawn barbarian");

    let observation = session.observe(player).expect("observation");
    let mut operations = Vec::new();
    let commands = tactical::plan(&observation, &mut operations);
    let attack = commands
        .iter()
        .find_map(|command| match command {
            Command::UnitAttack {
                attacker, defender, ..
            } => Some((*attacker, *defender)),
            _ => None,
        })
        .expect("adjacent enemy receives an attack order");
    assert_eq!(attack.0, UnitId::new(1));
    assert_eq!(operations, vec![Operation::Attack(attack.0, attack.1)]);
}

#[test]
fn tactical_plan_moves_healthy_unit_to_a_free_neighbor() {
    let player = PlayerId::new(1);
    let mut session = session_with_map(&[player]);
    let own_tile = TileId::new(0);
    session
        .dispatch(Command::UnitSpawn {
            player,
            unit: UnitId::new(1),
            kind: UnitKind::Warrior,
            tile: own_tile,
        })
        .expect("spawn warrior");

    let observation = session.observe(player).expect("observation");
    let mut operations = Vec::new();
    let commands = tactical::plan(&observation, &mut operations);
    let destination = commands
        .iter()
        .find_map(|command| match command {
            Command::UnitMove { destination, .. } => Some(*destination),
            _ => None,
        })
        .expect("healthy unit receives a move order");
    assert!(
        observation
            .visible_tiles()
            .iter()
            .any(|tile| tile.tile == destination && tile.neighbors.contains(&own_tile))
    );
    assert_eq!(operations, vec![Operation::UnitAction(UnitId::new(1))]);
}

#[test]
fn tactical_plan_reserves_city_tiles_and_multiple_destinations() {
    let player = PlayerId::new(1);
    let mut session = session_with_map(&[player]);
    found_city(
        &mut session,
        player,
        CityId::new(8),
        UnitId::new(8),
        TileId::new(0),
    );
    for (unit, tile) in [
        (UnitId::new(1), TileId::new(1)),
        (UnitId::new(2), TileId::new(2)),
    ] {
        session
            .dispatch(Command::UnitSpawn {
                player,
                unit,
                kind: UnitKind::Warrior,
                tile,
            })
            .expect("spawn warrior");
    }
    let observation = session.observe(player).expect("observation");
    let plan = ThreeLayerPlayer::default().plan(&observation);
    let destinations: Vec<_> = plan
        .commands
        .iter()
        .filter_map(|command| match command {
            Command::UnitMove { destination, .. } => Some(*destination),
            _ => None,
        })
        .collect();
    assert_eq!(destinations.len(), 2);
    assert_ne!(destinations[0], TileId::new(0));
    assert_ne!(destinations[1], TileId::new(0));
    assert_ne!(destinations[0], destinations[1]);
    for command in plan.commands {
        session.dispatch(command).expect("planned command is legal");
    }
}

#[test]
fn baseline_economies_settle_and_choose_dispatchable_production() {
    use histeries_rules_core::NamedRng;
    use std::collections::BTreeSet;

    let player = PlayerId::new(1);
    let mut session = session_with_map(&[player]);
    session
        .dispatch(Command::UnitSpawn {
            player,
            unit: UnitId::new(17),
            kind: UnitKind::Settler,
            tile: TileId::new(0),
        })
        .unwrap();
    let obs = session.observe(player).unwrap();
    let mut rng = NamedRng::new([9; 32], "player/1");
    for commands in [
        operational::greedy(&obs, &mut Vec::new()),
        operational::random(&obs, &mut rng, &mut Vec::new()),
        operational::plan(&obs, VictoryObjective::Science, &mut Vec::new()),
    ] {
        assert!(commands.contains(&Command::CitySettle {
            player,
            unit: UnitId::new(17)
        }));
    }
    session
        .dispatch(Command::CitySettle {
            player,
            unit: UnitId::new(17),
        })
        .unwrap();
    let obs = session.observe(player).unwrap();
    let greedy = operational::greedy(&obs, &mut Vec::new());

    assert!(greedy.iter().any(|command| matches!(
        command,
        Command::CitySetProduction {
            item: ProductionItem::Unit(UnitKind::Settler),
            ..
        }
    )));
    let mut sampled = BTreeSet::new();
    for _ in 0..32 {
        let mut copy = Session::from_state(session.state_snapshot());
        for command in operational::random(&obs, &mut rng, &mut Vec::new()) {
            if let Command::CitySetProduction { item, .. } = command {
                sampled.insert(item);
            }
            copy.dispatch(command).expect("valid sampled command");
        }
    }
    assert!(sampled.len() > 1);
}

#[test]
fn greedy_finances_a_settler_and_founds_another_city() {
    let player = PlayerId::new(1);
    let opponent = PlayerId::new(2);
    let mut session = session_with_map(&[player, opponent]);
    found_city(
        &mut session,
        player,
        CityId::new(5),
        UnitId::new(5),
        TileId::new(0),
    );
    found_city(
        &mut session,
        opponent,
        CityId::new(7),
        UnitId::new(7),
        TileId::new(15),
    );
    for _ in 0..20 {
        let obs = session.observe(player).unwrap();
        if obs.cities().len() > 1 {
            return;
        }
        for command in operational::greedy(&obs, &mut Vec::new()) {
            session.dispatch(command).expect("greedy command is legal");
        }
        end_round(&mut session, &[player, opponent]);
    }
    panic!("the greedy player failed to settle its second city");
}

#[test]
fn layered_spends_banked_science_then_queues_its_successor() {
    let player = PlayerId::new(1);
    let other = PlayerId::new(2);
    let map = MapState::blank(FlatHex::new(16, 16, false, false).unwrap());
    let mut session = Session::with_map([player, other], map);
    for id in 0..26 {
        found_city(
            &mut session,
            player,
            CityId::new(id),
            UnitId::new(id),
            TileId::new(id),
        );
    }
    found_city(
        &mut session,
        other,
        CityId::new(27),
        UnitId::new(27),
        TileId::new(27),
    );
    session
        .dispatch(Command::ResearchTechnology {
            player,
            technology: "tech_01".into(),
        })
        .unwrap();
    end_round(&mut session, &[player, other]);
    let obs = session.observe(player).unwrap();
    assert_eq!(obs.progress().technology_progress, 16);
    let commands = operational::plan(&obs, VictoryObjective::Science, &mut Vec::new());
    let tech: Vec<_> = commands
        .iter()
        .filter_map(|command| {
            if let Command::ResearchTechnology { technology, .. } = command {
                Some(technology.as_str())
            } else {
                None
            }
        })
        .collect();
    assert_eq!(tech, ["tech_02", "tech_03"]);
    for command in commands {
        session.dispatch(command).unwrap();
    }
    assert_eq!(
        session
            .observe(player)
            .unwrap()
            .progress()
            .current_technology
            .as_deref(),
        Some("tech_03")
    );
    // Keep the last expensive node unselected. A later population increase
    // puts enough points in the bank to complete it on dispatch.
    for id in 4..=24 {
        while session
            .observe(player)
            .unwrap()
            .progress()
            .current_technology
            .is_some()
        {
            end_round(&mut session, &[player, other]);
        }
        if id == 24 {
            for tile in 128..256 {
                found_city(
                    &mut session,
                    player,
                    CityId::new(tile),
                    UnitId::new(tile),
                    TileId::new(tile),
                );
            }
        }
        session
            .dispatch(Command::ResearchTechnology {
                player,
                technology: format!("tech_{id:02}"),
            })
            .unwrap();
    }
    while session
        .observe(player)
        .unwrap()
        .progress()
        .current_technology
        .is_some()
    {
        end_round(&mut session, &[player, other]);
    }
    let obs = session.observe(player).unwrap();
    assert!(obs.progress().technology_progress >= 58);
    let plan = ThreeLayerPlayer::default().plan(&obs);
    for command in plan.commands {
        if matches!(command, Command::TurnEnd { .. }) {
            assert_eq!(
                session.winner(),
                Some((player, histeries_sim::VictoryKind::Science))
            );
        }
        session.dispatch(command).unwrap();
    }
}

#[test]
fn greedy_chains_both_research_tracks_and_fills_new_policy_slots() {
    let player = PlayerId::new(1);
    let other = PlayerId::new(2);
    let mut session = Session::with_map(
        [player, other],
        MapState::blank(FlatHex::new(12, 12, false, false).unwrap()),
    );
    for tile in 0..105 {
        found_city(
            &mut session,
            player,
            CityId::new(tile),
            UnitId::new(tile),
            TileId::new(tile),
        );
    }
    found_city(
        &mut session,
        other,
        CityId::new(105),
        UnitId::new(105),
        TileId::new(105),
    );
    session
        .dispatch(Command::ResearchTechnology {
            player,
            technology: "tech_01".into(),
        })
        .unwrap();
    session
        .dispatch(Command::ResearchCivic {
            player,
            civic: "civic_01".into(),
        })
        .unwrap();
    end_round(&mut session, &[player, other]);
    let obs = session.observe(player).unwrap();
    assert_eq!(obs.progress().technology_progress, 95);
    assert_eq!(obs.progress().civic_progress, 95);
    for command in operational::greedy(&obs, &mut Vec::new()) {
        session.dispatch(command).unwrap();
    }
    let after = session.observe(player).unwrap();
    assert_eq!(after.progress().technologies.len(), 6);
    assert_eq!(after.progress().civics.len(), 6);
    assert_eq!(
        after.progress().current_technology.as_deref(),
        Some("tech_07")
    );
    assert_eq!(after.progress().current_civic.as_deref(), Some("civic_07"));
    assert_eq!(after.progress().policies.len(), 3);
    assert!(
        after
            .progress()
            .policies
            .iter()
            .any(|(_, id)| id == "policy_trade")
    );
}

#[test]
fn baselines_use_visible_district_sites_at_radius_two_and_three_without_a_spawn_site() {
    use histeries_rules_core::NamedRng;
    use histeries_sim::{BuildingKind, DistrictKind, Terrain};
    let player = PlayerId::new(1);
    let other = PlayerId::new(2);
    for radius in [2, 3] {
        let map = MapState::blank(FlatHex::new(7, 1, false, false).unwrap());
        let mut session = Session::with_map([player, other], map);
        let city = CityId::new(0);
        found_city(&mut session, player, city, UnitId::new(0), TileId::new(0));
        found_city(
            &mut session,
            other,
            CityId::new(6),
            UnitId::new(6),
            TileId::new(6),
        );
        session
            .dispatch(Command::UnitSpawn {
                player,
                unit: UnitId::new(4),
                kind: UnitKind::Warrior,
                tile: TileId::new(4),
            })
            .unwrap();
        for tile in (1..=3).filter(|tile| *tile != radius) {
            session
                .dispatch(Command::EditorSetTerrain {
                    player,
                    tile: TileId::new(tile),
                    terrain: Terrain::Ocean,
                })
                .unwrap();
        }
        for building in [BuildingKind::Granary, BuildingKind::Monument] {
            session
                .dispatch(Command::CitySetProduction {
                    player,
                    city,
                    item: ProductionItem::Building(building),
                })
                .unwrap();
            while !session.observe(player).unwrap().cities()[0]
                .buildings
                .contains(&building)
            {
                end_round(&mut session, &[player, other]);
            }
        }
        let obs = session.observe(player).unwrap();
        let target = Command::CitySetProduction {
            player,
            city,
            item: ProductionItem::District {
                kind: DistrictKind::IndustrialZone,
                tile: TileId::new(radius),
            },
        };
        assert!(operational::greedy(&obs, &mut Vec::new()).contains(&target));
        let mut rng = NamedRng::new([0; 32], "test");
        let mut chose_district = false;
        for _ in 0..32 {
            let options = operational::random(&obs, &mut rng, &mut Vec::new());
            assert!(!options.iter().any(|c| matches!(
                c,
                Command::CitySetProduction {
                    item: ProductionItem::Unit(_),
                    ..
                }
            )));
            chose_district |= options.contains(&target);
        }
        assert!(chose_district);
        session.dispatch(target).unwrap();
    }
}
