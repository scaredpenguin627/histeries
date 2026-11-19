//! End-to-end queue, construction-site and yield invariants.

use super::*;
use histeries_hex::FlatHex;

const OWNER: PlayerId = PlayerId::new(1);
const OTHER: PlayerId = PlayerId::new(2);
const CITY: CityId = CityId::new(1);
const RIVAL: CityId = CityId::new(2);

fn setup() -> Session {
    let map = MapState::blank(FlatHex::new(8, 3, false, false).unwrap());
    let mut session = Session::with_map([OWNER, OTHER], map);
    for (player, city, site) in [(OWNER, CITY, 9), (OTHER, RIVAL, 13)] {
        let unit = UnitId::new(site);
        session
            .dispatch(Command::UnitSpawn {
                player,
                unit,
                kind: UnitKind::Settler,
                tile: TileId::new(site),
            })
            .unwrap();
        session
            .dispatch(Command::CityFound { player, city, unit })
            .unwrap();
    }
    session
}

fn enqueue(item: ProductionItem) -> Command {
    Command::CityEnqueueProduction {
        player: OWNER,
        city: CITY,
        item,
    }
}

fn campus(site: u32) -> ProductionItem {
    ProductionItem::District {
        kind: DistrictKind::Campus,
        tile: TileId::new(site),
    }
}

fn round(session: &mut Session) {
    for player in [OWNER, OTHER] {
        session.dispatch(Command::TurnEnd { player }).unwrap();
    }
}

#[test]
fn fifo_orders_keep_progress_and_chain_district_building_unit_across_reload_and_replay() {
    let mut game = setup();
    let library = ProductionItem::Building(BuildingKind::Library);
    let warrior = ProductionItem::Unit(UnitKind::Warrior);
    assert!(matches!(
        game.dispatch(enqueue(library)),
        Err(DispatchError::MissingDistrict(_))
    ));
    let command = enqueue(campus(10));
    assert_eq!(Command::decode(&command.encode()), Ok(command.clone()));
    game.dispatch(command).unwrap();
    round(&mut game);
    let before_queue = game.state_hash();
    game.dispatch(enqueue(library)).unwrap();
    game.dispatch(enqueue(warrior)).unwrap();
    assert_ne!(game.state_hash(), before_queue);
    let observed = game.observe(OWNER).unwrap();
    let city = &observed.cities()[0];
    assert_eq!(city.production_stored, 1);
    assert_eq!(city.production, Some(campus(10)));
    assert_eq!(city.waiting, [library, warrior]);
    assert!(game.dispatch(enqueue(library)).is_err());
    let mut resumed =
        Session::from_state(SimulationState::decode(&game.state_snapshot().encode()).unwrap());
    let mut completed = Vec::new();
    for _ in 0..150 {
        let active = game.observe(OWNER).unwrap().cities()[0].production.unwrap();
        round(&mut game);
        round(&mut resumed);
        assert_eq!(game.state_hash(), resumed.state_hash());
        let next = game.observe(OWNER).unwrap().cities()[0].production;
        if Some(active) != next {
            completed.push(active);
        }
        if next.is_none() {
            break;
        }
    }
    assert_eq!(completed, [campus(10), library, warrior]);
    let end = game.observe(OWNER).unwrap();
    assert_eq!(end.cities()[0].buildings, [BuildingKind::Library]);
    assert_eq!(end.units()[0].kind, UnitKind::Warrior);
    assert_eq!(
        ReplayLog::decode(&game.replay().encode())
            .unwrap()
            .replay()
            .unwrap()
            .state_hash(),
        game.state_hash()
    );
}

