# ADR-0007: deterministic city simulation model

- Status: Accepted
- Date: 2026-11-18
- Provenance: designed
- Accepted by maintainer: 2026-11-18

## Context

P1-08 needs cities, housing-style growth, production, districts, buildings, and builder charges while preserving the command boundary and deterministic save format. The Phase 1 behavioral specifications do not yet define numerical formulas.

## Decision

Cities start at population 1 with base housing 2. Food output is twice population plus data-defined building bonuses. A city stores food and grows by one population when stored food reaches `population * 10` and population is below effective housing. Production is integer-valued, one order is queued at a time, and completed units use the first sorted free traversable tile adjacent to the city center.

The four base district identifiers are `city_center`, `campus`, `industrial_zone`, and `harbor`. A city center is implicit at the founding tile. Other districts occupy one tile within graph distance 3 and may be placed once per city. Adjacency is the count of existing district tiles plus the implicit city center when returned by the topology neighbor relation. The base ruleset supplies twelve building definitions, costs, bonuses, and district prerequisites. Simulation lookups use those validated tables.

Builders are civilian units with three permanent improvement charges. Building a road consumes one charge and the builder's current movement for the turn.

State and replay formats use version 3 after adding cities and builder charges. Command documents use version 2 after adding the city command family. Each version remains independently validated.

## Consequences

All city and builder changes cross typed commands and are visible through seat-scoped observations. Growth, production, district placement, and unit placement use integer arithmetic and canonical tile or entity ordering. Future ruleset changes can add content tables without changing the city storage layout when the existing categories are sufficient.

[ADR-0016](0016-city-production.md) extends the active order into a bounded FIFO queue and defines the per-turn production yield from adjacent districts.
