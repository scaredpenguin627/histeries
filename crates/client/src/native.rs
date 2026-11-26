mod lan;

use crate::{app::GameApp, game::ClientSession, lan_game::LanGame};
use eframe::egui;
use histeries_rules_core::PlayerId;
use std::{
    ffi::OsString,
    fs,
    io::Write,
    net::SocketAddr,
    path::{Path, PathBuf},
    sync::mpsc,
    time::{Duration, Instant},
};

const USAGE: &str = "histeries-client [--seed INTEGER | --load GAME.hst] [--hotseat] [--players 2..8 (new hotseat only)] [--3d] [--capture FRAME.png]\n     or: histeries-client --connect IP:PORT --seat ID [--3d] [--capture FRAME.png] (set HISTERIES_LAN_TOKEN)";

#[derive(Default)]
struct Options {
    seed: Option<u64>,
    load: Option<PathBuf>,
    hotseat: bool,
    three_d: bool,
    players: Option<u32>,
    connect: Option<SocketAddr>,
    seat: Option<PlayerId>,
    capture: Option<PathBuf>,
}

impl Options {
    fn parse(mut args: impl Iterator<Item = OsString>) -> Result<Option<Self>, String> {
        let mut options = Self::default();
        while let Some(arg) = args.next() {
            if arg == "--help" || arg == "-h" {
                return Ok(None);
            }
            if arg == "--hotseat" && !options.hotseat {
                options.hotseat = true;
                continue;
            }
            if arg == "--3d" && !options.three_d {
                options.three_d = true;
                continue;
            }
            let value = args.next().ok_or(USAGE)?;
            match arg.to_str() {
                Some("--seed") if options.seed.is_none() && options.load.is_none() => {
                    options.seed = Some(value.to_str().and_then(|s| s.parse().ok()).ok_or(USAGE)?);
                }
                Some("--load") if options.load.is_none() && options.seed.is_none() => {
                    options.load = Some(value.into());
                }
                Some("--capture") if options.capture.is_none() => {
                    options.capture = Some(value.into())
                }
                Some("--connect") if options.connect.is_none() => {
                    options.connect =
                        Some(value.to_str().and_then(|s| s.parse().ok()).ok_or(USAGE)?);
                }
                Some("--seat") if options.seat.is_none() => {
                    options.seat = Some(PlayerId::new(
                        value.to_str().and_then(|s| s.parse().ok()).ok_or(USAGE)?,
                    ));
                }
                Some("--players") if options.players.is_none() => {
                    options.players = Some(
                        value
                            .to_str()
                            .and_then(|s| s.parse().ok())
                            .filter(|n| histeries_sim::ScenarioDraft::PLAYERS.contains(n))
                            .ok_or(USAGE)?,
                    );
                }
                _ => return Err(USAGE.into()),
            }
        }
        if options.players.is_some() && (!options.hotseat || options.load.is_some()) {
            return Err(USAGE.into());
        }
        if options.connect.is_some() != options.seat.is_some()
            || (options.connect.is_some()
                && (options.hotseat || options.seed.is_some() || options.load.is_some()))
        {
            return Err(USAGE.into());
        }
        Ok(Some(options))
    }

    fn session(&self) -> Result<ClientSession, String> {
        match (&self.load, self.hotseat) {
            (Some(path), true) => ClientSession::hotseat_from_save(&open_bytes(path)?),
            (Some(path), false) => open(path),
            (None, true) => {
                ClientSession::hotseat(self.seed.unwrap_or(7), self.players.unwrap_or(2))
            }
            (None, false) => ClientSession::new(self.seed.unwrap_or(7)),
        }
    }

    fn application(&self, cc: &eframe::CreationContext<'_>) -> Result<DesktopGame, String> {
        if let (Some(address), Some(seat)) = (self.connect, self.seat) {
            let token = std::env::var("HISTERIES_LAN_TOKEN")
                .map_err(|_| "Set HISTERIES_LAN_TOKEN for the assigned seat.")?;
            let game = LanGame::connect(address, seat, token)?;
            crate::font::install(&cc.egui_ctx);
            crate::presenter::scene3d::gpu::install(cc);
            Ok(DesktopGame::Lan(Box::new(lan::LanApp::new(
                game,
                self.three_d,
            ))))
        } else {
            let mut app = GameApp::new(cc, self.session()?);
            app.set_3d(self.three_d);
            Ok(DesktopGame::Local(Box::new(app)))
        }
    }
}

pub(crate) fn open(path: &Path) -> Result<ClientSession, String> {
    ClientSession::from_save_bytes(&open_bytes(path)?)
}

pub(crate) fn open_bytes(path: &Path) -> Result<Vec<u8>, String> {
    fs::read(path).map_err(|e| format!("Open {}: {e}", path.display()))
}

pub(crate) fn save(path: &Path, game: &ClientSession) -> Result<(), String> {
    save_bytes(path, &game.save_bytes())
}

pub(crate) fn save_bytes(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let stage = || -> std::io::Result<()> {
        let mut file = tempfile::NamedTempFile::new_in(parent)?;
        file.write_all(bytes)?;
        file.as_file().sync_all()?;
        file.persist(path)?;
        Ok(())
    };
    stage().map_err(|e| format!("Save {}: {e}", path.display()))
}

struct Capture {
    path: Option<PathBuf>,
    started: Instant,
    result: mpsc::Sender<Result<(), String>>,
}

struct Desktop {
    app: DesktopGame,
    capture: Option<Capture>,
}

enum DesktopGame {
    Local(Box<GameApp>),
    Lan(Box<lan::LanApp>),
}

