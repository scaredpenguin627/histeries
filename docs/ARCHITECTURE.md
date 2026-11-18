# Histeries architecture

This document describes the Phase 0 boundary. The binding rules are in `AGENTS.md`.

## Crate graph

```text
hex -> rules-core -> ruleset -> sim -> ai -> net -> mcp -> cli -> client
```

Each arrow points from a lower layer to the next layer. `xtask layers` checks that no crate depends on a same-or-higher layer. The client is the only layer allowed to add windowing or GPU dependencies.

| Crate | Responsibility |
| --- | --- |
| `hex` | Topology trait, tile IDs, graph distance, rings, areas, lines of sight, and rendering embeddings. |
| `rules-core` | Entity identifiers, modifier language, and pure yield functions. |
| `ruleset` | Validated TOML tables and the base ruleset data. |
| `sim` | World state, turn loop, command registry, observations, events, serialization, replay, and sessions. |
| `ai` | Observation-only strategic, operational, and tactical players. |
| `net` | Lockstep transport over commands and state hashes. |
| `mcp` | The `rmcp` server exposing commands and observations with seat permissions. |
| `cli` | `histeries` subcommands, ASCII maps, self-play, and editor commands. |
| `client` | A thin observer and presenter built on wgpu, egui, and winit. |

## Data flow

State changes cross one typed command boundary. Reads cross one typed observation boundary with fog-of-war applied.

```text
CLI / MCP / AI / client -- Command --> sim session server
CLI / MCP / AI / client <-- Observation + Event -- sim session server
                                     |
                                     +-- per-turn hash --> replay and desync diagnostics
```

No caller outside `sim` receives a `GameState`. Replays store commands with periodic snapshots so the same registry drives live games, tests, and playback.

## Determinism

The simulation uses integers and the fixed-point representation selected in ADR-0003. Randomness is supplied by named, seeded streams whose algorithm and version are part of the save format. Simulation iteration is ordered by stable generational IDs. Hash maps are not used as an observable iteration source. State hashes are computed after every turn and compared across thread counts and save/load in CI.

## Threading

AI planning receives an immutable observation snapshot. Player plans may run in parallel, then commands are sorted by canonical seat and command order before sequential application. Pathfinding, visibility, and yield recomputation remain pure functions over snapshots and may use rayon once those crates are introduced. Changing the thread count changes scheduling and elapsed time only.

## Memory

Simulation entities live in bounded generational arenas. Caches declare a capacity and eviction policy. The nightly gate measures bytes per live entity, slope after turn 100, and the retained heap after a save/reload in a fresh process.

## Platform and repository boundaries

The lower crates remain pure Rust so OHOS and wasm32 checks do not need a native SDK. The workspace layout, ruleset data, provenance fixtures, assets, specs, ADRs, and session reports are kept in the paths listed in `AGENTS.md` section 14.

## FlatHex Phase 1

The first topology implementation uses stable row-major `TileId` values and keeps its internal axial representation private to `hex`. `FlatHex` supports independent horizontal and vertical wrapping. All returned collections are sorted and deduplicated, line-of-sight returns the endpoint-inclusive topology line, and the default pathfinding heuristic is the integer graph distance.

## Deterministic arithmetic and hashing

`rules-core` uses checked signed fixed-point values with scale 1,000. Named streams use the pinned ChaCha8 algorithm and stable stream labels. `sim` hashes canonical length-prefixed fields with a versioned little-endian FNV-1a algorithm.

## Save and replay format

ADR-0006 defines the custom binary boundary. State, command, and replay documents carry magic bytes and version 1. Integers are little-endian, maps are serialized in stable key order, lengths are bounded, and trailing bytes are rejected.

## Map generation

P1-06 uses `FlatHex` and a named `mapgen` stream to generate terrain, features, river edges, and resources in stable tile-ID order. Editor commands mutate map data only through `sim`, and map serialization rejects incompatible terrain edits and non-canonical tile order.
