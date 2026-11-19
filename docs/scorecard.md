# Histeries scorecard

The latest measured results appear first. The [Phase 1 audit](sessions/2026-11-19-phase1-completion.md) maps the requirements to their implementation and tests. [Gate budgets](budgets.md)

## Phase 3 measurements

The final native-client fast gate passed all 140 tests in 152.659 seconds locally, including nine client tests. Four session tests cover fog, rejected orders, deterministic AI turns, save/load, victory and starting units across 32 seeds. Four presenter tests exercise camera geometry, wide and compact layouts, selection and a rejected move followed by a successful retry. One desktop persistence test opens a replaced save and resumes its last saved round. A seed-7 Windows wgpu run read back a 1,500 × 1,000 PNG from the native surface. The seven MCP wire regressions and the existing CLI suite passed after extracting the shared local match host. [Native client report](sessions/2026-11-19-phase3-client.md)

### Scenario editor

On Windows GNU, editor revision `68fcc0d3e107` passed the 900-second fast gate in 859.427 s from an empty Cargo target and in 62.291 s with warm artifacts. All 151 tests, 34 parity checks and 1,200 round hashes passed. Under the same clean-build settings the pre-editor baseline `696c8c2d2929` took 934.900 s immediately before the editor and 913.207 s immediately after it. Both baseline runs completed their functional checks and exceeded the wall-time limit. Earlier editor revisions also exceeded that budget. [Scenario editor report](sessions/2026-11-19-phase3-editor.md)

## Phase 2 measurements · 2026-11-19

Three-seat stdio planning matched the expected unit positions and hit points on live advancement, save/resume and replay. [Regression results](sessions/2026-11-19-stdio-bot-snapshot.md)

The current controller played both seats on each of 32 reserved maps (seeds 40000–40031). The sign test treats a map as one paired observation and excludes tied maps. Elo is the smoothed rating difference within each matchup. The eight-worker repeat reproduced each of the 192 single-worker game rows, including its winner, length, conflict count and final hash. All 384 games finished without a crash, error or draw. [Individual games and commands](sessions/2026-11-19-phase2-tournament.txt)

| Opponent | Wins / games | Win rate | Relative Elo | One-sided paired p |
| --- | ---: | ---: | ---: | ---: |
| Frozen Phase 1 controller | 44 / 64 | 68.75% | +135 | 0.00209 |
| Greedy | 60 / 64 | 93.75% | +451 | 3.73 × 10⁻⁹ |
| Random | 64 / 64 | 100% | +844 | 2.33 × 10⁻¹⁰ |

