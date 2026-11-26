# Histeries architecture

The binding architecture rules are in `AGENTS.md`.

## Crate graph

```text
hex -> rules-core -> ruleset -> sim -> ai -> net -> mcp -> cli -> client
```

Each arrow points from a lower layer to the next layer. `xtask layers` checks that no crate depends on a same-or-higher layer. The client is the only layer allowed to add windowing or GPU dependencies.

The 2D and 3D match views share the seat observation, tile inspector and command controller. The 3D view converts visible topology embeddings into a depth-tested triangle mesh. Its camera and picker operate on the same geometry. [3D renderer decision](adr/0023-three-dimensional-match-view.md)

| Crate | Responsibility |
| --- | --- |
| `hex` | Topology trait, tile IDs, graph distance, rings, areas, lines of sight, and rendering embeddings. |
| `rules-core` | Entity identifiers, modifier language, and pure yield functions. |
| `ruleset` | Validated TOML tables and the base ruleset data. |
| `sim` | World state, turn loop, command registry, observations, events, serialization, replay, and sessions. |
| `ai` | Observation-only strategic, operational, and tactical players. |
| `net` | Orders regular player commands for hotseat and native TCP sessions. Replicas check the sequence, round and canonical state hash before and after each command. |
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

Hotseat and LAN share one authoritative command sequence. A trusted LAN peer restores a full simulation save and replays accepted commands locally. Player-facing views still use the observing seat's projection. A peer can catch up from retained commits or resume at a verified checkpoint when the history expires. [Lockstep multiplayer](adr/0022-lockstep-multiplayer.md)

## Determinism

The simulation uses integers and the fixed-point representation selected in ADR-0003. Randomness is supplied by named, seeded streams whose algorithm and version are part of the save format. Simulation iteration is ordered by stable generational IDs. Hash maps are not used as an observable iteration source. State hashes are computed after every turn and compared across thread counts and save/load in CI.

## Threading

Each controller plans from its own seat's immutable observation. Native workers can plan distinct seats in parallel. The referee applies the resulting command streams in ascending seat order and retains the sequence of operations and unit orders within each stream. A tactical order can depend on a tile vacated earlier in the same plan. The tournament checks the session for victory after each dispatch.

## Memory

Simulation entities live in bounded generational arenas. Caches declare a capacity and eviction policy. The nightly gate measures bytes per live entity, slope after turn 100, and the retained heap after a save/reload in a fresh process.

## Platform and repository boundaries

The lower crates remain pure Rust so OHOS and wasm32 checks do not need a native SDK. The workspace layout, ruleset data, provenance fixtures, assets, specs, ADRs, and session reports are kept in the paths listed in `AGENTS.md` section 14.

## FlatHex Phase 1

The first topology implementation uses stable row-major `TileId` values and keeps its internal axial representation private to `hex`. `FlatHex` supports independent horizontal and vertical wrapping. All returned collections are sorted and deduplicated, line-of-sight returns the endpoint-inclusive topology line, and the default pathfinding heuristic is the integer graph distance.

## Deterministic arithmetic and hashing

`rules-core` uses checked signed fixed-point values with scale 1,000. Named streams use the pinned ChaCha8 algorithm and stable stream labels. `sim` hashes canonical length-prefixed fields with a versioned little-endian FNV-1a algorithm.

## Save and replay format

State and replay writers use version 5 for games without expedition activity and version 6 when expedition records are needed. Readers also accept version 4 and preserve its replay rules. Expedition orders use command version 4. Existing orders retain version 3. Integers are little-endian, maps are serialized in stable key order, lengths are bounded, and trailing bytes are rejected. [Off-map theatres](adr/0027-off-map-theatres.md)

## Map generation

The validated TOML map profile selects continent counts, land coverage, and compatible terrain, feature, drainage and resource weights. Generation uses the named map stream and topology neighbors; saves contain the resulting tile state. See ADR-0012.

## Unit simulation

Units are stored in stable ID order. A tile can hold one military and one civilian unit of the same owner. Commands check both slots, movement cost and enemy zones of control. Melee and ranged combat, recovery and promotions are deterministic. Visible projections retain both occupants; neutral raiders use the round transition in current replays. See ADR-0015.

## City simulation

Cities have integer food, housing and production. Each city has one active order and a bounded FIFO of waiting orders; queued construction reserves district sites. Adjacent city centers and districts contribute production through topology neighbors. Builders spend one of three charges on a road. Capture transfers ownership intact and rechecks victory. See ADR-0013 and ADR-0016.

## Progression and victories

Technology and civic tables are validated TOML DAGs. Each seat has one selected technology and civic, integer progress, completed node sets, policy slots, active policies, and a victory state. Turn advancement accumulates owned-city science and culture plus active policy bonuses in sorted player order. Science and domination checks run after progression updates, and observations expose only the observing seat's progression state.

## CLI boundary

The CLI uses `Session::dispatch` for every state change and `Observation` or state snapshots for reads. It persists the simulation's versioned state and replay formats, renders maps from stable tile order, and has no independent rules or serialization layer.
