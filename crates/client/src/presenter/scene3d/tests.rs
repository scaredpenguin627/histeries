use super::*;
use crate::game::ClientSession;
use histeries_hex::Embedding;
use histeries_sim::{Command, Feature, Terrain, UnitKind};

#[test]
fn native_surface_uses_the_seat_globe_mesh_at_framebuffer_size() {
    let game = ClientSession::globe(7, 3).unwrap();
    let view = game.observation();
    assert!(!view.cities().is_empty() && !view.units().is_empty());
    assert!(surface_vertices(view, 0, 720).is_empty());
    let vertices = surface_vertices(view, 1280, 720);
    assert!(vertices.len() > 3 && vertices.len().is_multiple_of(3));
    assert!(vertices.iter().flatten().all(|v| v.is_finite()));
    // Fog lies at depth 1. The initial framebuffer must also contain visible game geometry.
    assert!(
        vertices
            .iter()
            .any(|v| v[0].abs() < 1.0 && v[1].abs() < 1.0 && (0.0..1.0).contains(&v[2]))
    );
    assert!(vertices.windows(2).any(|pair| pair[0][3..] != pair[1][3..]));
    assert_ne!(vertices, surface_vertices(view, 720, 1280));
}

#[test]
fn globe_edges_pick_the_incident_tiles_and_fog_hides_the_rotated_far_side() {
    use histeries_sim::{BoardTopology, ScenarioBrush, ScenarioDraft, Session};
    let topology = BoardTopology::geodesic(4).unwrap();
    let seat = PlayerId::new(1);
    let pentagon = TileId::new(0);
    let mut draft = ScenarioDraft::blank_on(topology.clone(), 2).unwrap();
    draft.apply(ScenarioBrush::City, pentagon, seat).unwrap();
    let view = Session::from_save_bytes(draft.bytes())
        .unwrap()
        .observe(seat)
        .unwrap();
    let home = visible(view.board(), pentagon).unwrap();
    let cell = home.surface.as_ref().unwrap();
    assert_eq!(cell.corners.len(), 5);
    let rect = Rect::from_min_size(pos2(19.0, 73.0), vec2(700.0, 560.0));
    let mut camera = Camera3d::default();
    camera.fit(view.board(), rect);
    camera.focus(home);
    let projection = Projection::new(&camera, rect);
    let scene = Scene::build(view.board(), seat, &camera, rect, None);
    assert_eq!(scene.pick(rect.center()), Some(pentagon));
    let center = screen(&projection, globe::position(cell.center));
    for (i, neighbor) in topology
        .edge_neighbors(pentagon)
        .into_iter()
        .flatten()
        .enumerate()
    {
        let other = visible(view.board(), neighbor)
            .unwrap()
            .surface
            .as_ref()
            .unwrap();
        assert_eq!(other.corners.len(), 6);
        let a = cell.corners[i];
        let b = cell.corners[(i + 1) % cell.corners.len()];
        assert!(other.corners.contains(&a) && other.corners.contains(&b));
        let edge = screen(&projection, globe::position(a))
            .lerp(screen(&projection, globe::position(b)), 0.5);
        let beyond = screen(&projection, globe::position(other.center));
        assert_eq!(scene.pick(edge.lerp(center, 0.25)), Some(pentagon));
        assert_eq!(scene.pick(edge.lerp(beyond, 0.25)), Some(neighbor));
    }

    let [x, y, z] = globe::position(cell.center);
    let latitude = y.atan2(x.hypot(z));
    camera.orbit(vec2(std::f32::consts::PI, -2.0 * latitude) * projection.globe.unwrap().radius);
    let opposite = Scene::build(view.board(), seat, &camera, rect, None);
    assert_eq!(opposite.pick(rect.center()), None);
    // The GPU receives opaque fog at the center, even with all known terrain behind it.
    let depth = opposite
        .vertices
        .as_chunks::<3>()
        .0
        .iter()
        .filter_map(|triangle| depth_at(triangle, Pos2::ZERO))
        .filter(|depth| (0.0..=1.0).contains(depth))
        .min_by(f32::total_cmp);
    assert_eq!(depth, Some(1.0));
}

fn screen(projection: &Projection, point: Point) -> Pos2 {
    let [x, y, _] = projection.clip(point);
    projection.rect.min + vec2(1.0 + x, 1.0 - y) * projection.rect.size() * 0.5
}