After a review correction to move-and-fire range, the three control matchups reproduced their original results. On another 32 previously unused maps (50000–50031), the candidate won 45/64 against v1 (70.3125%, +147 Elo, one-sided paired p = 0.00048828125) and 60/64 against greedy. The corrected executable completed 512 tournament games with zero errors, crashes or draws. Its contested-seat planning p95 was 31.2 µs versus 19.4 µs for v1. Two full external-agent MCP games reproduce 219 protocol exchanges on fresh native stdio servers, through science victories in rounds 60 and 57. [New game's wire transcript](sessions/2026-11-19-phase2-mcp-game.jsonl.gz)

| Reference suite | Checkpoints compared as individual assertions | Result |
| --- | ---: | --- |
| City growth and housing | 11 | 11 passed |
| Military escort and builder road | 10 | 10 passed |
| Seat visibility and round completion | 7 | 7 passed |
| City capture and victory | 6 | 6 passed |
| Maintainer observations | 0 | No samples supplied |

The four designed fixtures use hand-derived game-rule expectations. Each PR runs them with `cargo xtask parity` and publishes the expected and measured values in its fast-gate artifacts. [Fixture definitions and method](../fixtures/README.md)

On the Phase 2 PR, the hosted Linux, macOS and Windows jobs passed and compared all 1,200 fast round hashes. All five nightly shards and twelve weekly jobs passed, including the checkpointed 1,000-round fixtures. [Cross-platform fast gate](https://github.com/scaredpenguin627/histeries/actions/runs/44490666796) · [Nightly](https://github.com/scaredpenguin627/histeries/actions/runs/44490667328) · [Weekly](https://github.com/scaredpenguin627/histeries/actions/runs/44490673620)

The ordinary Phase 2 soak finished 1,000 games in 97.794 s (984 science, 16 domination, zero errors or crashes). All 40,414 round hashes matched across one and eight planning workers, save/load and replay; 8,710 hashes matched after checkpoint restoration. All 84,053 observed contacts received explicit orders. The highest per-game planning p95 on the unrestricted soak was 365.209 ms. On the six fast fixtures the highest p95 was 1.556 ms against the 250 ms limit. [Raw gate reports](sessions/2026-11-19-phase2-gates.zip)

## Phase 1 completion measurements · 2026-11-19

All 1,000 ordinary games reached a victory with zero panics or errors on Windows x86-64 GNU with Rust 1.100.0. The 2–8-seat sample produced 918 science and 82 domination victories. The three-layer AI won 58 of 64 games against greedy on the fresh 30000–30031 seed cohort (90.625%, acceptance threshold 75%). It won all 64 games against random on those seeds. The 20000–20031 replication cohort produced 59 and 64 wins respectively. Seats swapped on every map. The full three-cohort tournament covered 384 games with zero draws, errors or crashes. [Per-game tournament evidence](sessions/phase1-completion-tournament.txt)

| Local measurement | Fast · 2/4 seats, Small | Nightly · 8 seats, Large | Weekly · 16 seats, Huge | Ordinary · 2–8 seats, Small |
| --- | ---: | ---: | ---: | ---: |
| Completed games | 6 | 5 | 2 | 1,000 |
| Matched round hashes, each of 1 vs 8 workers, save/load and replay | 1,200 | 2,500 | 2,000 | 40,451 |
| Matched hashes after checkpoint restore | 432 | 1,220 | 976 | 10,435 |
| Military units in contact receiving explicit orders | 1,658 | 1,657 | 376 | 92,264 |
| Worst per-game planning p95 (µs) | 16,714 | 107,549 | 13,017 | 145,387 |
| Maximum planning round (µs) | 42,000 | 746,613 | 184,874 | 462,937 |
| Largest encoded save (bytes) | 6,340 | 18,337 | 55,757 | 8,038 |

| Live heap measurement | Nightly | Weekly |
| --- | ---: | ---: |
| Checkpoints compared to a fresh process | 85 | 74 |
| Peak live bytes per entity | 336.852 | 277.266 |
| Maximum increase over turn-100 density (bytes/entity/turn) | 0.162 | 0.073 |
| Maximum excess live bytes over fresh reload | 0 | 0 |
| Live bytes after dropping the game | 0 | 0 |

All seven heap final hashes matched the endurance runs. Both Huge games independently resumed from round 900 and matched all 1,000 hashes of their uninterrupted runs. Two earlier overlapping weekly runs exceeded the 2-second latency budget on seed 1, with maxima of 3,762,342 and 3,995,630 µs while builds and other fixtures ran. The archive preserves both failed gate receipts. The weekly table reports the isolated rerun. The final ordinary soak ran through `xtask soak`. An independent run of the frozen release executable used the same validation and seed schedule. All 40,451 ordinary-game and 1,200 fast-fixture round hashes matched the earlier build, as did the winners and contact orders. Replaying all 384 tournament games on the final build reproduced every per-game result and final hash. The endurance and heap fixtures ran before the final changes to post-victory validation and MCP access. Their stress mode disables conquest and terminal science, and the workers access the simulation directly. The final fast fixtures exercised that mode again. Strict local `cargo xtask ci-fast` passed in **391.783 seconds** against its 900-second budget, with all 118 tests passing. Hosted cross-OS hash comparisons and 4-vCPU runner timing require CI. In the two-seat MCP game, the agent issued 19 dispatches and 60 turn advances. Replaying turn 60 returned a science victory for seat 2. [MCP transcript](sessions/2026-11-19-mcp-llm-game.jsonl.gz)

The archive contains full round-hash traces, weekly checkpoint receipts and recovered traces, all seven heap probes, the strict fast-gate receipt, per-game tournament results and SHA-256 checksums for the original records. Archive SHA-256 is `08f5021dca963d9a5facd55a7e6b374b2d5c335a424b0d2266eeb10f209253e3`. [Phase 1 verification archive](sessions/2026-11-19-phase1-completion-evidence.zip)

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
