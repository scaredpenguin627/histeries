//! A seat's chart, selection and orders. Screen-space state lives here alone.

mod atlas;
mod inspector;
#[cfg(test)]
mod tests;

use eframe::egui::{
    self, Align, Align2, Button, Color32, FontId, Frame, Layout, Margin, Panel, RichText,
    ScrollArea, Sense, Stroke, Ui, Vec2, vec2,
};
use histeries_hex::TileId;
use histeries_rules_core::{CityId, UnitId};
use histeries_sim::{Command, DistrictKind, Observation, VisibleTileObservation};

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

/// Interactive state shared by successive frames of the native game window.
#[derive(Default)]
pub struct Presenter {
    camera: Camera,
    selected: Option<TileId>,
    target: Option<Target>,
    feedback: Option<(String, bool)>,
    realm: bool,
    initialized: bool,
}

impl Presenter {
    /// Paints a frame and routes any submitted order through the seat's session.
    pub fn show(&mut self, root: &mut Ui, game: &mut ClientSession, menu: impl FnOnce(&mut Ui)) {
        let compact = root.available_width() < 690.0;
        let mut command = None;
        let mut end_turn = false;
        self.sync_selection(game);
        let view = game.observation();
        let ended = game.winner().is_some();
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
                    if !compact {
                        ui.weak(format!("Seat {}", view.seat().index()));
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        end_turn = ui
                            .add_enabled(
                                !ended,
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
                    if ui.button("−").on_hover_text("Zoom out").clicked() {
                        self.camera.zoom(1.0 / 1.2, Vec2::ZERO);
                    }
                    if ui.button("+").on_hover_text("Zoom in").clicked() {
                        self.camera.zoom(1.2, Vec2::ZERO);
                    }
                    if ui
                        .button("Center")
                        .on_hover_text("Center the selected tile or home city")
                        .clicked()
                        && let Some(id) = self.selected.or_else(|| home(view))
                    {
                        self.focus(game, id);
                    }
                    if !compact {
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
                ScrollArea::vertical().show(ui, |ui| {
                    if let Some((winner, kind)) = game.winner() {
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
                        self.realm_controls(ui, game, &mut command);
                    } else {
                        self.inspector(ui, game, &mut command);
                    }
                });
            });
        egui::CentralPanel::default()
            .frame(Frame::NONE)
            .show(root, |ui| {
                self.map(ui, game, &mut command);
            });
        if end_turn {
            self.target = None;
            self.feedback = match game.end_turn() {
                Ok(()) => None,
                Err(reason) => Some((reason, true)),
            };
        } else if let Some(order) = command {
            let destination = match &order {
                Command::UnitMove { destination, .. } => Some(*destination),
                _ => None,
            };
            let name = order.id();
            match game.dispatch(order) {
                Ok(()) => {
                    self.target = None;
                    if let Some(tile) = destination {
                        self.selected = Some(tile);
                    }
                    self.feedback = Some((format!("Accepted  ·  {name}"), false));
                }
                Err(reason) => self.feedback = Some((reason, true)),
            }
        }
    }

    fn focus(&mut self, game: &ClientSession, tile: TileId) {
        if let Some(point) = game.embedding(tile) {
            self.camera.center = Camera::world(point);
            self.selected = Some(tile);
        }
    }

    fn sync_selection(&mut self, game: &ClientSession) {
        if !self.initialized {
            self.selected = home(game.observation());
        }
        if self
            .selected
            .is_some_and(|id| visible(game.observation(), id).is_none())
        {
            self.selected = None;
            self.target = None;
        }
    }

    fn map(&mut self, ui: &mut Ui, game: &ClientSession, command: &mut Option<Command>) {
        let (response, painter) = ui.allocate_painter(
            ui.available_size().max(vec2(1.0, 1.0)),
            Sense::click_and_drag(),
        );
        let rect = response.rect;
        if !self.initialized && rect.is_positive() {
            self.camera.fit(game.observation(), rect);
            self.initialized = true;
        }
        atlas::fog(&painter, rect);
        let pointer = response
            .interact_pointer_pos()
            .or_else(|| response.hover_pos());
        if let Some(at) = pointer {
            let (scroll, pinch) = ui.input(|i| (i.smooth_scroll_delta.y, i.zoom_delta()));
            self.camera
                .zoom((scroll * 0.003).exp() * pinch, at - rect.center());
        }
        if response.dragged() {
            self.camera.center -= response.drag_delta() / self.camera.radius;
        }
        if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.target = None;
        }
        let radius = self.camera.radius;
        let view = game.observation();
        let mut hovered = None;
        let locate = |id| {
            game.embedding(id)
                .map(|p| self.camera.project(p, rect))
                .filter(|center| rect.expand(radius).contains(*center))
        };
        for tile in view.visible_tiles() {
            let center = self.camera.project(tile.embedding, rect);
            if rect.expand(radius).contains(center) {
                atlas::ground(&painter, tile, center, radius);
                if pointer.is_some_and(|at| atlas::contains(center, radius, at)) {
                    hovered = Some(tile);
                }
            }
        }
        for tile in view.visible_tiles() {
            let center = self.camera.project(tile.embedding, rect);
            if rect.expand(radius).contains(center) {
                atlas::river(&painter, tile, center, radius);
            }
        }
        if let Some(center) = self.selected.and_then(locate) {
            atlas::outline(&painter, center, radius, GOLD);
        }
        atlas::occupants(&painter, rect, view, &self.camera);
        if let Some(tile) = hovered {
            let center = self.camera.project(tile.embedding, rect);
            atlas::outline(&painter, center, radius * 0.94, PAPER);
            if response.clicked() {
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
        }
        if response.hovered() {
            ui.ctx().set_cursor_icon(if response.dragged() {
                egui::CursorIcon::Grabbing
            } else if self.target.is_some() {
                egui::CursorIcon::Crosshair
            } else {
                egui::CursorIcon::Grab
            });
        }
        atlas::compass(&painter, rect);
        if rect.width() > 215.0 && rect.height() > 90.0 {
            let text = self
                .target
                .map(Target::prompt)
                .unwrap_or("Hatched area  ·  out of sight");
            let at = rect.left_bottom() + vec2(10.0, -9.0);
            let galley = painter.layout_no_wrap(text.into(), FontId::proportional(12.0), PAPER);
            let label_rect = Align2::LEFT_BOTTOM.anchor_size(at, galley.size());
            painter.rect_filled(label_rect.expand(4.0), 2.0, INK);
            painter.galley(label_rect.min, galley, PAPER);
        }
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

fn visible(view: &Observation, id: TileId) -> Option<&VisibleTileObservation> {
    let tiles = view.visible_tiles();
    tiles
        .binary_search_by_key(&id, |tile| tile.tile)
        .ok()
        .map(|i| &tiles[i])
}
