# ADR-0021 — Browser hosting and saved games

- Status: Accepted
- Provenance: designed
- Date: 2026-11-20

## Context

The desktop presenter and the editor already use the simulation's save format. The browser needs a WebGPU surface and a place to keep games and scenario drafts across page reloads.

## Decision

eframe's WebRunner hosts the Rust presenter on an HTML canvas. A short inline ES module starts the application through generated wasm-bindgen bindings. Those two modules are the release site's only JavaScript. The loader obtains the canvas, starts the Rust application and displays startup errors. All game logic runs in Rust, and the build uses Cargo and the wasm-bindgen CLI, as required by the scripting and toolchain constraints in AGENTS.md §5. The HTML, bindings and WebAssembly can be served together by a static HTTPS host. For local use the Rust preview server binds to loopback and serves just the three bundle files with their content types.

The browser stores the simulation's save bytes as base64 in localStorage. Each origin has a game slot and a separate scenario slot. A successful save replaces the selected slot. Opening a scenario retains it as an editor draft and playtesting starts a separate session.

## Consequences

A player can reopen the saved game or draft on a later visit to the same browser origin. Clearing the site's data removes both slots. Storage quota or permission failures are shown in the save dialog. Desktop files and browser slots contain the same simulation format. Transferring files between them would require an import or export operation.
