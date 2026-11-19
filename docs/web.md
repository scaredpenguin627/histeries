# Browser client

Run `rustup target add wasm32-unknown-unknown` for the pinned toolchain. Install `wasm-bindgen-cli` version 0.2.130, or point `WASM_BINDGEN` at that version's executable. From the repository root run the build command.

```text
cargo xtask web-build
```

For a local preview, run `cargo xtask web-serve` and open `http://127.0.0.1:8080` in a WebGPU-capable browser. To preview a downloaded and extracted bundle, pass its directory and an optional port to `cargo xtask web-serve`. Use the same hostname and port to reopen a save later.

`cargo xtask web-build` builds the release Rust client and emits a static site in the Cargo target directory under `web/`. Serve that directory over HTTPS or localhost with the `.wasm` files served as `application/wasm`. It can be deployed on any static host. PR builds attach the complete site as the `histeries-web` artifact. The checked-in HTML loads the generated bindings directly and displays startup errors.

The browser uses the same seat, map presenter, commands and opponent scheduler as the native client. A WebGPU-capable browser and adapter are required. Use **Game → Save game** to store the current world. On a later visit, use **Game → Open game** to resume. **Scenario → Save scenario** and **Open scenario** use a separate slot for the editor draft. Clearing the site's data removes both slots. The scenario mod manager can import a manifest and its plugin through the browser file picker.

The Linux browser acceptance script requires Chrome with a matching ChromeDriver, curl, jq and ImageMagick. Run `bash scripts/test-web.sh target/web` after the build. To test a release bundle with a prebuilt xtask, pass its executable path as the second argument. The script uses SwiftShader, saves the game and scenario, imports and applies Woodland, and writes the three saves under `target/gates/browser-observed/`. Generate native reference saves with `cargo run --locked -p histeries-client --example browser_reference -- target/gates/browser-reference`. The CI comparison checks the three files byte for byte.
