use eframe::egui;

use crate::{font, game::ClientSession, presenter::Presenter};

#[derive(Clone, Copy, PartialEq)]
enum FileAction {
    New,
    #[cfg(not(target_arch = "wasm32"))]
    Open,
    #[cfg(not(target_arch = "wasm32"))]
    Save,
}

impl FileAction {
    fn title(self) -> &'static str {
        match self {
            Self::New => "New game",
            #[cfg(not(target_arch = "wasm32"))]
            Self::Open => "Open game",
            #[cfg(not(target_arch = "wasm32"))]
            Self::Save => "Save game",
        }
    }
}

/// The portable 2D game interface presented by a native or web eframe host.
pub struct GameApp {
    game: ClientSession,
    presenter: Presenter,
    dialog: Option<FileAction>,
    seed: String,
    #[cfg(not(target_arch = "wasm32"))]
    path: String,
    error: String,
}

impl GameApp {
    /// Presents an existing session and initializes the client font atlas.
    pub fn new(cc: &eframe::CreationContext<'_>, game: ClientSession) -> Self {
        font::install(&cc.egui_ctx);
        Self {
            game,
            presenter: Presenter::default(),
            dialog: None,
            seed: "7".into(),
            #[cfg(not(target_arch = "wasm32"))]
            path: "histeries.hst".into(),
            error: String::new(),
        }
    }

    fn execute(&mut self, action: FileAction) -> Result<(), String> {
        let game = match action {
            FileAction::New => {
                let seed = self
                    .seed
                    .trim()
                    .parse()
                    .map_err(|_| "Enter an unsigned integer seed.")?;
                ClientSession::new(seed)?
            }
            #[cfg(not(target_arch = "wasm32"))]
            FileAction::Open => crate::native::open(std::path::Path::new(&self.path))?,
            #[cfg(not(target_arch = "wasm32"))]
            FileAction::Save => {
                return crate::native::save(std::path::Path::new(&self.path), &self.game);
            }
        };
        self.game = game;
        self.presenter = Presenter::default();
        Ok(())
    }

    fn file_dialog(&mut self, ctx: &egui::Context, action: FileAction) {
        egui::Modal::new(egui::Id::unique("session-file")).show(ctx, |ui| {
            ui.heading(action.title());
            match action {
                FileAction::New => {
                    ui.label("Map seed");
                    ui.text_edit_singleline(&mut self.seed);
                }
                #[cfg(not(target_arch = "wasm32"))]
                FileAction::Open | FileAction::Save => {
                    ui.label("Save file path (.hst)");
                    ui.text_edit_singleline(&mut self.path);
                }
            }
            if !self.error.is_empty() {
                ui.colored_label(ui.visuals().error_fg_color, &self.error);
            }
            ui.horizontal(|ui| {
                let label = match action {
                    FileAction::New => "Start game",
                    #[cfg(not(target_arch = "wasm32"))]
                    FileAction::Open => "Open",
                    #[cfg(not(target_arch = "wasm32"))]
                    FileAction::Save if std::path::Path::new(&self.path).exists() => "Replace save",
                    #[cfg(not(target_arch = "wasm32"))]
                    FileAction::Save => "Save",
                };
                if ui.button(label).clicked() {
                    match self.execute(action) {
                        Ok(()) => self.dialog = None,
                        Err(error) => self.error = error,
                    }
                }
                if ui.button("Cancel").clicked() || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                    self.dialog = None;
                }
            });
        });
    }
}

impl eframe::App for GameApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.presenter.show(ui, &mut self.game, |ui| {
            ui.menu_button("Game", |ui| {
                for action in [
                    FileAction::New,
                    #[cfg(not(target_arch = "wasm32"))]
                    FileAction::Open,
                    #[cfg(not(target_arch = "wasm32"))]
                    FileAction::Save,
                ] {
                    if ui.button(action.title()).clicked() {
                        self.error.clear();
                        self.dialog = Some(action);
                        ui.close();
                    }
                }
            });
        });
        if let Some(action) = self.dialog {
            self.file_dialog(ui.ctx(), action);
        }
    }
}
