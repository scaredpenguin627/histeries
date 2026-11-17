# Histeries architecture

Seed document. AGENTS.md holds the binding rules (section 5 and 6); this file holds the detail they point to. The Phase 0 bootstrap completes it (data-flow diagrams, the determinism and threading model) and keeps it true afterwards. If it disagrees with AGENTS.md, AGENTS.md wins.

## 1. Crate graph

```
hex → rules-core → ruleset → sim → ai → net → mcp → cli → client
```

A crate may depend only on crates to its left. `cargo xtask layers` enforces this in CI. Nothing below `client` may depend on a windowing or GPU crate, and nothing below `client` may pull in a `-sys` crate or C build script without an ADR (so that `cargo check` for OpenHarmony needs no SDK).

| Crate | Responsibility |
|---|---|
| `hex` | `TileTopology` trait: tile IDs, `neighbors` (degree 5 or 6), `distance`, `ring`, `area`, `line_of_sight`, an embedding for rendering. `FlatHex` (with wrap options) first; `Geodesic` (a Goldberg-polyhedron hex-pentagon sphere) later. The only crate that may know about axial or cube coordinates. No game knowledge. |
| `rules-core` | Entity types, the modifier language (`effect` + `conditions` + `scope`), yield math. Pure functions. |
| `ruleset` | Loads TOML rulesets into validated tables and ships the base ruleset (`data/rulesets/base/`). Eras and ages are data with transition hooks. |
| `sim` | World state in generational arenas, the turn loop, the command registry, the observation API, the event stream, serialization, replay, and the session server. |
| `ai` | Players. They see the world only through an `Observation`. Three layers: strategic (utility scoring), operational (HTN/GOAP), tactical (MCTS or beam search over joint unit orders). |
| `net` | Lockstep transport (Phase 4): the same commands and per-turn state hashes over a second transport. |
| `mcp` | The Model Context Protocol server (`rmcp`; stdio and streamable HTTP) exposing the command registry and observation API with seat-level permissions. |
| `cli` | The `histeries` binary with subcommands `new`, `step`, `run`, `replay`, `tournament`, `map`, `mcp` and `serve`; ASCII map dump; batch self-play; `editor.*` commands. |
| `client` | Thin observer and presenter on wgpu + egui + winit, 2D first. Never mutates state; contains no rules logic. The only crate allowed `unsafe`. |

## 2. Data flow

Every state change is a typed `Command` with a stable string ID and a validator; every read is an `Observation` for one seat, with fog of war applied. CLI, MCP, AI, replays, tests and the client all go through those two doors, so replays are command logs plus periodic snapshots, an agent can drive any feature, and a hosted server and peer lockstep are two transports over one model.

```
client / cli / mcp / ai ──Command──▶ session server (sim) ──Event, Observation──▶ every attached seat
                                         │
                                         └─ per-turn state hash ──▶ replay log, CI determinism gate, desync reports
```

## 3. Determinism and threading (to be completed in Phase 0)

- Integers and fixed-point only (format: ADR-0003). Seeded, named RNG streams with a pinned generator (ADR-0002). Sorted-ID iteration; no hash-map iteration in simulation code.
- AI planning for all players runs in parallel over an immutable snapshot; results are commands applied sequentially in canonical order. Pathfinding, visibility and yield recomputation are pure functions over snapshots (rayon allowed). Thread count changes speed only; CI compares per-turn hashes at 1 vs N threads.

## 4. Memory model

Arenas; no reference cycles in `sim`; every cache bounded with explicit eviction. State may legitimately grow with the empire, so the gates are bytes per live entity, a bounded slope after turn 100, and a save → reload-in-fresh-process → compare-heap test.

## 5. Platforms

Tier A: Linux x86_64, macOS arm64, Windows x86_64 (msvc). Tier B: wasm32 (web client). OpenHarmony: the core crates are `cargo check`ed for `aarch64-unknown-linux-ohos` and `x86_64-unknown-linux-ohos` from Phase 1; a native ArkTS/ArkUI + XComponent shell is a Phase 5+ goal that depends on wgpu Vulkan-on-OHOS support and the community winit-ohos backend.

## 6. Repository layout

```
Cargo.toml  Cargo.lock  rust-toolchain.toml  rustfmt.toml  clippy.toml  deny.toml  about.toml
.cargo/config.toml  .config/nextest.toml  .editorconfig  .gitignore  .gitattributes  .vscode/extensions.json
README.md  LICENSE-MIT  LICENSE-APACHE  THIRD_PARTY_LICENSES.md  CONTRIBUTING.md  CODE_OF_CONDUCT.md
SECURITY.md  CHANGELOG.md  ATTRIBUTION.md  AGENTS.md
.agents/skills/{session-start,session-finish,review-change}/SKILL.md
.codex/{hooks.json,hooks/,agents/}                       optional guardrails (docs/codex-cloud.md)
.github/workflows/{pr,nightly,weekly,release}.yml  CODEOWNERS  PR and issue templates
xtask/                                                   layers, ci-fast, soak, memgate, ohos-check
crates/{hex,rules-core,ruleset,sim,ai,net,mcp,cli,client}
data/rulesets/base/*.toml  data/LICENSE (CC-BY-4.0)
assets/  assets/LICENSES.md                              provenance line per asset
fixtures/oracle/                                         maintainer-supplied, tagged `observed`
specs/                                                   behavioral specs with provenance tags
scripts/                                                 codex-cloud-setup.sh, check-env.sh, check_port.py, test_hooks.py, test_check_port.py
tools/                                                   Python analytics via uv
docs/{ARCHITECTURE,CLEANROOM,ROADMAP,scorecard,budgets,assets,tooling,codex-cloud,commands}.md
docs/adr/  docs/sessions/  docs/prompts/
```
