# Running the gates

Run from the workspace root with the pinned Rust toolchain.

| Command | Checks |
| --- | --- |
| `cargo xtask layers` | Workspace dependency direction and client-only native dependencies. |
| `cargo xtask ohos-check` | Workspace checks for both OHOS targets. |
| `cargo xtask ci-fast` | Layers, formatting, Clippy, dependency audit, nextest, cross-target builds and the six fast self-play fixtures. |
| `cargo xtask soak` | 1,000 independent games with 2–8 players and a 500-round cap. Every game must end with a winner. |
| `cargo xtask fixture fast` | Three seeds, both two and four players, 200 rounds on Small. |
| `cargo xtask fixture nightly` | Five seeds, eight players, 500 rounds on Large. |
| `cargo xtask fixture weekly` | Two seeds, sixteen players, 1,000 rounds on Huge. |
| `cargo xtask memgate --size large --players 8 --turns 500 --seed 0` | Measures live heap and compares it with a fresh process at each sampling boundary. |

The fixture and soak reports are newline-delimited JSON under `target/gates/`. Each game record includes the ordered 64-bit hashes, verification counts, seed, player count, contact orders, terminal result, largest encoded save and planning latency. Use an integer-preserving JSON reader for the hashes. The final record summarizes the batch. `cargo xtask compare-fast` takes the paths of fast reports from independent hosts and compares their full round traces.

To reproduce a soak failure, pass the seed reported in its game record directly to `histeries selfplay --mode game --players N --size small --turns 500 --workers 1 --seed S`. The CLI also replays the game with eight requested workers. In `soak`, `--seed` selects the start of a deterministic seed sequence, `--offset` selects a game index, `--games` sets the shard length, and `--jobs` sets the number of concurrent games. `--output PATH` selects the report destination.

Endurance fixtures use `--mode stress` and withhold the final base technology leaf, `tech_25`, to keep all seats playing beyond the usual victory turn. The ordinary soak uses the complete base rules. Run the memory gate in a native process with the heap-profile feature (the xtask wrapper selects this automatically).
