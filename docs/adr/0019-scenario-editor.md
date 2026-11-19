# ADR-0019 Local scenario editor

- Status: Accepted
- Date: 2026-11-19
- Provenance: designed

## Context

A scenario author needs to see unoccupied terrain and assign units to different seats before starting a game. The normal player observation reveals only the area around that seat’s cities and units. Existing scenario saves and the CLI use the simulation’s versioned state format.

## Decision

The desktop client opens a separate local draft containing a copy of the current game, an existing save, or a blank non-wrapping board. The local match host can request a `BoardObservation` containing the whole map. It holds public board data and has no private player fields. An author who controls a local save can inspect terrain outside the live player's fog by opening a draft. The session exposes seat and spectator observations using their ordinary visibility rules. Player, AI and MCP sessions keep their existing viewpoints. Both maps render and pick tiles using the same topology embedding and chart code.

The simulation owns `ScenarioDraft`, which translates each brush into registered commands, allocates unit IDs and validates a candidate session. The client owns the draft alongside its presentation state. Placing a city uses a settler and issues the standard founding order. Building a road requires a builder with a remaining charge for the selected seat. A successful edit returns its ordered commands and replaces the draft after its encoded state passes the save decoder. Dispatching those commands against the pre-edit snapshot reproduces the saved bytes, including when one brush spawns and settles a unit. Failed brushes leave the original position and undo history intact.

The editor retains up to 32 earlier positions. Its combined history has a 16 MiB budget and always retains the active scenario. Undo followed by an edit discards the abandoned redo branch. Saving writes an ordinary Histeries save. Game and scenario file dialogs track separate paths, initially `histeries.hst` and `scenario.hst`. Confirming New, Open or Copy current game into editor replaces the draft and its undo history only after the replacement succeeds. Reopening the editor restores its draft, including after the live match advances. Playtest starts a separate local match from the draft and restores player visibility. The client retains the active game session and its presenter. Both Playtest menu choices discard the trial. The author can resume editing the draft or return to the original match. Starting a new game or opening a different game closes the draft associated with the previous session.

## Consequences

Scenario authors can open CLI-generated maps, inspect every tile and test their edits in the client or through the CLI and MCP. Undo can reverse placement while its checkpoint is retained. The command registry has no general entity-deletion command. Finished scenarios can be inspected, while the simulation still enforces victory and action-specific limits when commands run.
