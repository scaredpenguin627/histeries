#!/usr/bin/env bash
set -euo pipefail

site=${1:-target/web}
observed=target/gates/browser-observed
mkdir -p "$observed"
xtask=${2:-}
if [[ -z "$xtask" ]]; then
    xtask=$(cargo build --locked -q -p histeries-xtask --bin xtask --message-format=json | jq -r 'select(.reason=="compiler-artifact" and .target.name=="xtask") | .executable')
fi
"$xtask" web-serve "$site" 0 >target/gates/web-server.log 2>&1 &
server=$!
chromedriver --port=0 --allowed-ips=127.0.0.1 >target/gates/chromedriver.log 2>&1 &
driver=$!
wd=
session=
request() { curl --connect-timeout 3 --max-time 90 "$@"; }
fail() { echo "$*" >&2; exit 1; }
screenshot() { request --fail-with-body -sS "$wd/session/$session/screenshot" | jq -er .value | base64 --decode >"$1"; }
cleanup() {
    local code=$?
    if [[ -n "$session" ]]; then
        if (( code != 0 )); then screenshot target/gates/web-failure.png || true; fi
        request --max-time 5 -s -X DELETE "$wd/session/$session" >/dev/null || true
    fi
    kill "$driver" "$server" 2>/dev/null || true
    wait "$driver" 2>/dev/null || true
    wait "$server" 2>/dev/null || true
}
trap cleanup EXIT

origin=
ready=false
for _ in {1..60}; do
    kill -0 "$server" "$driver" 2>/dev/null || fail "Preview server or ChromeDriver exited at startup."
    origin=$(sed -n 's|^Serving .* at \(http://127\.0\.0\.1:[0-9]*\)/$|\1|p' target/gates/web-server.log)
    port=$(sed -nE 's/.*ChromeDriver was started successfully on port ([0-9]+).*/\1/p' target/gates/chromedriver.log)
    if [[ -n "$origin" && -n "$port" ]]; then
        wd="http://127.0.0.1:$port"
        if request --max-time 3 --silent --fail "$wd/status" >/dev/null && request --max-time 3 --silent --fail "$origin/" >/dev/null; then ready=true; break; fi
    fi
    sleep 0.5
done
$ready || fail "Preview server and ChromeDriver did not become ready."
for asset in 'histeries_client.js:text/javascript' 'histeries_client_bg.wasm:application/wasm'; do
    type=$(request --silent --fail --head --output /dev/null --write-out '%{content_type}' "$origin/pkg/${asset%%:*}")
    [[ $type == "${asset#*:}" ]] || fail "Unexpected content type for ${asset%%:*}: $type"
