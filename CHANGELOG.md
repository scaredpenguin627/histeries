# Changelog

All notable changes are recorded here.

## Unreleased

- Added ruleset-driven continents, same-owner military/civilian stacking, FIFO city production and adjacency income, playable city capture and neutral raiders, scenario command batches, and an authenticated MCP server with stdio and HTTP transports.

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
