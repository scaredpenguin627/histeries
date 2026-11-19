# Histeries

Histeries is an open-source, clean-room, turn-based 4X strategy game built in Rust.

The headless game supports seeded continents, military and civilian units, cities, research, policies, AI opponents and agent-controlled seats. Read `AGENTS.md` for the project contract and `docs/ROADMAP.md` for the current task list.

## Local checks

```text
cargo xtask layers
cargo xtask ci-fast
```

The repository uses jj for version control. The GitHub Actions fast gate runs the pinned Rust toolchain, formatting, clippy, license checks, tests, and platform checks.

## CLI

The headless CLI uses the simulation command registry and versioned binary files.

```text
histeries new --players 2 --setup frontier --width 8 --height 8 --seed 7 --output state.hst
histeries step state.hst --player 1
histeries run state.hst --turns 10
histeries map state.hst
histeries replay session.hrp
histeries editor state.hst set-terrain --player 1 --tile 4 --value plains
```

Use `--replay-output PATH` with `new`, `step`, `run`, or `editor` to write a replay log.

Run `histeries tournament --opponent greedy --seed 0 --seeds 32 --turns 500`
to play both seats on each of 32 generated maps. Other opponents are `random`
(seeded) and `v1` (the Phase 1 three-layer controller). The report gives each
game's winner, victory type, final hash and conflicts. Win rates use all games.
The relative Elo estimate credits draws with half a point and adds half a win
and half a loss. The one-sided exact sign test compares the candidate's two
games on each map to one win's worth of points and omits tied maps. Planning
p95 measures both seats together. An error or panic fails the tournament and
suppresses Elo and significance.

Start a playable agent session with `histeries mcp --players 2 --seed 7`. To host an edited save, use `histeries mcp --state state.hst`. The [MCP guide](docs/mcp.md) covers transports, credentials, resources, tools and replay. Use `new --setup empty` for a blank scenario or pass `--map-ruleset PATH` to select a generation profile.
