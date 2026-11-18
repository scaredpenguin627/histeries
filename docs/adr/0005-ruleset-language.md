# ADR-0005: TOML rulesets and modifier language

- Status: Accepted
- Date: 2026-11-18
- Provenance: designed
- Accepted by maintainer: 2026-11-18

## Context

Units, buildings, policies, factions, eras, and yields must be data additions rather than Rust changes.

## Options

1. Hard-code content in Rust.
2. Load validated TOML tables with an effect, conditions, and scope modifier model.
3. Embed a general-purpose scripting language.

## Decision

Recommend option 2. `ruleset` validates TOML into immutable tables. Modifiers contain `effect`, `conditions`, and `scope` and are evaluated by pure rules-core functions.

## Consequences

Content changes remain reviewable and deterministic. The schema must be versioned, validation errors must be actionable, and arbitrary scripting remains outside Phases 1–2.


## Phase 1 implementation details

`rules-core` represents entity handles as typed stable IDs, keeps six canonical yield channels in `YieldSet`, and evaluates modifiers as scope, condition, and effect values. Context tags are sorted and deduplicated before matching. Each application is checked for fixed-point overflow, and callers can enforce the no-negative-yield invariant before committing state.
