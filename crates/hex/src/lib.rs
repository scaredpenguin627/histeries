//! Deterministic tile-topology primitives.
//!
//! Coordinate representations are deliberately private to this crate. Callers use stable
//! [`TileId`] values and the [`TileTopology`] trait instead of axial or cube coordinates.

#![forbid(unsafe_code)]

use std::fmt;

mod geodesic;
pub use geodesic::{AtlasFace, Geodesic, SphereCell};

/// A stable tile identifier within a topology.
#[derive(
    Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, serde::Serialize, serde::Deserialize,
)]
pub struct TileId(u32);

impl TileId {
    /// Creates an identifier from a zero-based tile index.
    pub const fn new(index: u32) -> Self {
        Self(index)
    }

    /// Returns the zero-based tile index.
    pub const fn index(self) -> u32 {
        self.0
    }
}

/// An integer rendering embedding supplied by a topology.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct Embedding {
    /// Horizontal coordinate in topology-defined display units.
    pub x: i32,
    /// Vertical coordinate in topology-defined display units.
    pub y: i32,
}

/// Errors returned while constructing a topology.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TopologyError {
    /// A dimension was zero.
    ZeroDimension,
    /// The requested map contains more tiles than a `TileId` can address.
    TooManyTiles,
    /// A dimension cannot be represented by the topology's integer coordinate operations.
    DimensionTooLarge,
}

impl fmt::Display for TopologyError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ZeroDimension => formatter.write_str("topology dimensions must be non-zero"),
            Self::TooManyTiles => formatter.write_str("topology contains too many tiles"),
            Self::DimensionTooLarge => formatter.write_str("topology dimensions are too large"),
        }
    }
}

impl std::error::Error for TopologyError {}

/// Operations common to every tile topology.
pub trait TileTopology {
    /// Returns the number of tiles in this topology.
    fn tile_count(&self) -> usize;

    /// Returns whether a tile belongs to this topology.
    fn contains(&self, tile: TileId) -> bool;

    /// Returns neighboring tiles in ascending identifier order.
    fn neighbors(&self, tile: TileId) -> Vec<TileId>;

    /// Returns the shortest graph distance between two tiles.
    fn distance(&self, from: TileId, to: TileId) -> u32;

    /// Returns the deterministic A* heuristic supplied by this topology.
    fn heuristic(&self, from: TileId, to: TileId) -> u32 {
        self.distance(from, to)
    }

    /// Returns all tiles at exactly `radius` steps from `center` in ascending identifier order.
    fn ring(&self, center: TileId, radius: u32) -> Vec<TileId>;

    /// Returns all tiles at most `radius` steps from `center` in ascending identifier order.
    fn area(&self, center: TileId, radius: u32) -> Vec<TileId>;

    /// Returns a deterministic tile line including both endpoints.
    fn line(&self, from: TileId, to: TileId) -> Vec<TileId>;

    /// Returns the topology line used for visibility checks, including both endpoints.
    fn line_of_sight(&self, from: TileId, to: TileId) -> Vec<TileId> {
        self.line(from, to)
    }

    /// Returns an integer embedding suitable for a renderer.
    fn embedding(&self, tile: TileId) -> Option<Embedding>;
}

/// A rectangular flat-hex topology with optional horizontal and vertical wrapping.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FlatHex {
    width: u32,
    height: u32,
    wrap_x: bool,
    wrap_y: bool,
}

impl FlatHex {
    /// Creates a rectangular flat topology.
    pub fn new(width: u32, height: u32, wrap_x: bool, wrap_y: bool) -> Result<Self, TopologyError> {
        if width == 0 || height == 0 {
            return Err(TopologyError::ZeroDimension);
        }
        if width > i32::MAX as u32 || height > i32::MAX as u32 {
            return Err(TopologyError::DimensionTooLarge);
        }
        if u64::from(width) * u64::from(height) > u64::from(u32::MAX) {
            return Err(TopologyError::TooManyTiles);
        }
        Ok(Self {
            width,
            height,
            wrap_x,
            wrap_y,
        })
    }

    /// Returns the map width in tiles.
    pub const fn width(self) -> u32 {
        self.width
    }

    /// Returns the map height in tiles.
    pub const fn height(self) -> u32 {
        self.height
    }

    /// Returns whether horizontal wrapping is enabled.
    pub const fn wraps_x(self) -> bool {
        self.wrap_x
    }

    /// Returns whether vertical wrapping is enabled.
    pub const fn wraps_y(self) -> bool {
        self.wrap_y
    }

    /// Neighbors by edge, clockwise from north-east in the rendering embedding
    /// (north-east, east, south-east, south-west, west, north-west).
    /// Missing boundary edges and edges that wrap to the same tile are empty.
    pub fn edge_neighbors(self, tile: TileId) -> [Option<TileId>; 6] {
        let Some((q, r)) = self.coordinates(tile) else {
            return [None; 6];
        };
        DIRECTIONS.map(|(dq, dr)| {
            self.tile_at(q + dq, r + dr)
                .filter(|neighbor| *neighbor != tile)
        })
    }

