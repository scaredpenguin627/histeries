//! Faceted models in tile-local display units. Models share a fixed world-space light.

use eframe::egui::{Vec2, vec2};
use histeries_rules_core::PlayerId;
use histeries_sim::{Feature, ResourceKind, Terrain, UnitKind, VisibleTileObservation};

use super::{Material, Point, Projection, Vertex};

pub(super) const BASE: f32 = -0.35;
pub(super) const HEADROOM: f32 = 1.45;
pub(super) const MAX_ALTITUDE: f32 = 2.2;
// Below this projected tile radius, individual architectural details are subpixel.
const OVERVIEW_RADIUS: f32 = 8.0;
const HEX: [[f32; 2]; 6] = [
    [0.0, -1.0],
    [0.8660254, -0.5],
    [0.8660254, 0.5],
    [0.0, 1.0],
    [-0.8660254, 0.5],
    [-0.8660254, -0.5],
];
const SQUARE: [[f32; 2]; 4] = [[-1.0, -1.0], [1.0, -1.0], [1.0, 1.0], [-1.0, 1.0]];
const STONE: Material = [226, 210, 167];
const TIMBER: Material = [108, 79, 54];
const CREAM: Material = [250, 224, 163];

pub(super) fn elevation(terrain: Terrain) -> f32 {
    match terrain {
        Terrain::Ocean => -0.17,
        Terrain::Coast => -0.10,
        Terrain::Hill => 0.51,
        Terrain::Mountain => 0.44,
        Terrain::Tundra => 0.16,
        Terrain::Plains | Terrain::Grassland | Terrain::Desert => 0.13,
    }
}

fn pigment(terrain: Terrain) -> Material {
    match terrain {
        Terrain::Plains => [197, 177, 113],
        Terrain::Grassland => [139, 169, 101],
        Terrain::Desert => [224, 178, 112],
        Terrain::Coast => [94, 166, 168],
        Terrain::Ocean => [57, 118, 145],
        Terrain::Tundra => [167, 180, 161],
        Terrain::Hill => [160, 152, 105],
        Terrain::Mountain => [133, 133, 121],
    }
}

fn resource_color(resource: ResourceKind) -> Material {
    match resource {
        ResourceKind::Wheat => [245, 206, 102],
        ResourceKind::Stone => [184, 188, 182],
        ResourceKind::Iron => [91, 96, 108],
        ResourceKind::Gold => [240, 183, 68],
    }
}

pub(super) fn livery(owner: Option<PlayerId>, seat: PlayerId) -> Material {
    match owner {
        Some(player) if player == seat => [34, 99, 94],
        Some(_) => [167, 76, 52],
        None => [89, 71, 81],
    }
}

// A mesh borrows one frame's vertex stream. No geometry survives seat changes.
pub(super) struct Mesh<'a> {
    projection: &'a Projection,
    vertices: &'a mut Vec<Vertex>,
    center: Point,
}

impl<'a> Mesh<'a> {
    pub(super) fn new(
        projection: &'a Projection,
        vertices: &'a mut Vec<Vertex>,
        center: Vec2,
        height: f32,
    ) -> Self {
        Self {
            projection,
            vertices,
            center: [center.x, center.y, height],
        }
    }

    fn triangle(&mut self, a: Point, b: Point, c: Point, pigment: Material) {
        let sub = |p: Point| [p[0] - a[0], p[1] - a[1], p[2] - a[2]];
        let ([x, y, z], [u, v, w]) = (sub(b), sub(c));
        let n = [y * w - z * v, z * u - x * w, x * v - y * u];
        let length = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
        if length <= f32::EPSILON {
            return;
        }
        let sun = (-0.40 * n[0] - 0.46 * n[1] + 0.79 * n[2]) / length;
        let shade = 0.68 + 0.32 * sun.max(0.0);
        let color = pigment.map(|v| f32::from(v) * (shade / 255.0));
        self.vertices.extend([a, b, c].map(|[x, y, z]| Vertex {
            clip: self.projection.clip([
                self.center[0] + x,
                self.center[1] + y,
                self.center[2] + z,
            ]),
            color,
        }));
    }

