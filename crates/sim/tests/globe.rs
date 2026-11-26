//! End-to-end coverage of spherical world creation, editing, play and persistence.

use histeries_hex::{Geodesic, TileId, TileTopology};
use histeries_rules_core::{PlayerId, UnitId};
use histeries_sim::{
    BoardTopology, Command, MapError, MapState, ReplayLog, RiverDirection, ScenarioBrush,
    ScenarioDraft, Session, Terrain, UnitKind, generate_map, seeded_frontier_on,
};
use std::collections::BTreeSet;

#[test]
fn globe_records_validate_frequency_length_ids_and_pentagon_edges() {
    let topology = BoardTopology::geodesic(1).unwrap();
    let mut map = MapState::blank(topology);
    let pentagon = TileId::new(0);
    assert_eq!(
        map.set_river(pentagon, Some(RiverDirection::NorthWest)),
        Err(MapError::IncompatibleRiver(pentagon))
    );
    let bytes = map.encode();
    assert_eq!(&bytes[..15], b"MAP1\x02\0\x01\x01\0\0\0\x0c\0\0\0");
    assert_eq!(MapState::decode(&bytes).unwrap().encode(), bytes);
    let mut bad = bytes.clone();
    bad[6] = 99;
    assert_eq!(MapState::decode(&bad), Err(MapError::InvalidTag));
    for frequency in [0, Geodesic::MAX_FREQUENCY + 1, u32::MAX] {
        let mut bad = bytes.clone();
        bad[7..11].copy_from_slice(&frequency.to_le_bytes());
        assert!(matches!(MapState::decode(&bad), Err(MapError::Topology(_))));
    }
    let mut bad = bytes.clone();
    bad[11..15].copy_from_slice(&u32::MAX.to_le_bytes());
    assert_eq!(MapState::decode(&bad), Err(MapError::InvalidTileCount));
    assert_eq!(
        MapState::decode(&bytes[..bytes.len() - 1]),
        Err(MapError::InvalidFormat)
    );
    let mut bad = bytes.clone();
    bad.push(0);
    assert_eq!(MapState::decode(&bad), Err(MapError::TrailingBytes));
    let mut bad = bytes.clone();
    bad[23..27].copy_from_slice(&0u32.to_le_bytes());
    assert_eq!(MapState::decode(&bad), Err(MapError::NonCanonicalOrder));
    let mut bad = bytes;
    bad[21] = RiverDirection::NorthWest as u8;
    assert_eq!(
        MapState::decode(&bad),
        Err(MapError::IncompatibleRiver(pentagon))
    );
}

#[test]
fn generated_globe_rivers_follow_local_edges_to_coast() {
    let topology = BoardTopology::geodesic(4).unwrap();
    let mut observed = 0;
    for seed in [0, 1, 7] {
        let map = generate_map(topology.clone(), [seed; 32]);
        assert_eq!(generate_map(topology.clone(), [seed; 32]), map);
        for (mut tile, _) in map.tiles().filter(|(_, state)| state.river.is_some()) {
            observed += 1;
            let mut visited = BTreeSet::new();
            while let Some(flow) = map.tile(tile).unwrap().river {
                assert!(visited.insert(tile));
                tile = topology.edge_neighbors(tile)[flow as usize].unwrap();
            }
            assert_eq!(map.tile(tile).unwrap().terrain, Terrain::Coast);
        }
    }
    assert!(observed > 0);
}

#[test]
fn moving_across_a_pentagon_edge_advances_the_observed_unit_and_replays() {
    let topology = BoardTopology::geodesic(3).unwrap();
    let start = (0..topology.tile_count())
        .map(|id| TileId::new(id as u32))
        .find(|&tile| topology.neighbors(tile).len() == 5)
        .unwrap();
    let destination = topology.edge_neighbors(start)[4].unwrap();
    let player = PlayerId::new(1);
    let unit = UnitId::new(1);
    let mut session = Session::with_map([player, PlayerId::new(2)], MapState::blank(topology));
    session
        .dispatch(Command::UnitSpawn {
            player,
            unit,
            kind: UnitKind::Warrior,
            tile: start,
        })
        .unwrap();
    session
        .dispatch(Command::UnitMove {
            player,
            unit,
            destination,
        })
        .unwrap();
    assert_eq!(
        session.observe(player).unwrap().units()[0].tile,
        destination
    );
    assert!(
        session
            .observe(PlayerId::new(2))
            .unwrap()
            .visible_tiles()
            .is_empty()
    );
    assert_eq!(
        session.replay().replay().unwrap().state_hash(),
        session.state_hash()
    );
}

#[test]
fn generated_globe_seats_play_and_replay_the_same_position() {
    let topology = BoardTopology::geodesic(4).unwrap();
    let mut session = seeded_frontier_on(7, 8, topology.clone()).unwrap();
    let opening = session.save_bytes();
    assert_eq!(
        seeded_frontier_on(7, 8, topology.clone())
            .unwrap()
            .save_bytes(),
        opening
    );
    for seat in session.seat_ids().collect::<Vec<_>>() {
        let view = session.observe(seat).unwrap();
        assert_eq!(view.board().globe_frequency(), Some(4));
        assert_eq!(view.cities().len(), 1);
        assert!(
            view.units()
                .iter()
                .any(|unit| unit.kind == UnitKind::Warrior)
        );
        let known: BTreeSet<_> = view.visible_tiles().iter().map(|tile| tile.tile).collect();
        assert!(known.len() < topology.tile_count());
        for tile in view.visible_tiles() {
            assert_eq!(tile.surface, topology.surface(tile.tile));
            assert!(tile.neighbors.iter().all(|id| known.contains(id)));
        }
        session.dispatch(Command::TurnEnd { player: seat }).unwrap();
    }
    let restored = Session::from_save_bytes(&session.save_bytes()).unwrap();
    assert_eq!(restored.state_hash(), session.state_hash());
    assert_eq!(restored.save_bytes(), session.save_bytes());
    let replay = ReplayLog::decode(&session.replay().encode()).unwrap();
    assert_eq!(replay.replay().unwrap().state_hash(), session.state_hash());
}

#[test]
fn blank_globe_editor_history_keeps_geometry_while_players_see_only_revealed_cells() {
    let topology = BoardTopology::geodesic(2).unwrap();
    let seat = PlayerId::new(1);
    let mut draft = ScenarioDraft::blank_on(topology.clone(), 2).unwrap();
    let opening = draft.bytes().to_vec();
    let view = Session::from_save_bytes(&opening)
        .unwrap()
        .observe(seat)
        .unwrap();
    assert_eq!(view.board().globe_frequency(), Some(2));
    assert!(view.visible_tiles().is_empty());
    assert_eq!(draft.board().visible_tiles().len(), topology.tile_count());
    let tile = TileId::new(0);
    draft.apply(ScenarioBrush::City, tile, seat).unwrap();
    let edited = draft.bytes().to_vec();
    draft.travel(false).unwrap();
    assert_eq!(draft.bytes(), opening);
    draft.travel(true).unwrap();
    assert_eq!(draft.bytes(), edited);
    let reopened = ScenarioDraft::open(&edited).unwrap();
    assert_eq!(reopened.board(), draft.board());
    let view = Session::from_save_bytes(&edited)
        .unwrap()
        .observe(seat)
        .unwrap();
    assert_eq!(view.cities()[0].tile, tile);
    assert!(view.visible_tiles().len() < topology.tile_count());
    assert!(view.visible_tiles().iter().all(|t| t.surface.is_some()));
}
