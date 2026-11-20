# ADR-0018 — A single local host for desktop play

- Status: adopted
- Provenance: designed
- Date: 2026-11-19

## Context

Phase 3 introduces a playable interface while keeping the simulation authoritative. The desktop player and MCP already need the same turn limits, winner handling, opponent planning, private observations and historical views.

## Decision

The simulation hosts the match. A controller submits typed commands and receives the observing seat's projected state; the AI crate plans the remaining seats from one round boundary. A visible tile includes its topology-supplied embedding for placement. The presenter operates in screen coordinates, projects the hexes into the viewport and uses the same transform for hit testing. It opens a duel on a 12 × 9 flat map with a city and a warrior per side. The file format is the simulation's existing save format.

eframe connects the winit window and wgpu surface to egui. The desktop binary can capture the actual rendered surface to PNG for renderer verification. The UI uses original programmatically drawn terrain and symbols. Only the client depends on presentation libraries. The OHOS native shell remains a later milestone.

## Consequences

The UI can reject an invalid order without changing a game. The headless CLI and server continue to use the same underlying rules, and existing saves can be opened without converting them. A visible tile's embedding is a presentation hint; game commands continue to identify tiles by stable IDs.
