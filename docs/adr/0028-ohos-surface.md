# ADR-0028 — Present the seat's globe to an ArkUI native window

Status: proposed. Date: 2026-11-27. Provenance: designed.

## Context

The upstream Vulkan backend merged the OHOS window extension after the dependency used by the desktop client branched. A PC build must retain the native window across all surface accesses and share the game's existing mesh and turn logic.

## Decision

ArkUI owns the event loop. Its XComponent callbacks serialize creation, resize, redraw and destruction. The native window reference outlives the Vulkan surface. A separate game handle owns the local session and remains valid across surface replacement. The Rust renderer uses the seat observation, desktop mesh builder, shader and depth-tested pipeline.

For the PC shell, consume the upstream source at `8b419de0906f37af69d728f1735132e427a3436f` with a checked archive digest and a separate Cargo lockfile. The release label in that workspace is 30.0.0. The commit identity selects the change. Desktop and browser keep the existing registry dependency. The shell's C ABI and raw window handle are confined to the client integration.

## Consequences

Hvigor and ohpm run with the Node runtime supplied by Huawei's SDK to compile ArkTS and package the ability. This vendor toolchain is confined to the OHOS build. Simulation, AI and the renderer continue to run in Rust. The isolated static library forms part of the native client; the small unsafe boundary connects the SDK's opaque native handles to Rust.

The OHOS package can be compiled separately from desktop and browser. Building an unsigned package verifies linking and packaging. The device receipt records runtime Vulkan selection, presentation and visible game content. [Upstream change](https://github.com/gfx-rs/wgpu/pull/9908) · [Device acceptance](../../specs/ohos-client.md)
