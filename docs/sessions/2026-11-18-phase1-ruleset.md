# Session report — 2026-11-18 — phase1-ruleset

- Task: P1-04 validated TOML rulesets and base data tables (class B)
- Model: Codex GPT-6.2
- Reviewer: gpt-6.2-astra (APPROVE)
- Commit: 9600c0d4 `feat(ruleset): load validated TOML rulesets`

## What changed

Added a strict versioned TOML loader with schema validation, provenance tags, slug and duplicate-ID checks, deterministic BTreeMap tables, and conversion into the rules-core modifier model. Added original base ruleset units and a tile food modifier.

## Decisions and ADRs

- ADR-0005 now has a strict schema boundary. Unknown keys fail parsing and provenance is retained and validated.
- Base content is tagged `designed` and contains only original Histeries data.

## Gates run

| Command | Result |
|---|---|
| `cargo xtask ci-fast` | pass with Rust 1.100.0 GNU, cargo-deny 0.20.2, cargo-nextest 0.9.149, wasm32, and both OHOS targets |
| `cargo nextest run --workspace` | 22 passed |
| `cargo check --workspace --target wasm32-unknown-unknown` | pass |
| `cargo check --workspace --target aarch64-unknown-linux-ohos` | pass |
| `cargo check --workspace --target x86_64-unknown-linux-ohos` | pass |

## Scorecard deltas

P1-04 adds strict schema and provenance validation tests and the first base data tables.

## Open questions for the maintainer

1. Choose the next base ruleset tables when P1-05 begins simulation state integration.

## Next

P1-05: simulation state, command registry, observations, events, serialization, and replay.

