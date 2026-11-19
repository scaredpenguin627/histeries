# ADR-0012: ruleset-driven continents and river drainage

- Status: Proposed
- Date: 2026-11-19
- Provenance: designed

## Context

The first generator independently assigned each tile a terrain and each river a direction. Phase 1 calls for ruleset-defined continents and map content. The MAP1 format already stores terrain, feature, river edge and resource for each tile.

## Decision

Use the named `mapgen` stream over the supplied tile graph. Grow distinct connected landmasses from separated seeds, preserving a water channel between continents. Sample land terrain from the ruleset. Give each terrain its own optional feature and resource chances. Determine coasts from adjacency and route rivers along shortest traversable paths to the coast. Use a fixed clockwise edge order from north-east to encode flows on a flat topology. The original tuning values live in `data/rulesets/base/base.toml`. See [generation rules](../../specs/map-generation.md).

Keep this optional block in schema version 1. Rulesets written before the block existed inherit the bundled map profile. `histeries new --map-ruleset PATH` selects the map profile for a new world. Subsequent turns use the existing simulation rules.

## Consequences

Regenerating a seed previously used by the independent-tile generator can produce different terrain. Loading an existing save or replay preserves its stored tiles, including edited geography. The continent cap bounds seed selection work. Water channels take precedence over the land budget on tightly packed maps. Rivers from multiple sources can share their outlet.
