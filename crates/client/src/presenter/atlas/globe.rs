//! Central projection onto a cell's owning icosahedron face keeps its edges straight.
use eframe::egui::{Pos2, Vec2, vec2};
use histeries_hex::Geodesic;
use histeries_sim::VisibleTileObservation;

pub(super) struct Footprint {
    points: [Pos2; 6],
    degree: usize,
}

impl Footprint {
    pub(super) fn corners(&self) -> &[Pos2] {
        &self.points[..self.degree]
    }

    pub(super) fn contains(&self, at: Pos2) -> bool {
        let points = self.corners();
        points.iter().enumerate().all(|(i, &a)| {
            let edge = points[(i + 1) % self.degree] - a;
            let offset = at - a;
            edge.x * offset.y - edge.y * offset.x >= 0.0
        })
    }
}

fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

pub(super) fn footprint(
    tile: &VisibleTileObservation,
    frequency: Option<u32>,
    center: Pos2,
    radius: f32,
) -> Option<Footprint> {
    let cell = tile.surface.as_ref()?;
    let frequency = frequency?;
    let face = Geodesic::atlas_face(frequency, tile.embedding)?;
    let [a, b, c] = face.vertices.map(|v| v.map(f64::from));
    let rows = [cross(b, c), cross(c, a), cross(a, b)];
    let project = |point: [i32; 3]| -> Vec2 {
        let [x, y, z] = point.map(f64::from);
        // The three triple products give barycentric weights on the face plane.
        let [a, b, c] = rows.map(|v| v[0] * x + v[1] * y + v[2] * z);
        let scale = f64::from(frequency) / (a + b + c);
        vec2(
            ((2.0 * b + c) * scale) as f32 * super::HALF_WIDTH,
            (1.5 * c * scale) as f32,
        )
    };
    let origin = project(cell.center);
    let mut points = [center; 6];
    for (out, &corner) in points.iter_mut().zip(&cell.corners) {
        *out = center + (project(corner) - origin) * radius;
    }
    Some(Footprint {
        points,
        degree: cell.corners.len(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::presenter::atlas::Camera;
    use histeries_sim::{BoardTopology, ScenarioDraft};

    #[test]
    fn game_atlas_picks_a_visible_cell_and_leaves_a_hidden_cell_in_fog() {
        use eframe::egui::{Context, Event, RawInput, Rect};
        let game = crate::ClientSession::globe(7, 3).unwrap();
        let view = game.observation().board();
        let draft = ScenarioDraft::open(&game.save_bytes()).unwrap();
        let hidden = draft
            .board()
            .visible_tiles()
            .iter()
            .find(|tile| !view.visible_tiles().iter().any(|t| t.tile == tile.tile))
            .unwrap();
        let ctx = Context::default();
        let rect = Rect::from_min_size(Pos2::ZERO, vec2(500.0, 400.0));
        let mut camera = Camera {
            frequency: Some(3),
            ..Camera::default()
        };
        let mut warmup = ctx.run_ui(
            RawInput {
                screen_rect: Some(rect),
                ..RawInput::default()
            },
            |ui| {
                super::super::surface(ui, view, &mut camera);
            },
        );
        warmup.textures_delta.clear();
        for (tile, expected) in [
            (&view.visible_tiles()[0], Some(view.visible_tiles()[0].tile)),
            (hidden, None),
        ] {
            camera.center = camera.tile_world(tile.embedding);
            let input = RawInput {
                screen_rect: Some(rect),
                events: vec![Event::PointerMoved(rect.center())],
                ..RawInput::default()
            };
            let mut picked = None;
            let mut output = ctx.run_ui(input, |ui| {
                picked = super::super::surface(ui, view, &mut camera).2;
            });
            output.textures_delta.clear();
            assert_eq!(picked, expected);
        }
    }

    #[test]
    fn pentagons_and_face_cut_neighbors_pick_only_their_own_chart_polygon() {
        for f in [1, 3] {
            let draft = ScenarioDraft::blank_on(BoardTopology::geodesic(f).unwrap(), 2).unwrap();
            let camera = Camera {
                frequency: Some(f),
                ..Camera::default()
            };
            let cells: Vec<_> = draft
                .board()
                .visible_tiles()
                .iter()
                .map(|tile| {
                    let center = camera.tile_world(tile.embedding).to_pos2();
                    (tile, center, footprint(tile, Some(f), center, 1.0).unwrap())
                })
                .collect();
            let mut cut_edges = 0;
            let mut joined_edges = 0;
            for (tile, center, cell) in &cells {
                let corners = cell.corners();
                assert_eq!(corners.len(), tile.neighbors.len());
                for (i, &a) in corners.iter().enumerate() {
                    let b = corners[(i + 1) % corners.len()];
                    let inside = a.lerp(b, 0.5).lerp(*center, 0.02);
                    let owners: Vec<_> = cells
                        .iter()
                        .filter(|(_, _, c)| c.contains(inside))
                        .map(|(other, _, _)| other.tile)
                        .collect();
                    assert_eq!(owners, [tile.tile]);
                }
                let geo = tile.surface.as_ref().unwrap();
                for id in &tile.neighbors {
                    let (other, _, adjacent) = &cells[id.index() as usize];
                    let other_geo = other.surface.as_ref().unwrap();
                    for (i, shared) in geo.corners.iter().enumerate() {
                        if let Some(j) = other_geo.corners.iter().position(|p| p == shared) {
                            let gap = corners[i].distance(adjacent.corners()[j]);
                            if Geodesic::atlas_face(f, tile.embedding).unwrap().chart
                                == Geodesic::atlas_face(f, other.embedding).unwrap().chart
                            {
                                assert!(gap < 0.0001);
                                joined_edges += 1;
                            } else {
                                assert!(gap > 1.0);
                                cut_edges += 1;
                            }
                        }
                    }
                }
            }
            assert!(cut_edges > 0 && joined_edges > 0);
            assert_eq!(cells.iter().filter(|(_, _, c)| c.degree == 5).count(), 12);
        }
    }
}
