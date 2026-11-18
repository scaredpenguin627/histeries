# ADR-0001: topology trait and coordinate policy

- Status: Accepted
- Date: 2026-11-18
- Provenance: designed
- Accepted by maintainer: 2026-11-18

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


## Phase 1 implementation details

`FlatHex` uses a rectangular width-by-height topology with row-major `TileId` values. Horizontal and vertical wrapping are independent constructor options. Neighbor results, rings, areas, and line results are sorted by `TileId` and deduplicated at seams. `line_of_sight` includes both endpoints and returns the pure topology line. The rendering embedding uses integer display units, and the default A* heuristic is the topology distance.
