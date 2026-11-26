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
    draw_frame(ctx, size, events, |ui| {
        state.show(ui, game, |ui| {
            ui.label("Game");
        })
    })
}

fn draw_frame(
    ctx: &Context,
    size: Vec2,
    events: Vec<Event>,
    draw: impl FnMut(&mut Ui),
) -> FullOutput {
    let input = RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, size)),
        events,
        ..Default::default()
    };
    let mut output = ctx.run_ui(input, draw);
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
    click_at(at, |events| frame(ctx, state, game, size, events))
}

fn click_at(at: Pos2, mut draw: impl FnMut(Vec<Event>) -> FullOutput) -> FullOutput {
    let button = |pressed| Event::PointerButton {
        pos: at,
        button: PointerButton::Primary,
        pressed,
        modifiers: Modifiers::NONE,
    };
    draw(vec![Event::PointerMoved(at), button(true)]);
    draw(vec![Event::PointerMoved(at), button(false)]);
    draw(vec![])
}

fn labels(output: &FullOutput) -> Vec<&str> {
    output
        .shapes
        .iter()
        .filter_map(|shape| match &shape.shape {
            Shape::Text(text) => Some(text.galley.text()),
            _ => None,
        })
        .collect()
}

#[test]
fn completed_hotseat_displays_only_the_result_and_can_start_a_new_game() {
    use eframe::App as _;
    let (mut fixture, capture) = crate::game::tests::winning_capture();
    fixture.dispatch(capture).unwrap();
    let finished = ClientSession::hotseat_from_save(&fixture.save_bytes()).unwrap();
    let ctx = Context::default();
    let mut app = crate::GameApp::new(
        &eframe::CreationContext::_new_kittest(ctx.clone()),
        finished,
    );
    let mut host = eframe::Frame::_new_kittest();
    let mut draw = |events| {
        draw_frame(&ctx, vec2(1024.0, 700.0), events, |ui| {
            app.ui(ui, &mut host)
        })
    };
    draw(vec![]);
    let mut screen = draw(vec![]);
    assert_eq!(
        labels(&screen),
        ["Seat 2 won", "Domination victory", "Game"]
    );
    for label in ["Game", "New single-player game", "Start game"] {
        screen = click_at(text_center(&screen, label), &mut draw);
    }
    text_center(&screen, "End turn");
}

