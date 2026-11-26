# ADR-0023 — Render the seat observation as a depth-tested scene

- Status: accepted
- Date: 2026-11-26
- Provenance: designed

## Context

The desktop and browser clients share a presenter. The simulation supplies stable tile IDs, visible entities and a topology embedding per visible tile. Phase 5 calls for a playable 3D client while retaining the existing input, seat permissions and save format.

## Decision

Add a second map renderer inside `histeries-client`. Generate original triangle meshes from the visible tile and entity records. An orbiting orthographic camera projects those triangles into the map viewport. The GPU depth test resolves overlapping faces. CPU picking uses the same projected vertices, depth convention and owning tile IDs. A selected tile or target goes through the ordinary inspector and command path. The existing handoff screen controls whether the presenter draws a scene.

The native and WebGPU front ends use the existing wgpu device through an egui paint callback. Camera and graphics resources belong to the presentation layer. The 2D map continues to serve as the default view and the scenario editor's authoring surface.

## Consequences

The same save can open in either view. Visual height and camera position have no effect on movement costs, visibility or simulation hashes. Both backends need rendered-frame acceptance in addition to transform, picking and command tests. [3D match specification](../../specs/three-dimensional-client.md) · [Local host and observation boundary](0018-two-dimensional-client.md)
