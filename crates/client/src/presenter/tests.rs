use super::*;
use eframe::egui::{
    Context, Event, FullOutput, Modifiers, PointerButton, Pos2, RawInput, Rect, Shape, pos2,
};
use histeries_hex::Embedding;

#[test]
fn picks_hex_edges_and_keeps_the_hovered_world_point_fixed_when_zooming() {
    let mut camera = Camera::default();
    let rect = Rect::from_min_size(Pos2::ZERO, vec2(640.0, 400.0));
    let tile = Embedding { x: 2, y: 3 };
    let center = camera.project(tile, rect);
    assert!(atlas::contains(center, 48.0, center + vec2(40.0, 0.0)));
    assert!(!atlas::contains(center, 48.0, center + vec2(40.0, 32.0)));
    let anchor = center - rect.center();
    camera.zoom(2.0, anchor);
    assert!(camera.project(tile, rect).distance(center) < 0.001);
    camera.zoom(0.001, anchor);
    assert!(camera.project(tile, rect).distance(center) < 0.001);
}

fn frame(
    ctx: &Context,
    state: &mut Presenter,
    game: &mut ClientSession,
    size: Vec2,
    events: Vec<Event>,
) -> FullOutput {
    let input = RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, size)),
        events,
        ..Default::default()
    };
    let mut output = ctx.run_ui(input, |ui| {
        state.show(ui, game, |ui| {
            ui.label("Game");
        })
    });
    output.textures_delta.clear();
    output
}

fn click(
    ctx: &Context,
    state: &mut Presenter,
    game: &mut ClientSession,
    size: Vec2,
    at: Pos2,
) -> FullOutput {
    let button = |pressed| Event::PointerButton {
        pos: at,
        button: PointerButton::Primary,
        pressed,
        modifiers: Modifiers::NONE,
    };
    frame(
        ctx,
        state,
        game,
        size,
        vec![Event::PointerMoved(at), button(true)],
    );
    frame(
        ctx,
        state,
        game,
        size,
        vec![Event::PointerMoved(at), button(false)],
    );
    frame(ctx, state, game, size, vec![])
}

fn text_center(output: &FullOutput, label: &str) -> Pos2 {
    output
        .shapes
        .iter()
        .find_map(|s| {
            if let Shape::Text(text) = &s.shape
                && text.galley.text() == label
            {
                let center = text.visual_bounding_rect().center();
                if s.clip_rect.contains(center) {
                    return Some(center);
                }
            }
            None
        })
        .unwrap_or_else(|| panic!("missing visible control {label}"))
}

#[test]
fn mouse_orders_reach_the_session_in_wide_and_compact_layouts() {
    for size in [vec2(1024.0, 700.0), vec2(400.0, 760.0)] {
        let ctx = Context::default();
        crate::font::install(&ctx);
        let mut state = Presenter::default();
        let mut game = ClientSession::new(7).unwrap();
        let unit = game.observation().units()[0];
        let turn = game.observation().turn();
        frame(&ctx, &mut state, &mut game, size, vec![]);
        let output = frame(&ctx, &mut state, &mut game, size, vec![]);
        let badge = unit_badge(&output);
        assert!(
            !ctx.tessellate(output.shapes, output.pixels_per_point)
                .is_empty()
        );
        let panel = click(&ctx, &mut state, &mut game, size, badge);
        assert_eq!(state.selected, Some(unit.tile));
        let hold = text_center(&panel, "Hold");
        let after_hold = click(&ctx, &mut state, &mut game, size, hold);
        assert_eq!(game.observation().units()[0].movement, 0);
        let end_turn = text_center(&after_hold, "End turn");
        click(&ctx, &mut state, &mut game, size, end_turn);
        assert_eq!(game.observation().turn(), turn + 1);
        // The upper left of the map is well outside this seat's visible footprint.
        let before = state.selected;
        click(&ctx, &mut state, &mut game, size, pos2(12.0, 65.0));
        assert_eq!(state.selected, before);
    }
}

#[test]
fn targeted_move_preserves_an_invalid_order_for_retry_and_selects_its_destination() {
    let ctx = Context::default();
    crate::font::install(&ctx);
    let size = vec2(1024.0, 700.0);
    let mut state = Presenter::default();
    let mut game = ClientSession::new(7).unwrap();
    let first = game.observation();
    let unit = first.units()[0];
    let origin = visible(first.board(), unit.tile).unwrap();
    let command = |destination| Command::UnitMove {
        player: first.seat(),
        unit: unit.id,
        destination,
    };
    let destination = origin
        .neighbors
        .iter()
        .copied()
        .find(|id| {
            let mut copy = ClientSession::from_save_bytes(&game.save_bytes()).unwrap();
            copy.dispatch(command(*id)).is_ok()
        })
        .expect("the opening warrior has a legal move");
    let world_delta =
        Camera::world(game.embedding(destination).unwrap()) - Camera::world(origin.embedding);
    frame(&ctx, &mut state, &mut game, size, vec![]);
    let output = frame(&ctx, &mut state, &mut game, size, vec![]);
    let unit_at = unit_badge(&output);
    let selected = click(&ctx, &mut state, &mut game, size, unit_at);
    click(
        &ctx,
        &mut state,
        &mut game,
        size,
        text_center(&selected, "Move"),
    );
    click(&ctx, &mut state, &mut game, size, unit_at);
    assert!(
        state
            .feedback
            .as_ref()
            .is_some_and(|(_, is_error)| *is_error)
    );
    assert!(state.target == Some(Target::Move(unit.id)));
    let target_at = unit_at + world_delta * state.camera.radius;
    click(&ctx, &mut state, &mut game, size, target_at);
    assert_eq!(game.observation().units()[0].tile, destination);
    assert_eq!(state.selected, Some(destination));
    assert!(state.target.is_none());
}

fn unit_badge(output: &FullOutput) -> Pos2 {
    output
        .shapes
        .iter()
        .find_map(|s| match &s.shape {
            Shape::Path(path)
                if path.fill == Color32::from_rgb(44, 79, 73) && path.points.len() == 5 =>
            {
                Some(path.visual_bounding_rect().center())
            }
            _ => None,
        })
        .expect("the opening unit marker is visible")
}

#[test]
fn first_frame_centers_the_visible_footprint_in_the_chart() {
    let game = ClientSession::new(7).unwrap();
    let view = game.observation();
    let viewport = Rect::from_min_size(pos2(12.0, 45.0), vec2(860.0, 650.0));
    let mut camera = Camera::default();
    camera.fit(view.board(), viewport);
    let mut centers = Rect::NOTHING;
    for tile in view.visible_tiles() {
        centers.extend_with(camera.project(tile.embedding, viewport));
    }
    let extent = centers.expand2(vec2(3.0f32.sqrt() / 2.0, 1.0) * camera.radius);
    assert!(extent.center().distance(viewport.center()) < 0.01);
    let fill = (extent.width() / viewport.width()).max(extent.height() / viewport.height());
    assert!((0.55..0.70).contains(&fill), "opening fill {fill}");
}
