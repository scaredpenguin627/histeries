# P1-12 — Self-play verification

- Change: `znoplwkn` on `task/phase1-selfplay-gates`
- Base: P1-11 at `7f460f12` on `task/phase1-ai`
- Decision: ADR-0011 defines a round boundary, restores seat controller objectives with the world snapshot, and separates ordinary games from fixed-length endurance fixtures.

## What changed

The CLI runs headless multi-seat games in two independent sessions. After each complete round, or a terminal partial round, it compares canonical state hashes, verifies the decoded save and later verifies the corresponding range of commands from the decoded replay. A separate restored session plans and plays from a saved checkpoint to the finish. The referee checks that each observed military unit in contact receives an explicit order.

The first run exposed a valid state rejected by the save decoder: a fully funded production order waiting for a free site. The decoder now accepts deferred units and districts, including the interval between a tile becoming free and the next production update. A regression test saves in that interval and verifies both sessions complete the order.

The memory probe drains delivered events, hands off replay segments and compares live game allocations with a fresh subprocess at each checkpoint. A CLI run that requests a replay retains its complete recording. The heap and timing budgets, including the counted entities, are in `docs/budgets.md`. Third-party notices were regenerated for the additional measurement dependencies.

## Results

On Rust 1.100.0, Windows x86_64 GNU, the uninterrupted soak (`cargo xtask soak --games 1000 --jobs 2`) completed in 235.317 s. All 1,000 unique seeds reached a winner, all by science victory. Seats ranged from two through eight. All 52,706 round-boundary hashes matched across one and eight requested workers, decoded saves and serialized replays. The restored controllers matched on all 20,418 continuation rounds. The referee counted 159,964 contact-unit orders, with no missing orders. A separate 968-game run plus its 32-game continuation independently reproduced the deterministic results.

Five 8-seat Large fixtures each ran 500 rounds. All 85 comparison checkpoints from turn 100 through 500 matched the fresh-process world hash and entity count. Peak live density across the five games was 300.008 B/entity. The maximum retained surplus and live bytes after dropping the game were both zero. These are live heap measurements at the specified boundaries, not process RSS.

The local fast gate passed in 83.735 s, including 88 nextest tests, full cargo-deny, strict Clippy and formatting, both OHOS targets, wasm32 and six fixed-length games. The local fast fixture checked 1,200 rounds. Its worst per-game two-configuration p95 was 31.320 ms against the 250 ms budget. An initial cargo-deny advisory refresh failed on a TLS handshake; the complete gate was rerun successfully with the standard advisory fetch. A negative comparator test rejected a one-bit change to an intermediate hash. Hosted Tier A CI compares complete round traces across Linux, macOS and Windows.

Two 16-seat Huge seeds also ran 1,000 rounds uninterrupted. Each seed stopped at round 900 in a separate run, then continued in a new process from persisted checkpoints. Both recovered traces matched all 1,000 uninterrupted hashes. The final 100 rounds took 26.0 and 27.2 seconds respectively. The recovered runs stayed within the 40 MiB save limit and 2-second maximum planning time. Weekly CI persists boundaries at rounds 300, 600 and 900.

## Review and publication

Independent cross-model review requested durable weekly checkpoints and weekly maximum save and latency thresholds. Re-review returned **APPROVE** after the fixes and the two full-size recovery tests. The GitHub Actions cache configuration was inspected locally; hosted recovery is a separate execution. The [scorecard](../scorecard.md) records the measured totals and links to the evidence archive.
