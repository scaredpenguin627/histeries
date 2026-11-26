#!/usr/bin/env bash
set -euo pipefail

out=target/gates/multiplayer
mkdir -p "$out"
for seat in {1..8}; do
  export "HISTERIES_LAN_TOKEN_$seat=ci-seat-$seat"
done
target/debug/histeries serve --listen 127.0.0.1:0 --players 8 --seed 7 >"$out/host.json" &
host=$!
trap 'kill "$host" 2>/dev/null || true; wait "$host" 2>/dev/null || true' EXIT
for _ in {1..100}; do
  kill -0 "$host"
  if [[ -s "$out/host.json" ]]; then break; fi
  sleep 0.1
done
address=$(jq -er '.listening | select(startswith("127.0.0.1:"))' "$out/host.json")
capture() {
  timeout 90s xvfb-run -a -s '-screen 0 1280x1024x24' target/debug/histeries-client "$@"
}
capture --hotseat --players 8 --seed 7 --capture "$out/handoff.png"
export HISTERIES_LAN_TOKEN=ci-seat-1
capture --connect "$address" --seat 1 --capture "$out/lan.png"

join() {
  HISTERIES_LAN_TOKEN="ci-seat-$1" timeout 15s target/debug/histeries lan join --connect "$address" --seat "$1"
}
sequence=0
for round in 0 1; do
  for seat in {1..8}; do
    sequence=$((sequence + 1))
    printf 'end\nquit\n' | join "$seat" | tail -1 |
      jq -ce --argjson seq "$sequence" --argjson turn "$((round + seat / 8))" \
        'select(.sequence == $seq and .turn == $turn)' >"$out/last-position.json"
  done
done
expected=$(jq -er '.state_hash' "$out/last-position.json")
# Each seat reconstructs the host's final position in a new client process.
: > "$out/seat-positions.jsonl"
for seat in {1..8}; do
  printf 'observe\nquit\n' | join "$seat" | tail -1 |
    jq -ce --arg hash "$expected" --argjson seat "$seat" \
      'select(.observation.seat == $seat and .turn == 2 and .sequence == 16 and .state_hash == $hash)
       | {seat, turn, sequence, state_hash}' >> "$out/seat-positions.jsonl"
done
printf 'Eight LAN clients verified sequence %d and hash %s after two rounds.\n' "$sequence" "$expected"
