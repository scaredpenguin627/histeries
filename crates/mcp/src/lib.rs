//! Model Context Protocol server boundary.

#![forbid(unsafe_code)]

/// Bootstrap marker proving the crate is wired into the workspace.
pub fn bootstrap_marker() -> &'static str {
    "mcp"
}
