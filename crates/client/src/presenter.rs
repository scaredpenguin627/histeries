//! Map rendering, interaction and screen-space state for the game and scenario editor.

mod atlas;
pub(crate) mod editor;
mod inspector;
pub(crate) mod scene3d;
#[cfg(test)]
mod tests;

use eframe::egui::{
    self, Align, Align2, Button, Color32, FontId, Frame, Layout, Margin, Panel, RichText,
    ScrollArea, Stroke, Ui, Vec2, vec2,
};
use histeries_hex::TileId;
use histeries_rules_core::{CityId, PlayerId, UnitId};
use histeries_sim::{
    BoardObservation, Command, DistrictKind, Observation, VictoryKind, VisibleTileObservation,
};

use crate::game::ClientSession;
use atlas::{Camera, GOLD, INK, PAPER};

const MUTED: Color32 = Color32::from_rgb(176, 193, 183);
const PANEL: Color32 = Color32::from_rgb(36, 53, 52);

#[derive(Clone, Copy, PartialEq)]
enum Target {
    Move(UnitId),
    Attack(UnitId),
    Capture(UnitId),
    District(CityId, DistrictKind),
}

impl Target {
    fn prompt(self) -> &'static str {
        match self {
            Self::Move(_) => "Select a neighboring tile to move",
            Self::Attack(_) => "Select an enemy unit to attack",
            Self::Capture(_) => "Select an enemy city to capture",
            Self::District(_, _) => "Select the district site",
        }
    }

    fn command(
        self,
        tile: &VisibleTileObservation,
        view: &Observation,
    ) -> Result<Command, &'static str> {
        let player = view.seat();
        match self {
            Self::Move(unit) => Ok(Command::UnitMove {
                player,
                unit,
                destination: tile.tile,
            }),
            Self::Attack(attacker) => {
                let defender = tile
                    .units
                    .military
                    .or(tile.units.civilian)
                    .ok_or("Select a tile with an enemy unit")?;
                Ok(Command::UnitAttack {
                    player,
                    attacker,
                    defender,
                })
            }
            Self::Capture(unit) => {
                let city = view
                    .visible_cities()
                    .iter()
                    .find(|city| city.tile == tile.tile)
                    .ok_or("Select a tile with an enemy city")?;
                Ok(Command::UnitCaptureCity {
                    player,
                    unit,
                    city: city.id,
                })
            }
            Self::District(city, kind) => Ok(Command::CityBuildDistrict {
                player,
                city,
                kind,
                tile: tile.tile,
            }),
        }
    }
}

/// One seat's projected board and its ability to issue orders this frame.
pub(crate) struct BoardView<'a> {
    pub observation: &'a Observation,
    pub winner: Option<(PlayerId, VictoryKind)>,
    pub can_order: bool,
    pub show_seat: bool,
}

/// Interactive state shared by successive frames of the native game window.
#[derive(Default)]
pub struct Presenter {
    camera: Camera,
    camera3d: scene3d::Camera3d,
    pub(crate) three_d: bool,
    fit_3d: bool,
    selected: Option<TileId>,
    target: Option<Target>,
    feedback: Option<(String, bool)>,
    realm: bool,
    initialized: bool,
}

impl Presenter {
    pub(crate) fn new(three_d: bool) -> Self {
        Self {
            three_d,
            ..Default::default()
        }
    }

    /// Discard the outgoing seat's selection, camera and private order state.
    pub(crate) fn reset(&mut self) {
        *self = Self::new(self.three_d);
    }

