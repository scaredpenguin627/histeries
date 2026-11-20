# Histeries — standing instructions for every agent session

Histeries is an open-source, clean-room, turn-based 4X strategy game in the Civilization genre, written in Rust and built with AI coding agents. This file is the single source of truth for how work is done here. Read it in full at the start of every session. `CLAUDE.md` only imports this file; instructions live nowhere else.

## 0. How a session works (cloud workflow)
Every session starts in a fresh cloud workspace. Nothing from earlier sessions exists except what is in the GitHub repository. Therefore:
1. Install the pinned jj release (docs/tooling.md) from its GitHub release archive; fall back to `cargo install jj-cli --locked`. Confirm `jj --version`.
2. `jj git clone <repo-url> histeries` (non-colocated). Never run `git` yourself — see §10.
3. Read, in order: AGENTS.md, docs/ROADMAP.md, docs/scorecard.md, the spec section(s) for the task, and any ADRs it touches.
4. State your plan in one paragraph: task, crates touched, tests you will add, gates you will run. Then start.
5. Work on bookmark `task/<slug>`, one task per session unless told otherwise. Commit small, described changes as you go.
6. Before calling the task done: fast gate green locally (`cargo xtask ci-fast`); cross-model review passed (§11); docs/scorecard.md and CHANGELOG.md updated; session report written (§13).
7. `jj git fetch`, rebase onto `main@origin`, `jj git push --bookmark task/<slug>`, open or update the pull request. Nothing may be left unpushed when the session ends: an unpushed change is lost.
8. If blocked — credentials, an undecided ADR, a missing spec — stop and report. Do not improvise around the block.

## 1. Mission
Build a turn-based, hex-grid 4X game with the strategic depth of the genre's best and five engineering properties its commercial incumbents lack:
1. AI that plans rather than follows scripted rules, and fights competently at the unit level.
2. A simulation that scales across cores while staying bit-for-bit deterministic.
3. No memory leaks or unbounded growth, verified by measurement.
4. Strict internal decoupling: rules, simulation, AI, networking, MCP and UI are separate crates with an enforced dependency direction.
5. Every feature drivable by LLM agents through a Model Context Protocol (MCP) server.
Histeries is not a copy of any specific game's content, text, art, UI or code.

## 2. Glossary
- MCP: always the Model Context Protocol — the open standard (spec 2026-07-28) through which LLM hosts connect to tool servers. Histeries ships an MCP *server* built on the official Rust SDK (`rmcp`). Histeries never calls an LLM itself; in-game generative features are out of scope.
- Spec: a behavioral specification in `specs/`, written by the maintainer, with provenance tags (§3.1).
- Gate: a CI check that must pass before merge.
- Topology: the abstract tile graph a map is built on (§6.8).

## 3. Hard constraints

### 3.1 Clean-room protocol
- Histeries is written from the specs in `specs/` and from public documentation restated in the maintainer's words. Never request, read, summarize or reason from decompiled or disassembled code of any commercial game; from any game's shipped data files or scripts (XML, SQL, Lua, etc.); from Steam Workshop or other third-party mods (their code, data or text); or from a game's proprietary text.
- If any such material appears — pasted, linked, or found on disk — stop, say so, do not use it, and ask for a behavioral description in the maintainer's own words.
- Every spec rule carries a provenance tag: `observed` (measured by the maintainer in play through official tools), `public-doc` (public documentation or public design write-ups, restated), or `designed` (our own decision). When a rule is missing or ambiguous, propose a `designed` rule and record it as an ADR; never fill the gap by guessing another game's behavior.
- Numbers we tune are ours. Do not try to reproduce another game's formulas unless the spec supplies them under `observed`.
- Mod authors' public write-ups may be cited as `public-doc`, and mod authors are welcome as contributors under our license. Their mods are not.