    fn quad(&mut self, [a, b, c, d]: [Point; 4], color: Material) {
        self.triangle(a, b, c, color);
        self.triangle(a, c, d, color);
    }

    /// A horizontal footprint swept between two cross-sections. Zero tip radius makes a peak.
    fn solid<const N: usize>(
        &mut self,
        footprint: &[[f32; 2]; N],
        [x, y, z]: Point,
        bottom: Vec2,
        top: Vec2,
        height: f32,
        color: Material,
    ) {
        let ring =
            |radius: Vec2, at| footprint.map(|[u, v]| [x + radius.x * u, y + radius.y * v, at]);
        let low = ring(bottom, z);
        let high = ring(top, z + height);
        for i in 0..N {
            let j = (i + 1) % N;
            self.triangle(low[i], low[j], high[j], color);
            if top != Vec2::ZERO {
                self.triangle(low[i], high[j], high[i], color);
            }
        }
        if top != Vec2::ZERO {
            for i in 1..N - 1 {
                self.triangle(high[0], high[i], high[i + 1], color);
            }
        }
    }

    fn block(&mut self, at: Point, extent: Vec2, h: f32, color: Material) {
        self.solid(&SQUARE, at, extent, extent, h, color);
    }

    fn inset_edge(&mut self, i: usize, inner: f32, outer: f32, z: f32, color: Material) {
        let at = |edge: usize, radius| [HEX[edge][0] * radius, HEX[edge][1] * radius, z];
        let j = (i + 1) % 6;
        self.quad(
            [at(i, outer), at(j, outer), at(j, inner), at(i, inner)],
            color,
        );
    }

    fn overview_land(&mut self, tile: &VisibleTileObservation, selected: bool) {
        // The six edges meet every adjacent tile at the same altitude. The summit
        // carries the relief, so even an isolated visible tile needs no side walls.
        let rim = elevation(Terrain::Ocean) - self.center[2];
        let corners = HEX.map(|[x, y]| [x, y, rim]);
        let rise = if tile.terrain == Terrain::Mountain {
            1.18
        } else {
            0.0
        };
        let peak = [0.0, 0.0, rise];
        let color = match tile.feature {
            Feature::Forest => [70, 125, 84],
            Feature::Jungle => [59, 121, 78],
            Feature::Marsh => [98, 151, 134],
            Feature::None => pigment(tile.terrain),
        };
        // Subdivide the surface itself so its colored bands follow the relief.
        let inset =
            |p: Point, radius: f32| [radius * p[0], radius * p[1], rise + radius * (rim - rise)];
        for (i, &a) in corners.iter().enumerate() {
            let b = corners[(i + 1) % 6];
            let (mut outer_a, mut outer_b) = (a, b);
            let river = tile.river.is_some_and(|edge| edge as usize == i);
            if selected || river {
                let edge_color = if river {
                    [85, 170, 192]
                } else {
                    [254, 207, 91]
                };
                let (inner_a, inner_b) = (inset(a, 0.75), inset(b, 0.75));
                self.quad([a, b, inner_b, inner_a], edge_color);
                (outer_a, outer_b) = (inner_a, inner_b);
            }
            if tile.road && matches!(i, 4 | 5) {
                let (road_a, road_b) = (inset(a, 0.65), inset(b, 0.65));
                let (inner_a, inner_b) = (inset(a, 0.36), inset(b, 0.36));
                self.quad([outer_a, outer_b, road_b, road_a], color);
                self.quad([road_a, road_b, inner_b, inner_a], [142, 97, 66]);
                (outer_a, outer_b) = (inner_a, inner_b);
            }
            self.triangle(peak, outer_a, outer_b, color);
        }
        if let Some(resource) = tile.resource {
            // The deposit sits on the east facet, opposite the road. Its base
            // altitude interpolates between the summit and that facet's outer edge.
            let surface = |x: f32, y| [x, y, rise + (rim - rise) * (x / HEX[1][0])];
            let base =
                [(0.43, -0.24), (0.64, 0.0), (0.43, 0.24), (0.21, 0.0)].map(|(x, y)| surface(x, y));
            let mut crest = surface(0.43, 0.0);
            crest[2] += 0.20;
            for i in 0..4 {
                self.triangle(base[i], base[(i + 1) % 4], crest, resource_color(resource));
            }
        }
    }

