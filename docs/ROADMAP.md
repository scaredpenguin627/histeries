# Histeries roadmap

The current task is the first unchecked box. Each task carries a class from `AGENTS.md` section 11. Tick a box in the same change that completes it.

## Phase 0 — Bootstrap

Done when the scaffold, docs, five accepted ADRs, and both xtask gates are present.

- [x] P0-01 (B) Cargo workspace with nine crates, pinned toolchain, configs, editor and VCS hygiene files
- [x] P0-02 (A) `cargo xtask layers` with dependency-direction checks and tests
- [x] P0-03 (B) `cargo xtask ci-fast` plus PR, nightly, weekly, and release workflows
- [x] P0-04 (C) Architecture, clean-room, scorecard, budgets, tooling, and assets docs
- [x] P0-05 (C) Repository metadata, licenses, contribution policy, templates, and attribution
- [x] P0-06 (A) ADR-0001 through ADR-0005, each Accepted by the maintainer
- [x] P0-07 (maintainer) Review the bootstrap PR, accept ADR-0001 through ADR-0005, and republish the Codex environment

## Phase 1 — Headless base game

The first implementation phase is split into PR-sized work. Class A tasks are architecture-critical, class B tasks are core implementation, and class C tasks are routine or data work.

- [x] P1-01 (A) Implement `TileTopology`, `FlatHex`, graph distance, rings, areas, lines, and topology invariants
- [ ] P1-02 (A) Add fixed-point arithmetic, named RNG streams, and deterministic state hashing
- [ ] P1-03 (B) Add rules-core entities, modifiers, yields, and property tests
- [ ] P1-04 (B) Load validated TOML rulesets and ship the base data tables
- [ ] P1-05 (A) Build simulation state, command registry, observations, events, serialization, and replay
- [ ] P1-06 (B) Add map generation, terrain, features, rivers, resources, and the editor command family
- [ ] P1-07 (B) Add units, movement costs, roads, zones of control, combat, healing, promotions, and barbarians
- [ ] P1-08 (B) Add cities, growth caps, production, districts, buildings, and builders
- [ ] P1-09 (C) Add technology and civic data, policy slots, domination and science victory checks
- [ ] P1-10 (C) Add CLI `new`, `step`, `run`, `replay`, `map`, and scenario-editor commands
- [ ] P1-11 (A) Add strategic, operational, and tactical AI over the observation API
- [ ] P1-12 (A) Add determinism, replay, memory, and performance gates for 2–8 player self-play

Phase 1 is complete after 1,000 random-seed self-play games have zero panics, hashes match across thread counts and save/load, the memory gate passes, the three-layer AI beats `greedy` in at least 75% of games, every military unit in contact receives an explicit order, and OHOS, wasm32, and fast-gate budgets are green.

## Phase 2 — AI depth and MCP

- [ ] Complete the three-layer AI and tournament harness with Elo and significance reporting
- [ ] Complete the MCP server and its seat-scoped session protocol
- [ ] Add an acceptance test where an LLM agent completes a two-player game through MCP alone

## Phase 3 — 2D client

- [ ] Add the wgpu, egui, and winit presenter, GUI editor, mod-manager UI, WASM plugin ABI, and web build

## Phase 4 — Lockstep multiplayer

- [ ] Add the second transport, desync reports, hotseat, and LAN sessions

## Phase 5+

- [ ] Add the 3D client, HarmonyOS shell, Geodesic topology, late-game ages, and off-map theatres


