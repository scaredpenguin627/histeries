# ADR-0004: generational entity arenas

- Status: Proposed
- Date: 2026-11-18
- Provenance: designed

## Context

Entity references need stable identity without reference cycles or iteration-order nondeterminism.

## Options

1. A hash-map ECS.
2. Generational arenas with typed IDs and sorted iteration.
3. Reference-counted object graphs.

## Decision

Recommend option 2. `sim` stores entities in generational arenas, exposes typed IDs, and sorts IDs before observable iteration.

## Consequences

Stale handles fail safely and memory growth is measurable. Systems cannot rely on insertion order, and an ECS replacement would require a new determinism ADR.
