# Histeries scorecard

CI fills the measured values. A blank value means the gate has not run yet.

| Measure | Fast | Nightly | Weekly | Notes |
| --- | ---: | ---: | ---: | --- |
| Spec items implemented with tests |  |  |  | Counted from roadmap and test reports |
| Determinism across thread counts |  |  |  | Per-turn state hashes |
| Save/load hash equality |  |  |  | Replay and reload gate |
| Bytes per live entity |  |  |  | Baseline at turn 100 |
| Memory slope after turn 100 |  |  |  | Measured by dhat gate |
| AI Elo versus `greedy` |  |  |  | Tournament evidence |
| AI turn-time p95 |  |  |  | Budget in `docs/budgets.md` |
| Panics and crashes |  |  |  | Self-play count |
| Fast-gate wall time |  |  |  | Target under 15 minutes |

## P1-12 local measurements · 2026-11-19

On Windows x86-64 GNU (Rust 1.100.0, Intel i5-8250U, eight logical CPUs), all 1,000 Small-map games reached a science victory with zero panics. The uninterrupted soak covered 1,000 distinct SplitMix64 seeds at indices 0–999, with 143 games each at two through seven seats and 142 at eight seats. All 52,706 round hashes, winners and contact-order counts matched two earlier runs. One ran uninterrupted. The other was interrupted after 968 completed games and resumed for the final 32 indices.

| Local measurement | Fast · 2/4 seats, Small | Nightly · 8 seats, Large | Weekly · 16 seats, Huge | Normal soak · 2–8 seats, Small |
| --- | ---: | ---: | ---: | ---: |
| Completed fixtures or normal games | 6 | 5 | 2 | 1,000 |
| Round hashes matched for 1 vs 8 workers, save/load and replay (each) | 1,200 | 2,500 | 2,000 | 52,706 |
| Round hashes matched after resuming a checkpoint | 432 | 1,220 | 976 | 20,418 |
| Explicit orders for military units in contact | 601 | 11,024 | 18,775 | 159,964 |
| Worst per-game planning p95 (µs) | 31,320 | 562,243 | 71,908 | 1,334,553 |
| Maximum planning latency (µs) | 71,933 | 2,762,043 | 453,964 | 1,802,929 |
| Largest encoded world save (bytes) | 6,141 | 18,493 | 61,707 | 8,175 |
| Batch wall time (s) | 7.522 | 190.698 | 491.113 | 235.317 |

Fast ran 200 rounds per fixture, nightly ran 500 and weekly ran 1,000. These endurance scenarios reserve one technology-tree leaf so that the simulation can reach the measurement window. The normal soak used the full victory rules and a 500-round limit. Every contact observed on a planning turn received an explicit order. Each latency sample covers all seats and takes the slower of the one-worker and eight-worker plans.

| Local heap measurement | Nightly | Weekly |
| --- | ---: | ---: |
| Checkpoints compared with a fresh process | 85 | 74 |
| Peak live bytes per entity | 300.008 | 251.745 |
| Maximum density increase after turn 100 (bytes/entity/turn) | 0.0 | 0.0 |
| Maximum excess live bytes over fresh reload | 0 | 0 |
| Live bytes after game drop | 0 | 0 |

For each weekly seed, the first process wrote a checkpoint at round 900. A second process loaded a copy from a different directory and completed the last 100 rounds. Each resumed trace matched its uninterrupted counterpart on all 1,000 round hashes, with the same final hash as its heap probe. The weekly column reports the uninterrupted run. Across the two recovered runs, the slowest planning round took 1,379,516 µs (2,000,000 µs limit) and the largest save occupied 61,707 bytes (40 MiB limit).

All seven heap-probe final hashes matched the corresponding endurance fixture. Sampling began at turn 100 and continued every 25 turns. The complete local fast gate took **83.735 s** against a 900 s budget. A second local fast run matched all 1,200 round hashes. Hosted cross-platform comparisons and the 4-vCPU timing await CI.

The [evidence archive](sessions/2026-11-19-phase1-selfplay-evidence.zip) contains the uninterrupted soak (`p112-publish.jsonl`), all three endurance traces (`fast.jsonl`, `p112-nightly.jsonl`, `p112-weekly-final.jsonl`), the weekly round-900 receipts (`p112-weekly-stop-*.json`) and recovered full traces (`p112-weekly-recovered-*.jsonl`), the independent local fast trace (`p112-comparator-fast.jsonl`), the five `memory-*.json` and two `weekly-memory-*.json` heap reports, the fast-gate result (`ci-fast.json`) and an aggregate (`summary.json`). Each trace includes the per-round hashes and its batch summary. Archive SHA-256 is `56389aca56eddfe1f449c42132c12816b6273e6c59c00b3c5ccdddaec0c07a64`.

