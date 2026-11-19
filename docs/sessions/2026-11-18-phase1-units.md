# Session report — 2026-11-18 — phase1-units

- Task: P1-07 units, movement, roads, ZOC, combat, healing, promotions, and barbarians (class B)
- Model: Codex GPT-6.2
- Reviewer: APPROVE — independent clean-room re-review completed after corrective pass
- Commit: change `wnvyxkwxtznz` (`feat(sim): add deterministic unit simulation`)

## What changed

Added typed unit archetypes, owner-scoped observations, terrain and road movement costs, enemy ZOC restrictions, deterministic melee/ranged combat, healing, promotions, player and barbarian spawning, unit serialization, replay coverage, and editor/unit command schemas.

## Decisions and ADRs

- Unit IDs remain typed `rules-core` handles. Generation and storage order remain simulation responsibilities.
- Combat uses integer strength and deterministic counterattacks. Barbarian ownership is represented by an absent player owner.
- State and replay snapshots use format version 2 for per-seat turn-end tracking. Command encoding remains independently versioned at 1.

## Gates run

| Command | Result |
|---|---|
| `cargo xtask ci-fast` | pass with Rust 1.100.0 GNU, cargo-deny 0.20.2, cargo-nextest 0.9.149, wasm32, and both OHOS targets |
| `cargo nextest run --workspace` | 35 passed |
| `cargo check --workspace --target wasm32-unknown-unknown` | pass |
| `cargo check --workspace --target aarch64-unknown-linux-ohos` | pass |
| `cargo check --workspace --target x86_64-unknown-linux-ohos` | pass |

## Scorecard deltas

P1-07 adds unit simulation, combat, ZOC, healing, promotion, barbarian, serialization, and observation tests.

## Open questions for the maintainer

1. Tune unit statistics after the first self-play tournament fixture exists.

## Next

P1-08: cities, growth caps, production, districts, buildings, and builders.