done
post() { request --fail-with-body -sS -H 'Content-Type: application/json' --data-binary "$2" "$wd/$1"; }
capabilities=$(post session '{"capabilities":{"alwaysMatch":{"browserName":"chrome","pageLoadStrategy":"none","goog:chromeOptions":{"args":["--headless=new","--remote-debugging-pipe","--enable-features=Vulkan","--use-vulkan=swiftshader","--disable-dev-shm-usage","--enable-unsafe-webgpu","--enable-unsafe-swiftshader","--use-webgpu-adapter=swiftshader","--force-device-scale-factor=1","--use-angle=swiftshader","--window-size=1200,800"]}}}}')
session=$(jq -er .value.sessionId <<<"$capabilities")
post "session/$session/url" "$(jq -nc --arg origin "$origin/" '{url:$origin}')" >/dev/null
evaluate() { post "session/$session/execute/sync" "$(jq -nc --arg s "$1" '{script:$s,args:[]}')" | jq -r .value; }
wait_for() {
    for _ in {1..100}; do
        if [[ $(evaluate "$1") == true ]]; then return; fi
        sleep 0.2
    done
    fail "$2"
}
slot() { evaluate "return localStorage.getItem('histeries.$1.v1')"; }
wait_for_scenario_save() {
    wait_for "const save = localStorage.getItem('histeries.scenario.v1'); return Boolean(save) && save !== '$1';" "The changed scenario was not saved."
}
wait_turn() {
    for _ in {1..120}; do
        if [[ $(evaluate 'return document.querySelector("canvas")?.getAttribute("aria-label")') == "Histeries strategy map, turn $1" ]]; then return; fi
        if [[ $(evaluate 'return document.querySelector("#status")?.getAttribute("role")') == alert ]]; then break; fi
        sleep 0.5
    done
    evaluate 'return document.querySelector("#status").textContent' >&2
    fail "Browser did not reach turn $1."
}
reload_game() {
    local started
    started=$(evaluate 'return performance.timeOrigin')
    evaluate 'window.setTimeout(() => location.reload(), 100); return true' >/dev/null
    wait_for "return performance.timeOrigin !== $started" "Browser did not replace the previous document."
    wait_turn 0
}
# Input uses CSS pixels with a fixed viewport scale and the bundled game font.
wait_for_paint() {
    local script='const done = arguments[arguments.length - 1]; requestAnimationFrame(() => requestAnimationFrame(() => done(true)));'
    post "session/$session/execute/async" "$(jq -nc --arg s "$script" '{script:$s,args:[]}')" | jq -e '.value == true' >/dev/null
}
click() {
    post "session/$session/actions" "$(jq -nc --argjson x "$1" --argjson y "$2" '{actions:[{type:"pointer",id:"mouse",parameters:{pointerType:"mouse"},actions:[{type:"pointerMove",x:$x,y:$y,origin:"viewport",duration:120},{type:"pointerDown",button:0},{type:"pause",duration:180},{type:"pointerUp",button:0}]}]}')" >/dev/null
    wait_for_paint
}
view_key() {
    post "session/$session/actions" "$(jq -nc --arg key "$1" '{actions:[{type:"key",id:"keyboard",actions:[{type:"keyDown",value:$key},{type:"pause",duration:100},{type:"keyUp",value:$key}]}]}')" >/dev/null
    wait_for_paint
}
wait_3d_turn() {
    wait_for "return document.querySelector('canvas')?.getAttribute('aria-label') === 'Histeries 3D strategy map, turn $1'" "3D map did not reach turn $1."
}
game_menu() { click 150 27; }
scenario_menu() { click 209 24; }
open_editor() {
    game_menu
    click 166 129
    wait_for 'return document.querySelector("canvas")?.getAttribute("aria-label") === "Histeries scenario editor"' "Scenario editor did not open."
}
# Keyboard focus in a file dialog starts before its confirmation button.
confirm() {
    post "session/$session/actions" '{"actions":[{"type":"key","id":"keyboard","actions":[{"type":"keyDown","value":"\uE004"},{"type":"pause","duration":100},{"type":"keyUp","value":"\uE004"},{"type":"pause","duration":250},{"type":"keyDown","value":"\uE007"},{"type":"pause","duration":100},{"type":"keyUp","value":"\uE007"}]}]}' >/dev/null
    wait_for_paint
}
check_failed_open() {
    game_menu
    click 153 75
    confirm
    wait_for "return (document.querySelector('canvas').getAttribute('aria-description') || '').startsWith('$1')" "Missing file error: $1"
    post "session/$session/actions" '{"actions":[{"type":"key","id":"keyboard","actions":[{"type":"keyDown","value":"\uE00C"},{"type":"keyUp","value":"\uE00C"}]}]}' >/dev/null
    wait_for 'return !document.querySelector("canvas").getAttribute("aria-description")' "File dialog did not close."
    wait_turn 0
}
wait_turn 0
evaluate 'return Boolean(document.querySelector("canvas").getContext("webgpu"))' | jq -e . >/dev/null || fail "No WebGPU canvas."
# Query the active browser for its renderer information independently of the launch flags.
adapter_script='const done = arguments[arguments.length-1]; (async () => { const adapter = await navigator.gpu.requestAdapter(); if (!adapter) throw Error("No adapter"); const info = adapter.info ?? await adapter.requestAdapterInfo(); return {vendor: info.vendor, architecture: info.architecture, device: info.device, description: info.description, fallback: info.isFallbackAdapter ?? adapter.isFallbackAdapter ?? null}; })().then(done, e => done({error: String(e)}));'
adapter=$(post "session/$session/execute/async" "$(jq -nc --arg script "$adapter_script" '{script:$script,args:[]}')" | jq -ec '.value | select(.error == null)')
jq -n --argjson startup "$capabilities" --argjson adapter "$adapter" '{browser: $startup.value.capabilities.browserName, version: $startup.value.capabilities.browserVersion, platform: $startup.value.capabilities.platformName, chromeDriver: $startup.value.capabilities.chrome.chromedriverVersion, adapter: $adapter}' | tee target/gates/browser-runtime.json
jq -e '(.platform | ascii_downcase) == "linux" and (.adapter.architecture | ascii_downcase) == "swiftshader"' target/gates/browser-runtime.json >/dev/null || fail "Linux software-renderer acceptance did not report SwiftShader."
check_failed_open "This save slot is empty."
evaluate 'localStorage.setItem("histeries.world.v1", "invalid*"); return true' >/dev/null
check_failed_open "Invalid saved data:"
evaluate 'localStorage.removeItem("histeries.world.v1"); return true' >/dev/null
x=$(evaluate 'return Math.floor(document.querySelector("canvas").getBoundingClientRect().right - 48)')
screenshot target/gates/web-2d.png
view_key v
wait_3d_turn 0
screenshot target/gates/web-3d.png
bash scripts/measure-3d-frame.sh target/gates/web-2d.png target/gates/web-3d.png > target/gates/web-3d.json
post "session/$session/actions" '{"actions":[{"type":"pointer","id":"mouse","parameters":{"pointerType":"mouse"},"actions":[{"type":"pointerMove","origin":"viewport","x":350,"y":300},{"type":"pointerDown","button":2},{"type":"pointerMove","origin":"viewport","x":475,"y":345,"duration":400},{"type":"pointerUp","button":2}]}]}' >/dev/null
wait_for_paint
wait_3d_turn 0
screenshot target/gates/web-3d-orbit.png
bash scripts/measure-3d-frame.sh target/gates/web-3d.png target/gates/web-3d-orbit.png > target/gates/web-3d-orbit.json
click "$x" 24
wait_3d_turn 1
view_key v
wait_turn 1
game_menu
click 153 97                     # Save game
confirm
wait_for 'return Boolean(localStorage.getItem("histeries.world.v1")?.length)' "Game save slot is empty."
slot world | base64 --decode >"$observed/turn-1.hst"
reload_game
game_menu
click 153 75                     # Open game
confirm
wait_turn 1
screenshot target/gates/web.png
colors=$(convert target/gates/web.png -sample 12.5% -format '%k' info:)
(( colors > 100 )) || fail "Rendered frame has only $colors sampled colors."
game_save=$(evaluate 'return localStorage.getItem("histeries.world.v1")')
click "$x" 24
wait_turn 2
open_editor
scenario_menu
click 217 98                     # Scenario → Save scenario
confirm
wait_for 'return Boolean(localStorage.getItem("histeries.scenario.v1")) && localStorage.getItem("histeries.scenario.v1") !== localStorage.getItem("histeries.world.v1")' "Missing distinct scenario save."
[[ $(evaluate 'return localStorage.getItem("histeries.world.v1")') == "$game_save" ]] || fail "Saving the scenario changed the game slot."
reload_game
open_editor
scenario_menu
click 217 74                     # Scenario → Open scenario
confirm
scenario_menu
click 221 178                    # Scenario → Playtest scenario
wait_turn 2
echo "WebGPU reopened a turn-1 game and a separate turn-2 scenario after page reloads."

