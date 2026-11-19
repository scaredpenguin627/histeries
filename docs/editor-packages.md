# Editor packages

A package directory contains `manifest.toml` and `plugin.wasm`. Both are ordinary files. The sample plants a forest on the first open plains tile. Its WAT source is included beside its binary. [Woodland package](../data/extensions/woodland/)

```toml
abi = 1
id = "woodland"
name = "Plant a woodland"
version = "1.0.0"
description = "Plant a forest on the first open plains tile in map order."
```

The ID accepts 1–64 lowercase ASCII letters, digits and interior hyphens. Name and version are nonempty display strings limited to 96 and 32 UTF-8 bytes. The optional description accepts up to 512 UTF-8 bytes. Display text is single-line. Unknown fields and ABI versions fail validation.

## Running a package

In the scenario editor, open **Scenario → Mod manager**. Choose **Browse folder** on desktop, or select or drop both package files together in the browser. Browser input is compiled in P3-03 and its interactive acceptance runs in P3-04. You can also import the bundled Woodland example. Enable a package, then apply it to the current scenario. The editor reports the number of accepted commands and lets you undo the batch in one step. The manager retains up to 16 packages and their enabled settings between sessions.

```sh
histeries editor input.hst package --package data/extensions/woodland --player 1 --output woodland.hst --replay-output woodland.hrep
histeries replay woodland.hrep
```

An MCP editor calls `apply_editor_package` with `manifest` holding the UTF-8 TOML text and `wasm_base64` holding the standard padded Base64 encoding of the Wasm bytes. The result contains the package ID and the number of accepted commands. The connection supplies the editor's seat. [MCP guide](mcp.md)

## ABI v1

Export `apply() -> i32`. Return zero to submit all proposed edits. A nonzero return, trap, invalid command, incompatible terrain or invalid resulting save discards the entire batch. Each read sees the map as it stood before the invocation. Repeated calls start with fresh globals, tables and memory. The package's last call cannot influence its next call except through accepted simulation edits.

All imports belong to `histeries_editor_v1`. All parameters and results in this table are Wasm `i32`.

| Import | Result |
| --- | --- |
| `tile_count()` | Number of tiles. Valid tile IDs run from 0 up to this count, exclusive. |
| `neighbor(tile, slot)` | Tile ID of the neighbor in ascending ID order, or -1 if the slot is empty. Slots run from 0 through 5. |
| `read(tile, field)` | ABI v1 value for the requested field. |
| `emit(tile, field, value)` | No return value. Appends a typed editor command bound to the editor's seat. |

| Field | Values |
| --- | --- |
| 0, terrain | 0 plains, 1 grassland, 2 desert, 3 tundra, 4 coast, 5 ocean, 6 hill, 7 mountain |
| 1, feature | 0 none, 1 forest, 2 jungle, 3 marsh |
| 2, river | -1 none, 0 northeast, 1 east, 2 southeast, 3 southwest, 4 west, 5 northwest |
| 3, resource | -1 none, 0 wheat, 1 stone, 2 iron, 3 gold |

ABI v1 enables mutable globals, sign-extension instructions, multi-value functions, bulk memory, reference types, tail calls and extended constant expressions. It disables floating-point types and instructions, saturating float-to-integer conversion, multi-memory, custom page sizes, wide arithmetic and start functions. The pinned interpreter is built without memory64, SIMD or relaxed SIMD. A different map format or a newly added map value does not extend the numeric tables above.

At validation, modules may declare up to 1,000 globals, 10,000 functions, 100 tables, one memory, 1,000 element segments and 1,000 data segments. Each function type has at most 32 parameters and 32 results. When combined function bodies total at least 1,000 bytes, their mean size must be at least 40 bytes. The stricter runtime table and memory limits below also apply when instantiating the module.

The manifest is limited to 4 KiB and the module to 64 KiB. Runtime limits are eager compilation followed by 100,000 wasmi execution fuel units, one memory up to 256 KiB, one table up to 64 elements, 64 recursive calls, a 64 KiB value stack and 128 emitted commands. The ordinary interactive per-seat order budget still applies. Unsupported imports, start functions, and floating-point instructions fail at import time. A limit violation or invalid field cancels the invocation.

To rebuild the example using the standard WebAssembly text format, use `cargo install wasm-tools --version 1.263.0 --locked`, then run `wasm-tools parse data/extensions/woodland/plugin.wat -o data/extensions/woodland/plugin.wasm`. The example test checks the shipped binary against the source using the repository's pinned WAT parser.
