# Playing a multiplayer match

Hotseat and LAN games support two to eight human players. Seats act in ascending ID order. The last seat to end its turn advances the simulation round. A victory can finish the game before the round advances.

## Share a screen

Open **Game → New hotseat game** or start the desktop client with:

```text
histeries-client --hotseat --players 4 --seed 7
```

Pass the screen when the handoff panel appears. The incoming player confirms to reveal their own map and orders. To resume a saved position with humans controlling all seats, use **Open as hotseat** or `histeries-client --hotseat --load campaign.hst`. The regular single-player open action assigns the other seats to computer opponents.

## Host on a LAN

Choose one machine as the host. Set a different token for each seat and share each player’s token privately with that player. In this two-seat PowerShell example, replace the listen address with the host machine’s LAN address. For a single-machine test use `127.0.0.1`.

```powershell
$env:HISTERIES_LAN_TOKEN_1 = [guid]::NewGuid().ToString("N")
$env:HISTERIES_LAN_TOKEN_2 = [guid]::NewGuid().ToString("N")
histeries serve --listen 192.168.1.10:7878 --players 2 --seed 7
```

The host prints its listening address on startup. It serves until stopped. `histeries lan host` runs the same server. To start from an existing game, replace `--players 2 --seed 7` with `--state campaign.hst` and set the token variables for the actual seat IDs in that save.

On each player’s machine, set `HISTERIES_LAN_TOKEN` to the assigned token. For seat 1, launch the graphical client with:

```text
histeries-client --connect 192.168.1.10:7878 --seat 1
```

The desktop client shows its seat’s map and refreshes it as other players act. After an interrupted submission, synchronize before choosing another order. In the terminal, a recovery reply includes `recovered_submission`. The value is `accepted` if the order took effect or `canceled` if the host canceled it. If its receipt has expired, the client verifies the current checkpoint and reports `unknown`. Inspect the current position before deciding whether another order is needed. A restarted host requires a fresh join.

## Play from a terminal

```text
histeries lan join --connect 192.168.1.10:7878 --seat 1
```

Type one command per line. `observe` catches up and prints the seat’s fog-filtered observation. `sync` reports the current turn, active seat, sequence and state hash. On your turn, enter a registered player order as JSON or type `end` to hand over. `quit` disconnects this client. For example:

```text
observe
{"command":"research.technology","technology":"tech_01"}
end
sync
quit
```

The CLI fills in the authenticated player ID when omitted. After reconnecting, use `observe` to inspect the current position. If a live peer falls behind the retained command history, it reports a checkpoint recovery. A mismatch at a retained position reports the expected and actual positions. [Command registry](commands.md) · [Lockstep protocol](../specs/multiplayer.md)

With piped input, each recoverable submission error produces a JSON line with `error` and `"recovery_required": true`. Send `sync` or `observe` to the same process and read the result before issuing another order. Closing the pipe or sending `quit` while recovery is pending exits with a nonzero status. Invalid input and unauthorized orders also fail the batch.

An MCP host can launch `histeries mcp --connect IP:PORT --seat ID` with the same seat token. The agent can observe, issue orders, advance its turn and request its own historical views in this match alongside human players. [MCP seat configuration](mcp.md)

Use LAN play with trusted peers. Every participant receives the simulation state and committed orders to verify the same canonical hashes. The interface applies fog of war, but a player who inspects their process can read hidden state. The TCP transport sends tokens and match data in plaintext.
