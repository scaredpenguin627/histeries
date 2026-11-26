//! Map rendering, interaction and screen-space state for the game and scenario editor.

mod atlas;
pub(crate) mod editor;
mod inspector;
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
    selected: Option<TileId>,
    target: Option<Target>,
    feedback: Option<(String, bool)>,
    realm: bool,
    initialized: bool,
}

impl Presenter {
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
            *self = Self::default();
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
                        self.focus(view, id);
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

    fn focus(&mut self, view: &Observation, tile: TileId) {
        if let Some(point) = visible(view.board(), tile).map(|tile| tile.embedding) {
            self.camera.center = Camera::world(point);
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
        if !self.initialized && ui.available_rect_before_wrap().is_positive() {
            self.camera
                .fit(view.board(), ui.available_rect_before_wrap());
            self.initialized = true;
        }
        let (response, painter, hovered) = atlas::surface(ui, view.board(), &mut self.camera);
        let rect = response.rect;
        let radius = self.camera.radius;
        if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.target = None;
        }
        let locate = |id| {
            visible(view.board(), id)
                .map(|tile| self.camera.project(tile.embedding, rect))
                .filter(|center| rect.expand(radius).contains(*center))
        };
        if let Some(center) = self.selected.and_then(locate) {
            atlas::outline(&painter, center, radius, GOLD);
        }
        atlas::occupants(&painter, rect, view.board(), &self.camera, view.seat());
        if let Some(tile) = hovered.and_then(|id| visible(view.board(), id)) {
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

fn visible(view: &BoardObservation, id: TileId) -> Option<&VisibleTileObservation> {
    let tiles = view.visible_tiles();
    tiles
        .binary_search_by_key(&id, |tile| tile.tile)
        .ok()
        .map(|i| &tiles[i])
}
