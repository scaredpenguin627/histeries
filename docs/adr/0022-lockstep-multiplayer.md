# ADR-0022 — Host-ordered multiplayer with independent replay

- Status: Accepted
- Date: 2026-11-26
- Provenance: designed

## Context

The simulation already authorizes seat commands, projects fog of war and hashes the canonical state. Phase 4 needs local human handoffs and a second transport that can detect disagreement between machines. Players can join a saved game partway through a round or lose a network reply after the host accepts an order.

## Decision

Use one authoritative sequence of player commands. The lowest unfinished seat ID controls the match until it ends the turn or a player wins. Hotseat drives this host in process. Over TCP, the host authenticates each request, checks the supplied position and dispatches the regular player command under a match lock. At most one proposal can succeed against the same position. The submitting peer and the other peers replay accepted commands in that order.

Identify a position by its sequence number, simulation round and canonical state hash. A command can leave the hash unchanged, so the sequence also distinguishes successive acceptances of the same order. Each record identifies its seat and carries the simulation's encoded command and both positions. A replica stages the dispatch, compares the result and retains the last verified state on a mismatch. The report includes both sides' values for diagnosis.

Start a joining peer from the current round's opening save and its bounded command suffix. Replaying those commands reconstructs mid-round permissions and per-seat order counts as well as the world state. A peer recovering a lost acknowledgement asks for commits after its last verified position. Falling behind the 128-commit history triggers a verified checkpoint with a visible recovery result. When the host still has the requested boundary it reports a hash mismatch there directly.

The network transport serves trusted LANs. Independent verification requires complete simulation state on each peer. Seat credentials govern authority to issue orders, and the user interfaces display seat-filtered observations. The MCP adapter keeps canonical hashes and replica diagnostics private when reporting an action or error to its player. Multiplayer preserves the existing simulation save and replay formats. Hosting a position resumes it under standalone-save rules. [Multiplayer specification](../../specs/multiplayer.md) · [Save and replay format](0006-serialization-format.md) · [Historical replay compatibility](0016-city-production.md)

The terminal and native desktop clients use the same verified LAN replica. In the desktop client a worker owns the replica and sockets, and the window receives the authenticated seat's observation, the current actor and the public outcome. One request at a time keeps network latency off the rendering thread and prevents repeated clicks from queuing duplicate orders. A stale or interrupted submission leaves controls disabled until the player synchronizes. Because a new TCP connection can overtake a delayed order, the host fences the seat's previous write permit under the same lock that commits orders. Following successful recovery, a delayed submission carrying the old permit cannot change the verified position.

## Consequences

The same player validators and victory checks serve local and network play. One stalled active seat can delay the round. A reconnecting peer can verify every intervening command while the host retains it. After the history window expires, the peer verifies the current checkpoint and resumes from there.

Each LAN peer can inspect the full world, and a network observer can read the plaintext credentials and game data. The shared-screen handoff and seat-specific views protect what the normal interface displays. MCP remains the seat-scoped transport for clients that should receive only authorized observations. [Seat-scoped MCP sessions](0014-mcp-session.md)
