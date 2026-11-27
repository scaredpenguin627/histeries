# HarmonyOS PC shell

Open the `platform/ohos` directory as a Stage project in DevEco Studio. The launcher starts `EntryAbility` in the `org.histeries.game` bundle. The board occupies a native ArkUI XComponent. Window resize redraws the current game at the new surface size. Hiding the window releases its Vulkan renderer. Showing it recreates the renderer and redraws the same game, including when the window size is unchanged. **End turn** advances the session and requests another frame.

Each new draw request can submit three frames and retry up to eight temporary surface failures. It can attempt Vulkan renderer creation three times. Restoring or resizing a visible window starts a fresh set of attempts.

## Build

Install the repository's pinned Rust toolchain and both OHOS Rust targets. Use Huawei's HarmonyOS command-line tools with the HarmonyOS SDK, Hvigor, ohpm and native CMake on the build host. The project targets PC/2-in-1 devices and supports ARM64 and x86-64. [Huawei build profile](https://developer.huawei.com/consumer/en/doc/harmonyos-guides/ide-hvigor-build-profile-app/) · [PC device configuration](https://developer.huawei.com/consumer/en/doc/harmonyos-guides/module-configuration-file/)

Run from `platform/ohos` in a shell configured for the SDK.

```text
rustup target add aarch64-unknown-linux-ohos x86_64-unknown-linux-ohos
ohpm install
hvigorw --mode module -p product=default -p module=entry@default assembleHap --no-daemon
```

Hvigor calls CMake for each ABI. CMake verifies and extracts the pinned upstream wgpu archive, builds the locked Rust renderer, and links it into `libhisteries_board.so`. The default product produces an unsigned HAP in `entry/build/default/outputs/default`. Configure debug signing for your device in a local DevEco project copy to produce an installable HAP. Keep signing keys, passwords and device profiles outside the repository.

## Capture the first frame on a PC

Connect a physical HarmonyOS PC through HDC and use its connection key from `hdc list targets`. Keep the game window visible during capture. Run this from PowerShell with the signed HAP and a fresh output directory.

```powershell
./capture-first-frame.ps1 -Hap ./entry/build/default/outputs/default/entry-default-signed.hap -Target <connection-key> -OutputDirectory ./build/first-frame
```

The command installs the HAP, stops any running Histeries session, and starts the ability in a new process. It captures the display after that process logs its Vulkan adapter and first frame submission. The receipt records the device model and system version, surface size, HAP hash and screenshot hash. Open `screen.png` and verify that the game window contains the rendered globe and the **End turn** control. Record physical-device acceptance only after that visual check. [HDC](https://developer.huawei.com/consumer/en/doc/harmonyos-guides/hdc/) · [Display capture](https://developer.huawei.com/consumer/en/doc/doccenter-capabilities/uitest-guidelines)
