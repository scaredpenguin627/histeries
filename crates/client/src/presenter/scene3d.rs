//! Orthographic diorama built exclusively from one board observation.

mod globe;
#[cfg(not(target_env = "ohos"))]
pub(crate) mod gpu;
mod models;
#[cfg(not(target_env = "ohos"))]
mod pipeline;
#[cfg(all(test, not(target_env = "ohos")))]
mod tests;

use std::ops::Range;

use emath::{Pos2, Rect, Vec2, pos2, vec2};
use histeries_hex::TileId;
use histeries_rules_core::PlayerId;
use histeries_sim::{BoardObservation, VisibleTileObservation};

use crate::geometry::{visible, world};
use models::{Mesh, elevation};

type Point = [f32; 3]; // display x, display y, altitude
type Material = [u8; 3];

/// Gamma-encoded sRGB and WebGPU clip coordinates (orthographic w = 1).
#[derive(Clone, Copy, Debug)]
pub(super) struct Vertex {
    pub(super) clip: [f32; 3],
    pub(super) color: [f32; 3],
}

pub(super) struct Camera3d {
    globe: Option<globe::GlobeCamera>,
    center: Vec2,
    yaw: f32,
    pitch: f32,
    scale: f32,
    min_scale: f32,
}

impl Default for Camera3d {
    fn default() -> Self {
        Self {
            globe: None,
            center: Vec2::ZERO,
            yaw: -0.33,
            pitch: 0.78,
            scale: 48.0,
            min_scale: 1.0,
        }
    }
}

#[derive(Clone, Copy)]
struct Axes {
    right: Vec2,
    down: Vec2,
    sin_pitch: f32,
    cos_pitch: f32,
}

impl Axes {
    fn screen(self, ground: Vec2, altitude: f32) -> Vec2 {
        vec2(
            ground.dot(self.right),
            ground.dot(self.down) * self.sin_pitch - altitude * self.cos_pitch,
        )
    }

    fn ground_delta(self, screen: Vec2) -> Vec2 {
        self.right * screen.x + self.down * (screen.y / self.sin_pitch)
    }
}

impl Camera3d {
    fn axes(&self) -> Axes {
        let (s, c) = self.yaw.sin_cos();
        let (sin_pitch, cos_pitch) = self.pitch.sin_cos();
        Axes {
            right: vec2(c, -s),
            down: vec2(s, c),
            sin_pitch,
            cos_pitch,
        }
    }

    pub(super) fn focus(&mut self, tile: &VisibleTileObservation) {
        if let Some(cell) = &tile.surface {
            self.globe.get_or_insert_default().focus(cell);
            return;
        }
        let axes = self.axes();
        let position = axes.screen(world(tile.embedding), elevation(tile.terrain));
        self.center = axes.ground_delta(position);
    }

    pub(super) fn fit(&mut self, view: &BoardObservation, rect: Rect) {
        if let Some(frequency) = view.globe_frequency()
            && rect.is_positive()
        {
            self.globe.get_or_insert_default().fit(rect, frequency);
            return;
        }
        self.globe = None;
        if view.visible_tiles().is_empty() || !rect.is_positive() {
            return;
        }
        let axes = self.axes();
        let mut bounds = Rect::NOTHING;
        for tile in view.visible_tiles() {
            let ground = world(tile.embedding);
            let vertical = |height| axes.screen(ground, height);
            // Every mesh lies inside its unit-radius footprint, from plinth to peak.
            bounds.extend_with(
                (vertical(elevation(tile.terrain) + models::HEADROOM) - vec2(1.0, axes.sin_pitch))
                    .to_pos2(),
            );
            bounds.extend_with((vertical(models::BASE) + vec2(1.0, axes.sin_pitch)).to_pos2());
        }
        self.center = axes.ground_delta(bounds.center().to_vec2());
        self.scale = (0.84 * (rect.width() / bounds.width()).min(rect.height() / bounds.height()))
            .min(144.0);
        self.min_scale = self.scale.min(8.0) * 0.5;
    }

    pub(super) fn pan(&mut self, delta: Vec2) {
        if let Some(globe) = &mut self.globe {
            globe.pan(delta);
            return;
        }
        self.center -= self.axes().ground_delta(delta / self.scale);
    }

    pub(super) fn orbit(&mut self, delta: Vec2) {
        if let Some(globe) = &mut self.globe {
            globe.orbit(delta);
            return;
        }
        self.yaw = (self.yaw + delta.x * 0.009).rem_euclid(std::f32::consts::TAU);
        self.pitch = (self.pitch + delta.y * 0.009).clamp(0.38, 1.32);
    }

