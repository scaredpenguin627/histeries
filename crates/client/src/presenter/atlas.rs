//! Painting and picking share the same pointy-top footprint and display embedding.

use eframe::egui::{
    self, Align2, Color32, FontId, Painter, Pos2, Rect, Shape, Stroke, Vec2, pos2, vec2,
};
use histeries_hex::Embedding;
use histeries_sim::{
    Feature, Observation, ResourceKind, Terrain, UnitKind, VisibleTileObservation,
};

use super::visible;

pub(super) const INK: Color32 = Color32::from_rgb(29, 44, 44);
pub(super) const PAPER: Color32 = Color32::from_rgb(243, 237, 214);
pub(super) const GOLD: Color32 = Color32::from_rgb(233, 191, 108);
const FOG: Color32 = Color32::from_rgb(39, 54, 55);
const HALF_WIDTH: f32 = 0.866_025_4;

pub(super) struct Camera {
    pub(super) center: Vec2,
    pub(super) radius: f32,
}

impl Default for Camera {
    fn default() -> Self {
        Self {
            center: Vec2::ZERO,
            radius: 48.0,
        }
    }
}

impl Camera {
    pub(super) fn world(point: Embedding) -> Vec2 {
        vec2(point.x as f32 * HALF_WIDTH, point.y as f32 * 0.5)
    }

    pub(super) fn project(&self, point: Embedding, viewport: Rect) -> Pos2 {
        viewport.center() + (Self::world(point) - self.center) * self.radius
    }

    pub(super) fn fit(&mut self, view: &Observation, viewport: Rect) {
        let mut bounds = Rect::NOTHING;
        for tile in view.visible_tiles() {
            bounds.extend_with(Self::world(tile.embedding).to_pos2());
        }
        if bounds == Rect::NOTHING {
            return;
        }
        let footprint = bounds.expand2(vec2(HALF_WIDTH, 1.0));
        let fraction = if viewport.width() < 500.0 { 0.80 } else { 0.68 };
        let scale =
            (viewport.width() / footprint.width()).min(viewport.height() / footprint.height());
        self.radius = (fraction * scale).clamp(15.0, 88.0);
        self.center = footprint.center().to_vec2();
    }

    pub(super) fn zoom(&mut self, factor: f32, at: Vec2) {
        let radius = (self.radius * factor).clamp(15.0, 88.0);
        self.center += at * (self.radius.recip() - radius.recip());
        self.radius = radius;
    }
}

// Vertex zero is north. Each subsequent pair bounds the next clockwise river edge.
fn vertices(center: Pos2, radius: f32) -> [Pos2; 6] {
    [
        vec2(0.0, -1.0),
        vec2(HALF_WIDTH, -0.5),
        vec2(HALF_WIDTH, 0.5),
        vec2(0.0, 1.0),
        vec2(-HALF_WIDTH, 0.5),
        vec2(-HALF_WIDTH, -0.5),
    ]
    .map(|p| center + p * radius)
}

pub(super) fn contains(center: Pos2, radius: f32, point: Pos2) -> bool {
    let d = (point - center).abs() / radius;
    d.x <= HALF_WIDTH && d.y + d.x / (2.0 * HALF_WIDTH) <= 1.0
}

pub(super) fn fog(painter: &Painter, rect: Rect) {
    painter.rect_filled(rect, 0.0, FOG);
    let stroke = Stroke::new(0.8, Color32::from_rgb(47, 63, 64));
    let mut x = rect.left() - rect.height();
    while x < rect.right() {
        painter.line_segment(
            [pos2(x, rect.bottom()), pos2(x + rect.height(), rect.top())],
            stroke,
        );
        x += 15.0;
    }
}

pub(super) fn land_color(terrain: Terrain) -> Color32 {
    match terrain {
        Terrain::Plains => Color32::from_rgb(193, 177, 125),
        Terrain::Grassland => Color32::from_rgb(145, 168, 121),
        Terrain::Desert => Color32::from_rgb(220, 183, 126),
        Terrain::Tundra => Color32::from_rgb(171, 179, 166),
        Terrain::Coast => Color32::from_rgb(112, 166, 164),
        Terrain::Ocean => Color32::from_rgb(78, 130, 143),
        Terrain::Hill => Color32::from_rgb(164, 155, 121),
        Terrain::Mountain => Color32::from_rgb(158, 153, 140),
    }
}