    pub(super) fn land(&mut self, tile: &VisibleTileObservation, selected: bool) {
        if self.projection.scale < OVERVIEW_RADIUS {
            self.overview_land(tile, selected);
            return;
        }
        let color = pigment(tile.terrain);
        let bottom = BASE - self.center[2];
        let bevel = if tile.terrain == Terrain::Hill {
            0.29
        } else {
            0.075
        };
        let rim = Vec2::splat(0.989);
        self.solid(&HEX, [0.0, 0.0, bottom], rim, rim, -bevel - bottom, color);
        self.solid(
            &HEX,
            [0.0, 0.0, -bevel],
            rim,
            Vec2::splat(0.89),
            bevel,
            color,
        );
        match tile.terrain {
            Terrain::Mountain => {
                self.solid(
                    &HEX,
                    [-0.1, -0.12, 0.0],
                    vec2(0.62, 0.64),
                    vec2(0.19, 0.20),
                    0.85,
                    [140, 145, 137],
                );
                self.solid(
                    &HEX,
                    [-0.1, -0.12, 0.85],
                    vec2(0.19, 0.20),
                    Vec2::ZERO,
                    0.39,
                    [231, 232, 212],
                );
                self.solid(
                    &HEX,
                    [0.28, 0.18, 0.0],
                    vec2(0.39, 0.4),
                    Vec2::ZERO,
                    0.62,
                    [160, 156, 137],
                );
            }
            Terrain::Ocean | Terrain::Coast => {
                for (x, y) in [(-0.24, -0.15), (0.17, 0.18)] {
                    self.quad(
                        [
                            [x - 0.17, y, 0.006],
                            [x + 0.10, y - 0.025, 0.006],
                            [x + 0.16, y, 0.006],
                            [x - 0.07, y + 0.018, 0.006],
                        ],
                        [148, 201, 192],
                    );
                }
            }
            _ => {}
        }
        match tile.feature {
            Feature::Forest | Feature::Jungle => {
                // Leave room for the figures and roofs on an inhabited tile.
                let grove: &[(f32, f32, f32)] = if tile.occupied {
                    &[(0.47, -0.47, 0.71)]
                } else {
                    &[
                        (-0.33, -0.1, 0.71),
                        (-0.02, -0.33, 0.92),
                        (0.29, -0.08, 0.66),
                    ]
                };
                for &(x, y, h) in grove {
                    self.block([x, y, 0.0], Vec2::splat(0.044), 0.24, TIMBER);
                    if tile.feature == Feature::Forest {
                        self.solid(
                            &HEX,
                            [x, y, 0.16],
                            Vec2::splat(0.24),
                            Vec2::ZERO,
                            h * 0.69,
                            [54, 109, 76],
                        );
                        self.solid(
                            &HEX,
                            [x, y, 0.16 + h * 0.31],
                            Vec2::splat(0.17),
                            Vec2::ZERO,
                            h * 0.58,
                            [70, 130, 85],
                        );
                    } else {
                        self.solid(
                            &HEX,
                            [x, y, 0.21],
                            Vec2::splat(0.13),
                            Vec2::splat(0.30),
                            h * 0.23,
                            [43, 106, 74],
                        );
                        self.solid(
                            &HEX,
                            [x, y, 0.21 + h * 0.23],
                            Vec2::splat(0.30),
                            Vec2::splat(0.16),
                            h * 0.28,
                            [84, 145, 86],
                        );
                    }
                }
            }
            Feature::Marsh => {
                self.solid(
                    &HEX,
                    [-0.12, -0.11, 0.001],
                    vec2(0.44, 0.29),
                    vec2(0.44, 0.29),
                    0.01,
                    [98, 151, 134],
                );
                for x in [-0.32, -0.17, 0.06] {
                    self.solid(
                        &SQUARE,
                        [x, -0.11, 0.01],
                        vec2(0.045, 0.025),
                        Vec2::ZERO,
                        0.28,
                        [125, 132, 71],
                    );
                }
            }
            Feature::None => {}
        }
        if tile.road {
            let strip = |x: f32, y: f32| [[x, y - 0.06, 0.012], [x, y + 0.06, 0.012]];
            let [a, b] = strip(-0.68, 0.04);
            let [c, d] = strip(-0.13, 0.23);
            let [e, f] = strip(0.64, 0.15);
            self.quad([a, c, d, b], [147, 110, 77]);
            self.quad([c, e, f, d], [147, 110, 77]);
        }
        if let Some(edge) = tile.river {
            self.inset_edge(edge as usize, 0.785, 0.89, 0.014, [85, 170, 192]);
        }
        if let Some(resource) = tile.resource {
            self.deposit(resource);
        }
        if selected {
            for edge in 0..6 {
                self.inset_edge(edge, 0.816, 0.873, 0.025, [254, 207, 91]);
            }
        }
    }

