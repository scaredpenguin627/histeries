# Independent review model selection

The maintainer requested that every new review and every review after fixes start with Claude Fable 5.2 at maximum effort. Section 11 now defines the conditions for using Opus 5.5, the evidence that makes a verdict valid, and the handling of changes requested by either eligible reviewer.

A local routing check exercised three successive requests. Fable returned a valid request for changes on the first. The second fell back to Opus after Fable failed to deliver a verified verdict. The third started with Fable again. A separate check rejected a response in which Fable emitted a thinking event but Opus authored the verdict.

The first local Windows GNU fast gate ran the existing 140 workspace tests and 34 parity checks successfully. The six simulation fixtures matched all 1,200 round hashes. The run took 3,826.805 seconds, including 53 minutes and 37 seconds to build tests with debug information while three other workspaces were compiling. It exceeded the 900-second budget.

On the same Windows host, the complete warm-cache gate on `4c359ce8c9d4` passed in 55.103 seconds. It ran all 140 workspace tests, 34 parity checks and 1,200 round hashes before the first independent review. The final revision and gate log are recorded with the independent review receipt.

OpenAI Codex authored the submitted diff. Neither Claude family contributed changes to it. The project reviewer definitions pin both model IDs, maximum effort and the 40-turn limit.

The game scorecard has no new feature measurement from this procedure change. The delivery bookmark is `task/fable-first-review`.