pub(super) fn ground(painter: &Painter, tile: &VisibleTileObservation, center: Pos2, radius: f32) {
    let corners = vertices(center, radius);
    painter.add(Shape::convex_polygon(
        corners.to_vec(),
        land_color(tile.terrain),
        Stroke::new(0.8, INK.gamma_multiply(0.52)),
    ));
    let mark = |points: &[(f32, f32)]| {
        let positions = points
            .iter()
            .map(|&(x, y)| center + vec2(x, y) * radius)
            .collect();
        painter.add(Shape::line(
            positions,
            Stroke::new((radius * 0.038).max(0.8), INK.gamma_multiply(0.66)),
        ));
    };
    match tile.terrain {
        Terrain::Mountain => {
            mark(&[(-0.56, 0.17), (-0.16, -0.44), (0.23, 0.17)]);
            mark(&[(-0.24, -0.31), (-0.13, -0.21), (-0.07, -0.29)]);
            mark(&[(0.07, -0.05), (0.29, -0.34), (0.55, 0.12)]);
        }
        Terrain::Hill | Terrain::Desert => {
            mark(&[(-0.53, 0.04), (-0.34, -0.08), (-0.17, -0.11), (0.05, 0.0)]);
            mark(&[(0.01, 0.19), (0.2, 0.05), (0.4, 0.05)]);
        }
        Terrain::Coast | Terrain::Ocean => {
            mark(&[(-0.43, -0.12), (-0.26, -0.09), (-0.08, -0.12)]);
            mark(&[(0.01, 0.16), (0.18, 0.19), (0.36, 0.16)]);
        }
        Terrain::Grassland | Terrain::Plains if tile.feature == Feature::None => {
            for x in [-0.36, 0.07] {
                mark(&[(x - 0.06, 0.12), (x, 0.22), (x + 0.06, 0.07)]);
            }
        }
        Terrain::Tundra => {
            mark(&[(-0.35, -0.15), (-0.12, -0.2), (0.06, -0.14)]);
            mark(&[(0.13, 0.12), (0.32, 0.09)]);
        }
        _ => {}
    }
    match tile.feature {
        Feature::Forest => {
            for (x, y) in [(-0.26, -0.06), (0.05, -0.17), (0.23, 0.02)] {
                mark(&[
                    (x - 0.1, y + 0.12),
                    (x, y - 0.13),
                    (x + 0.1, y + 0.12),
                    (x - 0.1, y + 0.12),
                ]);
                mark(&[(x, y + 0.12), (x, y + 0.22)]);
            }
        }
        Feature::Jungle => {
            for (x, y) in [(-0.29, 0.05), (0.02, -0.16), (0.25, 0.02)] {
                mark(&[
                    (x - 0.14, y),
                    (x - 0.18, y - 0.1),
                    (x - 0.08, y - 0.18),
                    (x + 0.08, y - 0.18),
                    (x + 0.18, y - 0.06),
                    (x + 0.12, y),
                    (x - 0.14, y),
                ]);
                mark(&[(x, y), (x, y + 0.14)]);
            }
        }
        Feature::Marsh => {
            mark(&[(-0.35, 0.04), (-0.21, 0.04), (-0.27, -0.15)]);
            mark(&[(-0.11, -0.1), (-0.06, 0.12), (0.05, -0.12)]);
        }
        Feature::None => {}
    }
    if tile.road {
        for x in [-0.58, -0.30, -0.02, 0.26] {
            mark(&[(x, 0.37 - x * 0.18), (x + 0.16, 0.34 - x * 0.18)]);
        }
    }
    if let Some(resource) = tile.resource {
        let at = center + vec2(0.48, 0.42) * radius;
        let (initial, color) = match resource {
            ResourceKind::Wheat => ("W", Color32::from_rgb(242, 207, 124)),
            ResourceKind::Stone => ("S", Color32::from_rgb(212, 211, 197)),
            ResourceKind::Iron => ("I", Color32::from_rgb(196, 162, 147)),
            ResourceKind::Gold => ("G", GOLD),
        };
        painter.circle_filled(at, radius * 0.17, color);
        painter.circle_stroke(at, radius * 0.17, Stroke::new(0.9, INK));
        if radius >= 27.0 {
            painter.text(
                at,
                Align2::CENTER_CENTER,
                initial,
                FontId::monospace(radius * 0.23),
                INK,
            );
        }
    }
}

pub(super) fn river(painter: &Painter, tile: &VisibleTileObservation, center: Pos2, radius: f32) {
    if let Some(direction) = tile.river {
        let corners = vertices(center, radius);
        let edge = direction as usize;
        painter.line_segment(
            [corners[edge], corners[(edge + 1) % 6]],
            Stroke::new((radius * 0.1).max(2.0), Color32::from_rgb(68, 133, 159)),
        );
    }
}

pub(super) fn outline(painter: &Painter, center: Pos2, radius: f32, color: Color32) {
    painter.add(Shape::closed_line(
        vertices(center, radius * 0.92).to_vec(),
        Stroke::new(2.3, color),
    ));
}

fn owner_color(friendly: bool) -> Color32 {
    if friendly {
        Color32::from_rgb(44, 79, 73)
    } else {
        Color32::from_rgb(135, 65, 49)
    }
}