    pub(super) fn zoom(&mut self, factor: f32, at: Vec2) {
        if let Some(globe) = &mut self.globe {
            globe.zoom(factor, at);
            return;
        }
        let scale = (self.scale * factor).clamp(self.min_scale, 144.0);
        self.center += self
            .axes()
            .ground_delta(at * (self.scale.recip() - scale.recip()));
        self.scale = scale;
    }
}

/// Build the desktop game's depth-tested geometry for an external display surface.
pub fn surface_vertices(
    view: &histeries_sim::Observation,
    width: u32,
    height: u32,
) -> Vec<[f32; 6]> {
    if width == 0 || height == 0 {
        return Vec::new();
    }
    let rect = Rect::from_min_size(Pos2::ZERO, vec2(width as f32, height as f32));
    let mut camera = Camera3d::default();
    camera.fit(view.board(), rect);
    if let Some(tile) = crate::geometry::home(view).and_then(|id| visible(view.board(), id)) {
        camera.focus(tile);
    }
    Scene::build(view.board(), view.seat(), &camera, rect, None)
        .vertices
        .into_iter()
        .map(|v| {
            let [x, y, z] = v.clip;
            let [r, g, b] = v.color;
            [x, y, z, r, g, b]
        })
        .collect()
}

struct Projection {
    globe: Option<globe::GlobeProjection>,
    axes: Axes,
    center: Vec2,
    scale: f32,
    ndc_scale: Vec2,
    depth_scale: f32,
    rect: Rect,
}

impl Projection {
    fn new(camera: &Camera3d, rect: Rect) -> Self {
        let axes = camera.axes();
        let half = rect.size().max(Vec2::splat(1.0)) * 0.5;
        // For visible geometry, eye distance is screen_y * cot(pitch) + z / sin(pitch).
        // Size the depth interval to the viewport, independently of map dimensions.
        let eye_limit =
            (half.y / camera.scale * axes.cos_pitch + models::MAX_ALTITUDE) / axes.sin_pitch;
        Self {
            globe: camera
                .globe
                .map(|globe| globe::GlobeProjection::new(globe, rect)),
            axes,
            center: camera.center,
            scale: camera.scale,
            ndc_scale: vec2(camera.scale / half.x, -camera.scale / half.y),
            depth_scale: 0.48 / eye_limit,
            rect,
        }
    }

    fn clip(&self, [x, y, z]: Point) -> [f32; 3] {
        if let Some(globe) = &self.globe {
            return globe.clip([x, y, z]);
        }
        let ground = vec2(x, y) - self.center;
        let screen = self.axes.screen(ground, z) * self.ndc_scale;
        let eye = ground.dot(self.axes.down) * self.axes.cos_pitch + z * self.axes.sin_pitch;
        [screen.x, screen.y, 0.5 - eye * self.depth_scale]
    }

    fn footprint(&self, center: Vec2, top: f32) -> Rect {
        let plane = self.rect.center() + self.axes.screen(center - self.center, 0.0) * self.scale;
        let pad = vec2(1.0, self.axes.sin_pitch) * self.scale;
        Rect::from_min_max(
            plane - pad - vec2(0.0, top * self.axes.cos_pitch * self.scale),
            plane + pad - vec2(0.0, models::BASE * self.axes.cos_pitch * self.scale),
        )
    }
}

struct Batch {
    owner: TileId,
    bounds: Rect,
    vertices: Range<usize>,
}

pub(super) struct Scene {
    pub(super) vertices: Vec<Vertex>,
    batches: Vec<Batch>,
    rect: Rect,
}

