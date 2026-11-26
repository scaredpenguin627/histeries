//! The dual of an icosahedron whose triangular faces are subdivided at equal frequency.

mod mesh;

use std::cmp::Reverse;
use std::collections::{BTreeSet, BinaryHeap, VecDeque};
use std::sync::{Arc, Mutex};

use super::{Embedding, TileId, TileTopology, TopologyError};

/// A spherical polygon in Cartesian display units, centered on the globe's origin.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SphereCell {
    /// Outward direction of the tile center, scaled to [`Geodesic::RADIUS`].
    pub center: [i32; 3],
    /// Shared corners, counterclockwise as seen from outside the globe.
    pub corners: Vec<[i32; 3]>,
}

#[derive(Debug)]
struct Cell {
    neighbors: Vec<TileId>,
    atlas: Embedding,
    surface: SphereCell,
}

#[derive(Debug)]
struct Graph {
    cells: Vec<Cell>,
    /// Distances from the twelve original vertices; each row is a metric lower bound
    /// via the reverse triangle inequality.
    landmarks: [Vec<u16>; 12],
    distance_rows: Mutex<DistanceRows>,
}

#[derive(Debug)]
struct DistanceRows {
    /// Eight preallocated FIFO rows use 16 bytes per tile from creation, including
    /// after a reload. Map generation queries
    /// all destinations from a fixed seed; settlement scans can hold the other end fixed.
    rows: [(Option<TileId>, Vec<u16>); 8],
    next: usize,
    last_destination: Option<TileId>,
}

/// A class-I Goldberg sphere with twelve pentagons and `10 * frequency² - 10` hexagons.
/// Its stable IDs start with the twelve original vertices in mesh order.
#[derive(Clone, Debug)]
pub struct Geodesic {
    frequency: u32,
    graph: Arc<Graph>,
}

impl PartialEq for Geodesic {
    fn eq(&self, other: &Self) -> bool {
        self.frequency == other.frequency
    }
}

impl Eq for Geodesic {}

impl Geodesic {
    /// Largest supported subdivision frequency (10,242 tiles).
    pub const MAX_FREQUENCY: u32 = 32;
    /// Radius of the sphere's integer rendering embedding.
    pub const RADIUS: i32 = 1_000_000;

    /// Validates a frequency and returns its tile count without allocating a mesh.
    pub const fn tile_count_for_frequency(frequency: u32) -> Result<usize, TopologyError> {
        if frequency == 0 {
            return Err(TopologyError::ZeroDimension);
        }
        if frequency > Self::MAX_FREQUENCY {
            return Err(TopologyError::DimensionTooLarge);
        }
        Ok((10 * frequency * frequency + 2) as usize)
    }

    /// Generates a board with the given number of segments per original edge.
    pub fn new(frequency: u32) -> Result<Self, TopologyError> {
        let count = Self::tile_count_for_frequency(frequency)?;
        let cells = mesh::subdivide(frequency);
        debug_assert_eq!(cells.len(), count);
        let landmarks = std::array::from_fn(|i| {
            let mut row = vec![0; count];
            write_depths(&cells, TileId::new(i as u32), &mut row);
            row
        });
        Ok(Self {
            frequency,
            graph: Arc::new(Graph {
                cells,
                landmarks,
                distance_rows: Mutex::new(DistanceRows {
                    rows: std::array::from_fn(|_| (None, vec![0; count])),
                    next: 0,
                    last_destination: None,
                }),
            }),
        })
    }

    /// Returns the number of segments per original icosahedron edge.
    pub const fn frequency(&self) -> u32 {
        self.frequency
    }

    /// Returns the tile's polygon on the globe. Edge `i` joins corner `i` to
    /// corner `(i + 1) % degree` and faces neighbor `i` in [`Self::edge_neighbors`].
    pub fn surface(&self, tile: TileId) -> Option<SphereCell> {
        self.cell(tile).map(|cell| cell.surface.clone())
    }

    /// Returns outgoing edges in cyclic order, starting with the least neighbor
    /// ID. A pentagon leaves the sixth slot empty. Reciprocal edge indices are local.
    pub fn edge_neighbors(&self, tile: TileId) -> [Option<TileId>; 6] {
        let Some(cell) = self.cell(tile) else {
            return [None; 6];
        };
        std::array::from_fn(|slot| cell.neighbors.get(slot).copied())
    }

    fn cell(&self, tile: TileId) -> Option<&Cell> {
        self.graph.cells.get(tile.index() as usize)
    }

    fn within(&self, center: TileId, radius: u32, exact: bool) -> Vec<TileId> {
        if !self.contains(center) {
            return Vec::new();
        }
        let mut visited = BTreeSet::from([center]);
        let mut frontier = vec![center];
        for _ in 0..radius {
            if frontier.is_empty() {
                break;
            }
            let mut next = Vec::new();
            for tile in frontier {
                for &neighbor in &self.graph.cells[tile.index() as usize].neighbors {
                    if visited.insert(neighbor) {
                        next.push(neighbor);
                    }
                }
            }
            frontier = next;
        }
        if exact {
            frontier.sort_unstable();
            frontier
        } else {
            visited.into_iter().collect()
        }
    }

