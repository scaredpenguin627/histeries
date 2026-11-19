# Behavioral specifications

Phase 1 uses the headless-game scope and acceptance conditions in section 9 of [AGENTS.md](../AGENTS.md). Local design decisions have `designed` provenance.

| Subject | Specification or decision |
| --- | --- |
| Map generation | [Continent and drainage rules](map-generation.md) |
| Stacking and historical replay | [ADR-0015](../docs/adr/0015-unit-stacking.md) |
| Construction queues and adjacency | [ADR-0016](../docs/adr/0016-city-production.md) |
| City capture and neutral raiders | [ADR-0013](../docs/adr/0013-conquest-and-scenarios.md) |
| Seat authority, transports and replay access | [MCP user guide](../docs/mcp.md), [ADR-0014](../docs/adr/0014-mcp-session.md) |

The [Phase 1 closure audit](../docs/sessions/2026-11-19-phase1-completion.md) maps the remaining game scope to code and tests.
