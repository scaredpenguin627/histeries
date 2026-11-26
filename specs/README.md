# Behavioral specifications

Phase 1 uses the headless-game scope and acceptance conditions in section 9 of [AGENTS.md](../AGENTS.md). Local design decisions have `designed` provenance.

| Subject | Specification or decision |
| --- | --- |
| Map generation | [Continent and drainage rules](map-generation.md) |
| Stacking and historical replay | [ADR-0015](../docs/adr/0015-unit-stacking.md) |
| Construction queues and adjacency | [ADR-0016](../docs/adr/0016-city-production.md) |
| City capture and neutral raiders | [ADR-0013](../docs/adr/0013-conquest-and-scenarios.md) |
| Seat authority, transports and replay access | [MCP user guide](../docs/mcp.md), [ADR-0014](../docs/adr/0014-mcp-session.md) |
| Lockstep multiplayer, hotseat and trusted LAN sessions | [Multiplayer rules](multiplayer.md), [ADR-0022](../docs/adr/0022-lockstep-multiplayer.md) |
| Playable 3D map and picking | [3D match specification](three-dimensional-client.md), [ADR-0023](../docs/adr/0023-three-dimensional-match-view.md) |
| Spherical cell IDs, geometry and routes | [Geodesic rules](geodesic.md), [ADR-0024](../docs/adr/0024-geodesic-topology.md) |

The [Phase 1 closure audit](../docs/sessions/2026-11-19-phase1-completion.md) maps the remaining game scope to code and tests.

## Phase 3 editor packages

- `designed` An authenticated editor can call `apply_editor_package` in addition to the five core MCP tools. The request carries the TOML manifest and Wasm bytes. The host binds the proposed commands to the editor's seat and commits them atomically. Only editor credentials can discover or use this tool and its package-reference resource. [ADR-0020](../docs/adr/0020-editor-extension-abi.md)
