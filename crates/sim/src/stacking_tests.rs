//! Mixed-stack invariants at the command, observation and persistence boundaries.

use super::{
    Command, DispatchError, MapState, ProductionItem, ReplayLog, SerializationError, Session,
    SimulationState, UnitKind, UnitSlots,
};
use histeries_hex::{FlatHex, TileId};
use histeries_rules_core::{CityId, PlayerId, UnitId};

const BLUE: PlayerId = PlayerId::new(1);
const RED: PlayerId = PlayerId::new(2);

fn session() -> Session {
    Session::with_map(
        [BLUE, RED],
        MapState::blank(FlatHex::new(8, 1, false, false).unwrap()),
    )
}

fn spawn(player: PlayerId, id: u32, kind: UnitKind, tile: u32) -> Command {
    Command::UnitSpawn {
        player,
        unit: UnitId::new(id),
        kind,
        tile: TileId::new(tile),
    }
}

#[test]
fn friendly_slots_share_a_tile_and_survive_save_replay_and_fog_projection() {
    let mut game = session();
    let tile = TileId::new(1);
    for command in [
        spawn(BLUE, 4, UnitKind::Settler, 0),
        spawn(BLUE, 2, UnitKind::Warrior, 1),
        spawn(RED, 8, UnitKind::Warrior, 7),
        Command::UnitMove {
            player: BLUE,
            unit: UnitId::new(4),
            destination: tile,
        },
    ] {
        game.dispatch(command).unwrap();
    }
    let observation = game.observe(BLUE).unwrap();
    let seen = observation
        .visible_tiles()
        .iter()
        .find(|value| value.tile == tile)
        .unwrap();
    assert_eq!(
        seen.units,
        UnitSlots {
            owner: Some(BLUE),
            military: Some(UnitId::new(2)),
            civilian: Some(UnitId::new(4))
        }
    );
    assert_eq!(
        observation
            .visible_units()
            .iter()
            .map(|u| u.id)
            .collect::<Vec<_>>(),
        [UnitId::new(2), UnitId::new(4)]
    );
    let rival = game.observe(RED).unwrap();
    assert!(
        rival
            .visible_units()
            .iter()
            .all(|unit| unit.owner == Some(RED))
    );
    assert!(rival.visible_tiles().iter().all(|entry| entry.tile != tile));

    let original = game.state_snapshot();
    for command in [
        spawn(BLUE, 5, UnitKind::Archer, 1),
        spawn(BLUE, 5, UnitKind::Builder, 1),
        spawn(RED, 5, UnitKind::Builder, 1),
        Command::BarbarianSpawn {
            tile,
            kind: UnitKind::Barbarian,
        },
        Command::CitySettle {
            player: BLUE,
            unit: UnitId::new(4),
        },
    ] {
        assert_eq!(game.dispatch(command), Err(DispatchError::Occupied(tile)));
    }
    assert_eq!(game.state_snapshot(), original);
    let restored = SimulationState::decode(&original.encode()).unwrap();
    assert_eq!(restored, original);
    assert_eq!(
        ReplayLog::decode(&game.replay().encode())
            .unwrap()
            .replay()
            .unwrap(),
        restored
    );

    // Reject same-class and hostile stacks even when their save records are otherwise valid.
    for (owner, kind) in [(BLUE, UnitKind::Archer), (RED, UnitKind::Settler)] {
        let mut invalid = original.clone();
        let settler = invalid.units.get_mut(&UnitId::new(4)).unwrap();
        settler.kind = kind;
        settler.owner = Some(owner);
        assert_eq!(
            SimulationState::decode(&invalid.encode()),
            Err(SerializationError::InvalidUnitState)
        );
    }
    game.dispatch(Command::UnitMove {
        player: BLUE,
        unit: UnitId::new(2),
        destination: TileId::new(2),
    })
    .unwrap();
    game.dispatch(Command::CitySettle {
        player: BLUE,
        unit: UnitId::new(4),
    })
    .unwrap();
    assert_eq!(game.observe(BLUE).unwrap().cities()[0].tile, tile);
}

#[test]
fn historical_replays_keep_single_unit_production_and_new_games_fill_free_slot() {
    let mut game = session();
    for command in [
        spawn(BLUE, 0, UnitKind::Settler, 0),
        Command::CityFound {
            player: BLUE,
            unit: UnitId::new(0),
            city: CityId::new(0),
        },
        spawn(BLUE, 1, UnitKind::Builder, 1),
        Command::CitySetProduction {
            player: BLUE,
            city: CityId::new(0),
            item: ProductionItem::Unit(UnitKind::Warrior),
        },
    ] {
        game.dispatch(command).unwrap();
    }
    let cost = ProductionItem::Unit(UnitKind::Warrior).cost();
    for _ in 0..cost {
        for player in [BLUE, RED] {
            game.dispatch(Command::TurnEnd { player }).unwrap();
        }
    }
    let bytes = game.replay().encode();
    assert_eq!(
        ReplayLog::decode(&bytes).unwrap().replay().unwrap(),
        game.state_snapshot()
    );
    assert!(game.observe(BLUE).unwrap().cities()[0].production.is_none());
    assert_eq!(game.observe(BLUE).unwrap().units().len(), 2);

    let mut historical = bytes;
    historical[4..6].copy_from_slice(&4u16.to_le_bytes());
    historical[14..16].copy_from_slice(&4u16.to_le_bytes());
    let log = ReplayLog::decode(&historical).unwrap();
    assert_eq!(log.encode(), historical);
    let mut playback = log.playback_session();
    for command in log.commands() {
        playback.dispatch(command.clone()).unwrap();
    }
    assert_eq!(log.replay().unwrap(), playback.state_snapshot());
    let future_order = Command::CityEnqueueProduction {
        player: BLUE,
        city: CityId::new(0),
        item: ProductionItem::Unit(UnitKind::Archer),
    };
    assert_eq!(
        playback.dispatch(future_order.clone()),
        Err(DispatchError::UnregisteredCommand(future_order.id()))
    );
    let old = playback.observe(BLUE).unwrap();
    assert_eq!(old.units().len(), 1);
    assert_eq!(
        old.cities()[0].production,
        Some(ProductionItem::Unit(UnitKind::Warrior))
    );
    drop(playback.take_replay_segment());
    assert_eq!(&playback.replay().encode()[4..6], &4u16.to_le_bytes());
}

#[test]
fn only_the_other_friendly_class_may_join_a_single_occupant() {
    for resident in [UnitKind::Warrior, UnitKind::Builder] {
        for candidate in [UnitKind::Archer, UnitKind::Settler] {
            for player in [BLUE, RED] {
                let mut game = session();
                let tile = TileId::new(1);
                game.dispatch(spawn(BLUE, 1, resident, 1)).unwrap();
                game.dispatch(spawn(player, 2, candidate, 2)).unwrap();
                let observed = game.observe(player).unwrap();
                let can_enter = player == BLUE && resident.is_military() != candidate.is_military();
                let destination = observed
                    .visible_tiles()
                    .iter()
                    .find(|entry| entry.tile == tile)
                    .unwrap();
                assert_eq!(
                    destination.can_enter_unit(Some(player), candidate),
                    can_enter
                );
                let mut spawn_game = game.clone();
                assert_eq!(
                    spawn_game.dispatch(spawn(player, 3, candidate, 1)).is_ok(),
                    can_enter
                );
                let moved = game.dispatch(Command::UnitMove {
                    player,
                    unit: UnitId::new(2),
                    destination: tile,
                });
                assert_eq!(moved.is_ok(), can_enter);
            }
        }
    }
}
