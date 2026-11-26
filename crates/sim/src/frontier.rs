//! Opening positions for a playable generated map, without editing its terrain.
use std::collections::{BTreeMap, BTreeSet};

use petgraph::{algo::maximum_matching, graph::UnGraph};

use crate::{BoardTopology, Command, MapState, Session, Terrain, Unit, UnitKind};
use histeries_hex::{FlatHex, TileId, TileTopology};
use histeries_rules_core::{PlayerId, UnitId};

/// Opens the same generated 12 × 9 frontier for a shared-screen or network match.
pub fn seeded_frontier(seed: u64, players: u32) -> Result<Session, String> {
    let topology = FlatHex::new(12, 9, false, false).map_err(|error| error.to_string())?;
    seeded_frontier_on(seed, players, topology)
}

/// Opens a generated multiplayer frontier on the selected graph.
pub fn seeded_frontier_on(
    seed: u64,
    players: u32,
    topology: impl Into<BoardTopology>,
) -> Result<Session, String> {
    if !(2..=8).contains(&players) {
        return Err("multiplayer requires 2 to 8 seats".into());
    }
    let mut bytes = [0; 32];
    for lane in bytes.as_chunks_mut::<8>().0 {
        lane.copy_from_slice(&seed.to_le_bytes());
    }
    frontier(crate::generate_map(topology, bytes), players)
}

/// Creates the standard frontier opening: distant capitals, adjacent garrisons and a camp.
pub fn frontier(map: MapState, players: u32) -> Result<Session, String> {
    open_frontier(map, players, 0)
}

/// Opens a two-seat frontier with enough adjacent land for both warriors and city expansion.
pub fn frontier_duel(map: MapState) -> Result<Session, String> {
    open_frontier(map, 2, 3)
}

fn open_frontier(
    map: MapState,
    players: u32,
    minimum_land_neighbors: usize,
) -> Result<Session, String> {
    let topology = map.topology().clone();
    let land: Vec<_> = map
        .tiles()
        .filter(|(_, tile)| {
            !matches!(tile.terrain, Terrain::Coast | Terrain::Ocean)
                && Unit::movement_cost(tile.terrain, false).is_some()
        })
        .map(|(id, _)| id)
        .collect();
    let sites: Vec<_> = land
        .iter()
        .copied()
        .filter(|&tile| {
            minimum_land_neighbors == 0
                || topology
                    .neighbors(tile)
                    .iter()
                    .filter(|neighbor| land.binary_search(neighbor).is_ok())
                    .count()
                    >= minimum_land_neighbors
        })
        .collect();
    if sites.len() < players as usize {
        return Err(
            "not enough traversable land for these seats; choose another map or --setup empty"
                .into(),
        );
    }
    let opening = choose_capitals(&topology, &land, &sites, players, None)
        .or_else(|| {
            topology.globe_frequency()?;
            let matching = matched_garrisons(&topology, &land, &sites);
            choose_capitals(&topology, &land, &sites, players, Some(&matching))
        })
        .ok_or("not enough adjacent traversable land for every capital and garrison; use a larger globe or fewer players")?;
    let mut session = Session::with_map((1..=players).map(PlayerId::new), map);
    for (seat, &(start, _)) in (1..=players).zip(&opening.capitals) {
        let player = PlayerId::new(seat);
        let unit = UnitId::new(2 * seat);
        session
            .dispatch(Command::UnitSpawn {
                player,
                unit,
                kind: UnitKind::Settler,
                tile: start,
            })
            .map_err(|e| e.to_string())?;
        session
            .dispatch(Command::CitySettle { player, unit })
            .map_err(|e| e.to_string())?;
    }
    // Pick garrisons only after reserving every capital.
    let mut occupied: BTreeSet<_> = opening.capitals.iter().map(|&(center, _)| center).collect();
    for (i, &(center, reserved)) in opening.capitals.iter().enumerate() {
        if let Some(tile) = reserved.or_else(|| {
            topology
                .neighbors(center)
                .into_iter()
                .find(|id| !occupied.contains(id) && land.binary_search(id).is_ok())
        }) {
            occupied.insert(tile);
            session
                .dispatch(Command::UnitSpawn {
                    player: PlayerId::new(i as u32 + 1),
                    unit: UnitId::new(2 * i as u32 + 1),
                    kind: UnitKind::Warrior,
                    tile,
                })
                .map_err(|e| e.to_string())?;
        }
    }
    if let Some(&camp) = land
        .iter()
        .filter(|id| opening.nearest_capital[id.index() as usize] >= 3)
        .min_by_key(|&&id| (opening.nearest_capital[id.index() as usize], id))
    {
        session
            .dispatch(Command::BarbarianSpawn {
                tile: camp,
                kind: UnitKind::Barbarian,
            })
            .map_err(|e| e.to_string())?;
    }
    Ok(session)
}

