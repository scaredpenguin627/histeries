# Engineering budgets

These initial values are provisional until CI records a baseline. A regression fails the corresponding gate after the maintainer accepts a measured baseline.

| Budget | Provisional target | Gate |
| --- | ---: | --- |
| Fast gate wall time | 15 minutes on 4 vCPU | Pull request |
| Nightly wall time | 2 hours per shard | Nightly |
| Weekly shard wall time | 6 hours | Weekly |
| AI turn-time p95 | 250 ms on the fast fixture | Fast |
| Save size | 10 MiB on the Large fixture | Weekly |
| Memory slope after turn 100 | 0 unbounded growth | Nightly |

Measured values belong in `docs/scorecard.md`.
