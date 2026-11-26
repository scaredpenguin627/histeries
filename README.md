# Histeries

Histeries is an open-source, clean-room, turn-based 4X strategy game built in Rust.

The headless game supports seeded continents, military and civilian units, cities, research, policies, AI opponents and agent-controlled seats. Read `AGENTS.md` for the project contract and `docs/ROADMAP.md` for the current task list.

## Desktop game

Start a spherical world with `cargo run --locked -p histeries-client -- --globe 3 --seed 7`, choose **Globe** in the new-game dialog, or open the browser client with `?globe=3&seed=7`. Frequency 3 gives 92 cells. Right-drag to rotate the globe, click the near surface to inspect or target an order, and use **V** for the planar atlas. The editor can create, save and playtest spherical scenarios. [Globe match rules](specs/globe-matches.md)

Run `cargo run --locked -p histeries-client` to start a two-seat game. Click a hex to inspect it, select an order and click its target. Drag the map to pan and scroll or pinch to zoom. The computer plans its turn when you end yours. The Game menu starts a new game or opens and saves a standard simulation save.

For the 3D map, launch `cargo run --locked -p histeries-client -- --3d --seed 7` or switch views using the map toolbar. Press **V** to switch during a desktop or browser match. Drag with the right mouse button to orbit, drag with the left to pan, and scroll to zoom. Use the camera menu to reframe the map. Click terrain, a city or a unit to select its tile and use the same inspector to give orders. [3D match view](specs/three-dimensional-client.md)

```text
cargo run --locked -p histeries-client -- --seed 7
cargo run --locked -p histeries-client -- --load campaign.hst
cargo run --locked -p histeries-client -- --seed 7 --capture map.png
```

The capture command saves a PNG from the rendered window and exits. The desktop client requires a working wgpu graphics backend. Linux X11 sessions also require the xkbcommon X11 runtime (`libxkbcommon-x11-0` on Ubuntu).

## Local checks

```text
cargo xtask layers
cargo xtask ci-fast
```

The repository uses jj for version control. The GitHub Actions fast gate runs the pinned Rust toolchain, formatting, clippy, license checks, tests, and platform checks.

## Multiplayer

Two to eight people can share a screen or join a trusted LAN host. Start a hotseat game from the Game menu, or launch `histeries serve` and connect each seat through the desktop client or interactive terminal. [Host, join and resume a match](docs/multiplayer.md)

## CLI

The headless CLI uses the simulation command registry and versioned binary files.

Use `histeries new --globe 3 --seed 7 --output globe.hst` for a playable two-seat globe or add `--setup empty` to start with the generated terrain alone. `histeries map globe.hst` prints its terrain atlas. `histeries serve --globe 3 --seed 7` and `histeries mcp --globe 3 --seed 7` accept the same subdivision frequency. A saved world supplies its own topology when opened with `--state`.

Completing the Industrial and Orbital age requirements opens **Orbital Survey**. Open the board selector or press **O** to switch between the surface and the chart. Launch from an owned city, end the round and select an adjacent site to survey. Return to the entry site to recall the expedition. Visited sites remain on your chart. To try this part of a campaign directly, generate a save through normal research orders and open it in the desktop client.

```sh
cargo run --locked -p histeries-sim --example orbital_opening -- orbit.hst
cargo run --locked -p histeries-client -- --load orbit.hst
```

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

### Scenario editor

The first time you open **Game → Scenario editor**, the editor copies the current game. Reopening the editor returns to your last draft, even if you have advanced the game. The **Scenario** menu can open an existing `.hst` file or create a blank board. Choose **Scenario → Copy current game into editor** to replace the draft with a fresh copy. Creating, opening or copying a scenario replaces the current draft and clears its undo history when confirmed. The file dialog remembers separate game and scenario paths, initially `histeries.hst` and `scenario.hst`. Choose a brush, select the owning seat for units or cities, then click a tile. A road uses a builder belonging to the selected seat on that tile. **Undo** and **Redo** step through recent edits. Use Undo to remove a placed unit, city or road while its checkpoint is retained.

Choose **Scenario → Playtest scenario** to play the draft with normal fog of war. Both **Playtest** menu choices end the playtest and discard its moves. Choose **End playtest and edit scenario** to return to the draft, or **End playtest and return to game** to resume the original game. Starting a new game or opening a different game closes its editor draft. **Save scenario** writes a standard `.hst` file for the desktop client, CLI or MCP server.

## Browser game

Build the static client with `cargo xtask web-build` and serve `target/web/` over localhost or HTTPS. The browser includes the same game, scenario editor and mod manager as the desktop client. Games and editor drafts each have a separate save slot on the current site. [Build and serve the browser client](docs/web.md)
