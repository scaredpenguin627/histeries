# ADR-0002: named deterministic random streams

- Status: Accepted
- Date: 2026-11-18
- Provenance: designed
- Accepted by maintainer: 2026-11-18

## Context

Map generation, combat, barbarians, and players need reproducible randomness across machines and thread counts.

## Options

1. One global RNG.
2. Named streams derived from a save seed and stream label.
3. Platform-default randomness.

## Decision

Recommend option 2. Pin the generator crate version and algorithm, derive streams from a master seed and stable labels, and store the algorithm identifier in saves.

## Consequences

Independent systems do not perturb one another's sequence. Adding a stream is a save-format decision and requires a migration path.


## Phase 1 implementation details

The first implementation pins `rand_chacha` 0.9.0 and uses `ChaCha8Rng`. Each stream derives a 32-byte seed from the master seed and its stable name with the documented FNV-1a seed derivation. The algorithm identifier `chacha8-v0.9.0-named-fnv1a-seed-v1` is available to save metadata. Bounded values use rejection sampling to avoid modulo bias.
