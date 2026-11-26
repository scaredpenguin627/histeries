# ADR-0026 — Derive seat ages from completed research

- Status: accepted
- Date: 2026-11-27
- Provenance: designed

## Context

Research and civics already advance independently for each seat. A game can complete research either at a round boundary or immediately from an accumulated balance. Both paths must expose the same unlocks and preserve existing saves and command logs.

## Decision

Declare ages in an ordered TOML list. Read the longest consecutive prefix for which the seat meets all prerequisites. Its last entry supplies the age ID and display name. Reached entries supply cumulative capability IDs. Policy definitions name their required age. Validate all references when the ruleset loads.

At each research resolution, compare the prefix before and after completion. Apply any declared science or culture reset for newly entered ages. Other state carries forward. Seat observations publish only their owner's age, capabilities and eligible policies. Commands still pass through the simulation's validators.

The current three-layer planner can adopt an age-gated policy. The historical random, greedy and V1 tournament controllers keep their pre-age policy pool. This preserves the original guest AI in recorded MCP campaigns across the ruleset extension.

## Consequences

Completed technologies and civics already persist in saves and participate in state hashes. Loading a save reconstructs the age without firing an entry hook. The base ruleset uses carry-over throughout, so existing command logs continue to replay with the same research balances. Ruleset authors can add another age and capabilities using TOML. [Specification](../../specs/ages.md)
