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
- [x] P0-07 (maintainer) Review the bootstrap PR and accept ADR-0001 through ADR-0005; environment publication is external

## Phase 1 — Headless base game

The first implementation phase is split into PR-sized work. Class A tasks are architecture-critical, class B tasks are core implementation, and class C tasks are routine or data work.

- [x] P1-01 (A) Implement `TileTopology`, `FlatHex`, graph distance, rings, areas, lines, and topology invariants
- [x] P1-02 (A) Add fixed-point arithmetic, named RNG streams, and deterministic state hashing
- [x] P1-03 (B) Add rules-core entities, modifiers, yields, and invariant tests
- [x] P1-04 (B) Load validated TOML rulesets and ship the base data tables
- [x] P1-05 (A) Build simulation state, command registry, observations, events, custom serialization, and replay
- [x] P1-06 (B) Add deterministic map generation, terrain, features, rivers, resources, and the editor command family
- [x] P1-07 (B) Add units, movement costs, roads, zones of control, combat, healing, promotions, and barbarians
- [x] P1-08 (B) Add cities, growth caps, production, districts, buildings, and builders
- [x] P1-09 (C) Add technology and civic data, policy slots, domination and science victory checks
- [x] P1-10 (C) Add CLI `new`, `step`, `run`, `replay`, `map`, and scenario-editor commands
- [x] P1-11 (A) Add strategic, operational, and tactical AI over the observation API
- [x] P1-12 (A) Add determinism, replay, memory, and performance gates for 2–8 player self-play

- [x] P1-13 (A) Close the §9 scope audit for continents, stacking, city production and capture, scenario editing and the playable MCP server

Phase 1 is complete after 1,000 random-seed self-play games have zero panics, hashes match across thread counts and save/load, the memory gate passes, the three-layer AI beats `greedy` in at least 75% of games, every military unit in contact receives an explicit order, and OHOS, wasm32, and fast-gate budgets are green.

## Phase 2 — AI depth and MCP

- [x] Deepen all three AI layers and compare paired tournament results against random, greedy and the frozen Phase 1 controller using Elo and an exact significance test
- [x] Verify the complete MCP server, both transports, seat-scoped live and historical observations, and shared-snapshot planning for stdio opponents
- [x] Replay two complete external-agent games through a fresh MCP subprocess in the acceptance suite
- [x] Publish independently derived fixture comparisons, record external observation coverage and enforce the checks in CI

## Phase 3 — 2D client

- [x] P3-01 Ship a playable native 2D client using wgpu, egui and winit
- [x] P3-02 Add the GUI map and scenario editor
- [x] P3-03 Add the mod manager and a versioned, fuel-metered wasmi plugin ABI
- [x] P3-04 Build, serve and verify the web client

## Phase 4 — Lockstep multiplayer

- [x] P4-01 Replicate authorized commands and verify sequence, round and state hashes, including desync reports
- [x] P4-02 Play hotseat matches with seat-scoped handoffs and save/resume
- [x] P4-03 Host trusted LAN sessions and join from native graphical or terminal clients

## Phase 5+

- [x] P5-01 Add a playable, depth-tested 3D match view to the desktop and WebGPU clients
- [ ] Add the HarmonyOS NEXT PC shell with supported window and rendering backends
- [x] P5-02 Implement the deterministic Geodesic graph, surface cells and shortest routes
- [x] P5-03 Play geodesic matches with globe picking, save/load, replay and multiplayer
- [x] P5-04 Add data-defined late-game ages, per-seat transitions and unlocks
- [ ] P5-05 Unlock and play an off-map theatre within the same match










## Future editor work

- [ ] EDIT-01 Add registered authoring commands to remove units, cities and roads after their undo checkpoints expire.