Reproduce from the repository root with the pinned Rust toolchain and gate tools. The following commands use PowerShell.

```powershell
cargo xtask ci-fast
cargo xtask fixture fast --output target/gates/p112-comparator-fast.jsonl
cargo xtask compare-fast target/gates/fast.jsonl target/gates/p112-comparator-fast.jsonl
cargo xtask soak --games 1000 --jobs 2 --seed 0 --output target/gates/p112-publish.jsonl
cargo xtask fixture nightly --output target/gates/p112-nightly.jsonl
cargo xtask fixture weekly --output target/gates/p112-weekly-final.jsonl
0..1 | ForEach-Object {
    $checkpoint = "target/weekly-$($_)-$([guid]::NewGuid())"
    cargo xtask fixture weekly --seed $_ --checkpoint-dir $checkpoint --stop-after-round 900
    Copy-Item -Recurse -LiteralPath $checkpoint -Destination "$checkpoint-recovery"
    cargo xtask fixture weekly --seed $_ --checkpoint-dir "$checkpoint-recovery" --output "target/gates/p112-weekly-recovered-$_.jsonl"
}
0..4 | ForEach-Object { cargo xtask memgate --size large --players 8 --seed $_ --turns 500 }
0..1 | ForEach-Object { cargo xtask memgate --size huge --players 16 --seed $_ --turns 1000 }
```

## Current implementation evidence

P1-01 supplies the topology API, wrapping behavior, deterministic ordering, integer embedding, A* heuristic, and seven unit tests. The fast gate currently runs ten tests, with CI remaining authoritative for published scorecard values.


P1-02 supplies checked scale-1,000 fixed-point operations, ChaCha8 named streams, unbiased bounded sampling, and versioned length-prefixed state hashing. The fast gate currently runs 15 tests.

P1-03 supplies typed entity IDs, six canonical yield channels, scope/tag/effect modifiers, and invariant tests for matching and non-negative yields. The fast gate currently runs 18 tests.

P1-04 supplies a versioned TOML schema, strict unknown-field rejection, provenance validation, deterministic BTreeMap tables, base units, and modifier conversion. The fast gate currently runs 22 tests.

P1-05 supplies the command registry, session state, seat-scoped observations, event stream, strict custom binary state and command documents, and replay round trips. The fast gate currently runs 25 tests.

P1-06 supplies deterministic map generation, terrain/features/rivers/resources, strict map encoding, and editor command coverage. The fast gate currently runs 30 tests.


P1-07 supplies deterministic unit movement, roads, ZOC, combat, healing, promotions, barbarian spawning, owner-scoped observations, and replay coverage. The fast gate currently runs 35 tests.

P1-08 supplies data-validated districts and buildings, city founding, housing-capped integer growth, production queues, district placement and adjacency, builder charges, city serialization, and seat-scoped city observations.
The fast gate currently runs 39 tests.

P1-09 supplies 25-node technology and civic DAGs, research progress, policy slots and effects, domination and science victory checks, progression serialization, and seat-scoped observations. The fast gate currently runs 41 workspace tests.

P1-10 supplies deterministic session creation, turn stepping and runs, replay inspection, ASCII map dumps, and command-routed scenario editing. The fast gate currently runs 43 workspace tests.

P1-11 local release measurements use 32 held-out confirmation seeds (20000–20031), both seats per seed, equal starting resources, an 8×8 generated map, and a 500-turn limit.

| Opponent | Wins / games | Losses | Draws, errors, crashes | Smoothed Elo difference | Planning p95 for two seats, one worker |
| --- | ---: | ---: | ---: | ---: | ---: |
| Greedy | 63 / 64 (98.4375%) | 1 | 0 | +651 | 453 µs |
| Seeded random | 64 / 64 (100%) | 0 | 0 | +844 | 515 µs |

All 128 confirmation games ended in science victories. The winner, number of turns, conflicts and terminal hash matched on repeat and with eight requested workers (up to two active planners). Against greedy, the candidate won both seats on 31 maps and split the last pair. The one-sided paired sign test gives p = 4.66 × 10⁻¹⁰. The CLI records legal orders invalidated by simultaneous play separately from failures. [Protocol and per-game results](sessions/2026-11-19-phase1-ai-tournament.txt)