### 3.2 Content and naming
- Real-world history is free: nations, historical leaders, wonders, technologies, unit types.
- Other games' and studios' trademarks (Civilization, Civ, Sid Meier's, Firaxis, 2K, Humankind, Amplitude) may appear only inside `public-doc` citations in specs and ADRs — never in the project name, code, UI, docs or marketing. Never reproduce another game's art, audio, encyclopedia text, leader portraits, UI layout or iconography. All text and placeholder art is original, and every asset has a provenance line in assets/LICENSES.md.
- Name: Histeries. Crate prefix `histeries-`; binary `histeries`.

### 3.3 Code provenance
- License: MIT OR Apache-2.0. Do not paste or closely translate code from copyleft projects (Freeciv, Battle for Wesnoth, Veloren, 0 A.D., Civ6ModTool) or from Unciv (MPL-2.0). Learning from public architecture write-ups is fine; say so in the ADR.
- Dependencies must satisfy deny.toml (MIT/Apache/BSD/ISC/Zlib/Unicode class). `cargo deny check` is a gate. THIRD_PARTY_LICENSES.md is generated with cargo-about.

### 3.4 Deferred and excluded
- Deferred: mod-manager UI and WASM plugin ABI (Phase 3); lockstep multiplayer (Phase 4); 3D client and HarmonyOS native shell (Phase 5+).
- Excluded permanently: importing other games' save files, data or scripts; in-game generative-AI features; any use of decompilers or disassemblers.
- Included from Phase 1: a CLI map/scenario editor (`editor.*` commands). From Phase 3: a GUI editor in the client.

## 4. Platforms
- Tier A (CI builds and tests): Linux x86_64, macOS arm64, Windows x86_64 (msvc).
- Tier B (CI `cargo check` until Phase 3): wasm32 for the web client.
- HarmonyOS NEXT for PC (OpenHarmony-based): `hex`, `rules-core`, `ruleset`, `sim`, `ai`, `net`, `mcp` and `cli` must pass `cargo check` for `aarch64-unknown-linux-ohos` and `x86_64-unknown-linux-ohos` in CI from Phase 1 (Rust Tier 2 targets; no OpenHarmony SDK is needed for check). A native ArkTS/ArkUI + XComponent shell hosting the wgpu client is a Phase 5+ goal, contingent on upstream wgpu Vulkan-on-OHOS support (wgpu 31+) and the winit-ohos backend. AppGallery publication in China is out of scope.

## 5. Stack
- Rust, stable, pinned in rust-toolchain.toml with the OHOS and wasm32 targets listed. `#![forbid(unsafe_code)]` in every crate except `client`.
- Workspace crates and the only allowed dependency direction (lower never depends on higher):
  `hex → rules-core → ruleset → sim → ai → net → mcp → cli → client`
  - `hex`: the TileTopology trait, FlatHex (Geodesic later), distance, rings, lines, FOV. No game knowledge.
  - `rules-core`: entity types, the modifier language, yield math. Pure functions.
  - `ruleset`: loads TOML rulesets into validated tables; ships the base ruleset.
  - `sim`: world state, turn loop, command registry, observation API, event stream, serialization, replay, session server.
  - `ai`: players that see the world only through the observation API.
  - `net`: lockstep transport (Phase 4).
  - `mcp`: the MCP server (rmcp; stdio and streamable-HTTP transports) exposing the command registry and observation API with seat-level permissions.
  - `cli`: `histeries new|step|run|replay|tournament|map|mcp|serve`, ASCII map dump, batch self-play.
  - `client`: thin observer/presenter on wgpu + egui + winit, 2D first. Nothing below it may depend on a windowing or GPU crate.
  `cargo xtask layers` parses the workspace and fails on any violation. It exists before any feature code.
- Entity storage: generational arenas (`slotmap`) with stable IDs; all iteration in sorted-ID order. No ECS in `sim`; an ADR may revisit this only with a demonstrated iteration-order determinism guarantee.
- Scripting: none in Phases 1–2. Phase 3 adds a sandboxed WebAssembly plugin ABI on `wasmi` (fuel-metered, integer/fixed-point host ABI, no WASI). No Python, Lua or JavaScript in the game. Python (CPython via uv) is allowed only under `tools/` for analytics. No JavaScript toolchain anywhere; if one ever becomes unavoidable, use Bun and record an ADR.
- Assets: Blender 5.2 LTS (pinned in docs/assets.md), glTF 2.0; 2D sprites may be pre-rendered from it.

## 6. Architecture rules

### 6.1 Everything is a command
Every state change is a typed `Command` with a stable string ID (`unit.move`, `city.set_production`, `editor.set_terrain`, `turn.end`), a serializable schema and a validator. CLI, MCP, AI, replays, tests and the client all dispatch through the one registry. Replays are command logs plus periodic snapshots. A new command requires a schema, a validator, a test and an entry in the generated docs/commands.md.

### 6.2 Observation API
Reads are typed too: an `Observation` for a seat contains only what that seat may know, with fog of war applied. AI, MCP and the client read only through it. Nothing outside `sim` touches `GameState` directly.

### 6.3 MCP server
`histeries mcp` serves the registry and observation API over MCP: tools `list_commands`, `dispatch`, `observe`, `advance_turn`, `replay`; resources for the ruleset and docs. A seat token scopes each session to one player or to spectator. The server has no file or network access beyond the game session. Acceptance test (Phase 2): an LLM agent plays a complete 2-player game through MCP with no other interface.

### 6.4 Determinism
- Game state uses integers and fixed-point only (format fixed by ADR-0003). No floats in `hex`, `rules-core`, `ruleset`, `sim` or `ai`.
- Randomness comes from seeded, named streams (`mapgen`, `combat`, `barbarians`, one per player) using a generator whose crate version is pinned; the algorithm is part of the save format.
- No hash-map iteration in simulation code; use ordered collections or sorted IDs.
- The per-turn state hash is identical across runs, operating systems, architectures, thread counts and save/load. CI compares hashes at 1 vs N threads every turn.

### 6.5 Parallel by construction
AI planning for all players runs in parallel over an immutable snapshot; results are commands applied sequentially in canonical order. Pathfinding, visibility and yield recomputation are pure functions over snapshots and may use rayon. Thread count changes speed only.

### 6.6 Memory discipline
Arenas; no reference cycles in `sim`; every cache bounded with explicit eviction. State may legitimately grow with the empire, so the gates are: bytes per live entity and a bounded slope after turn 100; and a save → reload-in-fresh-process → compare-heap test, where anything the long-running process retains beyond the reloaded one is a leak.

### 6.7 Decoupled client
The session server owns the canonical state and serves any number of clients — desktop UI, CLI, AI, MCP, spectator, replay — through the same commands and observations. The client never mutates state and contains no rules logic.

### 6.8 Topology, not grid
`hex` exposes `TileTopology`: tile IDs, `neighbors` (degree 5 or 6), `distance`, `ring`, `area`, `line_of_sight`, and an embedding for rendering. `FlatHex` (with wrap options) ships first; `Geodesic` (a Goldberg-polyhedron hex-pentagon sphere) later. No crate outside `hex` may use axial or cube coordinates; a test enforces it. Pathfinding is A* with a topology-supplied heuristic; FOV is graph-based; AI influence maps are keyed by tile ID. Additional boards (an orbital or solar-system theatre, say) are additional topologies, not special cases.

### 6.9 Ages and eras are data
Eras, ages and their transitions (what persists, what resets, what unlocks) are ruleset data with hooks, so a late-game age or an off-map theatre is a data addition. Announced-but-unreleased features of other games are design references only (`public-doc`), never targets.

### 6.10 Data-driven rules
Content is TOML, not Rust. A modifier is `effect` + `conditions` + `scope` (e.g. `+1 food on tiles with feature=forest in cities with building=sawmill`). Adding a unit, building, policy or faction requires zero code changes. Difficulty levels are visible advantage modifiers in data; the AI never reads hidden state.

## 7. AI
Three layers, separate modules, each with tests and benchmarks: strategic (utility scoring over victory paths with hysteresis), operational (HTN or GOAP planner producing multi-turn operations), tactical (MCTS or beam search over joint unit orders inside a combat zone, using the sim's forward model). No hand-written behavior trees for combat. The AI sees only its seat's Observation plus declared difficulty modifiers. Baselines: `random`, `greedy`, and every prior AI version. `histeries tournament` runs N games over seeds and reports Elo, win rates by victory type, turn time, crash count and a binomial significance test. AI changes merge only with tournament evidence attached to the PR.

## 8. Testing and CI gates
| Tier | Trigger | Players / map / seeds | Turns | Gates | Budget |
|---|---|---|---|---|---|
| Fast gate | every PR | 2 and 4 players, Small, 3 seeds | 200 | layers, fmt, clippy, deny, nextest, proptest invariants, determinism 1 vs N threads, no panics, AI turn-time p95, OHOS and wasm32 `cargo check` | < 15 min on a 4-vCPU runner |
| Nightly | cron | 8 players, Large, 5 seeds | 500 | memory gate (dhat), replay round-trip, save/load hash equality, perf budgets | < 2 h per job, sharded by seed |
| Weekly stress | cron + manual | 16 players, Huge, 2 seeds | 1,000+ | memory slope, worst-case AI time, save size | checkpointed shards < 6 h each |
- Property tests for invariants: no negative yields, no unit on two tiles, ZOC symmetry, serialization round-trip preserves the hash.
- Oracle fixtures in `fixtures/oracle/` are supplied by the maintainer, tagged `observed`, and are golden tests with tolerances. Never fabricate fixtures.
- Fuzz the command decoder and the ruleset loader.
- Budgets live in docs/budgets.md; regressions fail CI.
- docs/scorecard.md is measured by CI, never estimated: spec items implemented with tests, determinism, memory, performance, AI Elo vs baselines. Every PR that moves a number updates it.

## 9. Roadmap
- Phase 0 — Bootstrap: scaffold, docs, ADR-0001…0005, empty crates with `cargo xtask layers` green.
- Phase 1 — Headless base game: ruleset-defined map generation (continents, terrain, features, rivers, resources); units (terrain costs, roads, ZOC, one military + one civilian per tile, melee and ranged combat, healing, small promotion tree, barbarians); cities (founding, growth with housing-style caps, production queue, 3–4 district types with adjacency, ~12 buildings, builders with charges); small tech and civic trees (~25 nodes each), policy slots; 2–8 players; domination and science victories; `histeries new|step|run|replay|map` with an ASCII dump; versioned save/load; CLI map/scenario editor; minimal `histeries mcp` (list, dispatch, observe).
  Definition of done: 1,000 self-play games over random seeds with zero panics; identical hashes at 1 vs 8 threads and across save/load; nightly memory gate met; the three-layer AI beats `greedy` in ≥ 75% of games, and every military unit in contact with an enemy receives an explicit order each turn; OHOS and wasm32 checks green; fast gate under 15 minutes; scorecard published.
- Phase 2 — AI depth and MCP: full three-layer AI; tournament harness with Elo and significance; complete MCP server; LLM-plays-a-full-game acceptance test; fixture-driven parity scorecard.
- Phase 3 — 2D client (wgpu/egui/winit), GUI map editor, mod-manager UI, WASM plugin ABI, web build.
- Phase 4 — Lockstep multiplayer: `net` as a second transport over the same commands and hashes; desync reports; hotseat and LAN.
- Phase 5+ — 3D client, HarmonyOS NEXT PC shell, Geodesic topology, late-game ages and off-map theatres.

## 10. Version control: jj only
- The repository uses jj with its Git backend, non-colocated (no `.git` directory in the working copy). GitHub is the remote.
- No agent and no human runs `git`. The git binary exists only as jj's fetch/push transport and as GitHub Actions' checkout. Do not use the harness's git-based repository-attach or worktree features; clone with `jj git clone`.
- Workflow: `jj git fetch` → `jj new main@origin` → work → `jj describe -m "<type>(<scope>): <summary>"` (Conventional Commits, with a `Signed-off-by` trailer for DCO; docs/tooling.md shows the jj trailer config) → `jj bookmark set task/<slug> -r @` → `jj git fetch` + `jj rebase -d main@origin` → `jj git push --bookmark task/<slug>` (`--allow-new` on the first push) → pull request. Never force-push `main`. Resolve conflicts with jj's conflict markers; never leave a conflicted commit on a pushed bookmark.
- Pinned jj version and install command: docs/tooling.md.

## 11. Agent roles and review policy
- CI is reviewer zero: no model reviews anything until the fast gate is green.
- Generation: Claude Fable 5.1 for architecture-critical work (determinism, fixed-point, command registry, session protocol, MCTS/HTN, topology trait). Claude Opus 5.5 for core implementation. Claude Sonnet 5.5 for routine work (data, CLI, tests, docs). The maintainer picks the session model; each task in ROADMAP.md is labelled with its class.
- Before pushing a task or calling it done, obtain an independent approval of the submitted diff after the fast gate passes. Start every review, including every review after fixes, by checking Claude Fable 5.2 for eligibility. A family that authored any part of the diff is ineligible to review it. Record the authoring families in the session report. Run an eligible Fable 5.2 at maximum effort with `.claude/agents/reviewer-fable.md`.
- Use Claude Opus 5.5 at maximum effort with `.claude/agents/reviewer-opus.md` only if Fable is ineligible, the Fable request fails or times out, or Fable returns no valid verdict. Apply the same authorship check to Opus. When both families are ineligible or neither eligible reviewer delivers a valid verdict, keep the change unapproved. Retry a transient failure starting with Fable. If both families authored the diff, report the lack of an independent reviewer to the maintainer.
- Give each reviewer a fresh context containing only the applicable spec section, the touched ADRs and the submitted diff. Never include the session transcript or another attempt’s output. Accept a verdict only when the harness identifies the requested model on the response message containing that verdict and its first line gives `APPROVE` or `REQUEST CHANGES`.
- A valid `REQUEST CHANGES` blocks the push. Fix the findings and begin a new Fable-first review. A fallback applies only to its current request. Only a valid independent `APPROVE` clears the review gate.
- No model reviews its own work.
- ROADMAP checkbox and session report updated for this task.

## 12. Tool policy
- Computer use: off. Never browse, open or read an installed game's files. Never attach or clone repositories containing decompiled, ripped or mod material.
- Network: crates.io, GitHub and documentation sites only.
- Fixtures are produced by the maintainer from their own play; agents only consume them.

## 13. Working rules
- Ask before decisions that are expensive to reverse (fixed-point format, serialization format, entity storage, topology trait shape, ruleset language). Otherwise decide, record an ADR (`docs/adr/NNNN-slug.md`, Nygard format) and keep going.
- Never report a feature as done without tests and a scorecard update. PR-sized changes. Docs change with code.
- When the spec is silent, propose a `designed` rule and flag it in the PR.
- Session report (`docs/sessions/YYYY-MM-DD-<slug>.md`, also pasted as the final message): what changed; decisions and ADRs; gates run and results; scorecard deltas; open questions; the bookmark pushed.

## 14. Repository layout
Cargo.toml, Cargo.lock, rust-toolchain.toml, rustfmt.toml, clippy.toml, deny.toml, about.toml, .cargo/config.toml, .config/nextest.toml, .editorconfig, .gitignore, .gitattributes, .vscode/extensions.json
README.md, LICENSE-MIT, LICENSE-APACHE, THIRD_PARTY_LICENSES.md, CONTRIBUTING.md (clean-room + DCO), CODE_OF_CONDUCT.md, SECURITY.md, CHANGELOG.md, ATTRIBUTION.md
AGENTS.md (this file), CLAUDE.md (`@AGENTS.md` only), .claude/settings.json, .claude/agents/reviewer-fable.md, .claude/agents/reviewer-opus.md, .claude/skills/
.github/workflows/{pr,nightly,weekly,release}.yml, CODEOWNERS, PR and issue templates
xtask/ (layers, ci-fast, soak, memgate, ohos-check); crates/{hex,rules-core,ruleset,sim,ai,net,mcp,cli,client}; data/rulesets/base/*.toml (+ data/LICENSE, CC-BY-4.0); assets/ (+ assets/LICENSES.md); fixtures/oracle/; specs/; docs/{ARCHITECTURE,CLEANROOM,ROADMAP,scorecard,budgets,assets,tooling,commands}.md; docs/adr/; docs/sessions/; tools/ (Python analytics via uv)

## 15. Context hygiene
Compaction is lossy and cannot be reliably disabled, so the project is designed to lose nothing when it happens.
- **State lives in the repo, not in the conversation.** Keep docs/ROADMAP.md checkboxes current as you work; describe every commit as you make it; record decisions in ADRs the moment they are made; update docs/sessions/<date>-<slug>.md incrementally, not only at the end. Anything not in a file after a step is treated as not done.
- **Delegate noise.** Any exploration, search across many files, long test or benchmark output, log reading, or dependency audit goes to a fresh-context subagent given one bounded question, an explicit file set, and an output cap ("return at most 30 lines: findings, file:line references, open questions"). The main session receives conclusions, never dumps. Read files by range (`Read` with offset/limit); never `cat` a large file into the main session.
- **Subagent bounds.** Every project subagent sets `maxTurns` in its frontmatter (reviewers 40, explorers 25). Never use fork-type subagents for exploration or review; forks copy the parent's context and defeat the purpose. If a subagent reports it could not finish within its bound, split the question and spawn a new one — do not continue the old one.
- **Session size.** One PR-sized task per session. Treat roughly 40 tool calls or 20 distinct files read in the main session as the ceiling: finish the current step, run the fast gate, push the bookmark, write the session report, and tell the maintainer to start a new session. A fresh session reading the repo beats a compacted one every time.
- **Never run `/compact` yourself.** If auto-compaction fires anyway, the hooks snapshot and re-inject state; after any compaction, re-read AGENTS.md and the current task before taking another action, and re-verify any fact you were relying on rather than trusting the summary.
- **Why even with 1M-token windows:** quality degrades as context grows long before the limit is reached (Anthropic's context-engineering guidance and the "context rot" studies), and long contexts cost more per call. Lean context is a quality rule, not a quota.