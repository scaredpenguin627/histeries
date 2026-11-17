# Histeries roadmap

The current task is the first unchecked box below. Each task carries a class (AGENTS.md section 11): **A** architecture-critical, **B** core implementation, **C** routine. Tick a box in the same change that completes the task. The Phase 0 bootstrap breaks Phase 1 into PR-sized tasks and adds them here.

## Phase 0 — Bootstrap
Done when the scaffold exists, the docs set and ADR-0001…0005 (Proposed) are written, and `cargo xtask layers` and `cargo xtask ci-fast` are green.
- [ ] P0-01 (B) Cargo workspace with nine empty crates, toolchain pins, rustfmt/clippy/deny/about configs, editor and VCS hygiene files
- [ ] P0-02 (A) `cargo xtask layers`: dependency direction, no windowing/GPU crate below `client`, no `-sys` crate below `client` without an allow-list entry; with tests
- [ ] P0-03 (B) `cargo xtask ci-fast` and the GitHub workflows (pr, nightly, weekly, release; nightly and weekly may start as stubs)
- [ ] P0-04 (C) Docs: ARCHITECTURE, CLEANROOM, scorecard, budgets, tooling, assets
- [ ] P0-05 (C) Repository files: README, CONTRIBUTING, CODE_OF_CONDUCT, SECURITY, CHANGELOG, ATTRIBUTION, licenses, templates
- [ ] P0-06 (A) ADR-0001…0005 as Proposed: topology trait shape and coordinate policy; RNG; fixed-point format; entity storage; ruleset and modifier language
- [ ] P0-07 (maintainer) Review the bootstrap PR with a different model, accept or amend the ADRs, republish the Codex environment

## Phase 1 — Headless base game
Scope: ruleset-defined map generation (continents, terrain, features, rivers, resources); units (terrain costs, roads, ZOC, one military and one civilian per tile, melee and ranged combat, healing, a small promotion tree, barbarians); cities (founding, growth with housing-style caps, production queue, 3–4 district types with adjacency, about 12 buildings, builders with charges); small tech and civic trees (about 25 nodes each) and policy slots; 2–8 players; domination and science victories; `histeries new|step|run|replay|map` with an ASCII dump; versioned save/load; a CLI map/scenario editor (`editor.*` commands); a minimal `histeries mcp` (list, dispatch, observe).

Definition of done:
- 1,000 self-play games over random seeds with zero panics;
- identical per-turn hashes at 1 vs 8 threads and across save/load;
- nightly memory gate met (baseline at turn 100; bytes per live entity and slope; reload comparison);
- the three-layer AI beats `greedy` in at least 75% of games, and every military unit in contact with an enemy receives an explicit order each turn;
- OHOS and wasm32 `cargo check` green; fast gate under 15 minutes; scorecard published.

Tasks: *to be written by the bootstrap session.*

## Phase 2 — AI depth and MCP
Full three-layer AI; tournament harness with Elo and significance; complete MCP server; an LLM agent plays a complete game through MCP alone (acceptance test); fixture-driven parity scorecard.

## Phase 3 — 2D client
wgpu/egui/winit client, GUI map editor, mod-manager UI, WASM plugin ABI (`wasmi`), web build.

## Phase 4 — Lockstep multiplayer
`net` as a second transport over the same commands and hashes; desync reports; hotseat and LAN.

## Phase 5+ — Later
3D client, HarmonyOS NEXT PC shell, Geodesic topology, late-game ages and off-map theatres.