impl eframe::App for Desktop {
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        match &mut self.app {
            DesktopGame::Local(app) => app.ui(ui, frame),
            DesktopGame::Lan(app) => app.ui(ui, frame),
        }
        if let Some(capture) = &mut self.capture {
            let ctx = ui.ctx().clone();
            let ready = match &self.app {
                DesktopGame::Local(_) => true,
                DesktopGame::Lan(app) => match app.board_ready() {
                    Ok(ready) => ready,
                    Err(error) => {
                        let _ = capture.result.send(Err(error.to_owned()));
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        return;
                    }
                },
            };
            if ready && let Some(path) = capture.path.take() {
                let result = capture.result.clone();
                ui.ctx().request_screenshot(move |image| {
                    let _ = result.send(write_frame(&path, &image));
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    ctx.request_repaint();
                });
            } else if capture.started.elapsed() > Duration::from_secs(45) {
                let _ = capture
                    .result
                    .send(Err("GPU frame readback timed out".into()));
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            ui.ctx().request_repaint_after(Duration::from_millis(100));
        }
    }
}

fn write_frame(path: &Path, image: &egui::ColorImage) -> Result<(), String> {
    if !image.pixels.windows(2).any(|pair| pair[0] != pair[1]) {
        return Err("The GPU returned an empty or uniform frame.".into());
    }
    let rgba: Vec<_> = image
        .pixels
        .iter()
        .flat_map(|pixel| pixel.to_array())
        .collect();
    image::save_buffer_with_format(
        path,
        &rgba,
        image.width() as u32,
        image.height() as u32,
        image::ColorType::Rgba8,
        image::ImageFormat::Png,
    )
    .map_err(|e| format!("Capture {}: {e}", path.display()))
}

/// Runs the native winit event loop and presents the session using wgpu.
pub fn run() -> Result<(), String> {
    let Some(options) = Options::parse(std::env::args_os().skip(1))? else {
        println!("{USAGE}");
        return Ok(());
    };
    let capturing = options.capture.is_some();
    let (tx, rx) = mpsc::channel();
    let native = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1200.0, 800.0])
            .with_min_inner_size([440.0, 380.0]),
        renderer: eframe::Renderer::Wgpu,
        depth_buffer: 24,
        ..Default::default()
    };
    eframe::run_native(
        "Histeries",
        native,
        Box::new(move |cc| {
            Ok(Box::new(Desktop {
                app: options.application(cc)?,
                capture: options.capture.map(|path| Capture {
                    path: Some(path),
                    started: Instant::now(),
                    result: tx,
                }),
            }))
        }),
    )
    .map_err(|e| e.to_string())?;
    if capturing {
        rx.try_recv()
            .map_err(|_| "Window closed before frame capture completed".to_owned())??;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relief_launch_supports_solo_hotseat_lan_and_capture() {
        let parse = |args: &[&str]| {
            Options::parse(args.iter().map(OsString::from))
                .unwrap()
                .unwrap()
        };
        assert!(!parse(&[]).three_d);
        for args in [
            vec!["--3d", "--seed", "7"],
            vec!["--hotseat", "--3d", "--load", "match.hst"],
            vec!["--connect", "127.0.0.1:8510", "--3d", "--seat", "2"],
        ] {
            let mut args = args;
            args.extend(["--capture", "view.png"]);
            let options = parse(&args);
            assert!(options.three_d);
            assert_eq!(options.capture.as_deref(), Some(Path::new("view.png")));
        }
    }

    #[test]
    fn lan_options_require_an_address_and_seat_and_exclude_local_modes() {
        let parse = |args: &[&str]| Options::parse(args.iter().map(OsString::from));
        let options = parse(&["--connect", "127.0.0.1:8510", "--seat", "5"])
            .unwrap()
            .unwrap();
        assert_eq!(options.seat, Some(PlayerId::new(5)));
        assert_eq!(options.connect.unwrap().port(), 8510);
        for args in [
            vec!["--connect", "127.0.0.1:8510"],
            vec!["--seat", "1"],
            vec!["--connect", "127.0.0.1:8510", "--seat", "1", "--hotseat"],
            vec![
                "--connect",
                "127.0.0.1:8510",
                "--seat",
                "1",
                "--load",
                "file.hst",
            ],
        ] {
            assert!(parse(&args).is_err());
        }
    }

    #[test]
    fn hotseat_launch_loads_a_pending_human_without_advancing_the_match() {
        let parse = |args: &[&str]| Options::parse(args.iter().map(OsString::from));
        let mut game = parse(&["--players", "3", "--hotseat", "--seed", "7"])
            .unwrap()
            .unwrap()
            .session()
            .unwrap();
        game.confirm_handoff();
        game.end_turn().unwrap();
        let directory = tempfile::tempdir().unwrap();
        let file = directory.path().join("shared.hst");
        save(&file, &game).unwrap();
        let file = file.to_str().unwrap();
        let loaded = parse(&["--hotseat", "--load", file])
            .unwrap()
            .unwrap()
            .session()
            .unwrap();
        assert_eq!(
            loaded.handoff_seat(),
            Some(histeries_rules_core::PlayerId::new(2))
        );
        assert_eq!(loaded.save_bytes(), game.save_bytes());
        for args in [
            vec!["--players", "3"],
            vec!["--hotseat", "--players", "9"],
            vec!["--hotseat", "--load", file, "--players", "2"],
        ] {
            assert!(parse(&args).is_err());
        }
    }

    #[test]
    fn opening_a_replaced_save_resumes_the_last_saved_round() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("campaign.hst");
        let mut game = ClientSession::new(7).unwrap();
        save(&path, &game).unwrap();
        game.end_turn().unwrap();
        save(&path, &game).unwrap();
        let loaded = open(&path).unwrap();
        assert_eq!(loaded.observation(), game.observation());
        assert_eq!(loaded.save_bytes(), game.save_bytes());
    }
}
