# Playing through MCP

Start a fresh two-player game with `histeries mcp`. Set `HISTERIES_MCP_SEAT_1_TOKEN` to a secret and `HISTERIES_MCP_TOKEN` to the same value in the launching environment. Your stdio client controls seat 1 while the greedy baseline plays the other seats. Set `--players 2..8` or `--seed N` to change the generated scenario. To host an existing save or edited scenario, pass `--state PATH` in place of the setup flags. Token variable names then use the numeric seat IDs in the save. A resumed game starts its replay history at the supplied snapshot, including any seats already finished for that turn.

To let separate clients share a game, set a different secret for each of `HISTERIES_MCP_SEAT_1_TOKEN` through `HISTERIES_MCP_SEAT_N_TOKEN` and start `histeries mcp --listen 127.0.0.1:8765`. Connect clients to `http://127.0.0.1:8765/mcp` with the appropriate `Authorization: Bearer <token>` header on every request. All HTTP seats are human or agent controlled. A host can enable a read-only all-seats observer with `HISTERIES_MCP_SPECTATOR_TOKEN` and scenario editing as the lowest-numbered seat with `HISTERIES_MCP_EDITOR_TOKEN`. Each configured credential must be unique. A stdio spectator can inspect the opening scenario; live spectating uses HTTP.

Call `observe` for your own cities, units, progress and visible board. Read `histeries://ruleset/base` for technologies and prerequisites. Call `list_commands` to discover allowed command IDs and their JSON argument schemas. Call `dispatch` with an `id` and an `args` object. The authenticated connection supplies the acting seat. To select a technology, use the `research.technology` command with its `technology` argument. End the seat's turn with `advance_turn`. After all seats end their turns the simulation advances. Stop when `winner` contains the winning seat and victory type.

Call `replay` with a turn number to read the observation at the beginning of that turn. On the winning turn, replay includes the final command even when the match ended before the round boundary. The server retains the last 32 completed rounds and their opening boundary. Replay respects the caller's seat. Each seat may issue up to 512 accepted orders per round in addition to ending its turn. Game and replay data stay in memory for the lifetime of the server process.

For example, send this argument object to `dispatch` to research the first base technology.

```json
{"id":"research.technology","args":{"technology":"tech_01"}}
```

For an external-agent acceptance run, start a fresh two-seat stdio game with seed 20261119. Give the connected agent the objective “Play seat one to a declared victory using only Histeries MCP tools and resources. Report the winner, victory type and turn, then check the result with replay.” Record the requests and responses from discovery through the terminal replay. Either seat can win. The [recorded 87-exchange run](sessions/2026-11-19-mcp-llm-game.jsonl.gz) ended on turn 60 with a science victory for the greedy opponent. The HTTP integration test separately exercises two independently authenticated clients through a science victory.
