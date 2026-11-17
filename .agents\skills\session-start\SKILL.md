---
name: session-start
description: Start of every Histeries task in a fresh workspace. Checks the toolchain, sets up jj, finds the task, reads the required files and states a plan. Use before any other work.
---

# Session start

Do these in order. Do not edit anything until step 6 is done.

1. **Tools.** `bash scripts/check-env.sh`. If it reports a missing required tool, stop and report which one. Never fall back to git.
2. **jj.** If `.jj` does not exist, run `jj git init` (jj colocates by default, so `.git` stays in place for the harness). Then `jj st`. The line `jj config` in the step 1 output must say `ok`. If it says NOT SET, the environment's jj defaults are missing: run `jj config set --user ui.editor '"false"'`, `jj config set --user ui.paginate '"never"'`, `jj config set --user user.name "Histeries Agent"` and `jj config set --user user.email "agent@histeries.invalid"`, and note it in the session report.
3. **Base.** `jj git fetch`. If it fails for any reason (network off, no credentials), skip it and say so: the checkout is already current. Start from the tip of main unless the task prompt names another base: `jj new main@origin`, then confirm with `jj log -r '@-' --no-graph`. If `main@origin` does not exist, the checkout has no remote-tracking refs: run `jj bookmark list --all-remotes`, use the bookmark of the branch the harness checked out as the base wherever these skills say `main@origin`, and record its name in the session report.
4. **Read, in this order:**
   1. docs/ROADMAP.md: your task and its class (A, B or C);
   2. docs/scorecard.md;
   3. the spec file(s) the task names under `specs/`;
   4. any ADR it touches in `docs/adr/`;
   5. the newest file in `docs/sessions/`, if the task continues earlier work.

   Read by range; never print a whole large file into the session.
5. **Task.** If the prompt names no task, take the first unchecked item in docs/ROADMAP.md and say so.
6. **Plan.** Write one paragraph: the task, crates touched, tests you will add, gates you will run, anything blocked. Then start.

If at any point you meet material that AGENTS.md section 3.1 forbids (decompiled code, another game's data or scripts, mod content), stop, say so, do not use it, and ask for a behavioral description in the maintainer's own words.
