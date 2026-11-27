//! The Histeries desktop and web client.

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
mod geometry;
#[cfg(not(target_env = "ohos"))]
mod mod_manager;
pub use game::ClientSession;
#[cfg(not(any(target_env = "ohos", target_arch = "wasm32")))]
mod lan_game;
#[cfg(not(any(target_env = "ohos", target_arch = "wasm32")))]
mod native;
#[cfg(not(target_env = "ohos"))]
mod presenter;
#[cfg(target_env = "ohos")]
mod presenter {
    #[allow(dead_code)] // Desktop picking and camera controls share this mesh builder.
    pub(crate) mod scene3d;
}

pub use presenter::scene3d::surface_vertices;

#[cfg(not(any(target_env = "ohos", target_arch = "wasm32")))]
pub use native::run;

#[cfg(target_arch = "wasm32")]
mod web;
#[cfg(target_arch = "wasm32")]
mod web_storage;
