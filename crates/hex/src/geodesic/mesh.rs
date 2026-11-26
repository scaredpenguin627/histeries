use std::collections::BTreeMap;

use super::{AtlasFace, Cell, Embedding, Geodesic, SphereCell, TileId};

// Cyclic permutations of (±1, ±phi, 0), scaled and rounded once. Vertex IDs
// and outward triangle order below are part of the board's indexing contract.
const U: i64 = 1_000_000;
const P: i64 = 1_618_034;
const VERTICES: [[i64; 3]; 12] = [
    [-U, P, 0],
    [U, P, 0],
    [-U, -P, 0],
    [U, -P, 0],
    [0, -U, P],
    [0, U, P],
    [0, -U, -P],
    [0, U, -P],
    [P, 0, -U],
    [P, 0, U],
    [-P, 0, -U],
    [-P, 0, U],
];
const FACES: [[usize; 3]; 20] = [
    [0, 11, 5],
    [0, 5, 1],
    [0, 1, 7],
    [0, 7, 10],
    [0, 10, 11],
    [1, 5, 9],
    [5, 11, 4],
    [11, 10, 2],
    [10, 7, 6],
    [7, 1, 8],
    [3, 9, 4],
    [3, 4, 2],
    [3, 2, 6],
    [3, 6, 8],
    [3, 8, 9],
    [4, 9, 5],
    [2, 4, 11],
    [6, 2, 10],
    [8, 6, 7],
    [9, 8, 1],
];

pub(super) fn atlas_face(chart: [u32; 2]) -> Option<AtlasFace> {
    let [column, row] = chart;
    if column >= 5 || row >= 4 {
        return None;
    }
    let vertices = FACES[(row * 5 + column) as usize].map(|i| VERTICES[i].map(|x| x as i32));
    Some(AtlasFace { chart, vertices })
}

struct Wedge {
    before: TileId,
    after: TileId,
    corner: [i32; 3],
}

struct Vertex {
    center: [i32; 3],
    atlas: Option<Embedding>,
    wedges: Vec<Wedge>,
}

impl Vertex {
    fn new(direction: [i64; 3]) -> Self {
        Self {
            center: project(direction),
            atlas: None,
            wedges: Vec::with_capacity(6),
        }
    }

    fn finish(self) -> Cell {
        // Each triangle supplies one oriented wedge between two neighbors.
        // Join them by ID, never by angular approximation near a seam or pole.
        let degree = self.wedges.len();
        let mut wedge = self
            .wedges
            .iter()
            .min_by_key(|w| w.after)
            .expect("closed vertex");
        let mut neighbors = Vec::with_capacity(degree);
        let mut corners = Vec::with_capacity(degree);
        for _ in 0..degree {
            neighbors.push(wedge.after);
            corners.push(wedge.corner);
            wedge = self
                .wedges
                .iter()
                .find(|w| w.before == wedge.after)
                .expect("outbound wedge");
        }
        debug_assert_eq!(wedge.after, neighbors[0]);
        Cell {
            neighbors,
            surface: SphereCell {
                center: self.center,
                corners,
            },
            atlas: self.atlas.expect("each vertex belongs to a face"),
        }
    }
}

pub(super) fn subdivide(frequency: u32) -> Vec<Cell> {
    let mut vertices: Vec<_> = VERTICES.into_iter().map(Vertex::new).collect();
    // The global barycentric weights identify an edge point identically from
    // either incident face, even when its local edge direction reverses.
    let mut ids = BTreeMap::<[u8; 12], TileId>::new();
    for index in 0..12 {
        let mut weights = [0; 12];
        weights[index] = frequency as u8;
        ids.insert(weights, TileId::new(index as u32));
    }
    for (face_index, face) in FACES.into_iter().enumerate() {
        let mut rows = Vec::new();
        for i in 0..=frequency {
            let mut row = Vec::new();
            for j in 0..=frequency - i {
                let local = [frequency - i - j, i, j];
                let mut weights = [0; 12];
                for (vertex, weight) in face.into_iter().zip(local) {
                    weights[vertex] = weight as u8;
                }
                let id = *ids.entry(weights).or_insert_with(|| {
                    let direction = std::array::from_fn(|axis| {
                        face.into_iter()
                            .zip(local)
                            .map(|(vertex, weight)| VERTICES[vertex][axis] * i64::from(weight))
                            .sum()
                    });
                    let id = TileId::new(vertices.len() as u32);
                    vertices.push(Vertex::new(direction));
                    id
                });
                // First incident face owns this point in the planar display atlas.
                vertices[id.index() as usize]
                    .atlas
                    .get_or_insert(Embedding {
                        x: ((face_index as u32 % 5) * (2 * frequency + 4) + 2 * i + j) as i32,
                        y: ((face_index as u32 / 5) * (3 * frequency + 6) + 3 * j) as i32,
                    });
                row.push(id);
            }
            rows.push(row);
        }
        for i in 0..frequency as usize {
            let row = &rows[i];
            let next = &rows[i + 1];
            for j in 0..next.len() {
                add_triangle(&mut vertices, [row[j], next[j], row[j + 1]]);
                if j + 1 < next.len() {
                    add_triangle(&mut vertices, [next[j], next[j + 1], row[j + 1]]);
                }
            }
        }
    }
    vertices.into_iter().map(Vertex::finish).collect()
}

fn add_triangle(vertices: &mut [Vertex], triangle: [TileId; 3]) {
    // Project the spherical triangle's centroid once. All three incident cells
    // receive its identical fixed-point representation.
    let centers = triangle.map(|tile| vertices[tile.index() as usize].center);
    let corner = project(std::array::from_fn(|axis| {
        centers.iter().map(|v| i64::from(v[axis])).sum()
    }));
    for i in 0..3 {
        vertices[triangle[i].index() as usize].wedges.push(Wedge {
            before: triangle[(i + 1) % 3],
            after: triangle[(i + 2) % 3],
            corner,
        });
    }
}

fn project(direction: [i64; 3]) -> [i32; 3] {
    // The longest possible input is 32 times one original vertex (< 61 million).
    // Round the integer square root, then each signed component, to nearest.
    let square: i64 = direction.iter().map(|&v| v * v).sum();
    let floor = square.isqrt();
    let length = floor + i64::from(square - floor * floor > floor);
    direction
        .map(|v| ((v * i64::from(Geodesic::RADIUS) + v.signum() * (length / 2)) / length) as i32)
}
