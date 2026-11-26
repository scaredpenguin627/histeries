//! The topology owned by a simulation map. Save records store its construction parameters.

pub use histeries_hex::SphereCell as GlobeCell;
use histeries_hex::{Embedding, FlatHex, Geodesic, TileId, TileTopology, TopologyError};

/// A map's immutable graph. Clones of a geodesic board share its geometry.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BoardTopology {
    /// Rectangular board with independent wrapping on each axis.
    Flat(FlatHex),
    /// Closed hexagon/pentagon sphere.
    Geodesic(Geodesic),
}

impl From<FlatHex> for BoardTopology {
    fn from(board: FlatHex) -> Self {
        Self::Flat(board)
    }
}

impl From<Geodesic> for BoardTopology {
    fn from(board: Geodesic) -> Self {
        Self::Geodesic(board)
    }
}

impl BoardTopology {
    /// Builds a sphere of the requested subdivision frequency.
    pub fn geodesic(frequency: u32) -> Result<Self, TopologyError> {
        Geodesic::new(frequency).map(Self::Geodesic)
    }

    /// Returns the subdivision frequency on a sphere.
    pub fn globe_frequency(&self) -> Option<u32> {
        match self {
            Self::Flat(_) => None,
            Self::Geodesic(board) => Some(board.frequency()),
        }
    }

    /// Returns polygon-edge slots. Five-sided cells have no sixth edge.
    pub fn edge_neighbors(&self, tile: TileId) -> [Option<TileId>; 6] {
        match self {
            Self::Flat(board) => board.edge_neighbors(tile),
            Self::Geodesic(board) => board.edge_neighbors(tile),
        }
    }

    pub(super) fn reverse_slot(&self, tile: TileId, slot: usize, neighbor: TileId) -> usize {
        match self {
            // Wrapped narrow boards may have multiple edges to the same tile.
            Self::Flat(_) => (slot + 3) % 6,
            Self::Geodesic(board) => board
                .edge_neighbors(neighbor)
                .iter()
                .position(|adjacent| *adjacent == Some(tile))
                .expect("reciprocal geodesic edge"),
        }
    }

    /// Returns the integer surface coordinates of a spherical cell.
    pub fn surface(&self, tile: TileId) -> Option<GlobeCell> {
        match self {
            Self::Flat(_) => None,
            Self::Geodesic(board) => board.surface(tile),
        }
    }

    fn graph(&self) -> &dyn TileTopology {
        match self {
            Self::Flat(board) => board,
            Self::Geodesic(board) => board,
        }
    }
}

impl TileTopology for BoardTopology {
    fn tile_count(&self) -> usize {
        self.graph().tile_count()
    }
    fn contains(&self, tile: TileId) -> bool {
        self.graph().contains(tile)
    }
    fn neighbors(&self, tile: TileId) -> Vec<TileId> {
        self.graph().neighbors(tile)
    }
    fn distance(&self, from: TileId, to: TileId) -> u32 {
        self.graph().distance(from, to)
    }
    fn heuristic(&self, from: TileId, to: TileId) -> u32 {
        self.graph().heuristic(from, to)
    }
    fn ring(&self, center: TileId, radius: u32) -> Vec<TileId> {
        self.graph().ring(center, radius)
    }
    fn area(&self, center: TileId, radius: u32) -> Vec<TileId> {
        self.graph().area(center, radius)
    }
    fn line(&self, from: TileId, to: TileId) -> Vec<TileId> {
        self.graph().line(from, to)
    }
    fn embedding(&self, tile: TileId) -> Option<Embedding> {
        self.graph().embedding(tile)
    }
}
