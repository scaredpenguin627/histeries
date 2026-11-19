# ADR-0016: queued construction and district adjacency

- Status: Proposed
- Date: 2026-11-19
- Provenance: designed

## Context

ADR-0007 introduced a single active production order. Phase 1 requires a production queue and district placement that affects city output.

## Decision

`city.enqueue_production` appends a unit, district or building order. A city accepts up to eight orders, including the active one. Appending to a busy city preserves its progress. All orders reserve their district tiles immediately. Earlier orders can satisfy a later building’s district prerequisite. Unique buildings, district kinds and district tiles can occur only once across completed and pending construction. Units may repeat.

Each city completes at most one order per round. An obstructed order keeps its accumulated progress and its place. After successful completion the next item becomes active with any excess production carried forward. `city.set_production`, `city.build_district` and `city.build_building` replace the queue and reset production. Seat observations list the owner's waiting orders in completion order.

Each completed district earns its data-defined `adjacency_production` per neighboring district belonging to the same city, including its city center. The three buildable base districts earn one production per neighbor. A pair of adjacent buildable districts earns the bonus on each district. Production uses the district layout at the beginning of that city’s turn.

## Consequences

State v5 stores waiting orders after each city record. The reader accepts state v4 with empty waiting queues. A nonempty queue extends the state hash. Archived replay v4 retains its historical production and occupancy rules and can round-trip through the v4 encoder. A resumed standalone v4 save uses the current rules.
