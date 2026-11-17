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
