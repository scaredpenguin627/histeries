# Session report — 2026-11-18 — phase1-determinism

- Task: P1-02 fixed-point arithmetic, named RNG streams, and deterministic state hashing (class A)
- Model: Codex GPT-6.2
- Reviewer: gpt-6.2-astra (APPROVE)\n- Commit: 8e143362 `feat(rules-core): add deterministic arithmetic and RNG`

## What changed

Accepted ADR-0002 implementation details and added scale-1,000 checked fixed-point arithmetic, ChaCha8 named random streams, unbiased bounded sampling, and a versioned little-endian FNV-1a state hasher.

## Decisions and ADRs

- ADR-0002 now pins `rand_chacha` 0.9.0 and records the stream seed derivation and algorithm identifier.
- Fixed-point values remain in `rules-core`. State hashing remains in `sim` so future world-state serialization can feed canonical fields directly.

## Gates run

| Command | Result |
|---|---|
| `cargo xtask ci-fast` | pass with Rust 1.100.0 GNU, cargo-deny 0.20.2, cargo-nextest 0.9.149, wasm32, and both OHOS targets |
| `cargo deny check` | pass; six unmatched license allowances are warnings because the graph is still minimal |
| `cargo nextest run --workspace` | 15 passed |
| `cargo check --workspace --target wasm32-unknown-unknown` | pass |
| `cargo check --workspace --target aarch64-unknown-linux-ohos` | pass |
| `cargo check --workspace --target x86_64-unknown-linux-ohos` | pass |

## Scorecard deltas

P1-02 adds six rules and simulation tests. The scorecard records the implementation evidence, while CI remains authoritative for published measurements.

## Open questions for the maintainer

1. Review whether a future save-format ADR should encode the hash width alongside the RNG algorithm identifier.

## Next

P1-03: rules-core entities, modifiers, yields, and property tests.


