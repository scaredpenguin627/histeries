# ADR-0010 — Planning from seat observations

- Status: adopted for the P1-11 implementation
- Provenance: designed
- Date: 2026-11-19

## Context

The simulation owns the rules and command log. A controller needs enough information to coordinate research, city development and military orders under fog of war. The Phase 1 base simulation declares science and domination victories. It has no command for capturing an enemy city.

## Decision

One controller receives one seat's immutable observation. Strategic utility uses integer counts of population, completed research, own forces and visible rivals. An incumbent objective changes when the challenger leads by more than six points. The caller can restore the last objective alongside a game checkpoint. The session's save format is unchanged.

The operational planner uses hierarchical task-network methods for settlement, local defense and city development. For example, a city pursuing a library first needs a site and a campus. The queue holds the active task between turns. Each fresh observation determines the next executable task. The planner reserves sites within a plan. The `city.settle` command assigns a city ID during dispatch so a settler can found a city without discovering IDs under fog.

The tactical planner keeps four candidate joint plans after each military unit acts in numeric ID order. Each candidate tracks simulated occupation, hit points and zones of control. Attacks, movement and healing use the simulation's public forecasts, including the effects of promotions. Holding position is a replayable command. The greedy baseline chooses immediate economic returns per cost and retains only the highest-scoring tactical choice at each step. The random baseline samples legal alternatives from seat- and turn-specific named streams. Both receive the same observations and opening resources as the three-layer player.

The referee takes observations before any seat acts and applies each plan in canonical seat order. Tactical order within a plan matters because earlier actions can vacate a tile or defeat a target. During tournaments, an order that the earlier seat invalidates counts as one conflict if the later seat's command prefix was legal on the original snapshot. When a move loses its destination, each remaining move or founding order for that unit depends on the lost position. The referee skips those orders, verifies their original prefixes, and counts each skipped order as another conflict. Other units continue their plans. The referee stops at the first simulation victory. For local commands and old replays, binary tags retain their original meanings. New commands use previously unused tags 21 and 22.

## Consequences

A same-seed seat swap controls for terrain and order of play. The tournament counts all games in the win-rate denominator. It reports draws at the turn limit, errors, crashes and simultaneous conflicts separately. It computes significance over seed-pair scores because the two games on a map are related. Phase 1 measurements use equal difficulty. Tactical scenarios measure coordinated combat separately from victory rates on generated maps.
