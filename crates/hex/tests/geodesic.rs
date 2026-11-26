//! Black-box checks of the Goldberg sphere through the topology and surface APIs.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use histeries_hex::{Geodesic, TileId, TileTopology, TopologyError};

fn tiles(board: &Geodesic) -> impl Iterator<Item = TileId> {
    (0..u32::try_from(board.tile_count()).unwrap()).map(TileId::new)
}

// The oracle traverses public adjacency only. It never asks for distance, lines or rings.
fn bfs(board: &Geodesic, start: TileId) -> Vec<u32> {
    let mut distances = vec![u32::MAX; board.tile_count()];
    let mut queue = VecDeque::from([start]);
    distances[start.index() as usize] = 0;
    while let Some(tile) = queue.pop_front() {
        let next = distances[tile.index() as usize] + 1;
        for neighbor in board.neighbors(tile) {
            let distance = &mut distances[neighbor.index() as usize];
            if *distance == u32::MAX {
                *distance = next;
                queue.push_back(neighbor);
            }
        }
    }
    distances
}

fn assert_on_sphere(point: [i32; 3]) {
    let squared_norm: i64 = point.into_iter().map(|v| i64::from(v).pow(2)).sum();
    let radius = i64::from(Geodesic::RADIUS);
    assert!(((radius - 2).pow(2)..=(radius + 2).pow(2)).contains(&squared_norm));
}

// The scalar triple product is positive when the edge goes counterclockwise
// as seen from outside the cell. Widen before taking coordinate products.
fn faces_out(center: [i32; 3], a: [i32; 3], b: [i32; 3]) -> bool {
    let [x, y, z] = center.map(i128::from);
    let [ax, ay, az] = a.map(i128::from);
    let [bx, by, bz] = b.map(i128::from);
    x * (ay * bz - az * by) + y * (az * bx - ax * bz) + z * (ax * by - ay * bx) > 0
}

#[test]
fn cells_form_one_closed_sphere_with_shared_oriented_edges() {
    for frequency in [1, 2, 3, 8, Geodesic::MAX_FREQUENCY] {
        let board = Geodesic::new(frequency).unwrap();
        assert_eq!(board.frequency(), frequency);
        assert_eq!(board.tile_count(), (10 * frequency.pow(2) + 2) as usize);
        assert!(!bfs(&board, TileId::new(0)).contains(&u32::MAX));

        let mut pentagons = 0;
        let mut edges = 0;
        let mut centers = BTreeSet::new();
        let mut atlas = BTreeSet::new();
        let mut corner_uses = BTreeMap::new();
        for tile in tiles(&board) {
            assert!(board.contains(tile));
            let neighbors = board.neighbors(tile);
            let degree = neighbors.len();
            assert!(degree == 5 || degree == 6);
            assert_eq!(degree == 5, tile.index() < 12);
            assert!(neighbors.windows(2).all(|pair| pair[0] < pair[1]));
            assert!(!neighbors.contains(&tile));
            pentagons += usize::from(degree == 5);

            let cycle = board.edge_neighbors(tile);
            assert_eq!(cycle[0], neighbors.first().copied());
            assert!(cycle[degree..].iter().all(Option::is_none));
            let mut sorted_cycle = cycle.into_iter().flatten().collect::<Vec<_>>();
            sorted_cycle.sort_unstable();
            assert_eq!(sorted_cycle, neighbors);

            let cell = board.surface(tile).unwrap();
            assert!(centers.insert(cell.center));
            assert_on_sphere(cell.center);
            let embedding = board.embedding(tile).unwrap();
            assert!(atlas.insert((embedding.x, embedding.y)));
            assert_eq!(cell.corners.len(), degree);
            for (i, &corner) in cell.corners.iter().enumerate() {
                let next = cell.corners[(i + 1) % degree];
                assert_on_sphere(corner);
                assert!(faces_out(cell.center, corner, next));
                *corner_uses.entry(corner).or_insert(0usize) += 1;
                let neighbor = cycle[i].unwrap();
                let j = board
                    .edge_neighbors(neighbor)
                    .iter()
                    .position(|n| *n == Some(tile));
                let j = j.expect("an edge must return to the original cell");
                if tile < neighbor {
                    let other = board.surface(neighbor).unwrap();
                    assert_eq!(other.corners[j], next);
                    assert_eq!(other.corners[(j + 1) % other.corners.len()], corner);
                    edges += 1;
                }
            }
        }
        let faces = board.tile_count();
        let vertices = corner_uses.len();
        assert_eq!(pentagons, 12);
        assert!(corner_uses.values().all(|&count| count == 3));
        assert_eq!(vertices + faces, edges + 2);
        println!(
            "geodesic_mesh frequency={frequency} cells={faces} pentagons={pentagons} hexagons={} edges={edges} corners={vertices}",
            faces - pentagons
        );
    }
}