impl Scene {
    pub(super) fn build(
        view: &BoardObservation,
        seat: PlayerId,
        camera: &Camera3d,
        rect: Rect,
        selected: Option<TileId>,
    ) -> Self {
        let mut scene = Self {
            vertices: Vec::new(),
            batches: Vec::new(),
            rect,
        };
        let projection = Projection::new(camera, rect);
        if let Some(globe) = &projection.globe {
            globe.fog_disk(&mut scene.vertices);
        }
        for tile in view.visible_tiles() {
            scene.batch(&projection, tile, |mesh| {
                mesh.land(tile, selected == Some(tile.tile))
            });
        }
        for city in view.visible_cities() {
            let accent = models::livery(Some(city.owner), seat);
            if let Some(tile) = visible(view, city.tile) {
                scene.batch(&projection, tile, |mesh| mesh.town(accent));
            }
            for &district in &city.districts {
                if district != city.tile
                    && let Some(tile) = visible(view, district)
                {
                    scene.batch(&projection, tile, |mesh| mesh.district(accent));
                }
            }
        }
        for unit in view.visible_units() {
            if let Some(tile) = visible(view, unit.tile) {
                let stacked = tile.units.military.is_some() && tile.units.civilian.is_some();
                let offset = if !stacked {
                    0.0
                } else if tile.units.military == Some(unit.id) {
                    -0.24
                } else {
                    0.24
                };
                scene.batch(&projection, tile, |mesh| {
                    mesh.unit(unit.kind, offset, models::livery(unit.owner, seat));
                });
            }
        }
        scene
    }

    fn batch(
        &mut self,
        projection: &Projection,
        tile: &VisibleTileObservation,
        draw: impl FnOnce(&mut Mesh<'_>),
    ) {
        if let (Some(globe), Some(cell)) = (&projection.globe, &tile.surface) {
            let frame = globe::Tangent::new(cell);
            let center = globe.clip(globe::position(cell.center));
            // Enclose all local terrain and models before emitting detailed geometry.
            let pad = frame.scale * 2.8;
            let at = self.rect.center() + vec2(center[0], -center[1]) * self.rect.size() * 0.5;
            let bounds = Rect::from_center_size(at, Vec2::splat(2.0 * pad * globe.radius));
            if center[2] - 0.45 * pad <= 1.0 && self.rect.intersects(bounds) {
                let start = self.vertices.len();
                draw(&mut Mesh::on_globe(
                    projection,
                    &mut self.vertices,
                    frame,
                    tile.terrain,
                ));
                self.batches.push(Batch {
                    owner: tile.tile,
                    bounds,
                    vertices: start..self.vertices.len(),
                });
            }
            return;
        }
        let center = world(tile.embedding);
        let height = elevation(tile.terrain);
        let bounds = projection.footprint(center, height + models::HEADROOM);
        if self.rect.intersects(bounds) {
            let start = self.vertices.len();
            draw(&mut Mesh::new(
                projection,
                &mut self.vertices,
                center,
                height,
            ));
            self.batches.push(Batch {
                owner: tile.tile,
                bounds,
                vertices: start..self.vertices.len(),
            });
        }
    }

    pub(super) fn highlight(&mut self, tile: TileId) {
        for batch in self.batches.iter().filter(|batch| batch.owner == tile) {
            for vertex in &mut self.vertices[batch.vertices.clone()] {
                vertex.color = vertex.color.map(|value| value + 0.18 * (1.0 - value));
            }
        }
    }

    pub(super) fn pick(&self, at: Pos2) -> Option<TileId> {
        if !self.rect.contains(at) || !self.rect.is_positive() {
            return None;
        }
        let uv = (at - self.rect.min) / self.rect.size();
        let ndc = pos2(2.0 * uv.x - 1.0, 1.0 - 2.0 * uv.y);
        let mut nearest = 1.0;
        let mut owner = None;
        for batch in self
            .batches
            .iter()
            .filter(|batch| batch.bounds.contains(at))
        {
            for triangle in self.vertices[batch.vertices.clone()].as_chunks::<3>().0 {
                if let Some(z) = depth_at(triangle, ndc)
                    && (0.0..=nearest).contains(&z)
                {
                    nearest = z;
                    owner = Some(batch.owner);
                }
            }
        }
        owner
    }
}

fn depth_at(triangle: &[Vertex; 3], point: Pos2) -> Option<f32> {
    let positions = |vertex: &Vertex| pos2(vertex.clip[0], vertex.clip[1]);
    let a = positions(&triangle[0]);
    let ab = positions(&triangle[1]) - a;
    let ac = positions(&triangle[2]) - a;
    let ap = point - a;
    let cross = |u: Vec2, v: Vec2| u.x * v.y - u.y * v.x;
    let area = cross(ab, ac);
    if area == 0.0 {
        return None;
    }
    let b = cross(ap, ac) / area;
    let c = cross(ab, ap) / area;
    (b >= 0.0 && c >= 0.0 && b + c <= 1.0).then(|| {
        let z = triangle[0].clip[2];
        z + b * (triangle[1].clip[2] - z) + c * (triangle[2].clip[2] - z)
    })
}