    fn coordinates(self, tile: TileId) -> Option<(i32, i32)> {
        if !self.contains(tile) {
            return None;
        }
        let index = tile.index();
        Some(((index % self.width) as i32, (index / self.width) as i32))
    }

    fn tile_at(self, q: i32, r: i32) -> Option<TileId> {
        let q = normalize(q, self.width as i32, self.wrap_x)?;
        let r = normalize(r, self.height as i32, self.wrap_y)?;
        Some(TileId::new(r as u32 * self.width + q as u32))
    }

    fn best_destination(self, from: (i32, i32), to: (i32, i32)) -> (i32, i32) {
        let q_offsets = if self.wrap_x {
            [-1, 0, 1].map(|offset| offset * self.width as i32)
        } else {
            [0, 0, 0]
        };
        let r_offsets = if self.wrap_y {
            [-1, 0, 1].map(|offset| offset * self.height as i32)
        } else {
            [0, 0, 0]
        };
        let mut best = to;
        let mut best_distance = axial_distance(from, to);
        for q_offset in q_offsets {
            for r_offset in r_offsets {
                let candidate = (to.0 + q_offset, to.1 + r_offset);
                let distance = axial_distance(from, candidate);
                if distance < best_distance || (distance == best_distance && candidate < best) {
                    best = candidate;
                    best_distance = distance;
                }
            }
        }
        best
    }
}

impl TileTopology for FlatHex {
    fn tile_count(&self) -> usize {
        self.width as usize * self.height as usize
    }

    fn contains(&self, tile: TileId) -> bool {
        (tile.index() as u64) < self.tile_count() as u64
    }

    fn neighbors(&self, tile: TileId) -> Vec<TileId> {
        let Some((q, r)) = self.coordinates(tile) else {
            return Vec::new();
        };
        let mut neighbors = Vec::with_capacity(6);
        for (dq, dr) in DIRECTIONS {
            if let Some(neighbor) = self.tile_at(q + dq, r + dr) {
                neighbors.push(neighbor);
            }
        }
        neighbors.sort_unstable();
        neighbors.dedup();
        neighbors
    }

    fn distance(&self, from: TileId, to: TileId) -> u32 {
        let (Some(from), Some(to)) = (self.coordinates(from), self.coordinates(to)) else {
            return u32::MAX;
        };
        axial_distance(from, self.best_destination(from, to))
    }

    fn ring(&self, center: TileId, radius: u32) -> Vec<TileId> {
        if !self.contains(center) {
            return Vec::new();
        }
        let mut tiles: Vec<_> = (0..self.tile_count())
            .map(|index| TileId::new(index as u32))
            .filter(|tile| self.distance(center, *tile) == radius)
            .collect();
        tiles.sort_unstable();
        tiles
    }

    fn area(&self, center: TileId, radius: u32) -> Vec<TileId> {
        if !self.contains(center) {
            return Vec::new();
        }
        let mut tiles: Vec<_> = (0..self.tile_count())
            .map(|index| TileId::new(index as u32))
            .filter(|tile| self.distance(center, *tile) <= radius)
            .collect();
        tiles.sort_unstable();
        tiles
    }

    fn line(&self, from: TileId, to: TileId) -> Vec<TileId> {
        let (Some(start), Some(end)) = (self.coordinates(from), self.coordinates(to)) else {
            return Vec::new();
        };
        let end = self.best_destination(start, end);
        let steps = axial_distance(start, end);
        if steps == 0 {
            return vec![from];
        }
        let mut line = Vec::with_capacity(steps as usize + 1);
        for step in 0..=steps {
            let q_numerator = i64::from(start.0) * i64::from(steps)
                + i64::from(end.0 - start.0) * i64::from(step);
            let r_numerator = i64::from(start.1) * i64::from(steps)
                + i64::from(end.1 - start.1) * i64::from(step);
            let (q, r) = cube_round(q_numerator, r_numerator, i64::from(steps));
            if let Some(tile) = self.tile_at(q, r)
                && line.last().copied() != Some(tile)
            {
                line.push(tile);
            }
        }
        line
    }

    fn embedding(&self, tile: TileId) -> Option<Embedding> {
        let (q, r) = self.coordinates(tile)?;
        Some(Embedding {
            x: q * 2 + r,
            y: r * 3,
        })
    }
}

// Clockwise from north-east, matching the public edge order.
const DIRECTIONS: [(i32, i32); 6] = [(1, -1), (1, 0), (0, 1), (-1, 1), (-1, 0), (0, -1)];

fn normalize(value: i32, size: i32, wrapping: bool) -> Option<i32> {
    if (0..size).contains(&value) {
        return Some(value);
    }
    if wrapping {
        Some(value.rem_euclid(size))
    } else {
        None
    }
}

fn axial_distance(from: (i32, i32), to: (i32, i32)) -> u32 {
    let dq = to.0 - from.0;
    let dr = to.1 - from.1;
    ((dq.abs() + (dq + dr).abs() + dr.abs()) / 2) as u32
}

