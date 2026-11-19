# ADR-0015 — Friendly military and civilian unit stacks

- Date 2026-11-19
- Status proposed
- Provenance designed

## Context

Phase 1 allows one military unit and one civilian on a tile. The earlier simulation rejected every occupied destination. Some existing replay logs depend on this rule when city production searches for a deployment tile.

## Decision

A player may share a traversable tile between one of their military units and one of their civilians. Either class can arrive first. A third unit, a second unit of the same class, a rival or a barbarian is denied entry. A city center or completed district still needs an exclusive tile. A settler travelling with an escort must separate before founding. Military combat targets an individual enemy military unit by ID. The civilian remains on the tile when its escort dies.

City production chooses the first adjacent traversable tile in canonical order that has room for the new unit. District construction and impassable terrain edits still require clearance from both slots. The observing seat learns both IDs and the common owner only on a revealed tile. Tactical orders and city planning use those visible slots.

New replay logs use HREP version 5. HREP version 4 playback keeps single-unit placement, including the case where a completed production order waited for an empty tile. An old world snapshot can start a new session under current rules.

## Consequences

The canonical unit table continues to serialize one record per ID. Loading rejects hostile and same-class stacks. The change affects outcomes of new games, so comparison with earlier self-play runs requires fresh tournament measurements.
