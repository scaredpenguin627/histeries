---
name: review-change
description: Fresh-context, read-only review of a Histeries change (a pull-request branch) against the spec, ADRs and AGENTS.md. Use only in a dedicated review task, never on work you wrote yourself.
---

# Review a change

You are the reviewer. You did not write this change and have not seen how it was written; do not ask. Read-only: never edit files, never `jj describe`/`jj commit`/`jj git push`, never run `git`.

## 1. Set up
1. `bash scripts/check-env.sh`. If `.jj` is missing, run `jj git init` (colocated by default).
2. The branch under review is what the harness checked out: `@-` after init. The base is `main@origin`.

## 2. Gather (bounded; read files by range, never dump a whole large file)
1. `jj log -r 'main@origin..@-' --no-graph -T 'change_id.short() ++ " " ++ description.first_line() ++ "\n"'`
2. `jj diff --from main@origin --to @- --stat`, then the diff file by file.
3. The spec section(s) and ADRs the task names. AGENTS.md sections 3, 5, 6 and 8 are already in your instructions; re-read them if unsure.

## 3. Check, in this order; give file:line for every finding
1. **Provenance** — every new rule carries `observed`, `public-doc` or `designed`; gaps are `designed` rules with an ADR, not guesses at another game's behavior.
2. **Clean room** — no decompiled or disassembled code, no other game's data or scripts, no mod content, no trademarks in code, UI or docs (AGENTS.md 3.1–3.3).
3. **Determinism** — in `hex`, `rules-core`, `ruleset`, `sim`, `ai`: no floats, no `HashMap`/`HashSet` iteration, no wall clock, no unseeded RNG, nothing that depends on thread count.
4. **Layering** — dependency direction intact; no `-sys` or C dependency below `client`; `#![forbid(unsafe_code)]` kept.
5. **Tests** — they exist, exercise the change, and would fail without it; property tests where invariants apply; no fabricated fixtures.
6. **Docs** — ROADMAP tick, scorecard, CHANGELOG and session report present and true.
7. **History** — commit messages say what the commit does; `Assisted-by` trailer present; no agent-added `Signed-off-by`.
8. **Scope** — PR-sized, nothing unrelated.

## 4. Gates
If the workspace allows, run `cargo xtask ci-fast` and report the result. Do not fix anything.

## 5. Output
First line exactly `VERDICT: APPROVE` or `VERDICT: REQUEST CHANGES`. Then findings ordered by severity (blocker, should-fix, nit), each with file:line and the reason. Finish with `Not checked:` listing anything you could not verify. A REQUEST CHANGES verdict blocks merge.
