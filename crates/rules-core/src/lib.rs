//! Pure entity, modifier, and yield rules.

#![forbid(unsafe_code)]

/// Bootstrap marker proving the crate is wired into the workspace.
pub fn bootstrap_marker() -> &'static str {
    "rules-core"
}