    fn deposit(&mut self, resource: ResourceKind) {
        match resource {
            ResourceKind::Wheat => {
                for (x, y) in [(0.30, 0.34), (0.47, 0.23), (0.49, 0.42)] {
                    self.block([x, y, 0.0], vec2(0.023, 0.023), 0.20, TIMBER);
                    self.solid(
                        &HEX,
                        [x, y, 0.17],
                        vec2(0.065, 0.052),
                        Vec2::ZERO,
                        0.19,
                        resource_color(resource),
                    );
                }
            }
            _ => {
                let color = resource_color(resource);
                for (x, y, r) in [(0.36, 0.26, 0.18), (0.49, 0.39, 0.12)] {
                    self.solid(
                        &HEX,
                        [x, y, 0.0],
                        Vec2::splat(r),
                        vec2(r * 0.58, r * 0.64),
                        1.2 * r,
                        color,
                    );
                }
            }
        }
    }

    fn house(&mut self, x: f32, y: f32, extent: Vec2, h: f32, roof: Material) {
        self.block([x, y, 0.0], extent, h, STONE);
        let e = extent * 1.12;
        let ridge = vec2(e.x * 0.65, 0.0);
        self.solid(&SQUARE, [x, y, h], e, ridge, e.y * 0.94, roof);
    }

    pub(super) fn town(&mut self, accent: Material) {
        if self.projection.scale < OVERVIEW_RADIUS {
            self.house(0.0, -0.22, vec2(0.37, 0.29), 0.60, accent);
            return;
        }
        self.house(-0.30, -0.14, vec2(0.18, 0.19), 0.34, accent);
        self.house(0.29, -0.08, vec2(0.19, 0.16), 0.27, accent);
        self.house(0.04, -0.40, vec2(0.16, 0.16), 0.65, accent);
        self.block([0.04, -0.40, 0.75], vec2(0.018, 0.018), 0.34, TIMBER);
        self.triangle(
            [0.04, -0.40, 1.09],
            [0.31, -0.40, 1.02],
            [0.04, -0.40, 0.94],
            accent,
        );
    }