#[test]
fn fit_contains_raised_geometry_after_orbit_and_zoom_preserves_the_anchor() {
    let game = ClientSession::new(7).unwrap();
    let view = game.observation();
    let mut camera = Camera3d::default();
    for (size, orbit) in [
        (vec2(900.0, 350.0), Vec2::ZERO),
        (vec2(190.0, 600.0), vec2(122.0, -700.0)),
        (vec2(500.0, 400.0), vec2(350.0, 900.0)),
    ] {
        let rect = Rect::from_min_size(pos2(70.0, 110.0), size);
        camera.orbit(orbit);
        camera.fit(view.board(), rect);
        let scene = Scene::build(view.board(), view.seat(), &camera, rect, None);
        assert!(
            view.visible_tiles()
                .iter()
                .all(|t| scene.batches.iter().any(|b| b.owner == t.tile))
        );
        assert!(scene.vertices.iter().all(|v| v.clip[0].abs() < 1.0
            && v.clip[1].abs() < 1.0
            && (0.0..1.0).contains(&v.clip[2])));
        let point = [0.7, 2.1, 0.82];
        let at = screen(&Projection::new(&camera, rect), point);
        camera.zoom(2.0, at - rect.center());
        assert!(screen(&Projection::new(&camera, rect), point).distance(at) < 0.002);
        let drag = vec2(-25.0, 36.0);
        camera.pan(drag);
        assert!(screen(&Projection::new(&camera, rect), point).distance(at + drag) < 0.002);
        camera.pan(Vec2::splat(100_000.0));
        let offscreen = Scene::build(view.board(), view.seat(), &camera, rect, None);
        assert!(offscreen.vertices.is_empty());
        assert_eq!(offscreen.pick(rect.center()), None);
    }
}

#[test]
fn overview_reduces_geometry_and_preserves_selection_and_visible_occupants() {
    let game = ClientSession::new(7).unwrap();
    let view = game.observation();
    let city = view.visible_cities().first().unwrap().tile;
    let home = visible(view.board(), city).unwrap();
    let rect = Rect::from_min_size(Pos2::ZERO, vec2(640.0, 480.0));
    let build = |radius| {
        let mut camera = Camera3d {
            scale: radius,
            pitch: 1.18,
            ..Default::default()
        };
        camera.focus(home);
        Scene::build(view.board(), view.seat(), &camera, rect, Some(city))
    };
    let normal = build(8.1);
    let overview = build(7.9);
    assert!(overview.vertices.len() * 3 < normal.vertices.len());
    // Zoom changes the models of terrain, cities, districts and units, preserving their tile IDs.
    assert!(
        normal
            .batches
            .iter()
            .map(|b| b.owner)
            .eq(overview.batches.iter().map(|b| b.owner))
    );
    assert_eq!(normal.pick(rect.center()), Some(city));
    assert_eq!(overview.pick(rect.center()), Some(city));
    let unit = view.visible_units().last().unwrap();
    let marker = overview.batches.last().unwrap();
    assert_eq!(marker.owner, unit.tile);
    let flag = overview.vertices[marker.vertices.clone()]
        .as_chunks::<3>()
        .0
        .last()
        .unwrap();
    let [a, b, c] = flag.map(|v| vec2(v.clip[0], -v.clip[1]));
    let centroid = (a + b + c) / 3.0;
    assert_eq!(
        overview.pick(rect.center() + centroid * rect.size() * 0.5),
        Some(unit.tile)
    );
    assert_eq!(overview.pick(rect.min), None);
}

#[test]
fn overview_keeps_roads_and_each_deposit_visible_together_on_a_selected_river_tile() {
    use histeries_sim::{ResourceKind, RiverDirection};
    let game = ClientSession::new(7).unwrap();
    let mut tile = game.observation().visible_tiles()[0].clone();
    tile.embedding = Embedding { x: 0, y: 0 };
    tile.terrain = Terrain::Hill;
    tile.feature = Feature::Forest;
    let camera = Camera3d {
        scale: 7.9,
        pitch: 1.24,
        yaw: 0.0,
        ..Default::default()
    };
    let rect = Rect::from_min_size(Pos2::ZERO, vec2(120.0, 90.0));
    let projection = Projection::new(&camera, rect);
    let mut samples = |road, resource, river, selected| {
        tile.road = road;
        tile.resource = resource;
        tile.river = river;
        let mut scene = Scene {
            vertices: vec![],
            batches: vec![],
            rect,
        };
        scene.batch(&projection, &tile, |mesh| mesh.land(&tile, selected));
        // On each side of the hill, sample the interior mark and the outer band
        // at the depth-tested surface visible to the player.
        [-0.44_f32, 0.43, -0.76, 0.76].map(|x| {
            let height = elevation(Terrain::Hill);
            let z = height + (elevation(Terrain::Ocean) - height) * x.abs() / 0.866_025_4;
            let point = [x, 0.018, z];
            assert_eq!(scene.pick(screen(&projection, point)), Some(tile.tile));
            let [u, v, _] = projection.clip(point);
            scene
                .vertices
                .as_chunks::<3>()
                .0
                .iter()
                .filter_map(|t| depth_at(t, pos2(u, v)).map(|depth| (depth, t[0].color)))
                .min_by(|a, b| a.0.total_cmp(&b.0))
                .unwrap()
                .1
                .map(|value| (255.0 * value).round() as u8)
        })
    };
    let river = Some(RiverDirection::West);
    let base = samples(false, None, river, true);
    let road = samples(true, None, river, true);
    assert_ne!(road[0], base[0]);
    assert_eq!(road[1..], base[1..]);
    assert_ne!(base[2], samples(false, None, None, true)[2]);
    assert_ne!(base[3], samples(false, None, river, false)[3]);
    let mut deposits = std::collections::BTreeSet::new();
    for kind in [
        ResourceKind::Wheat,
        ResourceKind::Stone,
        ResourceKind::Iron,
        ResourceKind::Gold,
    ] {
        let resource = samples(false, Some(kind), river, true);
        let both = samples(true, Some(kind), river, true);
        assert_ne!(resource[1], base[1]);
        assert!(deposits.insert(resource[1]));
        assert_eq!(resource, [base[0], resource[1], base[2], base[3]]);
        assert_eq!(both, [road[0], resource[1], base[2], base[3]]);
    }
}

