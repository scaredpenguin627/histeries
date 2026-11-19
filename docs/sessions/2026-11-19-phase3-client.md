# Phase 3 desktop client

The native `histeries-client` launches an eframe wgpu/winit window on a playable two-seat scenario. The egui map uses visible tile embeddings from the human seat, with map navigation, unit and city selection, targeted orders, production, research and turn control. The Game menu starts another seed or opens and saves an ordinary simulation save. The screenshot path reads the rendered GPU surface.

The simulation's local match host and the AI opponent scheduler are shared with MCP. The original MCP wire shape is preserved. The presenter sends typed commands and receives fog-filtered observations; integer topology geometry becomes screen-space coordinates only for rendering. [ADR-0018](../adr/0018-two-dimensional-client.md)

Four adapter tests, four presenter tests and a native save replacement/round-trip test passed on Windows GNU. The native graphics backend also produced a 1,500 × 1,000 image for seed 7. All seven previous MCP protocol integration tests passed. The Linux CI capture runs the client under Xvfb with Mesa Vulkan and publishes the frame with the fast-gate artifacts.

The delivery bookmark is `task/phase3-client`.

Opus review identified an unintended eight-seat cap in the extracted standard frontier. The cap was removed, and a nine-seat scenario regression now verifies each capital. The final local strict gate passed in 152.659 seconds with built dependencies, 140/140 tests, 34/34 fixture assertions and 1,200 matching round hashes. The previous revision passed in 124.815 seconds. Recompiling the revised code under contention exceeded the 900-second limit at 1,637.053 seconds before the successful final run. On the first run with concurrent builds and new graphics dependencies all functional checks passed, but the wall time of 1,820.827 seconds exceeded the 900-second threshold. Both OHOS checks, the wasm32 workspace check and cargo-deny passed with the original license policy. The pull request records the hosted matrix and independent review.
