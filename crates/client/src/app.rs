use eframe::egui;

const REPLACE_SCENARIO: &str =
    "Continuing replaces the current scenario and its undo history. Save a copy to keep it.";
const REPLACE_GAME: &str = "Continuing replaces the current game and discards its editor draft. Save any changes you want to keep.";

use crate::{
    editor::Editor, font, game::ClientSession, mod_manager::ModManager, presenter::Presenter,
};

#[derive(Clone, Copy, PartialEq)]
enum FileAction {
    New,
    NewHotseat,
    FromGame,
    Open,
    OpenHotseat,
    Save,
}

impl FileAction {
    fn title(self, editing: bool, hotseat: bool) -> &'static str {
        match self {
            Self::New if editing => "New blank scenario",
            Self::New if hotseat => "New single-player game",
            Self::New => "New game",
            Self::NewHotseat => "New hotseat game",
            Self::FromGame => "Copy current game into editor",
            Self::Open if editing => "Open scenario",
            Self::Open if hotseat => "Open single-player game",
            Self::Open => "Open game",
            Self::OpenHotseat => "Open as hotseat",
            Self::Save if editing => "Save scenario",
            Self::Save if hotseat => "Save hotseat game",
            Self::Save => "Save game",
        }
    }
}

#[cfg(any(test, target_arch = "wasm32"))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Screen {
    Editor,
    Map { turn: u32, three_d: bool },
    Handoff(histeries_rules_core::PlayerId),
    Result(histeries_rules_core::PlayerId, histeries_sim::VictoryKind),
}

/// The game interface shared by the native and web eframe hosts.
pub struct GameApp {
    game: ClientSession,
    playtest: Option<(ClientSession, Presenter)>,
    editor: Option<Editor>,
    mods: ModManager,
    editing: bool,
    dimensions: [u32; 3],
    presenter: Presenter,
    dialog: Option<FileAction>,
    seed: String,
    hotseat_players: u32,
    #[cfg(target_arch = "wasm32")]
    hotseat_source: &'static str,
    #[cfg(not(target_arch = "wasm32"))]
    game_path: String,
    #[cfg(not(target_arch = "wasm32"))]
    hotseat_path: String,
    #[cfg(not(target_arch = "wasm32"))]
    scenario_path: String,
    error: String,
}

impl GameApp {
    #[cfg(target_arch = "wasm32")]
    pub(crate) fn file_error(&self) -> &str {
        &self.error
    }

    #[cfg(any(test, target_arch = "wasm32"))]
    pub(crate) fn screen(&self) -> Screen {
        if self.editing {
            return Screen::Editor;
        }
        let (active, presenter) = self
            .playtest
            .as_ref()
            .map_or((&self.game, &self.presenter), |(trial, trial_view)| {
                (trial, trial_view)
            });
        if let Some(player) = active.handoff_seat() {
            Screen::Handoff(player)
        } else if active.is_hotseat()
            && let Some((winner, kind)) = active.winner()
        {
            Screen::Result(winner, kind)
        } else {
            Screen::Map {
                turn: active.observation().turn(),
                three_d: presenter.three_d,
            }
        }
    }

