//! Deterministic continent growth and downhill river drainage.

use std::collections::VecDeque;

use histeries_hex::{FlatHex, TileId, TileTopology};
use histeries_rules_core::NamedRng;
use histeries_ruleset::{MapGeneration, Ruleset, base_ruleset};

use super::{Feature, MapState, RiverDirection, Terrain, TileState};

/// Generates a map with the bundled ruleset.
pub fn generate_map(topology: FlatHex, seed: [u8; 32]) -> MapState {
    generate_map_with_ruleset(topology, seed, base_ruleset())
}

/// Generates a map using a validated ruleset's continent and placement settings.
pub fn generate_map_with_ruleset(topology: FlatHex, seed: [u8; 32], ruleset: &Ruleset) -> MapState {
    let profile = ruleset.map_generation();
    let mut rng = NamedRng::new(seed, "mapgen");
    let land = continents(topology, profile, &mut rng);
    let total_weight = profile
        .land
        .values()
        .map(|rule| u64::from(rule.weight))
        .sum();
    let mut tiles = Vec::with_capacity(land.len());
    for (index, &is_land) in land.iter().enumerate() {
        let tile = TileId::new(index as u32);
        let mut state = TileState::default();
        if is_land {
            let terrain = draw(
                &mut rng,
                total_weight,
                profile
                    .land
                    .iter()
                    .map(|(&kind, rule)| (kind, u64::from(rule.weight))),
            )
            .expect("validated terrain weights cover the full draw");
            let rule = &profile.land[&terrain];
            state.terrain = terrain;
            state.feature = draw(
                &mut rng,
                100,
                rule.features
                    .iter()
                    .map(|(&kind, &chance)| (kind, u64::from(chance))),
            )
            .unwrap_or(Feature::None);
            state.resource = draw(
                &mut rng,
                100,
                rule.resources
                    .iter()
                    .map(|(&kind, &chance)| (kind, u64::from(chance))),
            );
        } else if topology
            .neighbors(tile)
            .iter()
            .any(|n| land[n.index() as usize])
        {
            state.terrain = Terrain::Coast;
        } else {
            state.terrain = Terrain::Ocean;
        }
        tiles.push(state);
    }
    rivers(topology, &mut tiles, profile.river_source_percent, &mut rng);
    MapState {
        topology,
        tiles: tiles
            .into_iter()
            .enumerate()
            .map(|(i, state)| (TileId::new(i as u32), state))
            .collect(),
    }
}

fn draw<T>(rng: &mut NamedRng, scale: u64, choices: impl Iterator<Item = (T, u64)>) -> Option<T> {
    let mut roll = rng.next_bounded(scale)?;
    for (choice, weight) in choices {
        if roll < weight {
            return Some(choice);
        }
        roll -= weight;
    }
    None
}

fn continents(topology: FlatHex, profile: &MapGeneration, rng: &mut NamedRng) -> Vec<bool> {
    let count = topology.tile_count();
    let target = ((count as u64 * u64::from(profile.land_percent) + 50) / 100) as usize;
    if target == 0 {
        return vec![false; count];
    }

    let mut owner = vec![0_u8; count];
    let mut distance = vec![u32::MAX; count];
    let mut frontiers = Vec::new();
    let mut seed = TileId::new(rng.next_bounded(count as u64).unwrap() as u32);
    for id in 1..=profile.continents {
        owner[seed.index() as usize] = id;
        frontiers.push(topology.neighbors(seed));
        if frontiers.len() == target || id == profile.continents {
            break;
        }
        // Spread seeds by maximizing the distance to the nearest existing seed.
        // Resolve ties in ascending tile order.
        let mut farthest = 0;
        for (i, nearest) in distance.iter_mut().enumerate() {
            let candidate = TileId::new(i as u32);
            *nearest = (*nearest).min(topology.distance(seed, candidate));
            farthest = farthest.max(*nearest);
        }
        if farthest < 2 {
            break;
        }
        seed = TileId::new(distance.iter().position(|d| *d == farthest).unwrap() as u32);
    }

    let mut filled = frontiers.len();
    while filled < target {
        let before = filled;
        for (i, frontier) in frontiers.iter_mut().enumerate() {
            let id = i as u8 + 1;
            while !frontier.is_empty() {
                let choice = rng.next_bounded(frontier.len() as u64).unwrap() as usize;
                let candidate = frontier.swap_remove(choice);
                let index = candidate.index() as usize;
                if owner[index] != 0 {
                    continue;
                }
                let neighbors = topology.neighbors(candidate);
                if neighbors.iter().any(|n| {
                    let adjacent = owner[n.index() as usize];
                    adjacent != 0 && adjacent != id
                }) {
                    continue;
                }
                owner[index] = id;
                filled += 1;
                frontier.extend(
                    neighbors
                        .into_iter()
                        .filter(|n| owner[n.index() as usize] == 0),
                );
                break;
            }
            if filled == target {
                break;
            }
        }
        if filled == before {
            break;
        }
    }
    owner.iter().map(|id| *id != 0).collect()
}