    pub(crate) fn clear_order(&mut self) {
        self.target = None;
        self.feedback = None;
    }
    /// Paints a frame and routes any submitted order through the seat's session.
    pub fn show(&mut self, root: &mut Ui, game: &mut ClientSession, menu: impl FnOnce(&mut Ui)) {
        if let Some(seat) = game.handoff_seat() {
            egui::CentralPanel::default().show(root, |ui| {
                ui.vertical_centered(|ui| {
                    ui.add_space(ui.available_height() / 3.0);
                    ui.heading(format!("Pass to seat {}", seat.index()));
                    if ui.button("Reveal my map").clicked() {
                        game.confirm_handoff();
                        ui.ctx().request_repaint();
                    }
                    menu(ui);
                });
            });
            return;
        }
        if game.is_hotseat()
            && let Some((winner, victory)) = game.winner()
        {
            egui::CentralPanel::default().show(root, |ui| {
                ui.vertical_centered(|ui| {
                    ui.add_space(ui.available_height() / 3.0);
                    ui.heading(format!("Seat {} won", winner.index()));
                    ui.label(format!("{victory:?} victory"));
                    menu(ui);
                });
            });
            return;
        }
        let board = BoardView {
            observation: game.observation(),
            winner: game.winner(),
            can_order: game.winner().is_none(),
            show_seat: game.is_hotseat(),
        };
        if let Some(order) = self.board(root, board, menu) {
            let end_turn = matches!(order, Command::TurnEnd { .. });
            let destination = match &order {
                Command::UnitMove { destination, .. } => Some(*destination),
                _ => None,
            };
            let name = order.id();
            match game.dispatch(order) {
                Ok(()) => {
                    self.clear_order();
                    if let Some(tile) = destination {
                        self.selected = Some(tile);
                    }
                    self.feedback = (!end_turn).then(|| (format!("Accepted  ·  {name}"), false));
                }
                Err(reason) => self.feedback = Some((reason, true)),
            }
        }
        if game.is_hotseat() && (game.handoff_seat().is_some() || game.winner().is_some()) {
            self.reset();
            egui::Popup::close_all(root.ctx());
            root.ctx().request_discard("show the public hotseat screen");
            root.ctx().request_repaint();
        }
    }

