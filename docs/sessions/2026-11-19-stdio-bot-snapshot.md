# Shared-snapshot planning for stdio opponents

The Phase 1 retrospective found that stdio opponents observed the board one at a time after earlier bots had already moved. This violated the single-snapshot planning rule in AGENTS.md §6.5. On a human turn end or saved-turn resume, the MCP host now obtains each pending bot's fog-filtered observation before applying any bot command. The AI crate's existing parallel planner computes the plans. The session dispatches them in ascending seat order using the same validation, replay and winner handling as other MCP commands.

The regression launches the real stdio server with one human seat and two greedy opponents. Both warriors plan to enter tile 1. The original server let seat 3 attack the warrior that seat 2 had just moved, reducing it to 90 HP. The fixed server applies seat 2's move and rejects seat 3's contested move. The warriors finish on tiles 1 and 2 with 100 HP each. The test checks both normal advancement and resumption with the human turn already ended, then checks the replay for each outcome.

All seven MCP wire tests passed, including both external-agent transcripts and the HTTP permission and victory checks. This change implements the existing planning rule and does not introduce a new ADR. The delivery bookmark is `task/stdio-bot-snapshot`.

The first Opus 5.5 review requested a second seat-status check before dispatch. The final change rechecks each seat after earlier plans have run and explicitly tests that the baseline planner selects both contested moves and the attack caused by sequential replanning.

On the revised change the local strict `cargo xtask ci-fast` gate passed in 294.957 s against a 900 s budget. All 130 tests and 34 fixture assertions passed, along with formatting, lint, dependency layers, cargo-deny, both OHOS targets and wasm32. All 1,200 round hashes matched on the six fast fixtures.
