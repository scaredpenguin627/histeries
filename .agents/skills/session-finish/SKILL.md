---
name: session-finish
description: End of every Histeries task. Runs the gates, updates roadmap, scorecard, changelog and session report, closes the jj history cleanly and prepares the pull-request text. Use before declaring a task done.
---

# Session finish

1. **Gates.** Run `cargo xtask ci-fast`. Before that command exists (Phase 0), run the checks the task prompt lists plus `python3 scripts/check_port.py`. Fix failures; never weaken a gate to make it pass. If a gate cannot run in this workspace (disk, time), say exactly which and why.
2. **Docs.** Tick the task in docs/ROADMAP.md. Update docs/scorecard.md for every number that moved (measured, never estimated). Add a CHANGELOG.md line. Record decisions as ADRs.
3. **Session report.** `docs/sessions/YYYY-MM-DD-<slug>.md`, from docs/sessions/TEMPLATE.md.
4. **History.** Each change is described in Conventional Commits form with the trailer from AGENTS.md section 10: `jj describe -m "<type>(<scope>): <summary>" -m "Assisted-by: Codex (<model slug>)"`. `jj log -r 'main@origin..@'` should read like a changelog; tidy with `jj squash` or `jj split`.
5. **Close.** If main moved while you worked: `jj git fetch`, then `jj rebase -o main@origin`. Then run `jj new` so the working copy is an empty change on top of the work. `jj st` must report "The working copy has no changes". Do not use git to check.
6. **Deliver.** Do not push unless the task prompt says "push is enabled". If it does: `jj bookmark set task/<slug> -r @-`, then `jj git push --bookmark task/<slug>`. Never push main, never force, never push a conflicted commit. Without push, the maintainer opens the PR from the commits above.
7. **Final message** (this is what the maintainer reads), at most 12 lines: what changed; gates run and results; scorecard deltas; open questions; the commit list from `jj log -r 'main@origin..@-' --no-graph -T 'change_id.short() ++ " " ++ description.first_line() ++ "\n"'`; a suggested PR title and body; and the closing line `NEXT: start a review task with a different model (docs/prompts/review.md).`
