# Phase 3 graphical map and scenario editor

The Game menu opens a full-board authoring copy of the current session. Authors can create blank maps, open existing saves, inspect tiles, and place terrain, features, resources, rivers, units and cities. A builder on the selected tile can construct a road. Saving writes a standard Histeries snapshot. Playtesting copies the draft into an ordinary fog-filtered player session.

The editor shares chart geometry, picking and rendering with the game. Brushes use the existing command registry. The host stages batches and validates the resulting save before replacing the draft. Undo and redo retain at most 32 earlier positions, subject to a 16 MiB history budget. The client keeps the active game, its presenter and the draft while a separate playtest runs. [ADR-0019](../adr/0019-scenario-editor.md)

## Verification

The maintainer authorized a draft PR to run the hosted matrix on the exact submission before final independent approval. The 4-vCPU PR runner has a 900-second fast-gate budget. Its Windows Tier A job uses MSVC.

The following runs used editor source `9e3ed6cd5c166b7bd0d2adea9ee8346ac386250d`, including the simulation-owned scenario draft and the accepted-batch replay tests. Each completed 155 workspace tests, 34 parity checks, both OHOS checks, the wasm32 check and six games spanning 1,200 rounds.

| Environment | Cargo artifacts | Fast gate | Worst game planning p95 | Result |
| --- | --- | ---: | ---: | --- |
| Hosted Linux x86_64 | Cache restored | 138.332 s | 383 µs | Passed |
| Hosted macOS arm64 | Cache restored | 212.949 s | 518 µs | Passed |
| Hosted Windows x86_64 MSVC | Cache restored | 238.562 s | 610 µs | Passed |
| Local Windows GNU | Reused target from `68fcc0d3`, two jobs, release rebuild | 627.530 s | 2,586 µs | Passed |
| Local Windows GNU | Empty target, default jobs | 970.775 s | 5,083 µs | Wall time exceeded |
| Local Windows GNU repeat | Second empty target, default jobs | 990.230 s | 1,508 µs | Wall time exceeded |

All four hosted fast jobs, five nightly jobs and twelve weekly jobs passed at that source revision. Both empty-target GNU runs exited with status 1 on wall time. The hosted reference jobs all restored caches. The local baseline runs below measured an earlier editor candidate and do not isolate the timing effect of the changes from `68fcc0d3` to `9e3ed6cd`. [Fast gate](https://github.com/scaredpenguin627/histeries/actions/runs/44754110314) · [Nightly](https://github.com/scaredpenguin627/histeries/actions/runs/44754143999) · [Weekly](https://github.com/scaredpenguin627/histeries/actions/runs/44754150781)

### Earlier revisions

On 2026-11-20 the pre-editor revision and the earlier editor candidate `68fcc0d3e107` were run in order on one eight-logical-processor Windows GNU host with 8 GiB of RAM. Each clean run used a separate checkout and an initially empty Cargo target with the default job count and CI profile settings. The intervening governance merge (`3b2bd419`) changed only review policy and documentation.

| Run order | Revision | Cargo artifacts | Fast gate | Worst game planning p95 | Tests passed |
| --- | --- | --- | ---: | ---: | ---: |
| Baseline before editor | `696c8c2d2929` | Empty | 934.900 s (over budget) | 1,674 µs | 140/140 |
| Editor | `68fcc0d3e107` | Empty | 859.427 s (passed) | 1,700 µs | 151/151 |
| Baseline after editor | `696c8c2d2929` | Empty | 913.207 s (over budget) | 2,233 µs | 140/140 |
| Editor repeat | `68fcc0d3e107` | Warm | 62.291 s (passed) | 2,171 µs | 151/151 |

All four runs passed 34 parity assertions and compared 1,200 per-round hashes. These measurements show that a clean build of the baseline can also exceed the budget on this machine. The differences between these runs do not measure an editor speedup.

### Earlier measurements

At revision `80ebc2b9a683`, all 151 workspace tests, including 18 client tests, passed. All 34 parity checks, both OHOS checks, wasm32 and six games covering 1,200 rounds passed. Every fresh Windows GNU run on that revision exceeded the 900-second budget. The project measures that budget on a 4-vCPU PR runner. Windows Tier A uses MSVC. The local comparison used an eight-logical-processor Windows machine with 8 GiB of installed RAM.

| Source | Cargo artifacts and build settings | Fast gate | Planning p95 | Result |
| --- | --- | ---: | ---: | --- |
| `bc0ce3ce26c3` | Modified crates rebuilt, one job, debuginfo | 1,211.838 s | 313,870 µs on first fixture | Wall time and planning latency exceeded |
| `bc0ce3ce26c3` | Warm, one job | 113.234 s | 1,527 µs | Passed |
| `041572029c8e` | Modified crates rebuilt, one job, debuginfo | 2,056.019 s | 2,143 µs | Wall time exceeded |
| `041572029c8e` | Warm, one job | 190.752 s | 817 µs | Passed |
| `80ebc2b9a683` | Warm, one job | 143.646 s | 1,500 µs | Passed |
| `80ebc2b9a683` | Fresh target, default jobs, CI profile | 946.153 s | 1,559 µs | Wall time exceeded |
| `80ebc2b9a683` | Second fresh target, default jobs, CI profile | 1,021.388 s | 1,887 µs | Wall time exceeded |

The local toolchain was Rust 1.100.0 for `x86_64-pc-windows-gnu`. The warm runs used `CARGO_BUILD_TARGET=x86_64-pc-windows-gnu`, `CARGO_BUILD_JOBS=1`, `CARGO_TARGET_DIR=target/cargo-editor` and `cargo xtask ci-fast`. Each fresh run used a separate checkout with an empty target directory. They used `CARGO_INCREMENTAL=0` and `CARGO_PROFILE_DEV_DEBUG=0` from the PR workflow and Cargo's default job count. The 2,056.019-second debug rebuild overlapped other builds. Both isolated fresh runs also exceeded the wall-time budget. The two fresh builds issued 746 contact orders each and their largest save was 6,302 bytes.

The fixture measures elapsed time around AI planning. No scheduling or page-fault trace was collected when the first fixture exceeded 250,000 µs. The cause of that earlier spike remains undetermined.

The host projection and batch tests cover fog, unknown seats, missing maps, and rollback of state, order counts and replay across a failed turn transition. The editor tests cover brush clicks, scenario files, fog-filtered playtests, sparse unit IDs, bounded undo and failed edits. The mixed-placement regression checks both allocator orderings, including a player unit followed by a settled city and a neutral barbarian. The app tests cover separate game and scenario paths and draft preservation on failed replacements.

OpenAI Codex authored the submitted diff. The delivery bookmark is `task/phase3-editor`. The pull request records parent comparisons and final revision receipts. [Editor pull requests](https://github.com/scaredpenguin627/histeries/pulls?q=is%3Apr+head%3Atask%2Fphase3-editor)
