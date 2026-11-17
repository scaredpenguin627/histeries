# ADR-0003: fixed-point representation

- Status: Proposed
- Date: 2026-11-18
- Provenance: designed

## Context

Simulation results must be bit-for-bit identical across operating systems and thread counts.

## Options

1. IEEE floating point.
2. Integer-only values with an explicit scale.
3. A third-party decimal type.

## Decision

Recommend option 2. Use signed `i64` values with a scale of 1,000 for quantities that need fractions and integer types for counts. Centralize rounding and overflow checks in `rules-core`.

## Consequences

Serialization is stable and portable. Designers must express rules in the chosen scale, and overflow checks are part of every arithmetic boundary.