# Import the two packaged files using Chrome's file input. Compare snapshots around the edit.
reload_game
open_editor
previous_scenario=$(slot scenario)
scenario_menu
click 217 99
confirm
wait_for_scenario_save "$previous_scenario"
slot scenario | base64 --decode >"$observed/before-woodland.hst"
scenario_menu
click 236 155                     # Mod manager
cx=$(evaluate 'return Math.floor(innerWidth / 2)')
cy=$(evaluate 'return Math.floor(innerHeight / 2)')
click "$((cx+1))" "$((cy+24))"    # Remove the bundled copy
cdp() { post "session/$session/goog/cdp/execute" "$(jq -nc --arg cmd "$1" --argjson params "$2" '{cmd:$cmd,params:$params}')" | jq -e .value; }
cdp Page.setInterceptFileChooserDialog '{"enabled":true}' >/dev/null
click "$((cx-144))" "$((cy+27))"  # Choose package files
wait_for 'return Boolean(document.querySelector("input[type=file][multiple]"))' "Package file picker did not open."
root=$(cdp DOM.getDocument '{}' | jq -er .root.nodeId)
input=$(cdp DOM.querySelector "$(jq -nc --argjson node "$root" '{nodeId:$node,selector:"input[type=file][multiple]"}')" | jq -er .nodeId)
package=$(realpath data/extensions/woodland)
cdp DOM.setFileInputFiles "$(jq -nc --argjson node "$input" --arg base "$package" '{nodeId:$node,files:[($base+"/manifest.toml"),($base+"/plugin.wasm")]}')" >/dev/null
wait_for 'return !document.querySelector("input[type=file][multiple]")' "Package import did not return."
click "$((cx-221))" "$((cy+33))"  # Enable the imported package
click "$((cx-91))" "$((cy+33))"   # Apply
screenshot target/gates/web-woodland.png
click "$((cx-206))" "$((cy+110))" # Close manager
previous_scenario=$(slot scenario)
scenario_menu
click 217 99
confirm
wait_for_scenario_save "$previous_scenario"
slot scenario | base64 --decode >"$observed/after-woodland.hst"
if cmp -s "$observed/before-woodland.hst" "$observed/after-woodland.hst"; then fail "Applying the imported package left the draft unchanged."; fi
echo "Saved before and after Woodland for comparison with the native one-tile edit."

