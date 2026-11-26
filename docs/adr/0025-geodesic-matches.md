# ADR-0025 — Carry the board descriptor inside the map

- Status: accepted
- Date: 2026-11-26
- Provenance: designed

## Context

Geodesic provides canonical numbering, neighbor slots and polygon geometry. A game must restore the same graph when loading a map or joining an in-progress match. Historic flat-world commands and snapshots already have published encodings and measured hashes.

## Decision

Let the simulation own either a flat topology or the immutable geodesic graph. Continue to write flat maps using their existing version-1 map payload. For a globe the nested `MAP1` payload uses little-endian version 2, shape tag 1, its 32-bit subdivision frequency, and the existing tile count and sorted records. The reader checks the descriptor, expected count and record extent before building the graph. Algorithms and tile numbering are defined in ADR-0024. Saves, command logs and LAN checkpoints already embed map data and use the same decoder.

Generate the sphere's environment through the map ruleset. Obtain downstream and upstream river edge slots from the actual adjacent cells. The seat projection supplies visible cells and their integer surface geometry to the renderer. The client positions its camera and pieces in display space and continues submitting orders through the regular registry. [Game specification](../../specs/globe-matches.md) · [Stable geodesic IDs](0024-geodesic-topology.md) · [Save framing](0006-serialization-format.md)

## Consequences

A globe save can be opened by the CLI, native or browser client, editor and multiplayer host. A renderer can cover uncharted terrain without receiving private tile data. Flat campaigns retain their hashes and the existing archive can still be replayed.