    // Called with valid endpoints, in canonical ascending order. Prefer greater
    // progress, then lower TileId, among equally promising open nodes.
    fn shortest_line(&self, start: TileId, end: TileId) -> Vec<TileId> {
        let mut best = vec![u32::MAX; self.tile_count()];
        let mut parent = vec![start; self.tile_count()];
        let mut open = BinaryHeap::new();
        best[start.index() as usize] = 0;
        open.push(Reverse((self.heuristic(start, end), Reverse(0), start)));
        while let Some(Reverse((_, Reverse(steps), tile))) = open.pop() {
            if steps != best[tile.index() as usize] {
                continue;
            }
            if tile == end {
                let mut path = vec![tile];
                let mut cursor = tile;
                while cursor != start {
                    cursor = parent[cursor.index() as usize];
                    path.push(cursor);
                }
                path.reverse();
                return path;
            }
            for &next in &self.graph.cells[tile.index() as usize].neighbors {
                let index = next.index() as usize;
                let cost = steps + 1;
                if cost < best[index] {
                    best[index] = cost;
                    parent[index] = tile;
                    open.push(Reverse((
                        cost + self.heuristic(next, end),
                        Reverse(cost),
                        next,
                    )));
                }
            }
        }
        unreachable!("the triangulated icosahedron is connected")
    }
}

impl TileTopology for Geodesic {
    fn tile_count(&self) -> usize {
        self.graph.cells.len()
    }

    fn contains(&self, tile: TileId) -> bool {
        self.cell(tile).is_some()
    }

    fn neighbors(&self, tile: TileId) -> Vec<TileId> {
        let Some(cell) = self.cell(tile) else {
            return Vec::new();
        };
        let mut neighbors = cell.neighbors.clone();
        neighbors.sort_unstable();
        neighbors
    }

    fn distance(&self, from: TileId, to: TileId) -> u32 {
        if !self.contains(from) || !self.contains(to) {
            return u32::MAX;
        }
        if from == to {
            return 0;
        }
        for (source, target) in [(from, to), (to, from)] {
            if let Some(row) = self.graph.landmarks.get(source.index() as usize) {
                return u32::from(row[target.index() as usize]);
            }
        }
        let mut cache = self.graph.distance_rows.lock().expect("distance rows");
        for (origin, row) in &cache.rows {
            if *origin == Some(from) {
                return u32::from(row[to.index() as usize]);
            }
            if *origin == Some(to) {
                return u32::from(row[from.index() as usize]);
            }
        }
        let (origin, target) = if cache.last_destination == Some(to) {
            (to, from)
        } else {
            (from, to)
        };
        cache.last_destination = Some(to);
        let slot = cache.next;
        cache.next = (slot + 1) % cache.rows.len();
        let (source, row) = &mut cache.rows[slot];
        *source = Some(origin);
        write_depths(&self.graph.cells, origin, row);
        u32::from(row[target.index() as usize])
    }

    fn heuristic(&self, from: TileId, to: TileId) -> u32 {
        if !self.contains(from) || !self.contains(to) {
            return u32::MAX;
        }
        self.graph
            .landmarks
            .iter()
            .map(|row| u32::from(row[from.index() as usize].abs_diff(row[to.index() as usize])))
            .max()
            .unwrap_or(0)
    }

    fn ring(&self, center: TileId, radius: u32) -> Vec<TileId> {
        self.within(center, radius, true)
    }

    fn area(&self, center: TileId, radius: u32) -> Vec<TileId> {
        self.within(center, radius, false)
    }

    fn line(&self, from: TileId, to: TileId) -> Vec<TileId> {
        if !self.contains(from) || !self.contains(to) {
            return Vec::new();
        }
        if from == to {
            return vec![from];
        }
        let mut path = self.shortest_line(from.min(to), from.max(to));
        if from > to {
            path.reverse();
        }
        path
    }

    fn embedding(&self, tile: TileId) -> Option<Embedding> {
        self.cell(tile).map(|cell| cell.atlas)
    }
}

// A connected board has at most 10,242 tiles, so all finite depths fit in u16.
fn write_depths(cells: &[Cell], start: TileId, depths: &mut [u16]) {
    depths.fill(u16::MAX);
    depths[start.index() as usize] = 0;
    let mut queue = VecDeque::from([start]);
    while let Some(tile) = queue.pop_front() {
        let index = tile.index() as usize;
        let depth = depths[index];
        for next in &cells[index].neighbors {
            let next_depth = &mut depths[next.index() as usize];
            if *next_depth == u16::MAX {
                *next_depth = depth + 1;
                queue.push_back(*next);
            }
        }
    }
}