# Open a seeded spherical campaign through the public URL, orbit it, and resume its save.
post "session/$session/url" "$(jq -nc --arg url "$origin/?globe=3&seed=7" '{url:$url}')" >/dev/null
globe_turn() {
    wait_for "return document.querySelector('canvas')?.getAttribute('aria-label') === 'Histeries 3D globe, turn $1'" "Globe did not reach turn $1."
}
globe_turn 0
wait_for_paint
screenshot target/gates/web-globe.png
post "session/$session/actions" '{"actions":[{"type":"pointer","id":"mouse","parameters":{"pointerType":"mouse"},"actions":[{"type":"pointerMove","origin":"viewport","x":350,"y":300},{"type":"pointerDown","button":2},{"type":"pointerMove","origin":"viewport","x":475,"y":345,"duration":400},{"type":"pointerUp","button":2}]}]}' >/dev/null
wait_for_paint
globe_turn 0
screenshot target/gates/web-globe-orbit.png
bash scripts/measure-3d-frame.sh target/gates/web-globe.png target/gates/web-globe-orbit.png > target/gates/web-globe-orbit.json
click "$x" 24
globe_turn 1
previous_game=$(slot world)
game_menu
click 153 97
confirm
wait_for "const save=localStorage.getItem('histeries.world.v1'); return Boolean(save) && save !== '$previous_game';" "The globe save was not written."
slot world | base64 --decode >"$observed/globe-turn-1.hst"
post "session/$session/url" "$(jq -nc --arg url "$origin/" '{url:$url}')" >/dev/null
wait_turn 0
game_menu
click 153 75
confirm
globe_turn 1

# Open a late-game save earned through ordinary research in a separate native process.
opening=target/gates/orbital-opening.hst
opening_info=$(cargo run --locked -q -p histeries-sim --example orbital_opening -- "$opening")
round=$(sed -nE 's/^round=([0-9]+) .*/\1/p' <<<"$opening_info")
[[ "$round" =~ ^[0-9]+$ ]] || fail "Missing orbital opening round."
encoded=$(base64 -w0 "$opening")
evaluate "localStorage.setItem('histeries.world.v1','$encoded'); return true" >/dev/null
game_menu
click 153 75
confirm
globe_turn "$round"
chart() {
    wait_for "return document.querySelector('canvas')?.getAttribute('aria-label') === 'Histeries Orbital Survey, turn $1, $2'" "Missing orbital chart state: turn $1, $2."
}
view_key o
chart "$round" "no expedition, 0 visited"
view_key l
chart "$round" "expedition at site 0, 1 visited"
view_key e
round=$((round + 1))
chart "$round" "expedition at site 0, 1 visited"
view_key n
chart "$round" "expedition at site 1, 2 visited"
screenshot target/gates/web-orbital-survey.png
view_key o
globe_turn "$round"
game_menu
click 153 97
confirm
wait_for "return localStorage.getItem('histeries.world.v1') !== '$encoded'" "Orbital survey was not saved."
slot world | base64 --decode >"$observed/orbital-survey.hst"
reload_game
game_menu
click 153 75
confirm
globe_turn "$round"
view_key o
chart "$round" "expedition at site 1, 2 visited"
