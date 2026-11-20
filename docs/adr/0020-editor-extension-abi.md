# ADR-0020 — WebAssembly editor packages

- Status: Accepted
- Provenance: designed

## Context

A map author should be able to distribute a repeatable transformation and run it from a desktop, a browser, the CLI or an authorized MCP editor. Each of those hosts uses the same simulation command registry and replay format.

## Decision

ABI v1 pairs a UTF-8 TOML manifest with a binary Wasm module. A pinned wasmi interpreter validates the module with floating-point instructions and start functions disabled. Each invocation receives a fresh fuel-metered store, bounded linear memory, one bounded table, a bounded call stack, and four integer host functions for reading the original map and proposing tile edits. The sole entry point is `apply() -> i32`. ABI v1 pins the admitted Wasm proposals and structural limits listed in the package guide. A change to the wasmi version, a proposal flag or a limit requires an ABI revision if it changes which programs are admitted, how fuel is charged or how stack use is counted.

The optional `extensions` feature in `histeries-sim` owns the wasmi interpreter and the ABI. This puts command validation and atomic application in one implementation shared by the client, CLI and MCP server. A headless build can omit the interpreter. Saves and replays can be read with the feature disabled because they contain the resulting ordinary commands. A separate `extension-files` feature enables filesystem loaders for the native client and CLI. The MCP server enables only `extensions` and accepts package bytes in its request.

An explicitly authorized editor supplies the issuing seat outside the guest. The simulator stages the resulting commands in emission order, validates the candidate save, and publishes the entire batch at once. Replays retain the registered commands. Replaying needs only the corresponding simulation version. The guest cannot change its own authorization or access files, network, clocks, external randomness or WASI.

This decision extends the five base tools in the MCP specification with `apply_editor_package` for authenticated editors. The caller supplies the manifest and module bytes in the request. The server binds the operation to the editor's seat and returns the package ID and accepted command count. Player and spectator credentials cannot list or invoke this tool.

## Consequences

Programs can inspect any tile in the editor's map. The host bounds work per invocation and can reject an entire operation without changing history. Future interfaces for different lifecycle hooks, command sets or numeric formats require an ABI revision. [Package format and example](../editor-packages.md)