#[test]
fn cell_identity_and_directed_surface_match_the_published_layout() {
    // FNV-1a over little-endian 32-bit values in tile ID order: ID, integer
    // center, degree, then each cyclic neighbor ID and start corner together.
    // These fixed expectations distinguish a compatible reconstruction from
    // a renumbered mesh with identical graph invariants and distances.
    for (frequency, expected) in [
        (1, 0xe9a9_a343_d5f8_5551),
        (2, 0x016d_c81a_f25e_442f),
        (3, 0x088f_ca8e_0aa3_3fb1),
    ] {
        let board = Geodesic::new(frequency).unwrap();
        let mut fingerprint = 0xcbf2_9ce4_8422_2325u64;
        let mut record = |value: u32| {
            for byte in value.to_le_bytes() {
                fingerprint = (fingerprint ^ u64::from(byte)).wrapping_mul(0x100_0000_01b3);
            }
        };
        for id in tiles(&board) {
            let cell = board.surface(id).unwrap();
            record(id.index());
            for component in cell.center {
                record(component as u32);
            }
            record(cell.corners.len() as u32);
            for (neighbor, corner) in board
                .edge_neighbors(id)
                .into_iter()
                .flatten()
                .zip(cell.corners)
            {
                record(neighbor.index());
                for component in corner {
                    record(component as u32);
                }
            }
        }
        assert_eq!(fingerprint, expected, "frequency {frequency}");
    }
}

#[test]
fn independent_constructions_agree_on_every_tile() {
    for frequency in [1, 2, 3, 8] {
        let first = Geodesic::new(frequency).unwrap();
        let second = Geodesic::new(frequency).unwrap();
        for tile in tiles(&first) {
            assert_eq!(first.neighbors(tile), second.neighbors(tile));
            assert_eq!(first.edge_neighbors(tile), second.edge_neighbors(tile));
            assert_eq!(first.surface(tile), second.surface(tile));
            assert_eq!(first.embedding(tile), second.embedding(tile));
        }
    }
}

#[test]
fn distances_lines_and_visibility_agree_with_independent_bfs() {
    for frequency in [1, 2, 3, 8] {
        let board = Geodesic::new(frequency).unwrap();
        // Exhaust the smaller spheres. On the larger one include all pentagons
        // and a stride of IDs to cover the interiors of different base faces.
        let endpoints: Vec<_> = tiles(&board)
            .filter(|tile| {
                frequency <= 3
                    || tile.index().is_multiple_of(29)
                    || board.neighbors(*tile).len() == 5
            })
            .collect();
        let mut pairs = 0;
        let mut pentagon_pairs = 0;
        for &from in &endpoints {
            let expected = bfs(&board, from);
            let estimates: Vec<_> = tiles(&board)
                .map(|tile| board.heuristic(tile, from))
                .collect();
            for tile in tiles(&board) {
                let estimate = estimates[tile.index() as usize];
                assert!(estimate <= expected[tile.index() as usize]);
                assert!(
                    board
                        .neighbors(tile)
                        .iter()
                        .all(|neighbor| estimate <= 1 + estimates[neighbor.index() as usize])
                );
            }
            let diameter_from = *expected.iter().max().unwrap();
            assert!(diameter_from < u32::MAX);
            for radius in (0..=diameter_from + 1).chain([u32::MAX]) {
                let ring: Vec<_> = tiles(&board)
                    .filter(|id| expected[id.index() as usize] == radius)
                    .collect();
                let area: Vec<_> = tiles(&board)
                    .filter(|id| expected[id.index() as usize] <= radius)
                    .collect();
                assert_eq!(board.ring(from, radius), ring);
                assert_eq!(board.area(from, radius), area);
            }
            for &to in &endpoints {
                let distance = expected[to.index() as usize];
                assert_eq!(board.distance(from, to), distance);
                let line = board.line(from, to);
                assert_eq!(line.len(), distance as usize + 1);
                assert_eq!(line.first(), Some(&from));
                assert_eq!(line.last(), Some(&to));
                assert!(
                    line.windows(2)
                        .all(|pair| board.neighbors(pair[0]).contains(&pair[1]))
                );
                assert_eq!(board.line_of_sight(from, to), line);
                assert!(
                    board
                        .line(to, from)
                        .into_iter()
                        .eq(line.iter().rev().copied())
                );
                pairs += 1;
                if board.neighbors(from).len() == 5 || board.neighbors(to).len() == 5 {
                    pentagon_pairs += 1;
                }
            }
        }
        println!(
            "geodesic_bfs frequency={frequency} ordered_pairs={pairs} pairs_touching_pentagons={pentagon_pairs}"
        );
    }
}

#[test]
fn invalid_frequencies_and_tile_ids_are_bounded() {
    assert!(matches!(
        Geodesic::new(0),
        Err(TopologyError::ZeroDimension)
    ));
    for frequency in [Geodesic::MAX_FREQUENCY + 1, u32::MAX] {
        assert!(matches!(
            Geodesic::new(frequency),
            Err(TopologyError::DimensionTooLarge)
        ));
    }
    let board = Geodesic::new(2).unwrap();
    let first = TileId::new(0);
    for invalid in [
        TileId::new(board.tile_count() as u32),
        TileId::new(u32::MAX),
    ] {
        assert!(!board.contains(invalid));
        assert!(board.neighbors(invalid).is_empty());
        assert_eq!(board.edge_neighbors(invalid), [None; 6]);
        assert!(board.surface(invalid).is_none());
        assert!(board.embedding(invalid).is_none());
        assert!(board.area(invalid, 0).is_empty());
        assert!(board.ring(invalid, 0).is_empty());
        for (from, to) in [(first, invalid), (invalid, first), (invalid, invalid)] {
            assert_eq!(board.distance(from, to), u32::MAX);
            assert!(board.line(from, to).is_empty());
            assert!(board.line_of_sight(from, to).is_empty());
        }
    }
}
