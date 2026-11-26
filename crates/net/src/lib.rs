//! Deterministic multiplayer command ordering and replica verification.

#![forbid(unsafe_code)]

#[cfg(not(target_arch = "wasm32"))]
pub mod lan;
mod lockstep;

pub use lockstep::{
    Commit, DesyncReport, LockstepHost, Opening, PROTOCOL_VERSION, Position, Replica,
};
