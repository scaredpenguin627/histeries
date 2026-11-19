# ADR-0011 — Verification boundaries for self-play

- Status: adopted for P1-12
- Provenance: designed
- Date: 2026-11-19

## Context

A final state hash can match after two runs diverged on an earlier turn. A decoded save can also hash correctly while its restored AI makes a different decision on the following turn. Heap usage includes the current world and any undelivered events or command history. A growing population of cities can legitimately increase live bytes.

## Decision

Run each measured game from the same seed with one and eight requested planning workers. Apply complete seat plans in canonical order. Compare the committed world hash at each round boundary and check the final state even if victory occurs during a partial round. Threads are an upper bound on concurrently planned seats. The two executions, replay and restored continuations count as one logical game.

Serialize the command log and replay it from its recorded initial state. Resume an independently decoded world snapshot with the corresponding seat controllers' incumbent objectives. Use the normal observation, planning and dispatch path for the resumed continuation. Report the seed, player count and first differing boundary when a comparison fails. Keep timing out of the canonical state and its hash.

Use full victory rules for the 1,000-game soak. Long-duration fixtures require an explicit endurance mode because ordinary games can finish well before the memory measurement window. Retain a technology-tree leaf from selection in that fixture while running the other rules and commands. The two modes have separate acceptance conditions.

For heap measurements, flush delivered events and completed replay segments at defined boundaries. Compare current live allocations to a fresh process initialized from the same checkpoint. Measure growth relative to the fresh process as well as bytes per entity so that growth in the simulated world can be distinguished from retained allocations. Save size, turn latency and retained heap have separate budgets.

Weekly shards persist checksummed, immutable segments. Each segment records both workers' continuation snapshots and seat objectives, the verified round hashes and replay boundaries, and the measurements from that interval. A restarted worker checks the prefix against the fixture and uses the recorded boundary to continue planning. CI persists per-seed progress at rounds 300, 600 and 900 at the same source revision. The heap probe runs uninterrupted so its long-lived process retains its allocation history.

## Consequences

The gate can localize a regression to a reproducible game and round. Fast cross-platform fixture traces can also be compared without comparing wall-clock times. Independent game seeds can run in separate shards. Endurance results describe sustained simulations under their declared scenario restriction; match outcomes come from the ordinary soak.
