# Phase 5 — Playable 3D client

P5-01 adds a 3D map to the existing Rust client. Single-player, hotseat and LAN matches use the existing command paths and fog-filtered observations. The desktop entry point accepts `--3d`. The map toolbar and **V** switch views during desktop and browser play. The scenario editor keeps its 2D authoring surface. The working bookmark is `task/phase5-3d-client`. [Specification](../../specs/three-dimensional-client.md) · [ADR-0023](../adr/0023-three-dimensional-match-view.md)

The project generates the low-poly scenery and pieces procedurally. The camera, terrain heights, GPU buffers and picking state live in the client. The maintainer selected fresh Sol 6.2 subagents at maximum reasoning effort for independent reviews of the Codex-authored changes.

The first review identified two interactions to cover in regression tests. Zooming out must retain roads and known resources on the map. When a targeting prompt covers a destination, clicking the prompt must leave the order pending.

## Local acceptance

The Windows GNU client library tests cover a unit and a summit occluding another surface, picks outside the seat's visible area, movement in wide and compact 3D windows, turn advancement and hotseat handoffs. A device readback on Intel UHD 620 using D3D12 drew a near green triangle followed by a farther red one. The near surface remained visible on both linear and sRGB color targets.

The actual Windows client rendered the seed-7 match at 1,500 × 1,000. Its 2D and 3D captures of the covered eight-seat handoff were byte-identical. A separate terminal player ended seat 1’s turn on a TCP loopback host. The native graphical client connected as seat 2 and rendered its own city and orders in both views.

In Windows Chrome 158, the seed-7 game ran on an Intel Gen-9 WebGPU adapter with a 1,182 × 648 canvas. Switching to 3D changed 39.38% of 16,000 sampled map pixels. Orbiting changed 41.59% while the turn stayed at zero. The player ended the turn in 3D, returned to 2D, and saved 1,236 bytes. After reloading the page, the client reopened turn 1 from the browser slot.

At a 900 × 600 viewport, a local scale check fitted a fully revealed 128 × 128 scenario with 1,936 warriors. The overview emitted 359,370 vertices (8.23 MiB), down from 2,766,048 (63.31 MiB) with detailed geometry throughout. These are vertex-stream sizes at that camera setting. The picker uses the same detail level that the GPU displays.
