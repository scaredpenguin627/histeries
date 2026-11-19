use crate::{app::GameApp, game::ClientSession};
use eframe::egui;
use std::{
    ffi::OsString,
    fs,
    io::Write,
    path::{Path, PathBuf},
    sync::mpsc,
    time::{Duration, Instant},
};

const USAGE: &str = "histeries-client [--seed INTEGER | --load GAME.hst] [--capture FRAME.png]";

#[derive(Default)]
struct Options {
    seed: Option<u64>,
    load: Option<PathBuf>,
    capture: Option<PathBuf>,
}

impl Options {
    fn parse(mut args: impl Iterator<Item = OsString>) -> Result<Option<Self>, String> {
        let mut options = Self::default();
        while let Some(arg) = args.next() {
            if arg == "--help" || arg == "-h" {
                return Ok(None);
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
                _ => return Err(USAGE.into()),
            }
        }
        Ok(Some(options))
    }
}

pub(crate) fn open(path: &Path) -> Result<ClientSession, String> {
    let bytes = fs::read(path).map_err(|e| format!("Open {}: {e}", path.display()))?;
    ClientSession::from_save_bytes(&bytes)
}

pub(crate) fn save(path: &Path, game: &ClientSession) -> Result<(), String> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let stage = || -> std::io::Result<()> {
        let mut file = tempfile::NamedTempFile::new_in(parent)?;
        file.write_all(&game.save_bytes())?;
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
    app: GameApp,
    capture: Option<Capture>,
}

impl eframe::App for Desktop {
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        self.app.ui(ui, frame);
        if let Some(capture) = &mut self.capture {
            let ctx = ui.ctx().clone();
            if let Some(path) = capture.path.take() {
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
    let game = match options.load {
        Some(path) => open(&path)?,
        None => ClientSession::new(options.seed.unwrap_or(7))?,
    };
    let capturing = options.capture.is_some();
    let (tx, rx) = mpsc::channel();
    let native = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1200.0, 800.0])
            .with_min_inner_size([440.0, 380.0]),
        renderer: eframe::Renderer::Wgpu,
        ..Default::default()
    };
    eframe::run_native(
        "Histeries",
        native,
        Box::new(move |cc| {
            Ok(Box::new(Desktop {
                app: GameApp::new(cc, game),
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