#[test]
fn game_menu_starts_hotseat_and_hides_the_map_between_two_real_turns() {
    use eframe::App as _;
    let ctx = Context::default();
    let mut app = crate::GameApp::new(
        &eframe::CreationContext::_new_kittest(ctx.clone()),
        ClientSession::new(7).unwrap(),
    );
    let mut host = eframe::Frame::_new_kittest();
    let mut draw = |events| {
        draw_frame(&ctx, vec2(1024.0, 700.0), events, |ui| {
            app.ui(ui, &mut host)
        })
    };
    draw(vec![]);
    let mut screen = draw(vec![]);
    for label in ["Game", "New hotseat game", "Start hotseat"] {
        screen = click_at(text_center(&screen, label), &mut draw);
    }
    for seat in 1..=2 {
        assert_eq!(
            labels(&screen),
            [
                format!("Pass to seat {seat}"),
                "Reveal my map".into(),
                "Game".into()
            ]
        );
        assert!(map_callback(&screen).is_none());
        screen = click_at(text_center(&screen, "Reveal my map"), &mut draw);
        text_center(&screen, &format!("Seat {seat}"));
        text_center(&screen, "Turn 0");
        if seat == 1 {
            screen = click_at(text_center(&screen, "3D"), &mut draw);
            screen = click_at(text_center(&screen, "Game"), &mut draw);
            let menu = labels(&screen);
            assert!(menu.contains(&"Save hotseat game"));
            assert!(!menu.contains(&"Scenario editor"));
            screen = click_at(text_center(&screen, "Game"), &mut draw);
        }
        assert!(map_callback(&screen).is_some());
        screen = click_at(text_center(&screen, "End turn"), &mut draw);
    }
    assert_eq!(labels(&screen), ["Pass to seat 1", "Reveal my map", "Game"]);
    screen = click_at(text_center(&screen, "Reveal my map"), &mut draw);
    text_center(&screen, "Turn 1");
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn saves_the_covered_handoff_before_the_next_player_reveals_the_map() {
    use eframe::App as _;
    let ctx = Context::default();
    let mut app = crate::GameApp::new(
        &eframe::CreationContext::_new_kittest(ctx.clone()),
        ClientSession::hotseat(7, 2).unwrap(),
    );
    let mut host = eframe::Frame::_new_kittest();
    let mut draw = |events| {
        draw_frame(&ctx, vec2(1024.0, 700.0), events, |ui| {
            app.ui(ui, &mut host)
        })
    };
    draw(vec![]);
    let mut screen = draw(vec![]);
    for label in ["Reveal my map", "End turn", "Game", "Save hotseat game"] {
        screen = click_at(text_center(&screen, label), &mut draw);
    }
    text_center(&screen, "Pass to seat 2");
    assert!(!labels(&screen).contains(&"End turn"));

    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("handoff.hst");
    click_at(text_center(&screen, "hotseat.hst"), &mut draw);
    screen = draw(vec![
        Event::Key {
            key: egui::Key::A,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::COMMAND,
        },
        Event::Paste(path.display().to_string()),
    ]);
    screen = click_at(text_center(&screen, "Save"), &mut draw);
    assert_eq!(labels(&screen), ["Pass to seat 2", "Reveal my map", "Game"]);
    let saved = std::fs::read(path).unwrap();
    let mut resumed = ClientSession::hotseat_from_save(&saved).unwrap();
    assert_eq!(resumed.handoff_seat(), Some(PlayerId::new(2)));
    resumed.confirm_handoff();
    assert_eq!(resumed.observation().seat(), PlayerId::new(2));
    assert_eq!(resumed.observation().turn(), 0);
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
    move_past_caption(&ctx, &mut state, &mut game, size, target_at, destination);
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

fn map_callback(output: &FullOutput) -> Option<Rect> {
    output.shapes.iter().find_map(|shape| match &shape.shape {
        Shape::Callback(callback) => Some(callback.rect),
        _ => None,
    })
}

#[test]
fn clicking_a_globe_neighbor_moves_the_unit_across_a_pentagon_edge_and_preserves_the_save() {
    use histeries_sim::{BoardTopology, MapState, Session, UnitKind};
    let topology = BoardTopology::geodesic(4).unwrap();
    let origin = TileId::new(0);
    let destination = topology.edge_neighbors(origin)[4].unwrap();
    let seat = PlayerId::new(1);
    let unit = UnitId::new(1);
    let mut session = Session::with_map([seat, PlayerId::new(2)], MapState::blank(topology));
    session
        .dispatch(Command::UnitSpawn {
            player: seat,
            unit,
            kind: UnitKind::Warrior,
            tile: origin,
        })
        .unwrap();
    let mut game = ClientSession::from_save_bytes(&session.save_bytes()).unwrap();
    let mut state = Presenter::new(true);
    let ctx = Context::default();
    crate::font::install(&ctx);
    let size = vec2(1024.0, 700.0);
    frame(&ctx, &mut state, &mut game, size, vec![]);
    let screen = frame(&ctx, &mut state, &mut game, size, vec![]);
    let rect = map_callback(&screen).unwrap();
    let view = game.observation();
    let scene = scene3d::Scene::build(view.board(), seat, &state.camera3d, rect, None);
    assert_eq!(scene.pick(rect.center()), Some(origin));
    let destination_at = (6..rect.height() as u32)
        .step_by(6)
        .find_map(|y| {
            (6..rect.width() as u32)
                .step_by(6)
                .map(|x| rect.min + vec2(x as f32, y as f32))
                .find(|&p| scene.pick(p) == Some(destination))
        })
        .unwrap();
    let selected = click(&ctx, &mut state, &mut game, size, rect.center());
    click(
        &ctx,
        &mut state,
        &mut game,
        size,
        text_center(&selected, "Move"),
    );
    let moved = click(&ctx, &mut state, &mut game, size, destination_at);
    assert_eq!(game.observation().units()[0].tile, destination);
    let atlas = click(&ctx, &mut state, &mut game, size, text_center(&moved, "2D"));
    assert!(map_callback(&atlas).is_none());
    session
        .dispatch(Command::UnitMove {
            player: seat,
            unit,
            destination,
        })
        .unwrap();
    assert_eq!(game.save_bytes(), session.save_bytes());
    let restored = ClientSession::from_save_bytes(&game.save_bytes()).unwrap();
    assert_eq!(restored.observation(), game.observation());
}

/// Position a legal tile across the prompt edge and click each side.
fn move_past_caption(
    ctx: &Context,
    state: &mut Presenter,
    game: &mut ClientSession,
    size: Vec2,
    destination_at: Pos2,
    destination: TileId,
) -> FullOutput {
    let unit = game.observation().units()[0];
    let prompt = Target::Move(unit.id).prompt();
    let screen = frame(ctx, state, game, size, vec![]);
    let bounds = screen
        .shapes
        .windows(2)
        .find_map(|pair| match (&pair[0].shape, &pair[1].shape) {
            (Shape::Rect(background), Shape::Text(label))
                if label.galley.text() == prompt
                    && background.fill == INK
                    && background.rect.contains(label.pos) =>
            {
                Some(background.rect)
            }
            _ => None,
        })
        .expect("painted map prompt");
    let covered = bounds.center_top() + vec2(0.0, 1.0);
    let exposed = covered - vec2(0.0, 5.0);
    let delta = covered - destination_at;
    if state.three_d {
        state.camera3d.pan(delta);
        let view = game.observation();
        let scene = scene3d::Scene::build(
            view.board(),
            view.seat(),
            &state.camera3d,
            map_callback(&screen).unwrap(),
            state.selected,
        );
        assert_eq!(
            scene.pick(covered),
            Some(destination),
            "covered tile at {covered:?} on {size:?}"
        );
        assert_eq!(
            scene.pick(exposed),
            Some(destination),
            "exposed tile at {exposed:?} on {size:?}"
        );
    } else {
        state.camera.center -= delta / state.camera.radius;
    }
    let screen = click(ctx, state, game, size, covered);
    assert_ne!(
        screen.platform_output.cursor_icon,
        egui::CursorIcon::Crosshair
    );
    assert_eq!(game.observation().units()[0].tile, unit.tile);
    assert!(state.target == Some(Target::Move(unit.id)));
    click(ctx, state, game, size, exposed)
}

#[test]
fn relief_picks_visible_tiles_and_issues_moves_through_the_turn_controls() {
    for size in [vec2(1024.0, 700.0), vec2(400.0, 760.0)] {
        let ctx = Context::default();
        crate::font::install(&ctx);
        let mut state = Presenter::default();
        let mut game = ClientSession::new(7).unwrap();
        frame(&ctx, &mut state, &mut game, size, vec![]);
        let atlas = frame(&ctx, &mut state, &mut game, size, vec![]);
        assert!(map_callback(&atlas).is_none());
        let screen = click(&ctx, &mut state, &mut game, size, text_center(&atlas, "3D"));
        let rect = map_callback(&screen).expect("relief map submitted for drawing");
        let view = game.observation();
        let unit = view.units()[0];
        let origin = visible(view.board(), unit.tile).unwrap();
        let destination = origin
            .neighbors
            .iter()
            .copied()
            .find(|&destination| {
                let mut trial = ClientSession::from_save_bytes(&game.save_bytes()).unwrap();
                trial
                    .dispatch(Command::UnitMove {
                        player: view.seat(),
                        unit: unit.id,
                        destination,
                    })
                    .is_ok()
            })
            .unwrap();
        let scene = scene3d::Scene::build(
            view.board(),
            view.seat(),
            &state.camera3d,
            rect,
            state.selected,
        );
        let aim = |tile| {
            (8..rect.height() as u32)
                .step_by(8)
                .find_map(|y| {
                    (8..rect.width() as u32)
                        .step_by(8)
                        .map(|x| rect.min + vec2(x as f32, y as f32))
                        .find(|&at| {
                            scene.pick(at) == Some(tile)
                                && scene.pick(at - vec2(0.0, 5.0)) == Some(tile)
                        })
                })
                .expect("tile visible in the current camera")
        };
        let unit_at = aim(unit.tile);
        let destination_at = aim(destination);
        let selected = click(&ctx, &mut state, &mut game, size, unit_at);
        assert_eq!(state.selected, Some(unit.tile));
        click(
            &ctx,
            &mut state,
            &mut game,
            size,
            text_center(&selected, "Move"),
        );
        let fog = rect.min + Vec2::splat(2.0);
        assert_eq!(scene.pick(fog), None);
        click(&ctx, &mut state, &mut game, size, fog);
        assert_eq!(state.selected, Some(unit.tile));
        assert!(state.target == Some(Target::Move(unit.id)));
        let moved = move_past_caption(
            &ctx,
            &mut state,
            &mut game,
            size,
            destination_at,
            destination,
        );
        assert_eq!(game.observation().units()[0].tile, destination);
        assert_eq!(state.selected, Some(destination));
        let next = click(
            &ctx,
            &mut state,
            &mut game,
            size,
            text_center(&moved, "End turn"),
        );
        text_center(&next, "Turn 1");
        let shortcut = Event::Key {
            key: egui::Key::V,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        };
        let atlas = frame(&ctx, &mut state, &mut game, size, vec![shortcut]);
        assert!(map_callback(&atlas).is_none());
    }
}

#[test]
fn a_waiting_seat_can_inspect_its_warrior_without_issuing_orders() {
    let ctx = Context::default();
    crate::font::install(&ctx);
    let game = ClientSession::new(7).unwrap();
    let mut presenter = Presenter::default();
    let mut draw = |events| {
        draw_frame(&ctx, vec2(1024.0, 700.0), events, |ui| {
            let board = BoardView {
                observation: game.observation(),
                winner: None,
                can_order: false,
                show_seat: true,
            };
            assert!(presenter.board(ui, board, |_| {}).is_none());
        })
    };
    draw(vec![]);
    let mut screen = draw(vec![]);
    screen = click_at(unit_badge(&screen), &mut draw);
    for control in ["Hold", "End turn"] {
        screen = click_at(text_center(&screen, control), &mut draw);
    }
    text_center(&screen, "Turn 0");
}

#[test]
fn game_menu_starts_and_advances_a_globe_through_the_map_shape_controls() {
    use eframe::App as _;
    let ctx = Context::default();
    let mut app = crate::GameApp::new(
        &eframe::CreationContext::_new_kittest(ctx.clone()),
        ClientSession::new(7).unwrap(),
    );
    let mut host = eframe::Frame::_new_kittest();
    let mut draw = |events| {
        draw_frame(&ctx, vec2(1024.0, 700.0), events, |ui| {
            app.ui(ui, &mut host)
        })
    };
    draw(vec![]);
    let mut screen = draw(vec![]);
    for label in ["Game", "New game", "Globe"] {
        screen = click_at(text_center(&screen, label), &mut draw);
    }
    text_center(&screen, "Globe frequency");
    for label in ["Start game", "End turn"] {
        screen = click_at(text_center(&screen, label), &mut draw);
    }
    assert_eq!(
        app.screen(),
        crate::app::Screen::Map {
            turn: 1,
            three_d: true,
            globe: true,
        }
    );
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
