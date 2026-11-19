# Phase 2 verification

The Phase 2 scope in AGENTS.md section 9 calls for three-layer planning, Elo and significance reporting, a complete agent interface, a full game played through that interface and fixture-driven comparison. Phase 1 already supplied both MCP transports, the five tools, resources, player/editor/spectator permissions, a greedy controller and a first recorded external-agent game.

The new controller uses research infrastructure and visible capital threats in strategic utility. Operational planning can reserve a district and its dependent building in one multi-turn project. Tactical search models a ranged strike before an assault and evaluates combined move-and-attack or move-and-capture orders. The earlier controller remains a selectable baseline. [ADR-0017](../adr/0017-phase2-planning.md)

The tournament preserves seat swaps, records draws as half a point for relative Elo and reports an exact one-sided sign test on non-tied maps. On 32 reserved maps, the candidate won 44/64 against its predecessor, 60/64 against greedy and 64/64 against random. The 192 games reproduced with eight requested workers. [Tournament evidence](2026-11-19-phase2-tournament.txt)

A new external-agent game used seed 20261120. Seat 1 issued 56 accepted orders and received six gameplay rejections. The final observation and replay both identified seat 2's science victory on turn 57. New subprocess tests run both real agent transcripts across a fresh stdio server. HTTP and authorization tests run alongside them. [MCP guide](../mcp.md)

Four independently derived fixtures exercise housing-limited growth, escort and builder mechanics, private observations, and conquest. The initial comparison passed all 34 assertions. Observed reference coverage is zero because the maintainer supplied no external measurements. [Scorecard](../scorecard.md)

The official SDK pin from ADR-0014 still serves both transports. As a migration comparison, an isolated Cargo resolution of community `rust-mcp-sdk = 2.0.0` (defaults off, server, stdio, streamable-http) with `rust-mcp-axum = 2.0.0` selected 210 packages and contained no Chrono. A migration would require a stack decision, porting the server, and exercising protocol and platform gates. [SDK feature list](https://docs.rs/crate/rust-mcp-sdk/2.0.0/features) · [Axum adapter](https://docs.rs/crate/rust-mcp-axum/2.0.0)

Local strict `cargo xtask ci-fast` passed in 258.121 seconds with 129 tests, 34 reference checks, cargo-deny, both OHOS checks, wasm32 and all six fast fixtures. The ordinary 1,000-game soak matched at all 40,414 round boundaries and all 84,053 contact orders were present. [Measured results](2026-11-19-phase2-gates.zip)

The delivery bookmark is `task/phase2-verification`.

Independent review found that the walking-distance prefilter could suppress a ranged strike across water after moving. V2 now checks topological range. The regression dispatches two moves and two legal shots across an intervening ocean tile. After the correction, all 192 control tournament results reproduced, including hashes. A new 128-game confirmation on untouched seeds produced 45/64 wins against v1 and 60/64 against greedy.

The independent GPT-6.2 Astra reviewer returned APPROVE after the fix and the final fast gate. The pull request records the external-model review and hosted gate status.
