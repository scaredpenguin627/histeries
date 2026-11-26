# Playing through MCP

Use `histeries mcp --globe 3 --seed 7` to create a spherical frontier with 92 cells. On a globe, `observe` reports the public subdivision frequency and the observing seat's visible tiles. Orders and replay identify these cells with the same stable tile IDs used by the desktop and LAN clients.

Start a fresh two-player game with `histeries mcp`. Set `HISTERIES_MCP_SEAT_1_TOKEN` to a secret and `HISTERIES_MCP_TOKEN` to the same value in the launching environment. Your stdio client controls seat 1 while the greedy baseline plays the other seats. Set `--players 2..8` or `--seed N` to change the generated scenario. To host an existing save or edited scenario, pass `--state PATH` in place of the setup flags. Token variable names then use the numeric seat IDs in the save. A resumed game starts its replay history at the supplied snapshot, including any seats already finished for that turn.

After the human seat ends its turn, all remaining stdio bots plan in parallel. Each sees its own fog-filtered observation taken before the first bot acts. The server then applies the plans in ascending seat order. If two bots try to move into the same military slot, the first legal move succeeds and the later bot finishes its turn with its original plan. Loading a save after the human seat has ended uses the same rule.

To let separate clients share a game, set a different secret for each of `HISTERIES_MCP_SEAT_1_TOKEN` through `HISTERIES_MCP_SEAT_N_TOKEN` and start `histeries mcp --listen 127.0.0.1:8765`. Connect clients to `http://127.0.0.1:8765/mcp` with the appropriate `Authorization: Bearer <token>` header on every request. All HTTP seats are human or agent controlled. A host can enable a read-only all-seats observer with `HISTERIES_MCP_SPECTATOR_TOKEN` and scenario editing as the lowest-numbered seat with `HISTERIES_MCP_EDITOR_TOKEN`. Each configured credential must be unique. A stdio spectator can inspect the opening scenario; live spectating uses HTTP.

To join a running trusted LAN game, configure your MCP host to launch `histeries mcp --connect IP:PORT --seat ID` over stdio. Set `HISTERIES_LAN_TOKEN` in the process environment to the credential the LAN host assigned to that seat. The same match accepts terminal players and native clients. Each remaining seat waits for its own player to act.

In a LAN match, `observe` synchronizes the replica and returns your seat's view and the current `active_seat`. The adapter verifies replica hashes and command positions internally. Dispatch orders when your seat is active. After ending your turn, poll `observe` to see when you can act again. Historical `replay` reads your seat's projection from the LAN host, including rounds it retained before you joined.

If a reply is lost or the host reports a stale position, call `observe` before submitting another order. An observation that recovers an interrupted order includes `recovered_submission`. Its value is `"accepted"` if the host applied the order, `"canceled"` if the host ruled it out, or `"unknown"` if the host discarded the receipt. In all three cases, the original request can no longer arrive and change the board. For `"unknown"`, inspect the restored position before deciding what to do next. Ordinary observations omit the field. If the host restarted while your order was unresolved, the adapter reports that the outcome is unknown and you must rejoin with a fresh adapter.

Call `observe` for your own cities, units, progress and visible board. Read `histeries://ruleset/base` for technologies and prerequisites. Call `list_commands` to discover allowed command IDs and their JSON argument schemas. Call `dispatch` with an `id` and an `args` object. The authenticated connection supplies the acting seat. To select a technology, use the `research.technology` command with its `technology` argument. End the seat's turn with `advance_turn`. After all seats end their turns the simulation advances. Stop when `winner` contains the winning seat and victory type.

Call `replay` with a turn number to read the observation at the beginning of that turn. On the winning turn, replay includes the final command even when the match ended before the round boundary. The server retains the last 32 completed rounds and their opening boundary. Replay respects the caller's seat. Each seat may issue up to 512 accepted orders per round in addition to ending its turn. Game and replay data stay in memory for the lifetime of the server process.

For example, send this argument object to `dispatch` to research the first base technology.

```json
{"id":"research.technology","args":{"technology":"tech_01"}}
```

Editor credentials can call `apply_editor_package` with a TOML manifest and a Base64-encoded Wasm module. The connection selects the seat. The tool returns a package ID and an accepted command count after atomic validation. The ABI reference is also available as `histeries://docs/editor-packages`. [Package format and example](editor-packages.md)

## External-agent acceptance

Launch `histeries mcp --players 2 --seed N` with the seat 1 credentials above. Connect an external LLM host over stdio and ask it to play seat 1 to a declared victory using the MCP tools and resources, report the winning seat, victory type and turn, and check its result with `replay`. Record each JSON-RPC request and response from protocol discovery through the terminal replay. Either player may win.

| Seed | Wire exchanges | Agent actions and result |
| --- | ---: | --- |
| 20261119 | 87 | The agent issued 19 accepted orders and advanced 60 turns. Its terminal replay showed seat 2 won by science on turn 60. [Transcript](sessions/2026-11-19-mcp-llm-game.jsonl.gz) |
| 20261120 | 132 | The agent discovered the tools and command schemas, scouted with a warrior, fought, captured cities and adopted a science policy. It issued 56 accepted orders and received six command rejections. Its final observation and replay showed seat 2 won by science on turn 57. [Transcript](sessions/2026-11-19-phase2-mcp-game.jsonl.gz) |

Run `cargo test -p histeries-cli --test mcp recorded_agent` to play both transcripts against fresh CLI subprocesses. Each test sends only the archived requests over stdio and compares the live responses with the observations, rules and command outcomes the agent received. Both check that further turn advancement fails after victory. The test runs offline once dependencies are installed. The HTTP test in the same suite also plays two independently authenticated seats through a science victory.
