# Session report — 2026-11-18 — phase1-progression

- Task: P1-09 technology, civic, policy, domination, and science systems (class C)
- Model: Codex GPT-6.2
- Reviewer: APPROVE — independent clean-room re-review completed after corrective passes
- Commit: change `wnvyxkwxtznz` (`feat(sim): add deterministic progression and victories`)

## What changed

Added validated 25-node technology and civic DAGs, policy definitions and bonuses, city science and culture output, seat-scoped research progress, policy slots, typed research and policy commands, domination and science victory checks, progression serialization, replay support, and progression observations.

## Decisions and ADRs

- ADR-0008 defines the designed progression formulas, two starting policy slots with five-civic unlock steps, policy effects, and deterministic victory predicates.
- Command documents use version 3. State and replay documents use version 4.

## Gates run

| Command | Result |
|---|---|
| `cargo test --workspace` | 41 passed |
| `cargo xtask ci-fast` | pass. cargo-deny advisory refresh used cached checks after the local TLS fetch failed |
| `cargo check --workspace --target wasm32-unknown-unknown` | pass |
| `cargo check --workspace --target aarch64-unknown-linux-ohos` | pass |
| `cargo check --workspace --target x86_64-unknown-linux-ohos` | pass |

## Scorecard deltas

P1-09 adds progression, policy, victory, serialization, and seat-isolation tests.

## Open questions for the maintainer

1. Tune progression costs and policy bonuses after the first self-play tournament fixture exists.

## Next

P1-10: CLI `new`, `step`, `run`, `replay`, `map`, and scenario-editor commands.
