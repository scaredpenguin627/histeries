#!/usr/bin/env bash
set -eu -o pipefail
hap=${1:?supply the unsigned entry HAP}
tmp=$(mktemp -d)
trap 'rm -rf -- "$tmp"' EXIT

unzip -tq "$hap"
unzip -q "$hap" 'module.json' 'resources.index' 'ets/modules.abc' 'libs/*/libhisteries_board.so' -d "$tmp"
test -s "$tmp/resources.index"
test -s "$tmp/ets/modules.abc"
jq -e '
  .app.bundleName == "org.histeries.game" and
  .module.mainElement == "EntryAbility" and
  (.module.deviceTypes | index("2in1") != null) and
  any(.module.abilities[]; .name == "EntryAbility" and .exported == true)
' "$tmp/module.json" > /dev/null

for abi in arm64-v8a x86_64; do
    library="$tmp/libs/$abi/libhisteries_board.so"
    machine=$(readelf --file-header "$library" | awk -F: '/Machine:/ {sub(/^ +/, "", $2); print $2}')
    case "$abi/$machine" in
        arm64-v8a/AArch64 | x86_64/"Advanced Micro Devices X86-64") ;;
        *) echo "Unexpected ELF machine for $abi: $machine" >&2; exit 1 ;;
    esac
    readelf --dyn-syms --wide "$library" > "$tmp/symbols"
    awk '$7 != "UND" && $8 == "registerHisteriesBoard" {seen=1} END {exit !seen}' "$tmp/symbols"
    for symbol in napi_module_register OH_NativeXComponent_RegisterCallback \
                  OH_NativeXComponent_RegisterSurfaceShowCallback OH_NativeXComponent_RegisterSurfaceHideCallback; do
        awk -v name="$symbol" '$7 == "UND" && $8 == name {seen=1} END {exit !seen}' "$tmp/symbols"
    done
    readelf --sections --wide "$library" > "$tmp/sections"
    grep -Fq '.init_array' "$tmp/sections"
    readelf --dynamic --wide "$library" > "$tmp/dependencies"
    if grep -Fq '[libc++_shared.so]' "$tmp/dependencies"; then
        unzip -p "$hap" "libs/$abi/libc++_shared.so" > "$tmp/libc++_shared.so"
        test -s "$tmp/libc++_shared.so"
    fi
    echo "$abi: $machine, XComponent N-API registration linked"
done
sha256sum "$hap"
