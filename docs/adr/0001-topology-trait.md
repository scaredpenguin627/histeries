# ADR-0001: topology trait and coordinate policy

- Status: Proposed
- Date: 2026-11-18
- Provenance: designed

## Context

The simulation needs multiple boards without leaking axial or cube coordinates outside the topology crate.

## Options

1. Expose axial coordinates everywhere.
2. Hide coordinates behind a `TileTopology` trait and identify tiles with stable IDs.
3. Use a general ECS graph with topology-specific components.

## Decision

Recommend option 2. `hex` owns coordinate math, while callers use tile IDs, neighbors, distance, rings, areas, line of sight, and rendering embeddings.

## Consequences

Flat hexes can ship first and Geodesic or other theatres can be added as data-compatible topology implementations. A topology must provide a deterministic heuristic for pathfinding. Coordinate-sensitive code stays inside `hex`.
