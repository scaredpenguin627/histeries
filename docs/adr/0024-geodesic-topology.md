# ADR-0024 — Stable cells on a Goldberg sphere

- Status: accepted
- Provenance: designed
- Date: 2026-11-26

## Context

The flat map and clients already communicate through stable tile IDs. A spherical board needs a closed graph, a repeatable way to number shared edges, and a rendering surface whose adjacent polygons agree at their boundaries. Save, replay and network reconstruction depend on different machines generating the same board from the same descriptor.

## Decision

Use the dual of a class-I icosahedral subdivision. Pick a fixed ordering and winding for the base faces, visit their integer lattice vertices deterministically and unify shared vertices across faces. Make this numbering part of the geodesic board definition. Each triangular face in the refined lattice contributes one dual corner shared by its three incident cells. The implementation supplies directed edge slots and integer surface geometry along with the existing graph operations.

Use a bounded frequency of 1 through 32. At the supported maximum the board has 10,242 cells. Compute graph distance and deterministic shortest lines across the joined graph. Treat the lower-numbered endpoint as the canonical start when choosing among equally short lines so swapping endpoints reverses the result. The existing topology interface continues to support flat boards. [Topology policy](0001-topology-trait.md) · [Geodesic specification](../../specs/geodesic.md)

## Consequences

Pentagons are ordinary playable cells with five valid edge slots. Movement and visibility can use the same graph contract on either board. The renderer can obtain shared polygon corners directly and has no need to reconstruct topology from floating-point coordinates. Future changes to tile numbering or edge orientation require explicit compatibility handling.
