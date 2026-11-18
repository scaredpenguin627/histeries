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

