//! Opening positions for a playable generated map, without editing its terrain.
use std::collections::BTreeSet;

use histeries_hex::{TileId, TileTopology};
use histeries_rules_core::{PlayerId, UnitId};
use histeries_sim::{Command, MapState, Session, Terrain, Unit, UnitKind};

pub(super) fn frontier(map: MapState, players: u32) -> Result<Session, String> {
    let topology = map.topology();
    let land: Vec<_> = map
        .tiles()
        .filter(|(_, tile)| {
            !matches!(tile.terrain, Terrain::Coast | Terrain::Ocean)
                && Unit::movement_cost(tile.terrain, false).is_some()
        })
        .map(|(id, _)| id)
        .collect();
    if land.len() < players as usize {
        return Err(
            "not enough traversable land for these seats; choose another map or --setup empty"
                .into(),
        );
    }
    let mut centers_in_seat_order = Vec::new();
    let mut centers = BTreeSet::<TileId>::new();
    let mut session = Session::with_map((1..=players).map(PlayerId::new), map);
    for seat in 1..=players {
        let start = *land
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
    fn frontier_and_json_scenario_batch_use_normal_dispatch_and_preserve_input_on_failure() {
        let map = MapState::blank(FlatHex::new(7, 3, false, false).unwrap());
        let game = frontier(map, 2).unwrap();
        for seat in [PlayerId::new(1), PlayerId::new(2)] {
            assert_eq!(game.observe(seat).unwrap().cities().len(), 1);
            assert_eq!(game.observe(seat).unwrap().units().len(), 1);
        }
        let dir =
            std::env::temp_dir().join(format!("histeries-scenario-batch-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let state = dir.join("game.hst");
        let source = dir.join("orders.json");
        std::fs::write(&state, game.state_snapshot().encode()).unwrap();
        let run = || {
            crate::run(vec![
                "scenario-editor".into(),
                state.display().to_string(),
                "apply".into(),
                "--commands".into(),
                source.display().to_string(),
            ])
        };
        let marker = serde_json::json!({"command":"editor.set_marker","player":1,"value":23});
        let original = std::fs::read(&state).unwrap();
        std::fs::write(
            &source,
            serde_json::to_vec(&serde_json::json!([marker,{"command":"turn.end","player":42}]))
                .unwrap(),
        )
        .unwrap();
        assert!(run().is_err());
        assert_eq!(std::fs::read(&state).unwrap(), original);
        std::fs::write(
            &source,
            serde_json::to_vec(&serde_json::json!([marker])).unwrap(),
        )
        .unwrap();
        run().unwrap();
        assert_eq!(
            crate::read_state(state.to_str().unwrap())
                .unwrap()
                .marker(PlayerId::new(1)),
            Some(23)
        );
        for file in [state, source] {
            std::fs::remove_file(file).unwrap();
        }
        std::fs::remove_dir(dir).unwrap();
    }
}