    pub(super) fn district(&mut self, accent: Material) {
        if self.projection.scale < OVERVIEW_RADIUS {
            self.solid(
                &SQUARE,
                [0.0, -0.12, 0.0],
                vec2(0.33, 0.28),
                Vec2::ZERO,
                0.52,
                accent,
            );
            return;
        }
        self.house(0.0, -0.12, vec2(0.31, 0.23), 0.30, accent);
    }

    pub(super) fn unit(&mut self, kind: UnitKind, x: f32, accent: Material) {
        if self.projection.scale < OVERVIEW_RADIUS {
            // Face the pennant towards the camera so it stays legible around the orbit.
            let right = self.projection.axes.right;
            let flag = |along: f32, z| [x + along * right.x, 0.36 + along * right.y, z];
            self.quad(
                [
                    flag(-0.18, 0.0),
                    flag(-0.10, 0.0),
                    flag(-0.10, 0.96),
                    flag(-0.18, 0.96),
                ],
                CREAM,
            );
            self.triangle(
                flag(-0.10, 0.94),
                flag(0.44, 0.77),
                flag(-0.10, 0.51),
                accent,
            );
            return;
        }
        // Turn markers carry small standing figures. The colored base survives distant zooms.
        let y = 0.32;
        self.solid(
            &HEX,
            [x, y, 0.0],
            vec2(0.24, 0.19),
            vec2(0.23, 0.18),
            0.08,
            accent,
        );
        for dx in [-0.071, 0.071] {
            self.block([x + dx, y, 0.08], vec2(0.044, 0.054), 0.19, [79, 76, 66]);
        }
        self.solid(
            &SQUARE,
            [x, y, 0.25],
            vec2(0.106, 0.085),
            vec2(0.134, 0.095),
            0.25,
            accent,
        );
        self.solid(
            &HEX,
            [x, y, 0.49],
            vec2(0.09, 0.085),
            vec2(0.081, 0.08),
            0.14,
            CREAM,
        );
        match kind {
            UnitKind::Warrior | UnitKind::Barbarian => {
                self.solid(
                    &HEX,
                    [x, y, 0.595],
                    vec2(0.114, 0.11),
                    Vec2::splat(0.052),
                    0.08,
                    [140, 153, 149],
                );
                self.block([x + 0.16, y, 0.09], Vec2::splat(0.018), 0.65, TIMBER);
                self.solid(
                    &SQUARE,
                    [x + 0.16, y, 0.73],
                    vec2(0.039, 0.03),
                    Vec2::ZERO,
                    0.14,
                    [210, 211, 191],
                );
                self.solid(
                    &SQUARE,
                    [x - 0.13, y + 0.08, 0.18],
                    vec2(0.04, 0.04),
                    vec2(0.105, 0.05),
                    0.24,
                    accent,
                );
            }
            UnitKind::Archer => {
                for (dx, h) in [(0.12, 0.29), (0.17, 0.22)] {
                    self.block([x + dx, y, 0.23], vec2(0.023, 0.025), h, TIMBER);
                }
                self.solid(
                    &HEX,
                    [x, y, 0.61],
                    Vec2::splat(0.10),
                    Vec2::ZERO,
                    0.09,
                    accent,
                );
            }
            UnitKind::Builder | UnitKind::Settler => {
                self.solid(
                    &HEX,
                    [x, y, 0.59],
                    Vec2::splat(0.145),
                    Vec2::splat(0.066),
                    0.078,
                    [190, 152, 91],
                );
                if kind == UnitKind::Builder {
                    self.block([x + 0.16, y, 0.19], Vec2::splat(0.024), 0.33, TIMBER);
                    self.block(
                        [x + 0.16, y, 0.50],
                        vec2(0.105, 0.04),
                        0.068,
                        [118, 125, 121],
                    );
                } else {
                    self.block([x, y - 0.11, 0.23], vec2(0.12, 0.086), 0.23, TIMBER);
                }
            }
        }
    }
}
