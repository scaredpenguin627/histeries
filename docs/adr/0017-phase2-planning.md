# ADR-0017 ¡ª Planning city projects and joint assaults under fog

- Status: Proposed
- Date: 2026-11-19
- Provenance: designed

## Context

The Phase 1 controller scores victory paths using population, research and force counts. A district project starts one item at a time. Tactical search visits units in ID order and evaluates a single order per unit. The simulation now supports waiting production orders, district adjacency and city captures.

## Decision

The current controller retains the six-point hysteresis margin. Science utility includes observed research buildings. Domination utility rises when an unguarded friendly or enemy city has opposing melee on its perimeter. The operational planner can supply a missing assault unit for a domination objective. It ranks district sites by production adjacency, then queues the selected district and its dependent building together. The simulation queue preserves the second task and any production overflow across rounds and checkpoints.

A width-four beam resolves ranged fire support before melee actors. IDs break ties within a role. Each candidate can move and then attack or capture when movement remains. The second action uses the combat forecast at the projected position. A move-and-capture requires full prior sight of the city's perimeter. Movement scores consider reachable assault positions, threatened city perimeters and scouting away from friendly cities. Search uses seat observations, integer scores and a bounded action depth. Engine state and save formats are unchanged.

The tournament retains the Phase 1 controller as the versioned v1 opponent, including the original objective scoring, production choices and tactical order. Both controllers run under the same simulation rules.

## Consequences

A ranged unit may remove the last defender before a lower-ID melee unit captures a city. Terminal science orders use holds for remaining military units, since the registry closes capture after a victory. The referee tracks attack and capture orders that depend on an invalidated move. The benchmark reports each layer and compares the two complete controllers on the same contested observation.
