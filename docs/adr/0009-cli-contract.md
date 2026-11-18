# ADR-0009: headless CLI contract

- Status: Accepted
- Date: 2026-11-19
- Provenance: designed
- Accepted by maintainer: 2026-11-19

## Context

P1-10 needs a deterministic command-line boundary for creating sessions, advancing turns, replaying logs, dumping maps, and editing scenarios. No CLI flags or output format were specified previously.

## Decision

`histeries new` creates a non-wrapping `FlatHex` map and a session with seats `1..=N`. Width, height, player count, numeric seed, state output, and optional replay output are explicit flags with defaults of 8, 8, 2, 0, and `state.hst`. The seed is expanded into four repeated little-endian 64-bit lanes.

`step` ends one selected seat turn and `run` ends every seat in canonical order for each requested turn. Both read and write versioned state snapshots and may emit a replay log. `replay` strictly decodes a replay log and prints the final turn, hash, and command count.

`map` prints terrain glyphs in stable tile-ID order. `editor` and `scenario-editor` route marker, terrain, feature, river, and resource edits through `Session::dispatch`. CLI state changes never bypass the simulation command registry.

## Consequences

CLI output remains deterministic and can be checked in scripts. State and replay files continue to use the simulation's versioned binary formats. The CLI does not introduce a second rules engine or serialization format.
