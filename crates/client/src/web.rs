//! Browser entry point and accessible status for the WebGPU canvas.
use crate::{
    app::{GameApp, Screen},
    game::ClientSession,
};
use wasm_bindgen::prelude::*;
use web_sys::HtmlCanvasElement;

/// A browser instance owns its renderer and event listeners until the page leaves.
#[wasm_bindgen]
pub struct WebGame {
    runner: eframe::WebRunner,
}

struct BrowserApp {
    game: GameApp,
    canvas: HtmlCanvasElement,
    announced: Option<Screen>,
    announced_error: String,
}

impl eframe::App for BrowserApp {
    fn ui(&mut self, ui: &mut eframe::egui::Ui, frame: &mut eframe::Frame) {
        self.game.ui(ui, frame);
        let error = self.game.file_error();
        if self.announced_error != error {
            let _ = self.canvas.set_attribute("aria-description", error);
            self.announced_error.clear();
            self.announced_error.push_str(error);
        }
        let screen = self.game.screen();
        if self.announced.as_ref() != Some(&screen) {
            let label = match &screen {
                Screen::Map {
                    turn,
                    three_d,
                    globe,
                } => {
                    let mode = if *three_d { "3D " } else { "" };
                    let shape = if *globe { "globe" } else { "strategy map" };
                    format!("Histeries {mode}{shape}, turn {turn}")
                }
                Screen::Theatre {
                    name,
                    turn,
                    expedition,
                    visited,
                } => {
                    let position = expedition.map_or_else(
                        || "no expedition".to_owned(),
                        |tile| format!("expedition at site {}", tile.index()),
                    );
                    format!("Histeries {name}, turn {turn}, {position}, {visited} visited")
                }
                Screen::Editor { globe: false } => "Histeries scenario editor".to_owned(),
                Screen::Editor { globe: true } => "Histeries globe scenario editor".to_owned(),
                Screen::Handoff(seat) => {
                    format!("Histeries hotseat handoff to seat {}", seat.index())
                }
                Screen::Result(winner, kind) => {
                    format!(
                        "Histeries hotseat result, seat {} won a {kind:?} victory",
                        winner.index()
                    )
                }
            };
            let _ = self.canvas.set_attribute("aria-label", &label);
            self.announced = Some(screen);
        }
    }
}

#[wasm_bindgen]
impl WebGame {
    /// Reports a panic caught by the renderer after startup.
    pub fn panic_message(&self) -> Option<String> {
        self.runner.panic_summary().map(|summary| summary.message())
    }

    /// Releases the renderer and input listeners.
    pub fn shutdown(&self) {
        self.runner.destroy();
    }
}

/// Starts the desktop game interface on a page-owned WebGPU canvas.
#[wasm_bindgen]
pub async fn start_game(
    canvas: HtmlCanvasElement,
    seed: u64,
    globe_frequency: Option<u32>,
) -> Result<WebGame, JsValue> {
    let game =
        ClientSession::start(seed, None, globe_frequency).map_err(|e| JsValue::from_str(&e))?;
    let runner = eframe::WebRunner::new();
    runner
        .start(
            canvas.clone(),
            eframe::WebOptions {
                depth_buffer: 24,
                ..Default::default()
            },
            Box::new(move |cc| {
                Ok(Box::new(BrowserApp {
                    game: GameApp::new(cc, game),
                    canvas,
                    announced: None,
                    announced_error: String::new(),
                }))
            }),
        )
        .await?;
    Ok(WebGame { runner })
}