    /// Presents an existing session and initializes the client font atlas.
    pub fn new(cc: &eframe::CreationContext<'_>, game: ClientSession) -> Self {
        font::install(&cc.egui_ctx);
        crate::presenter::scene3d::gpu::install(cc);
        Self::with_game(game, ModManager::restore(cc.storage))
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn set_3d(&mut self, enabled: bool) {
        self.presenter.three_d = enabled;
    }

    fn with_game(game: ClientSession, mods: ModManager) -> Self {
        Self {
            game,
            playtest: None,
            editor: None,
            mods,
            editing: false,
            dimensions: [12, 9, 2],
            presenter: Presenter::default(),
            dialog: None,
            seed: "7".into(),
            hotseat_players: 2,
            #[cfg(target_arch = "wasm32")]
            hotseat_source: crate::web_storage::HOTSEAT,
            #[cfg(not(target_arch = "wasm32"))]
            game_path: "histeries.hst".into(),
            #[cfg(not(target_arch = "wasm32"))]
            hotseat_path: "hotseat.hst".into(),
            #[cfg(not(target_arch = "wasm32"))]
            scenario_path: "scenario.hst".into(),
            error: String::new(),
        }
    }

    fn open_editor(&mut self) -> Result<(), String> {
        if self.game.is_hotseat() {
            return Err("The scenario editor is available in single-player games.".into());
        }
        if self.editor.is_none() {
            self.editor = Some(Editor::open(
                self.game.save_bytes(),
                "Game snapshot".into(),
            )?);
        }
        self.close_dialog();
        self.playtest = None;
        self.editing = true;
        Ok(())
    }

    fn start_playtest(&mut self) -> Result<(), String> {
        let draft = self.editor.as_ref().ok_or("No scenario is open.")?;
        let trial = ClientSession::from_save_bytes(draft.bytes())?;
        self.close_dialog();
        self.playtest = Some((trial, Presenter::new(self.presenter.three_d)));
        self.editing = false;
        Ok(())
    }

    fn return_to_game(&mut self) {
        self.close_dialog();
        self.playtest = None;
        self.editing = false;
    }

    fn close_dialog(&mut self) {
        self.dialog = None;
        self.error.clear();
    }

    fn execute(&mut self, action: FileAction) -> Result<(), String> {
        if self.editing {
            let editor = match action {
                FileAction::NewHotseat | FileAction::OpenHotseat => {
                    return Err("Return to the game first.".into());
                }
                #[cfg(target_arch = "wasm32")]
                FileAction::Open => Editor::open(
                    crate::web_storage::read(crate::web_storage::SCENARIO)?,
                    "Saved browser scenario".to_owned(),
                )?,
                #[cfg(target_arch = "wasm32")]
                FileAction::Save => {
                    let draft = self.editor.as_mut().ok_or("No scenario is open.")?;
                    crate::web_storage::write(crate::web_storage::SCENARIO, draft.bytes())?;
                    draft.set_source("Saved browser scenario".into());
                    return Ok(());
                }
                FileAction::New => {
                    Editor::blank(self.dimensions[0], self.dimensions[1], self.dimensions[2])?
                }
                FileAction::FromGame => {
                    Editor::open(self.game.save_bytes(), "Game snapshot".into())?
                }
                #[cfg(not(target_arch = "wasm32"))]
                FileAction::Open => {
                    let path = std::path::Path::new(&self.scenario_path);
                    Editor::open(crate::native::open_bytes(path)?, file_name(path))?
                }
                #[cfg(not(target_arch = "wasm32"))]
                FileAction::Save => {
                    let draft = self.editor.as_mut().ok_or("No scenario is open.")?;
                    let path = std::path::Path::new(&self.scenario_path);
                    crate::native::save_bytes(path, draft.bytes())?;
                    draft.set_source(file_name(path));
                    return Ok(());
                }
            };
            self.editor = Some(editor);
            return Ok(());
        }
        let game = match action {
            FileAction::FromGame => return Err("Open the scenario editor first.".into()),
            #[cfg(target_arch = "wasm32")]
            FileAction::Open => ClientSession::from_save_bytes(&crate::web_storage::read(
                crate::web_storage::GAME,
            )?)?,
            #[cfg(target_arch = "wasm32")]
            FileAction::OpenHotseat => {
                ClientSession::hotseat_from_save(&crate::web_storage::read(self.hotseat_source)?)?
            }
            #[cfg(target_arch = "wasm32")]
            FileAction::Save => {
                return crate::web_storage::write(
                    if self.game.is_hotseat() {
                        crate::web_storage::HOTSEAT
                    } else {
                        crate::web_storage::GAME
                    },
                    &self.game.save_bytes(),
                );
            }
            FileAction::New | FileAction::NewHotseat => {
                let seed = self
                    .seed
                    .trim()
                    .parse()
                    .map_err(|_| "Enter an unsigned integer seed.")?;
                if action == FileAction::NewHotseat {
                    ClientSession::hotseat(seed, self.hotseat_players)?
                } else {
                    ClientSession::new(seed)?
                }
            }
            #[cfg(not(target_arch = "wasm32"))]
            FileAction::Open => crate::native::open(std::path::Path::new(&self.game_path))?,
            #[cfg(not(target_arch = "wasm32"))]
            FileAction::OpenHotseat => ClientSession::hotseat_from_save(
                &crate::native::open_bytes(std::path::Path::new(&self.hotseat_path))?,
            )?,
            #[cfg(not(target_arch = "wasm32"))]
            FileAction::Save => {
                let path = if self.game.is_hotseat() {
                    &self.hotseat_path
                } else {
                    &self.game_path
                };
                return crate::native::save(std::path::Path::new(path), &self.game);
            }
        };
        self.editor = None;
        self.return_to_game();
        self.game = game;
        self.presenter.reset();
        Ok(())
    }

    fn file_dialog(&mut self, ctx: &egui::Context, action: FileAction) {
        egui::Modal::new(egui::Id::unique("session-file")).show(ctx, |ui| {
            ui.heading(action.title(self.editing, self.game.is_hotseat()));
            let replaces_session = action != FileAction::Save;
            if replaces_session && self.editor.is_some() {
                ui.label(if self.editing {
                    REPLACE_SCENARIO
                } else {
                    REPLACE_GAME
                });
            }
            #[cfg(not(target_arch = "wasm32"))]
            let mut replacing_file = false;
            match action {
                #[cfg(target_arch = "wasm32")]
                FileAction::Open | FileAction::Save => {
                    let slot = if self.editing {
                        "scenario"
                    } else if action == FileAction::Save && self.game.is_hotseat() {
                        "hotseat"
                    } else {
                        "game"
                    };
                    if action == FileAction::Save {
                        ui.label(format!("Replaces this site's {slot} slot."));
                    } else {
                        ui.label(format!("Read this site's {slot} slot."));
                    }
                }
                #[cfg(target_arch = "wasm32")]
                FileAction::OpenHotseat => {
                    ui.label("Resume from this site's save slot");
                    for (slot, label) in [
                        (crate::web_storage::HOTSEAT, "Hotseat"),
                        (crate::web_storage::GAME, "Single-player"),
                        (crate::web_storage::SCENARIO, "Scenario"),
                    ] {
                        ui.radio_value(&mut self.hotseat_source, slot, label);
                    }
                }
                FileAction::New if self.editing => {
                    for (index, label) in ["Width", "Height", "Players"].into_iter().enumerate() {
                        ui.horizontal(|ui| {
                            ui.label(label);
                            let range = if index == 2 {
                                histeries_sim::ScenarioDraft::PLAYERS
                            } else {
                                histeries_sim::ScenarioDraft::DIMENSIONS
                            };
                            ui.add(egui::DragValue::new(&mut self.dimensions[index]).range(range));
                        });
                    }
                }
                FileAction::New | FileAction::NewHotseat => {
                    ui.label("Map seed");
                    ui.text_edit_singleline(&mut self.seed);
                    if action == FileAction::NewHotseat {
                        ui.horizontal(|ui| {
                            ui.label("Human players");
                            ui.add(
                                egui::DragValue::new(&mut self.hotseat_players)
                                    .range(histeries_sim::ScenarioDraft::PLAYERS),
                            );
                        });
                    }
                }
                FileAction::FromGame => {}
                #[cfg(not(target_arch = "wasm32"))]
                FileAction::Open | FileAction::OpenHotseat | FileAction::Save => {
                    ui.label("Save file path (.hst)");
                    let path = if self.editing {
                        &mut self.scenario_path
                    } else if action == FileAction::OpenHotseat
                        || (action == FileAction::Save && self.game.is_hotseat())
                    {
                        &mut self.hotseat_path
                    } else {
                        &mut self.game_path
                    };
                    ui.text_edit_singleline(path);
                    replacing_file = std::path::Path::new(path).exists();
                }
            }
            if !self.error.is_empty() {
                ui.colored_label(ui.visuals().error_fg_color, &self.error);
            }
            ui.horizontal(|ui| {
                let label = match action {
                    FileAction::New if self.editing => "Create blank map",
                    FileAction::New => "Start game",
                    FileAction::NewHotseat => "Start hotseat",
                    FileAction::FromGame => "Replace with game snapshot",
                    FileAction::Open | FileAction::OpenHotseat => "Open",
                    #[cfg(not(target_arch = "wasm32"))]
                    FileAction::Save if replacing_file => "Replace save",
                    FileAction::Save => "Save",
                };
                if ui.button(label).clicked() {
                    match self.execute(action) {
                        Ok(()) => {
                            self.close_dialog();
                            if matches!(action, FileAction::NewHotseat | FileAction::OpenHotseat) {
                                ctx.request_discard("show the hotseat handoff");
                            }
                        }
                        Err(error) => self.error = error,
                    }
                }
                if ui.button("Cancel").clicked() || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                    self.close_dialog();
                }
            });
        });
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn file_name(path: &std::path::Path) -> String {
    path.file_name()
        .unwrap_or(path.as_os_str())
        .to_string_lossy()
        .into_owned()
}

impl eframe::App for GameApp {
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        let mut edit = false;
        let mut mods = false;
        let mut play = false;
        let mut resume = false;
        let mut file = None;
        let editing = self.editing;
        let playtesting = self.playtest.is_some();
        let hotseat = self.game.is_hotseat();
        let menu = |ui: &mut egui::Ui| {
            ui.menu_button(
                if editing {
                    "Scenario"
                } else if playtesting {
                    "Playtest"
                } else {
                    "Game"
                },
                |ui| {
                    if playtesting {
                        if ui.button("End playtest and edit scenario").clicked() {
                            edit = true;
                            ui.close();
                        }
                        if ui.button("End playtest and return to game").clicked() {
                            resume = true;
                            ui.close();
                        }
                        return;
                    }
                    for action in [FileAction::New, FileAction::Open, FileAction::Save] {
                        if ui.button(action.title(editing, hotseat)).clicked() {
                            file = Some(action);
                            ui.close();
                        }
                    }
                    ui.separator();
                    if editing {
                        if ui.button(FileAction::FromGame.title(true, false)).clicked() {
                            file = Some(FileAction::FromGame);
                            ui.close();
                        }
                        if ui.button("Mod manager").clicked() {
                            mods = true;
                            ui.close();
                        }
                        if ui.button("Playtest scenario").clicked() {
                            play = true;
                            ui.close();
                        }
                        if ui.button("Return to game").clicked() {
                            resume = true;
                            ui.close();
                        }
                    } else {
                        if !hotseat && ui.button("Scenario editor").clicked() {
                            edit = true;
                            ui.close();
                        }
                        for action in [FileAction::NewHotseat, FileAction::OpenHotseat] {
                            if ui.button(action.title(false, hotseat)).clicked() {
                                file = Some(action);
                                ui.close();
                            }
                        }
                    }
                },
            );
        };
        if let Some(editor) = self.editor.as_mut().filter(|_| editing) {
            editor.show(ui, menu);
        } else if let Some((game, presenter)) = &mut self.playtest {
            presenter.show(ui, game, menu);
        } else {
            self.presenter.show(ui, &mut self.game, menu);
        }
        if edit && let Err(error) = self.open_editor() {
            self.error = error;
        }
        if play && let Err(error) = self.start_playtest() {
            self.error = error;
        }
        if resume {
            self.return_to_game();
        }
        if mods {
            self.mods.open();
        }
        if let Some(editor) = self.editor.as_mut().filter(|_| self.editing) {
            self.mods
                .show(ui.ctx(), |package| editor.apply_package(package));
            self.mods.save_changes(frame.storage_mut());
        }
        if let Some(action) = file {
            #[cfg(target_arch = "wasm32")]
            {
                self.hotseat_source = crate::web_storage::HOTSEAT;
            }
            self.dialog = Some(action);
            self.error.clear();
        }
        if self.dialog.is_none() && !self.error.is_empty() {
            egui::Modal::new(egui::Id::unique("scenario_error")).show(ui.ctx(), |ui| {
                ui.label(&self.error);
                if ui.button("Close").clicked() {
                    self.error.clear();
                }
            });
        }
        if let Some(action) = self.dialog {
            self.file_dialog(ui.ctx(), action);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn playtesting_a_retained_or_blank_draft_preserves_the_live_match() {
        let mut app = GameApp::with_game(ClientSession::new(7).unwrap(), ModManager::restore(None));
        app.open_editor().unwrap();
        let draft = app.editor.as_ref().unwrap().bytes().to_vec();
        app.return_to_game();
        app.game.end_turn().unwrap();
        let live = app.game.save_bytes();
        assert_ne!(draft, live);

        app.dialog = Some(FileAction::New);
        app.error = "previous dialog error".into();
        app.open_editor().unwrap();
        assert!(app.dialog.is_none() && app.error.is_empty());
        app.dialog = Some(FileAction::New);
        app.error = "previous dialog error".into();
        app.start_playtest().unwrap();
        assert!(app.dialog.is_none() && app.error.is_empty());
        let (trial, _) = app.playtest.as_mut().unwrap();
        assert_eq!(trial.save_bytes(), draft);
        trial.end_turn().unwrap();
        trial.end_turn().unwrap();
        assert_ne!(trial.save_bytes(), live);
        assert_ne!(trial.save_bytes(), draft);
        let trial_turn = trial.observation().turn();
        assert_eq!(
            app.screen(),
            Screen::Map {
                turn: trial_turn,
                three_d: false
            }
        );
        app.open_editor().unwrap();
        assert!(app.editing && app.playtest.is_none());
        assert_eq!(app.screen(), Screen::Editor);
        assert_eq!(app.editor.as_ref().unwrap().bytes(), draft);
        app.dialog = Some(FileAction::New);
        app.error = "previous dialog error".into();
        app.return_to_game();
        assert!(app.dialog.is_none() && app.error.is_empty());
        assert_eq!(app.game.save_bytes(), live);
        assert_eq!(
            app.screen(),
            Screen::Map {
                turn: app.game.observation().turn(),
                three_d: false
            }
        );
        assert!(!app.editing && app.playtest.is_none());

        app.open_editor().unwrap();
        app.execute(FileAction::New).unwrap();
        app.start_playtest().unwrap();
        assert!(
            app.playtest
                .as_ref()
                .unwrap()
                .0
                .observation()
                .visible_tiles()
                .is_empty()
        );
        app.return_to_game();
        assert_eq!(app.game.save_bytes(), live);
        assert!(!app.editing && app.playtest.is_none());
        app.open_editor().unwrap();
        assert_ne!(app.editor.as_ref().unwrap().bytes(), live);
        app.execute(FileAction::FromGame).unwrap();
        assert_eq!(app.editor.as_ref().unwrap().bytes(), live);
        assert_eq!(app.game.save_bytes(), live);
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn starting_or_opening_a_game_discards_its_predecessors_draft() {
        let mut app = GameApp::with_game(ClientSession::new(7).unwrap(), ModManager::restore(None));
        let directory = tempfile::tempdir().unwrap();
        app.game_path = directory.path().join("original.hst").display().to_string();
        app.execute(FileAction::Save).unwrap();
        let original = app.game.save_bytes();

        app.open_editor().unwrap();
        app.return_to_game();
        app.seed = "12".into();
        app.execute(FileAction::New).unwrap();
        assert!(app.editor.is_none() && app.playtest.is_none() && !app.editing);
        assert_ne!(app.game.save_bytes(), original);

        app.open_editor().unwrap();
        assert_eq!(app.editor.as_ref().unwrap().bytes(), app.game.save_bytes());
        app.return_to_game();
        app.execute(FileAction::Open).unwrap();
        assert!(app.editor.is_none() && app.playtest.is_none() && !app.editing);
        assert_eq!(app.game.save_bytes(), original);
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn failed_game_replacement_preserves_the_live_match_and_its_retained_draft() {
        let mut app = GameApp::with_game(ClientSession::new(7).unwrap(), ModManager::restore(None));
        app.open_editor().unwrap();
        app.execute(FileAction::New).unwrap();
        let draft = app.editor.as_ref().unwrap().bytes().to_vec();
        app.return_to_game();
        let live = app.game.save_bytes();
        assert_ne!(live, draft);

        let directory = tempfile::tempdir().unwrap();
        app.game_path = directory.path().join("missing.hst").display().to_string();
        app.seed = "x".into();
        for action in [FileAction::New, FileAction::Open] {
            assert!(app.execute(action).is_err());
            assert_eq!(app.game.save_bytes(), live);
            assert_eq!(app.editor.as_ref().unwrap().bytes(), draft);
            assert!(!app.editing && app.playtest.is_none());
        }
        app.open_editor().unwrap();
        app.dimensions[0] = 0;
        let invalid = directory.path().join("invalid.hst");
        std::fs::write(&invalid, b"invalid save").unwrap();
        app.scenario_path = invalid.display().to_string();
        for action in [FileAction::New, FileAction::Open] {
            assert!(app.execute(action).is_err());
            assert_eq!(app.editor.as_ref().unwrap().bytes(), draft);
            assert_eq!(app.game.save_bytes(), live);
            assert!(app.editing && app.playtest.is_none());
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn hotseat_files_resume_unfinished_humans_and_preserve_the_solo_slot() {
        let mut app = GameApp::with_game(ClientSession::new(7).unwrap(), ModManager::restore(None));
        let directory = tempfile::tempdir().unwrap();
        app.game_path = directory.path().join("solo.hst").display().to_string();
        app.hotseat_path = directory.path().join("shared.hst").display().to_string();
        app.execute(FileAction::Save).unwrap();
        let solo = app.game.save_bytes();
        app.open_editor().unwrap();
        app.return_to_game();
        app.hotseat_players = 3;
        app.execute(FileAction::NewHotseat).unwrap();
        assert!(app.editor.is_none() && app.playtest.is_none());
        assert!(app.open_editor().is_err());
        assert!(!app.editing);
        app.game.confirm_handoff();
        app.game.end_turn().unwrap();
        app.execute(FileAction::Save).unwrap();
        let shared = app.game.save_bytes();
        assert_ne!(shared, solo);
        assert_eq!(std::fs::read(&app.game_path).unwrap(), solo);
        assert_eq!(std::fs::read(&app.hotseat_path).unwrap(), shared);
        app.execute(FileAction::Open).unwrap();
        assert!(!app.game.is_hotseat());
        assert_eq!(app.game.save_bytes(), solo);
        app.execute(FileAction::OpenHotseat).unwrap();
        assert_eq!(app.game.save_bytes(), shared);
        assert_eq!(
            app.screen(),
            Screen::Handoff(histeries_rules_core::PlayerId::new(2))
        );
        app.hotseat_path = app.game_path.clone();
        app.execute(FileAction::OpenHotseat).unwrap();
        assert_eq!(app.game.save_bytes(), solo);
        assert_eq!(
            app.screen(),
            Screen::Handoff(histeries_rules_core::PlayerId::new(1))
        );
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn game_and_scenario_files_round_trip_independently() {
        let mut app = GameApp::with_game(ClientSession::new(7).unwrap(), ModManager::restore(None));
        assert_eq!(app.game_path, "histeries.hst");
        assert_eq!(app.scenario_path, "scenario.hst");
        let directory = tempfile::tempdir().unwrap();
        let game_file = directory.path().join(&app.game_path);
        let scenario_file = directory.path().join(&app.scenario_path);
        app.game_path = game_file.display().to_string();
        app.scenario_path = scenario_file.display().to_string();

        let live = app.game.save_bytes();
        app.execute(FileAction::Save).unwrap();
        app.open_editor().unwrap();
        app.execute(FileAction::New).unwrap();
        let scenario = app.editor.as_ref().unwrap().bytes().to_vec();
        assert_ne!(scenario, live);
        app.execute(FileAction::Save).unwrap();
        assert_eq!(std::fs::read(&game_file).unwrap(), live);
        assert_eq!(std::fs::read(&scenario_file).unwrap(), scenario);

        app.dimensions = [6, 7, 3];
        app.execute(FileAction::New).unwrap();
        assert_ne!(app.editor.as_ref().unwrap().bytes(), scenario);
        app.execute(FileAction::Open).unwrap();
        assert_eq!(app.editor.as_ref().unwrap().bytes(), scenario);
        assert_eq!(app.game.save_bytes(), live);

        app.return_to_game();
        app.game.end_turn().unwrap();
        app.execute(FileAction::Save).unwrap();
        assert_eq!(std::fs::read(&game_file).unwrap(), app.game.save_bytes());
        assert_eq!(std::fs::read(&scenario_file).unwrap(), scenario);
    }
}
