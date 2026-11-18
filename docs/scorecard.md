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

## Current implementation evidence

P1-01 supplies the topology API, wrapping behavior, deterministic ordering, integer embedding, A* heuristic, and seven unit tests. The fast gate currently runs ten tests, with CI remaining authoritative for published scorecard values.


P1-02 supplies checked scale-1,000 fixed-point operations, ChaCha8 named streams, unbiased bounded sampling, and versioned length-prefixed state hashing. The fast gate currently runs 15 tests.

P1-03 supplies typed entity IDs, six canonical yield channels, scope/tag/effect modifiers, and invariant tests for matching and non-negative yields. The fast gate currently runs 18 tests.

P1-04 supplies a versioned TOML schema, strict unknown-field rejection, provenance validation, deterministic BTreeMap tables, base units, and modifier conversion. The fast gate currently runs 22 tests.

P1-05 supplies the command registry, session state, seat-scoped observations, event stream, strict custom binary state and command documents, and replay round trips. The fast gate currently runs 25 tests.

P1-06 supplies deterministic map generation, terrain/features/rivers/resources, strict map encoding, and editor command coverage. The fast gate currently runs 30 tests.

