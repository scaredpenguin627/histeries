# Phase 4 multiplayer

The `histeries-net` crate orders commands on a single host and verifies them in independent simulation replicas. Hotseat, native LAN and terminal LAN players use the same command validation and turn ordering. A trusted peer can reconnect and reconstruct the current round, including the orders already spent. An out-of-order proposal, a mismatching hash or a rejected command cannot silently take effect twice. [ADR-0022](../adr/0022-lockstep-multiplayer.md) · [Host and player guide](../multiplayer.md)

The graphical hotseat client covers the board between players and opens a saved game at the correct active seat. The graphical LAN client keeps socket work off the UI thread and presents only the assigned seat’s observation. Its terminal counterpart accepts the same player commands, prints the verified position and can display the seat’s view as JSON.

An MCP host can attach a stdio player to the same LAN session. The authenticated seat can discover and dispatch commands, observe and advance its turn, and read its own retained historical views alongside human players.

## Local acceptance

On Windows GNU, one host and eight persistent terminal clients played the seed-7 frontier over real TCP loopback. The players issued 16 end-turn commands over two rounds. All eight clients independently synchronized, reported their own seat and agreed on sequence 16, round 2, and state hash `312f74ed01bfecd1`.

The multi-process CLI regression also exercised a research order, seat handoff, rejected credentials, a forged issuer, private observations and clean client exits. Network tests cover retained commits, checkpoint recovery, mismatched positions and recovery after a lost reply. Desktop tests cover delayed connections, explicit synchronization following a failed submission and the hotseat privacy screen.

The actual Windows desktop binary also captured two 1,500 × 1,000 wgpu frames. The four-player opening showed the seat-1 handoff cover. After a terminal player ended seat 1's turn on a two-seat TCP host, the graphical client for seat 2 showed its own city, fog and available orders. Both capture commands exited successfully.

Codex authored these changes. The maintainer selected a fresh Sol 6.2 subagent at maximum reasoning effort for each independent Phase 4 review. The first review found that saving after ending a hotseat turn required revealing the next player's board. The handoff screen now includes the save menu. A regression clicks through the handoff and saves the position before the next player reveals the map. The delivery bookmark is `task/phase4-lockstep`.

The second review found that the MCP adapter exposed the full-world digest and that a fresh synchronization connection could reach the host ahead of an interrupted order. The adapter now reports only seat-safe results and errors. The host binds submission permits to the running match and to one seat, and fences the old permit while resolving the interrupted order. Recovery can then report whether the order was accepted or canceled.

The third review covered reconnection after the receipt expires, two seats with the same permit generation, and replay requests after a host restart. On the same host, a peer with an expired receipt now verifies the current checkpoint and resumes with the old order's outcome marked `unknown`. Permit identity includes the seat. A replay request identifies the match that the client joined.

The fourth review identified three player-visible issues. MCP now keeps all replica positions private, including after a checkpoint. The subprocess regression compares seat 1's response before and after seat 2's private research orders. A client whose interrupted order lost its original host reports an unknown result and requires a fresh join. Terminal clients controlled through a pipe can synchronize after a lost reply without discarding their pending order.

The fifth review tested the same restart with newly issued seat credentials. When recovery of an interrupted submission can no longer authenticate the seat or use the host's protocol, the client reports an unknown outcome and requires a fresh join. Initial login continues to report the credential or protocol error directly.

## Hosted acceptance

Sol 6.2 (max) approved source `1c84ea35b1fcf709fcd1ef04c454bb0ad48becd9` after its local fast gate passed in 300.490 s with 210 tests, 34 fixture checks and 1,200 round hashes. PR #12 runs that source on Linux, macOS and Windows. The respective fast-gate receipts report 316.908 s, 347.724 s and 443.374 s, within the 900 s budget on all three. The nightly and weekly stress workflows also passed. [PR #12](https://github.com/scaredpenguin627/histeries/pull/12) · [Fast gate](https://github.com/scaredpenguin627/histeries/actions/runs/45818090566) · [Nightly](https://github.com/scaredpenguin627/histeries/actions/runs/45818117340) · [Weekly](https://github.com/scaredpenguin627/histeries/actions/runs/45818121754)

On hosted Linux, eight seats agreed on sequence 16, round 2 and hash `312f74ed01bfecd1` after two rounds over TCP loopback. The four-CPU runner captured a 1,200 × 800 shared-screen handoff and a separate 1,200 × 800 LAN player view. The Windows and Linux multiplayer acceptance runs used separate processes on one machine. [Seat positions and screenshots](https://github.com/scaredpenguin627/histeries/actions/runs/45818090566/artifacts/15117556541) · [Scorecard](../scorecard.md)
