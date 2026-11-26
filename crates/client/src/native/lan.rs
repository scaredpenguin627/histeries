//! A desktop player bound to a single remote seat.
use crate::{
    lan_game::LanGame,
    presenter::{BoardView, Presenter},
};
use eframe::egui;
use std::time::Duration;

pub(super) struct LanApp {
    game: LanGame,
    presenter: Presenter,
    submission_error: Option<String>,
}

impl LanApp {
    pub(super) fn new(game: LanGame, three_d: bool) -> Self {
        Self {
            game,
            presenter: Presenter::new(three_d),
            submission_error: None,
        }
    }

    pub(super) fn board_ready(&self) -> Result<bool, &str> {
        if self.game.observation().is_some() {
            Ok(true)
        } else if let Some(error) = self.game.error() {
            Err(error)
        } else {
            Ok(false)
        }
    }
}

impl eframe::App for LanApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.game.poll();
        ui.ctx().request_repaint_after(Duration::from_millis(200));
        let joined = self.game.observation().is_some();
        egui::Panel::top("lan_connection").show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.strong(format!("LAN · Seat {}", self.game.seat().index()));
                if let Some(error) = self.game.error().or(self.submission_error.as_deref()) {
                    ui.colored_label(ui.visuals().error_fg_color, error);
                } else if !joined {
                    ui.label("Connecting…");
                } else if self.game.winner().is_none() {
                    if let Some(active) = self.game.active_seat()
                        && active != self.game.seat()
                    {
                        ui.label(format!("Waiting for seat {}", active.index()));
                    } else if self.game.busy() {
                        ui.label("Waiting for host…");
                    } else {
                        ui.label("Your turn");
                    }
                }
                if let Some(status) = self.game.status() {
                    ui.label(status);
                }
                if self.game.can_sync() && ui.button("Synchronize").clicked() {
                    self.submission_error = self.game.sync().err();
                }
                if ui.button("Leave match").clicked() {
                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                }
            });
        });
        if let Some(observation) = self.game.observation() {
            let board = BoardView {
                observation,
                winner: self.game.winner(),
                can_order: self.game.ready(),
                show_seat: true,
            };
            if let Some(command) = self.presenter.board(ui, board, |_| {}) {
                self.submission_error = self.game.submit(command).err();
                if self.submission_error.is_none() {
                    self.presenter.clear_order();
                }
                ui.ctx().request_repaint();
            }
        } else {
            egui::CentralPanel::default().show(ui, |_| {});
        }
    }
}
