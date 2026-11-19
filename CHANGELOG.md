# Changelog

All notable changes are recorded here.

## Unreleased

- Added a static WebGPU browser client with separate local saves for games and scenario drafts.

- Added a graphical scenario editor with full-map inspection, validated brushes, undo and redo, and interoperable saves. A fog-filtered playtest keeps the original game and draft available when the author returns.

- Added the desktop scenario mod manager and a fuel-metered WebAssembly editor ABI for desktop, CLI and authorized MCP editors. The browser file picker imports a manifest and its plugin for use in the scenario editor.

- Added a native 2D game client with a fog-filtered hex map, seat orders, computer opponents and interoperable saves.

- Fixed multi-bot stdio turns so every pending bot plans from the same round state, including when resuming a saved game.

- Deepened strategic evaluation, city project planning and joint tactical orders. Added paired tournaments against the frozen Phase 1 AI, two replayable external-agent MCP games and a fixture-driven parity scorecard.

- Added ruleset-driven continents, same-owner military/civilian stacking, FIFO city production and adjacency income, playable city capture and neutral raiders, scenario command batches, and an authenticated MCP server with stdio and HTTP transports.

- Updated the independent review procedure to try Fable 5.2 first on every request and use Opus 5.5 only when that attempt cannot produce a valid independent verdict.

- Bootstrapped the Phase 0 workspace and documentation scaffold.
- Accepted ADR-0001 through ADR-0005 and implemented the Phase 1 `FlatHex` topology foundation.
- Added fixed-point arithmetic, named ChaCha8 streams, and deterministic state hashing for P1-02.




- Added typed rules entities, canonical yield channels, and deterministic modifiers for P1-03.
- Added strict, provenance-aware TOML ruleset loading with base data for P1-04.
- Added the P1-05 command/session boundary with strict custom binary saves and replay logs.
- Added deterministic map generation, terrain, features, rivers, resources, and editor commands for P1-06.
- Added unit movement, roads, zones of control, combat, healing, promotions, and barbarian commands for P1-07.
- Added city founding, housing-capped growth, production queues, districts, twelve buildings, and builder charges for P1-08.
- Added 25-node technology and civic trees, research progress, policy slots, and domination and science victory checks for P1-09.
- Added deterministic CLI `new`, `step`, `run`, `replay`, `map`, and scenario-editor commands for P1-10.
- Added strategic victory selection with hysteresis, multi-turn city and research planning, joint tactical beam search, and a paired tournament against greedy and seeded random players.
- Added 2–8 seat self-play verification of every round across worker counts, binary replay and save/load, with checkpointed controller continuation, native heap measurements and planning-time budgets. Fixed loading funded production orders waiting for an available tile.
