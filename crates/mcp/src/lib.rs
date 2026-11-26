//! Seat-scoped Model Context Protocol interface to a Histeries game.
#![forbid(unsafe_code)]

#[cfg(not(target_family = "wasm"))]
mod game;
#[cfg(not(target_family = "wasm"))]
mod server;
#[cfg(not(target_family = "wasm"))]
mod wire;

#[cfg(not(target_family = "wasm"))]
pub use server::{connect, serve};