pub(super) fn occupants(painter: &Painter, rect: Rect, observation: &Observation, camera: &Camera) {
    let radius = camera.radius;
    let locate = |tile| {
        visible(observation, tile)
            .map(|t| camera.project(t.embedding, rect))
            .filter(|p| rect.expand(radius).contains(*p))
    };
    for city in observation.visible_cities() {
        for &district in &city.districts {
            if district != city.tile
                && let Some(at) = locate(district)
            {
                let sign = [
                    vec2(0.0, -0.21),
                    vec2(0.21, 0.0),
                    vec2(0.0, 0.21),
                    vec2(-0.21, 0.0),
                ]
                .map(|point| at + point * radius);
                painter.add(Shape::convex_polygon(
                    sign.to_vec(),
                    owner_color(city.owner == observation.seat()),
                    Stroke::new(1.2, PAPER),
                ));
            }
        }
        let Some(center) = locate(city.tile) else {
            continue;
        };
        let at = center - vec2(0.0, radius * 0.32);
        let color = owner_color(city.owner == observation.seat());
        let points = [
            (-0.30, 0.16),
            (-0.30, -0.1),
            (-0.17, -0.1),
            (-0.17, -0.23),
            (0.0, -0.36),
            (0.17, -0.23),
            (0.17, -0.1),
            (0.30, -0.1),
            (0.30, 0.16),
        ];
        // A stepped town silhouette, assembled from three convex houses.
        for (left, right, top) in [(-0.30, -0.1, -0.1), (-0.16, 0.16, -0.23), (0.1, 0.30, -0.1)] {
            painter.rect_filled(
                Rect::from_two_pos(
                    at + vec2(left, top) * radius,
                    at + vec2(right, 0.16) * radius,
                ),
                0.0,
                color,
            );
        }
        painter.add(Shape::convex_polygon(
            vec![
                at + vec2(-0.16, -0.23) * radius,
                at + vec2(0.0, -0.36) * radius,
                at + vec2(0.16, -0.23) * radius,
            ],
            color,
            Stroke::NONE,
        ));
        painter.add(Shape::closed_line(
            points.map(|(x, y)| at + vec2(x, y) * radius).to_vec(),
            Stroke::new(1.2, PAPER),
        ));
        painter.line_segment(
            [at + vec2(0.0, 0.16) * radius, at - vec2(0.0, 0.02) * radius],
            Stroke::new((radius * 0.07).max(1.0), PAPER),
        );
    }
    for unit in observation.visible_units() {
        let Some(center) = locate(unit.tile) else {
            continue;
        };
        let Some(tile) = visible(observation, unit.tile) else {
            continue;
        };
        let stacked = tile.units.military.is_some() && tile.units.civilian.is_some();
        let x = if !stacked {
            0.0
        } else if tile.units.military == Some(unit.id) {
            -0.25
        } else {
            0.25
        };
        let at = center + vec2(x, 0.16) * radius;
        let color = owner_color(unit.owner == Some(observation.seat()));
        if unit.kind == UnitKind::Warrior {
            let shield = [
                (-0.25, -0.22),
                (0.25, -0.22),
                (0.21, 0.08),
                (0.0, 0.30),
                (-0.21, 0.08),
            ];
            painter.add(Shape::convex_polygon(
                shield.map(|(x, y)| at + vec2(x, y) * radius).to_vec(),
                color,
                Stroke::new(1.5, PAPER),
            ));
            painter.line_segment(
                [
                    at + vec2(-0.09, 0.15) * radius,
                    at + vec2(0.06, -0.06) * radius,
                ],
                Stroke::new((radius * 0.04).max(1.0), PAPER),
            );
            let spear = [(-0.01, -0.07), (0.13, -0.15), (0.10, 0.0)];
            painter.add(Shape::convex_polygon(
                spear.map(|(x, y)| at + vec2(x, y) * radius).to_vec(),
                PAPER,
                Stroke::NONE,
            ));
            continue;
        }
        painter.circle_filled(at, radius * 0.23, color);
        painter.circle_stroke(at, radius * 0.23, Stroke::new(1.4, PAPER));
        let initial = match unit.kind {
            UnitKind::Warrior => "W",
            UnitKind::Archer => "A",
            UnitKind::Settler => "S",
            UnitKind::Builder => "B",
            UnitKind::Barbarian => "×",
        };
        painter.text(
            at,
            Align2::CENTER_CENTER,
            initial,
            FontId::proportional((radius * 0.34).max(6.0)),
            Color32::WHITE,
        );
    }
}

pub(super) fn compass(painter: &Painter, rect: Rect) {
    let at = rect.right_top() + vec2(-26.0, 34.0);
    let dark = Color32::from_black_alpha(85);
    painter.circle_filled(at, 17.0, dark);
    painter.line_segment(
        [at + vec2(0.0, 9.0), at - vec2(0.0, 8.0)],
        Stroke::new(1.0, PAPER),
    );
    painter.add(egui::Shape::convex_polygon(
        vec![
            at - vec2(0.0, 12.0),
            at + vec2(4.0, 1.0),
            at + vec2(-4.0, 1.0),
        ],
        GOLD,
        Stroke::NONE,
    ));
}