    /// Draws only the supplied observation and returns at most one proposed seat command.
    pub(crate) fn board(
        &mut self,
        root: &mut Ui,
        board: BoardView<'_>,
        menu: impl FnOnce(&mut Ui),
    ) -> Option<Command> {
        let compact = root.available_width() < 690.0;
        let mut command = None;
        let mut end_turn = false;
        let view = board.observation;
        self.sync_selection(view);
        if !board.can_order {
            self.target = None;
        }
        let style = root.style_mut();
        style.visuals = egui::Visuals::dark();
        style.visuals.override_text_color = Some(PAPER);
        style.visuals.weak_text_color = Some(MUTED);
        style.visuals.widgets.inactive.weak_bg_fill = Color32::from_rgb(57, 77, 73);
        style.visuals.widgets.hovered.weak_bg_fill = Color32::from_rgb(71, 94, 85);
        style.spacing.button_padding = vec2(9.0, 5.0);

        Panel::top("match_heading")
            .frame(frame(INK, 12, 8))
            .show(root, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new("Histeries")
                            .size(if compact { 21.0 } else { 27.0 })
                            .strong(),
                    );
                    menu(ui);
                    ui.separator();
                    ui.label(
                        RichText::new(format!("Turn {}", view.turn()))
                            .color(GOLD)
                            .strong(),
                    );
                    if !compact || board.show_seat {
                        ui.weak(format!("Seat {}", view.seat().index()));
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        end_turn = ui
                            .add_enabled(
                                board.can_order,
                                Button::new(RichText::new("End turn").strong().color(INK))
                                    .fill(GOLD),
                            )
                            .clicked();
                    });
                });
            });
        Panel::bottom("map_controls")
            .frame(frame(INK, 10, 5))
            .show(root, |ui| {
                ui.horizontal_wrapped(|ui| {
                    if !ui.ctx().text_edit_focused()
                        && ui.memory(|m| m.is_above_modal_layer(ui.layer_id()))
                        && ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::V))
                    {
                        self.three_d = !self.three_d;
                    }
                    ui.selectable_value(&mut self.three_d, false, "2D");
                    ui.selectable_value(&mut self.three_d, true, "3D")
                        .on_hover_text("Switch map view (V)");
                    ui.separator();
                    for (label, hint, factor) in
                        [("−", "Zoom out", 1.0 / 1.2), ("+", "Zoom in", 1.2)]
                    {
                        if ui.button(label).on_hover_text(hint).clicked() {
                            if self.three_d {
                                self.camera3d.zoom(factor, Vec2::ZERO);
                            } else {
                                self.camera.zoom(factor, Vec2::ZERO);
                            }
                        }
                    }
                    if ui
                        .button("Center")
                        .on_hover_text("Center the selected tile or home city")
                        .clicked()
                        && let Some(id) = self.selected.or_else(|| home(view))
                    {
                        self.focus(view, id);
                    }
                    if self.three_d {
                        self.camera_controls(ui);
                        if !compact {
                            ui.weak(if view.board().globe_frequency().is_some() {
                                "Drag to rotate   ·   Shift-drag to pan   ·   Scroll to zoom"
                            } else {
                                "Drag to pan   ·   Right-drag to orbit   ·   Scroll to zoom"
                            });
                        }
                    } else if !compact {
                        ui.weak("Drag to pan   ·   Scroll to zoom");
                    }
                });
            });
        let inspector = if compact {
            Panel::bottom("compact_inspector")
                .exact_size((root.available_height() * 0.37).clamp(78.0, 235.0))
        } else {
            Panel::right("inspector").exact_size(270.0)
        };
        inspector
            .resizable(false)
            .frame(frame(PANEL, 14, 10))
            .show(root, |ui| {
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut self.realm, false, "Tile");
                    ui.selectable_value(&mut self.realm, true, "Realm");
                });
                ui.separator();
                let inspector_scroll = ScrollArea::vertical().id_salt(view.seat().index());
                inspector_scroll.show(ui, |ui| {
                    if let Some((winner, kind)) = board.winner {
                        let name = if winner == view.seat() {
                            "You won".to_owned()
                        } else {
                            format!("Seat {} won", winner.index())
                        };
                        ui.colored_label(GOLD, format!("{name} · {kind:?}"));
                    }
                    if let Some((message, is_error)) = &self.feedback {
                        ui.colored_label(
                            if *is_error { Color32::LIGHT_RED } else { MUTED },
                            message,
                        );
                    }
                    if let Some(target) = self.target {
                        ui.colored_label(GOLD, target.prompt());
                        if ui.small_button("Cancel order").clicked() {
                            self.target = None;
                        }
                    }
                    if self.realm {
                        self.realm_controls(ui, &board, &mut command);
                    } else {
                        self.inspector(ui, &board, &mut command);
                    }
                });
            });
        egui::CentralPanel::default()
            .frame(Frame::NONE)
            .show(root, |ui| {
                self.map(ui, view, &mut command);
            });
        if end_turn {
            Some(Command::TurnEnd {
                player: view.seat(),
            })
        } else {
            command
        }
    }

    fn camera_controls(&mut self, ui: &mut Ui) {
        ui.menu_button("Camera", |ui| {
            for orbit in [true, false] {
                ui.label(if orbit { "Orbit" } else { "Pan" });
                ui.horizontal(|ui| {
                    for (label, axis) in [
                        ("Left", vec2(-1.0, 0.0)),
                        ("Right", vec2(1.0, 0.0)),
                        ("Up", vec2(0.0, -1.0)),
                        ("Down", vec2(0.0, 1.0)),
                    ] {
                        if ui.button(label).clicked() {
                            if orbit {
                                self.camera3d.orbit(axis * 35.0);
                            } else {
                                self.camera3d.pan(axis * -40.0);
                            }
                        }
                    }
                });
            }
            if ui.button("Fit visible map").clicked() {
                self.fit_3d = true;
                ui.close();
            }
            if ui.button("Reset camera").clicked() {
                self.camera3d = scene3d::Camera3d::default();
                self.fit_3d = true;
                ui.close();
            }
        });
    }

    fn focus(&mut self, view: &Observation, tile: TileId) {
        if let Some(observed) = visible(view.board(), tile) {
            self.camera.center = self.camera.tile_world(observed.embedding);
            self.camera3d.focus(observed);
            self.selected = Some(tile);
        }
    }

    fn sync_selection(&mut self, view: &Observation) {
        if !self.initialized {
            self.selected = home(view);
        }
        if self
            .selected
            .is_some_and(|id| visible(view.board(), id).is_none())
        {
            self.selected = None;
            self.target = None;
        }
    }

    fn map(&mut self, ui: &mut Ui, view: &Observation, command: &mut Option<Command>) {
        let viewport = ui.available_rect_before_wrap();
        if !self.initialized && viewport.is_positive() {
            self.camera.fit(view.board(), viewport);
            self.fit_3d = true;
            self.initialized = true;
        }
        if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.target = None;
        }
        let caption = (viewport.width() > 215.0 && viewport.height() > 90.0).then(|| {
            let text = self.target.map(Target::prompt).unwrap_or(if self.three_d {
                "Uncharted area  ·  out of sight"
            } else {
                "Hatched area  ·  out of sight"
            });
            let at = viewport.left_bottom() + vec2(10.0, -9.0);
            let galley =
                ui.painter()
                    .layout_no_wrap(text.into(), FontId::proportional(12.0), PAPER);
            let bounds = Align2::LEFT_BOTTOM
                .anchor_size(at, galley.size())
                .expand(4.0);
            (bounds, galley)
        });
        let over_caption = caption.as_ref().is_some_and(|(bounds, _)| {
            ui.input(|i| {
                i.pointer
                    .interact_pos()
                    .is_some_and(|at| bounds.contains(at))
            })
        });
        let (response, painter, hovered) = if self.three_d {
            self.map_3d(ui, view, over_caption)
        } else {
            self.map_2d(ui, view, over_caption)
        };
        if response.clicked()
            && let Some(tile) = hovered.and_then(|id| visible(view.board(), id))
        {
            if let Some(target) = self.target {
                match target.command(tile, view) {
                    Ok(order) => *command = Some(order),
                    Err(reason) => self.feedback = Some((reason.to_owned(), true)),
                }
            } else {
                self.selected = Some(tile.tile);
                self.realm = false;
                self.feedback = None;
            }
        }
        if response.hovered() && !over_caption {
            ui.ctx().set_cursor_icon(if response.dragged() {
                egui::CursorIcon::Grabbing
            } else if self.target.is_some() {
                egui::CursorIcon::Crosshair
            } else {
                egui::CursorIcon::Grab
            });
        }
        if let Some((bounds, galley)) = caption {
            painter.rect_filled(bounds, 2.0, INK);
            painter.galley(bounds.min + Vec2::splat(4.0), galley, PAPER);
        }
    }

    fn map_2d(
        &mut self,
        ui: &mut Ui,
        view: &Observation,
        over_caption: bool,
    ) -> (egui::Response, egui::Painter, Option<TileId>) {
        let (response, painter, hovered) = atlas::surface(ui, view.board(), &mut self.camera);
        let hovered = hovered.filter(|_| !over_caption);
        let rect = response.rect;
        if let Some(tile) = self.selected.and_then(|id| visible(view.board(), id)) {
            atlas::outline(&painter, &self.camera, tile, rect, 1.0, GOLD);
        }
        atlas::occupants(&painter, rect, view.board(), &self.camera, view.seat());
        if let Some(tile) = hovered.and_then(|id| visible(view.board(), id)) {
            atlas::outline(&painter, &self.camera, tile, rect, 0.94, PAPER);
        }
        atlas::compass(&painter, rect);
        (response, painter, hovered)
    }

    fn map_3d(
        &mut self,
        ui: &mut Ui,
        view: &Observation,
        over_caption: bool,
    ) -> (egui::Response, egui::Painter, Option<TileId>) {
        let (response, painter) = ui.allocate_painter(
            ui.available_size().max(Vec2::splat(1.0)),
            egui::Sense::click_and_drag(),
        );
        let rect = response.rect;
        if self.fit_3d {
            self.camera3d.fit(view.board(), rect);
            if view.board().globe_frequency().is_some()
                && let Some(tile) = self
                    .selected
                    .or_else(|| home(view))
                    .and_then(|id| visible(view.board(), id))
            {
                self.camera3d.focus(tile);
            }
            self.fit_3d = false;
        }
        let pointer = response
            .interact_pointer_pos()
            .or_else(|| response.hover_pos());
        if let Some(at) = pointer {
            let (scroll, pinch) = ui.input(|i| (i.smooth_scroll_delta.y, i.zoom_delta()));
            self.camera3d
                .zoom((scroll * 0.003).exp() * pinch, at - rect.center());
        }
        let globe_drag = view.board().globe_frequency().is_some()
            && response.dragged_by(egui::PointerButton::Primary)
            && !ui.input(|i| i.modifiers.shift);
        if globe_drag || response.dragged_by(egui::PointerButton::Secondary) {
            self.camera3d.orbit(response.drag_delta());
        } else if response.dragged_by(egui::PointerButton::Primary) {
            self.camera3d.pan(response.drag_delta());
        }
        let mut scene = scene3d::Scene::build(
            view.board(),
            view.seat(),
            &self.camera3d,
            rect,
            self.selected,
        );
        let hovered = pointer
            .filter(|at| !over_caption && rect.contains(*at))
            .and_then(|at| scene.pick(at));
        if let Some(tile) = hovered {
            scene.highlight(tile);
        }
        painter.rect_filled(rect, 0.0, INK);
        scene3d::gpu::paint(&painter, rect, scene.vertices);
        (response, painter, hovered)
    }
}

fn frame(fill: Color32, horizontal: i8, vertical: i8) -> Frame {
    Frame::new()
        .fill(fill)
        .inner_margin(Margin::symmetric(horizontal, vertical))
        .stroke(Stroke::new(1.0, INK))
}

fn home(view: &Observation) -> Option<TileId> {
    view.cities()
        .first()
        .map(|c| c.tile)
        .or_else(|| view.units().first().map(|u| u.tile))
        .or_else(|| view.visible_tiles().first().map(|t| t.tile))
}

fn visible(view: &BoardObservation, id: TileId) -> Option<&VisibleTileObservation> {
    let tiles = view.visible_tiles();
    tiles
        .binary_search_by_key(&id, |tile| tile.tile)
        .ok()
        .map(|i| &tiles[i])
}
