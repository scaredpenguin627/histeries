# ADR-0008: progression and Phase 1 victories

- Status: Accepted
- Date: 2026-11-18
- Provenance: designed
- Accepted by maintainer: 2026-11-18

## Context

P1-09 needs small technology and civic trees, policy slots, and deterministic domination and science victory checks. The Phase 1 specifications do not define progression costs or victory predicates.

## Decision

The base ruleset contains 25 technology nodes and 25 civic nodes. Each node has a positive integer cost and a validated acyclic prerequisite list. A seat selects one technology and one civic at a time. Science and culture output come from owned cities, their data-defined building bonuses, and active policy bonuses. Progress is accumulated only for a selected node, and completion carries excess progress forward.

Every seat starts with two policy slots. Completing each five civics unlocks one more slot, up to five. A policy is data-defined with an optional civic prerequisite and integer science or culture bonuses. A seat may activate one policy per slot and cannot duplicate an active policy.

Domination is reached when a seat owns at least one city and every city in the session belongs to that seat. Science is reached when a seat owns at least one city and completes every technology node. The first eligible seat in canonical `PlayerId` order wins when predicates become true. A seat that has reached a victory cannot issue further progression or policy state changes.

## Consequences

Progression state, policy slots, active policies, and victories are part of the deterministic state hash and versioned save/replay documents. Observations expose only the observing seat's progression and policies. The domination predicate reflects the Phase 1 simulation's current city ownership model.
