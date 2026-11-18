# ADR-0006: versioned custom binary serialization

- Status: Accepted
- Date: 2026-11-18
- Provenance: designed
- Accepted by maintainer: 2026-11-18

## Context

Saves and replays must remain deterministic across operating systems, thread counts, and future schema versions. The format needs strict bounds and actionable corruption errors.

## Options

1. Use a general-purpose serialization format with implementation-defined layout.
2. Use a versioned custom binary format with explicit field widths, little-endian encoding, magic bytes, and length bounds.
3. Use normalized JSON for every save and replay record.

## Decision

Use option 2. State, commands, and replay logs have independent magic bytes, format version 1, explicit little-endian integer fields, canonical BTreeMap ordering, and strict trailing-byte rejection. Replay logs contain an initial state snapshot followed by length-prefixed command documents.

## Consequences

The format is compact and stable, and corruption is rejected before state mutation. Every future field requires a versioning decision and round-trip tests. A migration can decode older versions without changing the canonical in-memory model.
