//! Browser entry point and accessible status for the WebGPU canvas.
use crate::{app::GameApp, game::ClientSession};
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
    announced_turn: Option<u32>,
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
        let turn = self.game.displayed_game_turn();
        if self.announced_turn != turn {
            let label = match turn {
                Some(turn) => format!("Histeries strategy map, turn {turn}"),
                None => "Histeries scenario editor".to_owned(),
            };
            let _ = self.canvas.set_attribute("aria-label", &label);
            self.announced_turn = turn;
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

/// Starts the same 2D application used by the desktop client on a page-owned canvas.
#[wasm_bindgen]
pub async fn start_game(canvas: HtmlCanvasElement, seed: u64) -> Result<WebGame, JsValue> {
    let game = ClientSession::new(seed).map_err(|e| JsValue::from_str(&e))?;
    let runner = eframe::WebRunner::new();
    runner
        .start(
            canvas.clone(),
            eframe::WebOptions::default(),
            Box::new(move |cc| {
                Ok(Box::new(BrowserApp {
                    game: GameApp::new(cc, game),
                    canvas,
                    announced_turn: None,
                    announced_error: String::new(),
                }))
            }),
        )
        .await?;
    Ok(WebGame { runner })
}
