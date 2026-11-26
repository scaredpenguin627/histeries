# ADR-0027 — Scope expedition tiles to a ruleset board

- Status: accepted
- Date: 2026-11-27
- Provenance: designed

## Context

The surface already has stable tile IDs shared by orders, visibility, saves and clients. An orbit campaign adds another graph to the same turn sequence. Both graphs can contain a tile numbered zero.

## Decision

A theatre order names its board and uses tile IDs local to that graph. The ruleset owns the layout and capability requirement. The sim obtains adjacency and display positions from the topology. Each seat owns one survey record per explored theatre, containing the expedition's position, its spent action and its discoveries. The controller receives the observing seat's chart.

The same command registry handles launch, movement and recall for desktop, browser, CLI, lockstep and MCP. All seats use the normal end-turn command. Reserve versioned binary records for expedition orders and state, and preserve the established encoding of a campaign without expedition activity. A player or MCP seat token receives only the owner's projected chart. The host and trusted LAN replica processes retain every expedition in their canonical state so they can check the same hashes. [Lockstep trust boundary](0022-lockstep-multiplayer.md)

## Consequences

The game can add another survey theatre in TOML. A surface unit and an expedition can occupy the same numeric tile ID on separate boards. Connections, saves and replays maintain one canonical match, including survey knowledge that survives a recall. [Specification](../../specs/theatres.md)