fn cube_round(q_numerator: i64, r_numerator: i64, denominator: i64) -> (i32, i32) {
    let q = round_ratio(q_numerator, denominator);
    let r = round_ratio(r_numerator, denominator);
    let y_numerator = -(q_numerator + r_numerator);
    let y = round_ratio(y_numerator, denominator);
    let q_error = (i64::from(q) * denominator - q_numerator).abs();
    let r_error = (i64::from(r) * denominator - r_numerator).abs();
    let y_error = (i64::from(y) * denominator - y_numerator).abs();
    if q_error > r_error && q_error > y_error {
        (-r - y, r)
    } else if r_error > y_error {
        (q, -q - y)
    } else {
        (q, r)
    }
}

fn round_ratio(numerator: i64, denominator: i64) -> i32 {
    let quotient = numerator / denominator;
    let remainder = numerator % denominator;
    let adjustment = if remainder.abs() * 2 >= denominator {
        numerator.signum()
    } else {
        0
    };
    (quotient + adjustment) as i32
}

#[cfg(test)]
mod tests {
    use super::{FlatHex, TileId, TileTopology, TopologyError};

    fn map() -> FlatHex {
        FlatHex::new(3, 3, false, false).expect("valid map")
    }

    #[test]
    fn rejects_invalid_dimensions() {
        assert_eq!(
            FlatHex::new(0, 3, false, false),
            Err(TopologyError::ZeroDimension)
        );
    }

    #[test]
    fn neighbors_are_sorted_and_bounded() {
        let map = map();
        assert_eq!(
            map.neighbors(TileId::new(0)),
            vec![TileId::new(1), TileId::new(3)]
        );
        assert_eq!(map.neighbors(TileId::new(4)).len(), 6);
        assert!(map.neighbors(TileId::new(99)).is_empty());
    }

    #[test]
    fn edge_neighbors_keep_compass_order_at_boundaries_and_wrapped_seams() {
        let center = TileId::new(4);
        assert_eq!(
            map().edge_neighbors(center),
            [2, 5, 7, 6, 3, 1].map(|id| Some(TileId::new(id)))
        );
        assert_eq!(
            map().edge_neighbors(TileId::new(0)),
            [
                None,
                Some(TileId::new(1)),
                Some(TileId::new(3)),
                None,
                None,
                None
            ]
        );
        let seam = FlatHex::new(3, 3, true, true).unwrap();
        assert_eq!(
            seam.edge_neighbors(TileId::new(0)),
            [7, 1, 3, 5, 2, 6].map(|id| Some(TileId::new(id)))
        );
        assert_eq!(
            FlatHex::new(1, 1, true, true)
                .unwrap()
                .edge_neighbors(TileId::new(0)),
            [None; 6]
        );
    }

    #[test]
    fn distance_ring_and_area_are_deterministic() {
        let map = map();
        let center = TileId::new(4);
        assert_eq!(map.distance(center, TileId::new(8)), 2);
        assert_eq!(map.heuristic(center, TileId::new(8)), 2);
        assert_eq!(map.ring(center, 1).len(), 6);
        assert_eq!(map.area(center, 1).len(), 7);
        assert!(map.area(center, 1).windows(2).all(|pair| pair[0] < pair[1]));
    }

    #[test]
    fn wrapping_keeps_degree_six() {
        let map = FlatHex::new(3, 3, true, true).expect("valid map");
        assert_eq!(map.neighbors(TileId::new(0)).len(), 6);
        assert_eq!(map.distance(TileId::new(0), TileId::new(2)), 1);
    }

    #[test]
    fn line_of_sight_includes_endpoints() {
        let map = map();
        let line = map.line_of_sight(TileId::new(0), TileId::new(8));
        assert_eq!(line.first(), Some(&TileId::new(0)));
        assert_eq!(line.last(), Some(&TileId::new(8)));
        assert!(
            line.windows(2)
                .all(|pair| map.neighbors(pair[0]).contains(&pair[1]))
        );
    }

    #[test]
    fn embedding_is_integer_and_stable() {
        let map = map();
        assert_eq!(map.embedding(TileId::new(4)).unwrap().x, 3);
        assert_eq!(map.embedding(TileId::new(4)).unwrap().y, 3);
        assert_eq!(map.embedding(TileId::new(99)), None);
    }

    #[test]
    fn topology_invariants_hold() {
        let map = map();
        for index in 0..map.tile_count() {
            let tile = TileId::new(index as u32);
            assert_eq!(map.distance(tile, tile), 0);
            let neighbors = map.neighbors(tile);
            assert!(neighbors.windows(2).all(|pair| pair[0] < pair[1]));
            for neighbor in neighbors {
                assert!(map.neighbors(neighbor).contains(&tile));
                assert_eq!(map.distance(tile, neighbor), 1);
            }
        }
        for a in 0..map.tile_count() {
            for b in 0..map.tile_count() {
                let a = TileId::new(a as u32);
                let b = TileId::new(b as u32);
                assert_eq!(map.distance(a, b), map.distance(b, a));
                for c in 0..map.tile_count() {
                    let c = TileId::new(c as u32);
                    assert!(map.distance(a, c) <= map.distance(a, b) + map.distance(b, c));
                }
            }
        }
    }
}