#[test]
fn a_mountain_occludes_the_tile_behind_it_in_either_submission_order() {
    let game = ClientSession::new(7).unwrap();
    let mut near = game.observation().visible_tiles()[0].clone();
    near.tile = TileId::new(0);
    near.embedding = Embedding { x: 0, y: 0 };
    near.feature = Feature::None;
    near.river = None;
    near.resource = None;
    near.road = false;
    near.terrain = Terrain::Mountain;
    let mut far = near.clone();
    far.tile = TileId::new(1);
    far.embedding = Embedding { x: 1, y: -3 };
    far.terrain = Terrain::Plains;
    let camera = Camera3d {
        yaw: -std::f32::consts::FRAC_PI_6,
        pitch: 0.38,
        ..Default::default()
    };
    let rect = Rect::from_min_size(Pos2::ZERO, vec2(500.0, 300.0));
    let projection = Projection::new(&camera, rect);
    let ground = world(far.embedding);
    let at = screen(&projection, [ground.x, ground.y, elevation(far.terrain)]);
    let scene_from = |tiles: &[&VisibleTileObservation]| {
        let mut scene = Scene {
            vertices: vec![],
            batches: vec![],
            rect,
        };
        for tile in tiles {
            scene.batch(&projection, tile, |m| m.land(tile, false));
        }
        scene
    };
    assert_eq!(scene_from(&[&far]).pick(at), Some(far.tile));
    assert_eq!(scene_from(&[&near, &far]).pick(at), Some(near.tile));
    assert_eq!(scene_from(&[&far, &near]).pick(at), Some(near.tile));

    // A standing figure's head also remains selectable above the tile silhouette.
    far.embedding = Embedding { x: 0, y: 0 };
    let head = screen(&projection, [0.0, 0.32, elevation(far.terrain) + 0.58]);
    let mut figure = scene_from(&[&far]);
    assert_eq!(figure.pick(head), None);
    figure.batch(&projection, &far, |m| {
        m.unit(UnitKind::Warrior, 0.0, [50, 80, 89])
    });
    assert_eq!(figure.pick(head), Some(far.tile));
}

#[test]
fn handoff_builds_only_the_new_seats_terrain_and_occupants() {
    let mut game = ClientSession::hotseat(7, 2).unwrap();
    game.confirm_handoff();
    let first = game.observation();
    let old_home = first.cities()[0].tile;
    let old_home_tile = visible(first.board(), old_home).unwrap().clone();
    game.dispatch(Command::TurnEnd {
        player: first.seat(),
    })
    .unwrap();
    game.confirm_handoff();
    let view = game.observation();
    assert!(visible(view.board(), old_home).is_none());
    let rect = Rect::from_min_size(Pos2::ZERO, vec2(640.0, 480.0));
    let mut camera = Camera3d::default();
    camera.fit(view.board(), rect);
    let scene = Scene::build(view.board(), view.seat(), &camera, rect, Some(old_home));
    assert!(!scene.batches.is_empty());
    assert!(
        scene
            .batches
            .iter()
            .all(|b| visible(view.board(), b.owner).is_some())
    );
    camera.focus(&old_home_tile);
    let scene = Scene::build(view.board(), view.seat(), &camera, rect, None);
    assert_eq!(scene.pick(rect.center()), None);
}