#[test]
fn reservations_check_the_whole_queue_and_replacement_releases_sites() {
    let mut game = setup();
    let warrior = ProductionItem::Unit(UnitKind::Warrior);
    game.dispatch(enqueue(warrior)).unwrap();
    let site = TileId::new(11);
    game.dispatch(enqueue(campus(11))).unwrap();
    let rival_order = Command::CityEnqueueProduction {
        player: OTHER,
        city: RIVAL,
        item: campus(11),
    };
    assert_eq!(
        game.dispatch(rival_order.clone()),
        Err(DispatchError::Occupied(site))
    );
    assert!(
        game.observe(OWNER)
            .unwrap()
            .visible_tiles()
            .iter()
            .find(|t| t.tile == site)
            .unwrap()
            .district_reserved
    );
    assert!(
        game.dispatch(Command::EditorSetTerrain {
            player: OWNER,
            tile: site,
            terrain: Terrain::Ocean
        })
        .is_err()
    );
    for _ in 2..city::MAX_PRODUCTION_ORDERS {
        game.dispatch(enqueue(warrior)).unwrap();
    }
    let before = game.state_hash();
    assert_eq!(
        game.dispatch(enqueue(warrior)),
        Err(DispatchError::ProductionQueueFull)
    );
    assert_eq!(game.state_hash(), before);
    let mut corrupt = game.state_snapshot();
    corrupt.cities.get_mut(&RIVAL).unwrap().production = Some(campus(11));
    assert_eq!(
        SimulationState::decode(&corrupt.encode()),
        Err(SerializationError::InvalidCityState)
    );
    let mut no_head = game.state_snapshot();
    no_head.cities.get_mut(&CITY).unwrap().production = None;
    assert_eq!(
        SimulationState::decode(&no_head.encode()),
        Err(SerializationError::InvalidCityState)
    );
    let mut future_dependency = game.state_snapshot();
    future_dependency.cities.get_mut(&CITY).unwrap().production =
        Some(ProductionItem::Building(BuildingKind::Library));
    assert_eq!(
        SimulationState::decode(&future_dependency.encode()),
        Err(SerializationError::InvalidCityState)
    );
    round(&mut game);
    game.dispatch(Command::CitySetProduction {
        player: OWNER,
        city: CITY,
        item: warrior,
    })
    .unwrap();
    let observed = game.observe(OWNER).unwrap();
    assert_eq!(observed.cities()[0].production_stored, 0);
    assert!(observed.cities()[0].waiting.is_empty());
    game.dispatch(rival_order).unwrap();
}

#[test]
fn constructed_district_adjacency_changes_real_turn_income_and_v4_replays_keep_legacy_yields() {
    let mut near = setup();
    let mut far = setup();
    near.dispatch(enqueue(campus(10))).unwrap();
    far.dispatch(enqueue(campus(11))).unwrap();
    while near.observe(OWNER).unwrap().cities()[0]
        .production
        .is_some()
    {
        round(&mut near);
        round(&mut far);
    }
    let initial = near.state_snapshot();
    let old_snapshot = initial.encode_version(4);
    let restored = SimulationState::decode(&old_snapshot).unwrap();
    assert_eq!(restored.state_hash(), initial.state_hash());
    let old = ReplayLog {
        rules: ReplayRules::LegacyV4,
        initial: restored,
        commands: vec![
            Command::TurnEnd { player: OWNER },
            Command::TurnEnd { player: OTHER },
        ],
    };
    let old_bytes = old.encode();
    let decoded = ReplayLog::decode(&old_bytes).unwrap();
    assert_eq!(decoded.encode(), old_bytes);
    let historical = decoded.replay().unwrap();
    round(&mut near);
    round(&mut far);
    let actual = near.observe(OWNER).unwrap().cities()[0].production_stored;
    assert_eq!(
        actual,
        far.observe(OWNER).unwrap().cities()[0].production_stored + 1
    );
    assert_eq!(actual, historical.cities[&CITY].production_stored + 1);
}

#[test]
fn blocked_front_order_keeps_its_successor_and_accumulated_progress() {
    let mut game = setup();
    let first = campus(10);
    let next = ProductionItem::Building(BuildingKind::Library);
    game.dispatch(enqueue(first)).unwrap();
    game.dispatch(enqueue(next)).unwrap();
    let unit = UnitId::new(99);
    game.dispatch(Command::UnitSpawn {
        player: OWNER,
        unit,
        kind: UnitKind::Warrior,
        tile: TileId::new(10),
    })
    .unwrap();
    for _ in 0..80 {
        round(&mut game);
    }
    let snapshot = game.state_snapshot();
    assert_eq!(snapshot.cities[&CITY].production, Some(first));
    assert_eq!(snapshot.cities[&CITY].waiting, [next]);
    assert!(snapshot.cities[&CITY].production_stored >= first.cost());
    let mut game = Session::from_state(SimulationState::decode(&snapshot.encode()).unwrap());
    game.dispatch(Command::UnitMove {
        player: OWNER,
        unit,
        destination: TileId::new(11),
    })
    .unwrap();
    round(&mut game);
    let promoted = game.state_snapshot();
    assert_eq!(promoted.cities[&CITY].production, Some(next));
    assert!(promoted.cities[&CITY].waiting.is_empty());
    assert!(promoted.cities[&CITY].production_stored > 0);
    assert_eq!(
        SimulationState::decode(&promoted.encode()).unwrap(),
        promoted
    );
}

#[test]
fn pre_continent_state_v4_keeps_its_original_tiles_and_hash() {
    // Captured from the v4 CLI: new --players 2 --width 2 --height 1 --seed 4.
    let bytes = include_bytes!("../tests/data/state-v4.hst");
    let loaded = SimulationState::decode(bytes).unwrap();
    assert_eq!(loaded.state_hash().value(), 3_584_599_679_396_369_706);
    assert_eq!(loaded.encode_version(4), bytes);
    assert_eq!(SimulationState::decode(&loaded.encode()).unwrap(), loaded);
}
