# Session report — 2026-11-18 — phase1-cities

- Task: P1-08 cities, growth caps, production, districts, buildings, and builders (class B)
- Model: Codex GPT-6.2
- Reviewer: APPROVE — independent clean-room re-review completed after corrective passes
- Commit: change `wnvyxkwxtznz` (`feat(sim): add deterministic city simulation`)

## What changed

Added data-validated district and building tables, city founding through owned settlers, integer food and housing growth, production queues, graph-validated district placement with adjacency calculation, twelve building kinds, builder units with permanent charges, city commands, state/replay serialization, and seat-scoped city observations.

## Decisions and ADRs

- ADR-0007 defines the designed Phase 1 city formulas, district identifiers, production completion placement, and builder charge behavior.
- State and replay documents use version 3. Command documents use version 2 for the city command family.

## Gates run

| Command | Result |
|---|---|
| `cargo test --workspace` | 39 passed |
| `cargo xtask ci-fast` | pass. cargo-deny advisory refresh used the cached bans, licenses, and sources checks after the local TLS fetch failed |
| `cargo check --workspace --target wasm32-unknown-unknown` | pass |
| `cargo check --workspace --target aarch64-unknown-linux-ohos` | pass |
| `cargo check --workspace --target x86_64-unknown-linux-ohos` | pass |

## Scorecard deltas

P1-08 adds city, growth, production, district, building, builder-charge, serialization, and seat-observation tests.

## Open questions for the maintainer

1. Tune city and building costs after the first self-play tournament fixture exists.

## Next

P1-09: technology and civic data, policy slots, and victory checks.
