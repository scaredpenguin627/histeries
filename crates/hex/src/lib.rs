//! Topology-independent tile graph primitives.

#![forbid(unsafe_code)]

/// Bootstrap marker proving the crate is wired into the workspace.
pub fn bootstrap_marker() -> &'static str {
    "hex"
}
