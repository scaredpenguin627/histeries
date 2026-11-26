//! Seat-private expedition charts share the surface turn and command dispatcher.

use super::{BoardView, GOLD, INK, MUTED, PANEL, PAPER, Presenter, atlas, frame};
use eframe::egui::{
    self, Align2, Button, Color32, FontId, Frame, Key, Panel, Rect, ScrollArea, Sense, Shape,
    Stroke, Ui, vec2,
};
use histeries_hex::TileId;
use histeries_sim::{Command, Observation, TheatreObservation, TheatreTileObservation};

/// Chart shortcuts apply to the foreground game, once per physical keypress.
fn shortcut(ui: &Ui, key: Key) -> bool {
    if ui.ctx().text_edit_focused()
        || egui::Popup::is_any_open(ui.ctx())
        || !ui.memory(|m| m.is_above_modal_layer(ui.layer_id()))
    {
        return false;
    }
    ui.input_mut(|i| {
        i.focused
            && i.events.iter().any(|event| {
                matches!(event,
                    egui::Event::Key { key: actual, pressed: true, repeat: false, modifiers, .. }
                    if *actual == key && modifiers.is_none()
                )
            })
            && i.consume_key(egui::Modifiers::NONE, key)
    })
}

impl Presenter {
    pub(crate) fn active_theatre<'a>(
        &self,
        view: &'a Observation,
    ) -> Option<&'a TheatreObservation> {
        let id = self.theatre_id.as_ref()?;
        view.theatres().iter().find(|chart| &chart.id == id)
    }

    pub(super) fn board_chooser(&mut self, ui: &mut Ui, view: &Observation) {
        let charts = view.theatres();
        if self.active_theatre(view).is_none() {
            self.theatre_id = None;
        }
        let Some(first) = charts.first() else { return };
        let previous = self.theatre_id.as_deref();
        let mut chosen = previous;
        ui.horizontal_wrapped(|ui| {
            ui.label("Board");
            ui.selectable_value(&mut chosen, None, "Surface")
                .on_hover_text("Switch board (O)");
            for chart in charts {
                ui.selectable_value(&mut chosen, Some(chart.id.as_str()), &chart.name);
            }
        });
        if shortcut(ui, Key::O) {
            chosen = if chosen.is_some() {
                None
            } else {
                Some(&first.id)
            };
        }
        if chosen != previous {
            self.theatre_id = chosen.map(str::to_owned);
            self.clear_order();
        }
    }

    pub(super) fn theatre(
        &self,
        root: &mut Ui,
        board: &BoardView<'_>,
        chart: &TheatreObservation,
    ) -> Option<Command> {
        let view = board.observation;
        let player = view.seat();
        let position = chart
            .tiles
            .iter()
            .find(|site| Some(site.tile) == chart.expedition);
        let mut command = None;
        let compact = root.available_width() < 690.0;
        let panel = if compact {
            Panel::bottom("expedition_orders_compact")
                .exact_size((root.available_height() * 0.4).clamp(78.0, 235.0))
        } else {
            Panel::right("expedition_orders").exact_size(270.0)
        };
        panel
            .resizable(false)
            .frame(frame(PANEL, 14, 10))
            .show(root, |ui| {
                ScrollArea::vertical()
                    .id_salt(("expedition_orders", player.index(), &chart.id))
                    .show(ui, |ui| {
                        ui.heading(&chart.name);
                        if let Some((winner, kind)) = board.winner {
                            let result = format!("Seat {} won · {kind:?}", winner.index());
                            ui.colored_label(GOLD, result);
                        }
                        let visited = chart.tiles.iter().filter(|site| site.visited).count();
                        ui.weak(format!("{visited} visited · {} charted", chart.tiles.len()));
                        ui.separator();
                        command = if let Some(position) = position {
                            travel_controls(ui, board, chart, position)
                        } else {
                            launch_controls(ui, board, chart)
                        };
                        let has_orders = chart.can_move || chart.can_launch || chart.can_recall;
                        if board.can_order
                            && !has_orders
                            && (position.is_some() || !view.cities().is_empty())
                        {
                            ui.weak("End this turn to refresh expedition orders.");
                        }
                        ui.separator();
                        ui.weak(concat!(
                            "O  Switch board   ·   L  Launch\n",
                            "N  Survey next site   ·   R  Recall\n",
                            "E  End turn"
                        ));
                        if let Some((message, error)) = &self.feedback {
                            let color = if *error { Color32::LIGHT_RED } else { MUTED };
                            ui.colored_label(color, message);
                        }
                    });
            });
        egui::CentralPanel::default()
            .frame(Frame::NONE)
            .show(root, |ui| {
                if board.can_order && shortcut(ui, Key::E) {
                    command = Some(Command::TurnEnd { player });
                }
                let reachable = position
                    .filter(|_| chart.can_move && board.can_order)
                    .map_or(&[][..], |site| site.neighbors.as_slice());
                if let Some(tile) = draw_chart(ui, chart, reachable) {
                    command = Some(Command::TheatreMove {
                        player,
                        theatre: chart.id.clone(),
                        tile,
                    });
                }
            });
        command
    }
}

