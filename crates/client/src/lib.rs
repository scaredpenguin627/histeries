//! The Histeries 2D client.

#![forbid(unsafe_code)]

#[cfg(not(target_env = "ohos"))]
mod app;
#[cfg(not(target_env = "ohos"))]
mod editor;
#[cfg(not(target_env = "ohos"))]
pub use app::GameApp;
#[cfg(not(target_env = "ohos"))]
mod font;
mod game;
pub use game::ClientSession;
#[cfg(not(any(target_env = "ohos", target_arch = "wasm32")))]
mod native;
#[cfg(not(target_env = "ohos"))]
mod presenter;

#[cfg(not(any(target_env = "ohos", target_arch = "wasm32")))]
pub use native::run;