struct Opening {
    capitals: Vec<(TileId, Option<TileId>)>,
    nearest_capital: Vec<u32>,
}

fn choose_capitals(
    topology: &BoardTopology,
    land: &[TileId],
    sites: &[TileId],
    players: u32,
    matching: Option<&BTreeMap<TileId, TileId>>,
) -> Option<Opening> {
    let mut capitals = Vec::new();
    let mut occupied = BTreeSet::new();
    let mut nearest_capital = vec![u32::MAX; topology.tile_count()];
    for _ in 0..players {
        let (start, garrison) = sites
            .iter()
            .copied()
            .filter(|id| !occupied.contains(id))
            .filter_map(|id| {
                let garrison = if let Some(mates) = matching {
                    Some(*mates.get(&id)?)
                } else if topology.globe_frequency().is_some() {
                    Some(topology.neighbors(id).into_iter().find(|neighbor| {
                        !occupied.contains(neighbor) && land.binary_search(neighbor).is_ok()
                    })?)
                } else {
                    None
                };
                Some((id, garrison))
            })
            .max_by_key(|&(id, _)| (nearest_capital[id.index() as usize], std::cmp::Reverse(id)))?;
        occupied.insert(start);
        occupied.extend(garrison);
        capitals.push((start, garrison));
        // Query a single distance source at a time so spherical graph searches are reused.
        for &id in land {
            let nearest = &mut nearest_capital[id.index() as usize];
            *nearest = (*nearest).min(topology.distance(start, id));
        }
    }
    Some(Opening {
        capitals,
        nearest_capital,
    })
}

fn matched_garrisons(
    topology: &BoardTopology,
    land: &[TileId],
    sites: &[TileId],
) -> BTreeMap<TileId, TileId> {
    // After greedy placement stalls, every usable edge touches an occupied tile.
    // Only add tiles incident to such edges: at most twelve edges per chosen pair.
    let mut graph = UnGraph::new_undirected();
    let mut nodes = BTreeMap::new();
    for &site in sites {
        for neighbor in topology.neighbors(site) {
            if land.binary_search(&neighbor).is_err()
                || (neighbor < site && sites.binary_search(&neighbor).is_ok())
            {
                continue;
            }
            let a = *nodes.entry(site).or_insert_with(|| graph.add_node(site));
            let b = *nodes
                .entry(neighbor)
                .or_insert_with(|| graph.add_node(neighbor));
            graph.add_edge(a, b, ());
        }
    }
    maximum_matching(&graph)
        .edges()
        .flat_map(|(a, b)| [(graph[a], graph[b]), (graph[b], graph[a])])
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use histeries_hex::FlatHex;

    #[test]
    fn crowded_generated_globe_finds_three_disjoint_pairs() {
        let topology = BoardTopology::geodesic(1).unwrap();
        let session = seeded_frontier_on(10, 3, topology.clone()).unwrap();
        let mut occupied = BTreeSet::new();
        for seat in session.seat_ids() {
            let view = session.observe(seat).unwrap();
            let center = view.cities()[0].tile;
            let warrior = view
                .units()
                .iter()
                .find(|unit| unit.kind == UnitKind::Warrior)
                .unwrap();
            assert!(topology.neighbors(center).contains(&warrior.tile));
            assert!(occupied.insert(center));
            assert!(occupied.insert(warrior.tile));
        }
        assert_eq!(occupied.len(), 6);
        assert_eq!(
            seeded_frontier_on(10, 3, topology).unwrap().save_bytes(),
            session.save_bytes()
        );
    }

    #[test]
    fn globe_with_three_dead_ends_at_one_junction_cannot_fit_two_pairs() {
        let topology = BoardTopology::geodesic(2).unwrap();
        let junction = TileId::new(12);
        let neighbors = topology.edge_neighbors(junction);
        assert!(neighbors.iter().all(Option::is_some));
        let land = [
            junction,
            neighbors[0].unwrap(),
            neighbors[2].unwrap(),
            neighbors[4].unwrap(),
        ];
        let mut map = MapState::blank(topology.clone());
        for tile in (0..topology.tile_count()).map(|i| TileId::new(i as u32)) {
            if !land.contains(&tile) {
                map.set_terrain(tile, Terrain::Ocean).unwrap();
            }
        }
        assert!(frontier(map, 2).is_err());
    }

    #[test]
    fn standard_frontier_accepts_a_large_roster_when_land_is_available() {
        let map = MapState::blank(FlatHex::new(12, 9, false, false).unwrap());
        let session = frontier(map, 9).unwrap();
        assert_eq!(session.seat_ids().count(), 9);
        for seat in session.seat_ids() {
            assert_eq!(session.observe(seat).unwrap().cities().len(), 1);
        }
    }
}