fn launch_controls(
    ui: &mut Ui,
    board: &BoardView<'_>,
    chart: &TheatreObservation,
) -> Option<Command> {
    ui.strong("No expedition");
    ui.label(format!("Launch to entry site {}", chart.entry.index()));
    let can_launch = chart.can_launch && board.can_order;
    let launch_first = can_launch && shortcut(ui, Key::L);
    let mut command = None;
    for (index, city) in board.observation.cities().iter().enumerate() {
        let label = format!("Launch from city {}", city.id.index());
        if ui.add_enabled(can_launch, Button::new(label)).clicked() || (index == 0 && launch_first)
        {
            command = Some(Command::TheatreLaunch {
                player: board.observation.seat(),
                theatre: chart.id.clone(),
                city: city.id,
            });
        }
    }
    if board.observation.cities().is_empty() {
        ui.weak("Found a surface city to launch an expedition.");
    }
    command
}

fn travel_controls(
    ui: &mut Ui,
    board: &BoardView<'_>,
    chart: &TheatreObservation,
    position: &TheatreTileObservation,
) -> Option<Command> {
    ui.strong(format!("Expedition at site {}", position.tile.index()));
    ui.weak(format!("Recall at entry site {}", chart.entry.index()));
    ui.label("Travel to an adjacent site");
    let can_move = chart.can_move && board.can_order;
    let mut next = None;
    if can_move && shortcut(ui, Key::N) {
        next = chart
            .tiles
            .iter()
            .filter(|site| position.neighbors.contains(&site.tile))
            .min_by_key(|site| (site.visited, site.tile))
            .map(|site| site.tile);
    }
    ui.horizontal_wrapped(|ui| {
        for &tile in &position.neighbors {
            if ui
                .add_enabled(can_move, Button::new(tile.index().to_string()))
                .clicked()
            {
                next = Some(tile);
            }
        }
    });
    let can_recall = chart.can_recall && board.can_order;
    let recall = ui
        .add_enabled(can_recall, Button::new("Recall expedition"))
        .on_hover_text("Recall the expedition at the entry site (R)");
    let player = board.observation.seat();
    if recall.clicked() || (can_recall && shortcut(ui, Key::R)) {
        return Some(Command::TheatreRecall {
            player,
            theatre: chart.id.clone(),
        });
    }
    next.map(|tile| Command::TheatreMove {
        player,
        theatre: chart.id.clone(),
        tile,
    })
}

fn draw_chart(ui: &mut Ui, chart: &TheatreObservation, reachable: &[TileId]) -> Option<TileId> {
    let (response, painter) = ui.allocate_painter(ui.available_size(), Sense::click());
    let rect = response.rect;
    atlas::fog(&painter, rect);
    let mut extent = Rect::NOTHING;
    for tile in &chart.tiles {
        extent.extend_with(atlas::Camera::world(tile.embedding).to_pos2());
    }
    if !extent.is_finite() || !rect.is_positive() {
        return None;
    }
    let bounds = extent.expand(1.25);
    let radius = (rect.width() / bounds.width())
        .min(rect.height() / bounds.height())
        .min(54.0);
    let pointer = response
        .interact_pointer_pos()
        .or_else(|| response.hover_pos());
    let mut hovered = None;
    for tile in &chart.tiles {
        let center = rect.center()
            + (atlas::Camera::world(tile.embedding) - bounds.center().to_vec2()) * radius;
        let pick = pointer.is_some_and(|p| atlas::contains(center, radius, p));
        let can_travel = reachable.contains(&tile.tile);
        let occupied = chart.expedition == Some(tile.tile);
        let fill = match (tile.visible, tile.visited) {
            (true, true) => Color32::from_rgb(70, 111, 105),
            (true, false) => Color32::from_rgb(53, 81, 84),
            (false, true) => Color32::from_rgb(57, 72, 71),
            (false, false) => INK,
        };
        let outline = if can_travel && pick {
            PAPER
        } else if can_travel || occupied {
            GOLD
        } else {
            MUTED
        };
        painter.add(Shape::convex_polygon(
            atlas::vertices(center, radius * 0.96).to_vec(),
            fill,
            Stroke::new(if occupied { 2.5 } else { 1.0 }, outline),
        ));
        if radius >= 10.0 {
            let text = if tile.tile == chart.entry && radius >= 24.0 {
                format!("Entry\n{}", tile.tile.index())
            } else {
                tile.tile.index().to_string()
            };
            painter.text(
                center,
                Align2::CENTER_CENTER,
                text,
                FontId::proportional((radius * 0.38).clamp(9.0, 18.0)),
                PAPER,
            );
        }
        if occupied {
            painter.circle_filled(
                center - vec2(0.0, radius * 0.66),
                (radius * 0.10).max(1.0),
                GOLD,
            );
        } else if tile.visited {
            painter.circle_stroke(
                center - vec2(0.0, radius * 0.66),
                (radius * 0.09).max(1.0),
                Stroke::new(1.1, PAPER),
            );
        }
        if pick {
            hovered = Some((tile, can_travel));
        }
    }
    let (tile, can_travel) = hovered?;
    let clicked = response.clicked();
    let state = describe_site(tile);
    if can_travel {
        response
            .on_hover_cursor(egui::CursorIcon::PointingHand)
            .on_hover_text(format!(
                "Site {} · {state}\nClick to travel",
                tile.tile.index()
            ));
    } else {
        response.on_hover_text(format!("Site {} · {state}", tile.tile.index()));
    }
    (clicked && can_travel).then_some(tile.tile)
}

fn describe_site(tile: &TheatreTileObservation) -> &'static str {
    match (tile.visited, tile.visible) {
        (true, true) => "Visited · in view",
        (true, false) => "Visited · out of view",
        (false, true) => "Unvisited · in view",
        (false, false) => "Unvisited · out of view",
    }
}