fn rivers(topology: FlatHex, tiles: &mut [TileState], source_percent: u8, rng: &mut NamedRng) {
    if source_percent == 0 {
        return;
    }
    // Flood inland from the coasts. Each discovered tile drains to an adjacent
    // tile one step closer to water, so every river has an outlet and no cycles.
    let mut queue: VecDeque<_> = tiles
        .iter()
        .enumerate()
        .filter(|(_, state)| state.terrain == Terrain::Coast)
        .map(|(i, _)| TileId::new(i as u32))
        .collect();
    let mut drainage = vec![None; tiles.len()];
    while let Some(outlet) = queue.pop_front() {
        for (i, upstream) in topology.edge_neighbors(outlet).into_iter().enumerate() {
            let Some(upstream) = upstream else { continue };
            let index = upstream.index() as usize;
            if drainage[index].is_some()
                || matches!(
                    tiles[index].terrain,
                    Terrain::Ocean | Terrain::Coast | Terrain::Mountain
                )
            {
                continue;
            }
            let direction = RiverDirection::from_u8(((i + 3) % 6) as u8).unwrap();
            drainage[index] = Some((direction, outlet));
            queue.push_back(upstream);
        }
    }
    for source in 0..tiles.len() {
        if drainage[source].is_none() || rng.next_bounded(100).unwrap() >= u64::from(source_percent)
        {
            continue;
        }
        let mut current = source;
        while let Some((direction, next)) = drainage[current] {
            if tiles[current].river.is_some() {
                break;
            }
            tiles[current].river = Some(direction);
            current = next.index() as usize;
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;
    use crate::map::ResourceKind;

    fn rules(continents: u8, land_percent: u8, rivers: u8) -> Ruleset {
        Ruleset::from_toml(&format!(
            r#"
schema_version = 1
id = "orchard"
name = "Orchard"
provenance = "designed"

[mapgen]
continents = {continents}
land_percent = {land_percent}
river_source_percent = {rivers}
[mapgen.land.grassland]
weight = 1
features = {{ forest = 100 }}
resources = {{ wheat = 100 }}
"#
        ))
        .unwrap()
    }

    fn components(map: &MapState) -> Vec<usize> {
        let mut land: BTreeSet<_> = map
            .tiles()
            .filter(|(_, t)| !matches!(t.terrain, Terrain::Coast | Terrain::Ocean))
            .map(|(id, _)| id)
            .collect();
        let mut sizes = Vec::new();
        while let Some(seed) = land.pop_first() {
            let mut size = 0;
            let mut stack = vec![seed];
            while let Some(tile) = stack.pop() {
                size += 1;
                stack.extend(
                    map.topology()
                        .neighbors(tile)
                        .into_iter()
                        .filter(|n| land.remove(n)),
                );
            }
            sizes.push(size);
        }
        sizes.sort_unstable();
        sizes
    }

    #[test]
    fn land_budget_grows_distinct_connected_continents_across_seams() {
        let profile = rules(3, 48, 100);
        for wrap in [(false, false), (true, false), (true, true)] {
            let topology = FlatHex::new(25, 17, wrap.0, wrap.1).unwrap();
            for seed in [0, 1, 4, 29] {
                let map = generate_map_with_ruleset(topology, [seed; 32], &profile);
                let sizes = components(&map);
                assert_eq!(sizes.len(), 3);
                assert!(sizes[0] > 25, "{sizes:?}");
                assert_eq!(sizes.iter().sum::<usize>(), 204);
                for (tile, state) in map.tiles() {
                    if matches!(state.terrain, Terrain::Ocean | Terrain::Coast) {
                        let beside_land = topology
                            .neighbors(tile)
                            .iter()
                            .any(|t| map.tile(*t).unwrap().terrain == Terrain::Grassland);
                        assert_eq!(state.terrain == Terrain::Coast, beside_land);
                        continue;
                    }
                    assert_eq!(state.terrain, Terrain::Grassland);
                    assert_eq!(state.feature, Feature::Forest);
                    assert_eq!(state.resource, Some(ResourceKind::Wheat));
                    let mut cursor = tile;
                    let mut seen = BTreeSet::new();
                    while let Some(edge) = map.tile(cursor).unwrap().river {
                        assert!(seen.insert(cursor), "river loop");
                        cursor = topology.edge_neighbors(cursor)[edge as usize].unwrap();
                    }
                    assert_eq!(map.tile(cursor).unwrap().terrain, Terrain::Coast);
                }
            }
        }
    }

    #[test]
    fn custom_land_and_open_water_extremes_work_on_tiny_wrapped_boards() {
        for (w, h) in [(1, 1), (2, 1), (1, 7), (4, 4)] {
            let topology = FlatHex::new(w, h, true, true).unwrap();
            let ocean = generate_map_with_ruleset(topology, [3; 32], &rules(16, 0, 100));
            assert!(ocean.tiles().all(|(_, t)| t.terrain == Terrain::Ocean));
            let land = generate_map_with_ruleset(topology, [3; 32], &rules(1, 100, 100));
            assert_eq!(components(&land), [topology.tile_count()]);
            assert!(land.tiles().all(|(_, t)| t.river.is_none()));
            let crowded = generate_map_with_ruleset(topology, [3; 32], &rules(16, 85, 0));
            assert!(crowded.tiles().all(|(_, t)| t.river.is_none()));
            assert_eq!(MapState::decode(&crowded.encode()).unwrap(), crowded);
        }
    }

    #[test]
    fn base_seed_changes_geography_and_profile_controls_component_count() {
        let topology = FlatHex::new(20, 16, true, false).unwrap();
        let mask = |map: MapState| {
            map.tiles()
                .map(|(_, t)| matches!(t.terrain, Terrain::Ocean | Terrain::Coast))
                .collect::<Vec<_>>()
        };
        let first = generate_map(topology, [7; 32]);
        assert_eq!(components(&first).len(), 2);
        assert_ne!(mask(first), mask(generate_map(topology, [8; 32])));
        let single = generate_map_with_ruleset(topology, [7; 32], &rules(1, 48, 0));
        assert_eq!(components(&single), [154]);
    }

    #[test]
    fn existing_map1_bytes_retain_edited_and_generated_tile_data() {
        // A pre-continent MAP1 map can contain an isolated coast and a river
        // pointing outside the board. Loading a snapshot must preserve both.
        let bytes = b"MAP1\x01\0\x02\0\0\0\x01\0\0\0\0\0\x02\0\0\0\0\0\0\0\x04\0\xff\xff\x01\0\0\0\0\x01\x01\x03";
        let map = MapState::decode(bytes).unwrap();
        assert_eq!(map.encode(), bytes);
        assert_eq!(
            map.tile(TileId::new(1)).unwrap().river,
            Some(RiverDirection::East)
        );
    }
}
