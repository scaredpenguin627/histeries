# ADR-0013 — Playable conquest and scenario actions

- Decision: adopted for Phase 1
- Provenance: designed
- Date: 2026-11-19

## Context

The simulation could recognize sole control of the existing cities. Once two players had founded cities, the registry had no way to transfer ownership between them. Neutral military units could retaliate but had no orders of their own.

## Decision

An owned melee military unit with remaining movement may capture an adjacent enemy city if none of the current owner's military units occupies its perimeter. The attacker spends its movement and stays on its tile. The city transfers intact, including its districts, buildings and production. Combat and capture are separate commands, so a defender can be defeated before the capture in the same series of orders. The simulation tests the existing domination predicate immediately after transfer. Under current rules the first victory stays recorded. A defeated seat cannot found a city or recapture one after domination. Tactical search evaluates visible captures for every controller. Long-duration fixtures substitute a hold for a capture to keep their capitals active through the measurement window.

In current-rules replays, the last seat's end-turn command also activates existing barbarians after the movement refresh. Each can engage an adjacent military unit using the same combat forecast, or close by one legal step on a military target within two graph steps. Distance and stable IDs break ties. The legacy v4 replay path retains the old round rules.

The scenario editor accepts an ordered JSON array of typed registry commands. It validates the batch in memory. When saving a replay alongside a world, the CLI stages both outputs and publishes the world last, so an invalid or unwritable replay path leaves the input save available for a retry. The save, replay and JSON command list must have distinct destinations. The optional frontier opening places cities on traversable land, prioritizes separation, adds garrisons where there is space and places a raider if a suitably distant site exists. The default empty setup remains available for map authoring.

## Consequences

Conquest is possible in a live two-seat game without editing ownership. The ordinary soak and tournaments permit both victories. The endurance fixture retains one terminal technology and prevents capital transfers; its fixed number of measured rounds does not depend on the chosen victory path.
