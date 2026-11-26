//! Orthographic globe camera and per-cell tangent frames. All floats are display-only.

use std::f32::consts::{FRAC_PI_2, TAU};

use eframe::egui::{Rect, Vec2, vec2};
use histeries_hex::Geodesic;
use histeries_sim::GlobeCell;

use super::{Point, Vertex};

fn dot(a: Point, b: Point) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn cross(a: Point, b: Point) -> Point {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn unit(v: Point) -> Point {
    let length = dot(v, v).sqrt();
    v.map(|x| x / length)
}

pub(super) fn position(p: [i32; 3]) -> Point {
    p.map(|v| v as f32 / Geodesic::RADIUS as f32)
}

#[derive(Clone, Copy, Default)]
pub(super) struct GlobeCamera {
    longitude: f32,
    latitude: f32,
    offset: Vec2,
    radius: f32,
    fit_radius: f32,
}

impl GlobeCamera {
    pub(super) fn fit(&mut self, rect: Rect, frequency: u32) {
        // Coarse globes have wide tangent planes and tall silhouettes at the limb.
        let cell_radius = 0.75 / frequency as f32;
        let silhouette = (1.0 + 1.7 * cell_radius.min(0.24)).hypot(cell_radius);
        self.fit_radius = rect.width().min(rect.height()) * 0.48 / silhouette;
        self.radius = self.fit_radius;
        self.offset = Vec2::ZERO;
    }

    pub(super) fn focus(&mut self, cell: &GlobeCell) {
        let [x, y, z] = unit(position(cell.center));
        self.longitude = x.atan2(z);
        self.latitude = y.clamp(-1.0, 1.0).asin();
        self.offset = Vec2::ZERO;
    }

    pub(super) fn pan(&mut self, delta: Vec2) {
        self.offset += delta;
    }

    pub(super) fn orbit(&mut self, delta: Vec2) {
        let radians = delta / self.radius.max(1.0);
        self.longitude = (self.longitude - radians.x).rem_euclid(TAU);
        self.latitude = (self.latitude + radians.y).rem_euclid(TAU);
    }

    pub(super) fn zoom(&mut self, factor: f32, at: Vec2) {
        if self.radius > 0.0 {
            let radius =
                (self.radius * factor).clamp(self.fit_radius * 0.25, self.fit_radius * 32.0);
            self.offset = at + (self.offset - at) * (radius / self.radius);
            self.radius = radius;
        }
    }
}

pub(super) struct GlobeProjection {
    right: Point,
    up: Point,
    eye: Point,
    offset: Vec2,
    ndc: Vec2,
    pub(super) radius: f32,
}

impl GlobeProjection {
    pub(super) fn new(camera: GlobeCamera, rect: Rect) -> Self {
        let (s, c) = camera.longitude.sin_cos();
        let (t, d) = camera.latitude.sin_cos();
        let ndc = vec2(2.0, -2.0) / rect.size().max(Vec2::splat(1.0));
        Self {
            right: [c, 0.0, -s],
            up: [-s * t, d, -c * t],
            eye: [s * d, t, c * d],
            offset: camera.offset * ndc,
            ndc: camera.radius * ndc,
            radius: camera.radius,
        }
    }

    pub(super) fn clip(&self, p: Point) -> Point {
        let xy = self.offset + self.ndc * vec2(dot(self.right, p), -dot(self.up, p));
        // WebGPU's far clip plane cuts every triangle at the horizon. CPU picking
        // applies the same depth range, including when the near hemisphere is fogged.
        [xy.x, xy.y, 1.0 - 0.45 * dot(self.eye, p)]
    }

    pub(super) fn light(&self, normal: Point) -> f32 {
        dot(normal, self.right) * -0.38 + dot(normal, self.up) * 0.56 + dot(normal, self.eye) * 0.73
    }

    /// An unselectable silhouette for unknown territory, independent of game data.
    pub(super) fn fog_disk(&self, vertices: &mut Vec<Vertex>) {
        const SEGMENTS: usize = 96;
        const RINGS: usize = 8;
        let rim = |ring, segment| {
            let r = (ring as f32 * FRAC_PI_2 / RINGS as f32).sin();
            let (s, c) = (segment as f32 * TAU / SEGMENTS as f32).sin_cos();
            let xy = self.offset + self.ndc * vec2(r * c, r * s);
            let light = (0.65 + 0.3 * ((1.0 - r * r).max(0.0).sqrt() - 0.45 * r * c - 0.6 * r * s))
                .clamp(0.4, 1.0);
            Vertex {
                clip: [xy.x, xy.y, 1.0],
                color: [0.28, 0.39, 0.42].map(|v| v * light),
            }
        };
        for ring in 1..=RINGS {
            for i in 0..SEGMENTS {
                let (a, b, c, d) = (
                    rim(ring - 1, i),
                    rim(ring, i),
                    rim(ring, i + 1),
                    rim(ring - 1, i + 1),
                );
                vertices.extend([a, b, c]);
                if ring > 1 {
                    vertices.extend([a, c, d]);
                }
            }
        }
    }
}

/// Local model units follow the radius of the cell and its outward normal.
#[derive(Clone, Copy)]
pub(super) struct Tangent {
    normal: Point,
    x: Point,
    y: Point,
    pub(super) scale: f32,
}

impl Tangent {
    pub(super) fn new(cell: &GlobeCell) -> Self {
        let normal = unit(position(cell.center));
        let corner = position(cell.corners[0]);
        let y = unit(cross(normal, corner));
        let x = cross(y, normal);
        let scale = cell
            .corners
            .iter()
            .map(|&p| {
                let v = position(p);
                dot(v, x).hypot(dot(v, y))
            })
            .fold(f32::INFINITY, f32::min);
        Self {
            normal,
            x,
            y,
            scale,
        }
    }

    pub(super) fn world(self, [x, y, z]: Point) -> Point {
        std::array::from_fn(|i| {
            self.normal[i] * (1.0 + z * self.scale.min(0.24))
                + self.scale * (x * self.x[i] + y * self.y[i])
        })
    }

    pub(super) fn plane(self, p: Point, factor: f32, height: f32) -> Point {
        self.world([
            factor * dot(p, self.x) / self.scale,
            factor * dot(p, self.y) / self.scale,
            height,
        ])
    }
}
