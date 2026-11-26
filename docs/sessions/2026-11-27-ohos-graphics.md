# HarmonyOS NEXT PC graphics preflight

On 2026-11-27 a dependency probe using Rust 1.100.0 compiled for `aarch64-unknown-linux-ohos` and `x86_64-unknown-linux-ohos`. It passed an `Arc<winit_ohos::Window>` to `wgpu::Instance::create_surface`. This exercised the Rust type boundary between a real window type and the published Vulkan API. The warm, locked run took 24.59 seconds on Windows GNU.

## Reproduce the compile check

Use a scratch Rust library with these dependencies and put the following function in its `src/lib.rs`.

```toml
[dependencies]
wgpu = { version = "=30.0.2", default-features = false, features = ["std", "vulkan", "wgsl"] }
winit-ohos = "=0.31.0-beta.3"
openharmony-ability = "=1.0.0-beta.2"
openharmony-ability-derive = "=1.0.0-beta.1"
```

```rust
pub fn surface(
    gpu: &wgpu::Instance,
    window: std::sync::Arc<winit_ohos::Window>,
) -> Result<wgpu::Surface<'static>, wgpu::CreateSurfaceError> {
    gpu.create_surface(window)
}
```

```sh
cargo generate-lockfile
cargo check --locked --target aarch64-unknown-linux-ohos --target x86_64-unknown-linux-ohos
```

Resolving the window backend with the newer `openharmony-ability 1.0.0-beta.5` produced six compile errors in the backend's resize and input handling. Selecting the ability and derive versions from the backend's own lockfile passed both checks. The production client uses `eframe 0.37.0` with `winit 0.30.13`. The separate OHOS backend uses `winit-core 0.31.0-beta.3`. [Window backend](https://docs.rs/winit-ohos/0.31.0-beta.3/winit_ohos/) · [Client dependencies](../../crates/client/Cargo.toml)

## Native frame acceptance

The published `wgpu-hal 30.0.2` Vulkan surface code falls through to the unsupported-handle case for `OhosNdk`. Upstream added `VK_OHOS_surface` after the version used in this check. The host inventory found neither an OpenHarmony SDK nor HDC, so device enumeration and HAP deployment were unavailable. The native shell remains open on the roadmap. [Tagged Vulkan backend](https://docs.rs/crate/wgpu-hal/30.0.2/source/src/vulkan/instance.rs) · [Upstream Vulkan change](https://github.com/gfx-rs/wgpu/pull/9908) · [Rust target requirements](https://doc.rust-lang.org/rustc/platform-support/openharmony.html)

Complete the shell milestone by building and installing its HAP on a HarmonyOS NEXT PC, creating a Vulkan surface for the ArkUI XComponent, presenting the game's first frame, and recording the device, adapter, surface dimensions and screen capture.
