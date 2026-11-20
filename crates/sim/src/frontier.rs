//! Opening positions for a playable generated map, without editing its terrain.
use std::collections::BTreeSet;

use crate::{Command, MapState, Session, Terrain, Unit, UnitKind};
use histeries_hex::{TileId, TileTopology};
use histeries_rules_core::{PlayerId, UnitId};

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
    let topology = map.topology();
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
    let mut centers_in_seat_order = Vec::new();
    let mut centers = BTreeSet::<TileId>::new();
    let mut session = Session::with_map((1..=players).map(PlayerId::new), map);
    for seat in 1..=players {
        let start = *sites
            .iter()
            .filter(|id| !centers.contains(id))
            .max_by_key(|&&id| {
                (
                    centers
                        .iter()
                        .map(|&other| topology.distance(id, other))
                        .min()
                        .unwrap_or(0),
                    std::cmp::Reverse(id),
                )
            })
            .expect("sufficient distinct land tiles");
        centers.insert(start);
        centers_in_seat_order.push(start);
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
    let mut occupied = centers.clone();
    for (i, &center) in centers_in_seat_order.iter().enumerate() {
        if let Some(tile) = topology
            .neighbors(center)
            .into_iter()
            .find(|id| !occupied.contains(id) && land.contains(id))
        {
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
        .filter(|&&id| centers.iter().all(|&c| topology.distance(c, id) >= 3))
        .min_by_key(|&&id| (centers.iter().map(|&c| topology.distance(c, id)).min(), id))
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

#[cfg(test)]
mod tests {
    use super::*;
    use histeries_hex::FlatHex;

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
