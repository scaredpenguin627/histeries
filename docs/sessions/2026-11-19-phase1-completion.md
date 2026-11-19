# Phase 1 closure audit

The reference is AGENTS.md §9. The earlier roadmap boxes tracked PR-sized tasks. The full scope also required continents configured by a ruleset, separate military and civilian occupancy, and a functioning MCP server. This audit also found that the domination predicate needed an ownership-changing command, city production had a single slot, adjacency was only a query, and barbarians could only retaliate.

| Scope | Implementation and evidence |
| --- | --- |
| Continents, terrain, features, rivers, resources | Validated TOML generation profiles, bounded graph growth, drainage, deterministic seeds and wrapping tests. CLI accepts an alternate map profile. ADR-0012. |
| Movement, roads, ZOC, stacking | Cost-checked topology moves, separate same-owner military/civilian slots, hostile blocking, observable slot ownership, AI reservations, old save/replay compatibility. ADR-0015. |
| Combat, healing, promotions, barbarians | Melee/ranged forecasts and retaliation, safe healing, three XP-funded promotion choices, and neutral raiders acting at the end of a round. |
| Cities, capped growth, construction and builders | Founding, food and housing, FIFO production, implicit city center and three placeable districts, adjacency production, twelve buildings, builders consuming charges on roads. ADR-0007 and ADR-0016. |
| Technologies, civics, policies and victories | The base ruleset has 25 technologies and 25 civics. Policies consume unlocked slots. Completing technologies can win science; an adjacent melee unit can capture a cleared city and win domination. ADR-0008 and ADR-0013. |
| CLI, editor, saves and replays | `new`, `step`, `run`, `replay`, `map` and the ASCII view. `scenario-editor STATE apply --commands PATH` can author and replay a validated batch. Versioned binary saves and command logs have legacy round-trip tests. |
| Agent interface | Native executable serves the MCP tools and resources over stdio and Streamable HTTP, enforcing seat, editor and spectator credentials. Scenario loading is a trusted host operation. Two-seat protocol gameplay tests exercise real command dispatch and fog-filtered observation. ADR-0014. |
| 2–8 players, AI, determinism, memory, performance and platforms | The 1,000 seeded games covered all seven seat counts and both victory conditions. Ordinary games allow conquest. Fixed-length measurements keep capitals and the final technology out of play. See the [scorecard](../scorecard.md) for measured gates. |

## Games and gates

- The unmodified `xtask soak` completed 1,000 games in 91.265 seconds. Of those, 918 ended by science and 82 by domination, with zero errors or panics. Runs using one and eight workers agreed at all 40,451 turn boundaries. Replay and save decoding matched at every boundary, and a second simulation continued for 10,435 matching rounds from restored checkpoints. All 92,264 military units in contact received an explicit order. The [verification archive](2026-11-19-phase1-completion-evidence.zip) contains the measured results.
- The [paired tournament](phase1-completion-tournament.txt) recorded 384 games across three seed sets and two opponents. With both seat assignments on the 32 new seeds, the three-layer AI won 58 of 64 against greedy and 64 of 64 against random.
- The five Large and two Huge heap measurements compared 159 live checkpoints with independent fresh processes. Excess live heap and post-drop retained bytes were zero throughout.
- The official nightly run checked 2,500 rounds. Both uninterrupted Huge games matched their independently resumed round-900 checkpoints through round 1,000. Two weekly runs overlapped with other workloads and exceeded the 2-second planning limit. The isolated official rerun passed both seeds with a peak of 184,874 µs; the evidence archive retains both earlier failures.
- Four real MCP wire integration tests exercise stdio, Streamable HTTP, credentials, saved scenarios and game completion. The [agent game transcript](2026-11-19-mcp-llm-game.jsonl.gz) contains 87 exchanges through a two-seat science victory in round 60. After the review fixes, every gameplay response matched. One ruleset resource response had different text-file line endings. A separate official SDK client tested both transports.
- An independent 1,000-game run used the repository gate code with its executable lookup pointed at a frozen release binary. Before and after the review fixes, this run reproduced the same 40,451 boundary hashes. All 384 tournament game outcomes and hashes also reproduced.
- After the review fixes, strict `CI=true cargo xtask ci-fast` passed locally in 391.783 seconds against a 900-second limit. All 118 nextest tests passed. Layers, formatting, Clippy, dependency checks, both OHOS targets and wasm32 passed. The six fast fixtures matched at 1,200 turn boundaries. The initial pre-review gate took 839.807 seconds and ran 114 tests.

## Implementation decisions

ADRs [0012](../adr/0012-continental-map-generation.md), [0013](../adr/0013-conquest-and-scenarios.md), [0014](../adr/0014-mcp-session.md), [0015](../adr/0015-unit-stacking.md) and [0016](../adr/0016-city-production.md) record the new rules and interfaces. The server uses an immutable commit from the official Rust MCP SDK while its optional-Chrono change awaits release. The dependency audit runs with the repository's advisory policy.

## Delivery

The first independent reviews requested three changes. CLI writes now stage both outputs and can retry a failed scenario batch without advancing the save twice. Seat and spectator projections for live and historical matches are owned by the simulation. Established domination and science outcomes remain valid if later orders arrive. Targeted regressions exercise these cases.

Two independent Codex reviewers using GPT-6.2 Astra and the repository's Fable review instructions returned **APPROVE** after the fixes and the final fast gate. They reviewed the game and the MCP/CLI boundary separately in fresh contexts. The named Claude models were unavailable in this environment.

The delivery bookmark is `task/phase1-completion`, based on `main` at `46a54dbf`.
