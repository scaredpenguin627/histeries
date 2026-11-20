# P3-03 · Editor packages and mod manager

The Scenario menu opens a manager for importing, enabling, applying and removing editor packages. Native clients load a manifest and module from the selected directory. The browser accepts the same pair through file selection or drop. Woodland ships as original WAT source and matching Wasm bytes. P3-04 records the browser's end-to-end acceptance.

ABI v1 exposes integer operations for counting tiles, finding neighbors, reading fields and submitting edits. The interpreter eagerly translates a module, gives each invocation a fresh store with bounded resources, then submits ordinary commands through the simulator. The CLI and the MCP editor use the same command path. An ordinary player or spectator cannot list or use the package tool or its documentation resource. [ADR-0020](../adr/0020-editor-extension-abi.md) · [Package guide](../editor-packages.md)

## Verification on Windows GNU

Thirteen interpreter tests passed on Windows GNU and on `wasm32-unknown-unknown`. They cover field encodings, neighbor order and wrap, admission, stack limits, fuel, traps, atomic rollback, and deterministic replay. Woodland consumed 29 fuel units on the blank 4 × 3 fixture for both a newly compiled and a reused module. It emitted no edits on a fully planted board and preserved the save and replay.

The wasm32 run compiled the same Rust test bodies and interpreter code with the workspace's locked dependency versions. A temporary test entry point called each test directly in WebAssembly. The resulting module imported no host functions. Native and wasm32 both accepted the 62-level narrow call and rejected depth 63. With 256 local variables per call, depth 29 succeeded and depth 30 exceeded the value-stack limit.

One CLI test applied an edit and reproduced the saved hash through replay. One MCP test exercised a permitted editor request and rejected unauthorized sessions, file paths and oversized input. Four native manager tests covered file import, persistence, rejection and removal, including enabling and applying Woodland with mouse input. One click on Undo restored the original save bytes. The first headless run of the undo test failed when egui dropped an unconsumed texture update. Clearing each simulated frame's texture updates fixed the test harness. The standalone executable from the measured implementation produced a 1,500 × 1,000 seed-7 GPU capture when launched from a separate directory with only Windows system paths.

The implementation commit preceding the measurement commit has Git tree `a6ea81f80693224d205588e3cb4d55f519f9d64e`. Its warm fast gate passed in 59.101 seconds with 175 workspace tests, 34 parity checks and 1,200 matching round hashes. Both OpenHarmony targets and wasm32 passed their checks. Cargo-deny passed the advisory, ban, license and source checks. Cargo-about regenerated the all-features attributions. Neither `string-interner` nor `wasmparser@0.263.0` appeared in the resolved all-targets, all-features build tree.

## Earlier measurements

| Integrated revision or measurement | Fast gate | Outcome |
| --- | ---: | --- |
| First 155-test integration, including a 6 min 54 s native client test build | 738.121 s | Passed |
| Same code, measured documentation on `4837b09f9f84`, warm caches | 58.021 s | Passed |
| First integrated run with ten interpreter tests and 166 workspace tests | 77.700 s | Passed |
| Editor integration with 167 tests, native test rebuild | 2,576.511 s | Functional checks passed. The run exceeded the 900 s budget. Native test binaries took 31 min 15 s to rebuild while other workspaces were building. |
| Same editor integration, warm caches before recording measurements | 75.852 s | Passed |

Codex authored the implementation and its revisions. Fable 5.2 requested tests for the v1 fields, neighbor slots and sandbox limits. A later Opus 5.5 fallback requested explicit admission rules, fixed field encodings and a single current scorecard entry. The delivery bookmark is `task/phase3-extensions`.
