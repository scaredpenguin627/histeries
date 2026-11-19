# Engineering budgets

The gates enforce the following budgets. Memory limits are provisional P1-12 regression thresholds. Bounds use integer arithmetic. Reported ratios and wall-clock measurements are diagnostic.

The 900-second fast-gate reference is the 4-vCPU PR runner specified in AGENTS.md. The Tier A Windows target is `x86_64-pc-windows-msvc`. The same wall-time limit is enforced by local `ci-fast` runs. Local measurements identify the host toolchain and whether Cargo build artifacts were present when the run started.

| Measure | Limit | Fixture |
| --- | ---: | --- |
| Fast gate wall time | Under 15 minutes | PR, target runner 4 vCPU |
| Scenario editor history | At most 33 snapshots and 16 MiB combined, retaining the current position even if it alone exceeds 16 MiB | Local map and scenario authoring |
| Planning latency | Per-game p95 at most 250 ms | Fast, both worker configurations |
| Nightly wall time | 2 hours per seed shard | Nightly |
| Weekly wall time | 6 hours per shard | Weekly |
| Largest encoded world save | 10 MiB | Nightly Large fixture |
| Largest encoded world save | 40 MiB | Weekly Huge fixture |
| Slowest planning round | 2 seconds | Weekly Huge fixture, both worker configurations |
| Live heap density from turn 100 | At most 512 bytes per live entity | Nightly and weekly memory probes |
| Increase over turn-100 density | At most 1 byte per entity per elapsed turn | Every memory checkpoint |
| Excess live bytes over fresh reload | 0 | Every memory checkpoint |
| Live game allocations after drop | 0 bytes | Long-lived and fresh probe processes |

The planning sample covers all seats in a round. The gate uses the slower of the single-worker and multi-worker measurements for that round. It also reports maximum latency and actual save sizes for all tiers.

The Huge map has four times as many tiles as Large, so its save limit is four times the Large limit. The weekly per-round maximum allows eight times the fast planning p95 limit. A single round over 2 seconds fails the weekly seed.

The weekly workflow persists a separate checkpoint for each seed after rounds 300, 600 and 900. Re-running a failed job at the same revision restores its latest cached boundary. A manual dispatch can continue a previous run at that revision by setting `resume_run_id` to its workflow run ID. Locally, use `cargo xtask fixture weekly --seed 0 --checkpoint-dir target/weekly-checkpoints --stop-after-round 900`, then repeat the command without `--stop-after-round 900` to finish the fixture and write its full report.

The memory denominator counts map tiles, players, units and cities. The heap probe uses `dhat` with ruleset and process-global warm-up excluded from both processes. It drains delivered events, rotates replay history in 25-turn segments and samples from turn 100 at 25-turn intervals, including the final turn. Live and fresh probes must agree on world hash and entity count. Checkpoint density is measured before dropping the game. Deliberately archived replays remain part of the output size, outside the steady-state heap comparison.

Measured values and reproduction details are recorded in [the scorecard](scorecard.md).
