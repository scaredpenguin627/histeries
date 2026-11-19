# ADR-0014 Seat-scoped MCP game sessions

- Status: Proposed
- Provenance: designed
- Date: 2026-11-19

## Context

The same command registry and observation rules must serve multiple concurrent clients and allow an external agent to finish a game. A process connected to a single stdio host also needs an opponent. Clients must be able to play a scenario loaded by the host, including a save partway through a round.

## Decision

The server uses the official Rust SDK for the 2026-07-28 protocol over stdio and loopback Streamable HTTP. A host-provided token binds each connection or HTTP request to a player, a privileged editor acting as the lowest-numbered player, or a read-only spectator. The server reads credentials at startup. Every HTTP request authenticates before it reaches the role-specific MCP handler. A spectator can read every seat projection. Stdio assigns greedy controllers to the active player's opponents. On HTTP every seat is independently controlled.

Each action runs under the shared session lock. Tool arguments cannot specify the acting player. Ordinary seats issue commands against known targets through their observation. Trusted editors may also specify scenario IDs, spawn entities and change terrain. The server sends the same public winner to all seats and closes the game to further orders after a victory.

Replay reconstructs the round boundary through the simulation's replay API and applies the same observation projection. It retains 32 completed rounds plus the current replay segment. Limiting each seat to 512 accepted orders per round bounds retention even if a seat delays ending the turn. The compiled-in rules and guide resources use fixed URIs. Resources and tools do not accept filesystem paths or outbound network destinations.

## Consequences

Multiple HTTP hosts share one authoritative game in memory. Each stdio process starts an independent game. Credentials and history expire with the process. Current HTTP clients can negotiate the July 2026 protocol; the SDK also translates requests from earlier clients. The web target remains a client build and the native hosts serve stdio and HTTP.
The server pins the official Rust SDK repository at commit `9043cdfa8694d83bf506c2f3eb0313ec173edd7e` from [upstream PR 1474](https://github.com/modelcontextprotocol/rust-sdk/pull/1474). The PR was open on 2026-11-19. Its two commits add an optional Chrono compatibility feature to release 3.11.0 and document that feature. Histeries disables SDK default features and uses the Jiff timestamp implementation. This avoids the mandatory Chrono dependency in the published 3.11.0 archive. Replace the temporary source pin and remove the repository allowlist entry when a published SDK offers the same feature selection and passes the MCP protocol tests and dependency gates. [RustSec advisory](https://rustsec.org/advisories/RUSTSEC-2026-0543.html)
