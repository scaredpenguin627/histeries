# Codex Cloud: how Histeries runs there

Checked against OpenAI's Codex documentation and by tests on 2026-10-24. **Verified** means it comes from those docs or from a test run while this setup was built. **Unverified** means it could not be checked from outside Codex; the smoke tests in `docs/prompts/smoke-tests.md` settle each one, and section 7 says what to do when one fails.

## 1. Assumptions and their status

| # | Assumption | Status |
|---|---|---|
| 1 | A published environment holds repositories, an install script, a start skill, variables and network secrets. Publishing captures the prepared filesystem; each new task gets its own isolated workspace from it; a repository refresh at task start does **not** rerun installation. | Verified (Cloud environments guide) |
| 2 | Agent internet access is off unless enabled; the "Package managers" preset allows crates.io, index.crates.io, static.crates.io, static.rust-lang.org, github.com, codeload.github.com and release-assets.githubusercontent.com. | Verified (internet-access docs) |
| 3 | `AGENTS.md` is loaded in cloud tasks, up to 32 KiB (`project_doc_max_bytes = 32768`, shared by every AGENTS.md from repo root to the working directory; overflow is dropped silently). Ours is about 21 KB. | Cap verified; loading in the current cloud flow unverified (smoke test 2) |
| 4 | Skills stored in the repository (`.agents/skills/<name>/SKILL.md`) are available in cloud tasks. | Verified |
| 5 | `.codex/hooks.json` loads in cloud tasks and can be trusted. Project hooks run only after review (`/hooks`) and only when the project `.codex/` layer is trusted. | Unverified (smoke test 6). Nothing depends on it |
| 6 | Custom agents in `.codex/agents/*.toml` work in cloud tasks. Docs describe them for local clients; an agent file cannot set its own sandbox. | Unverified (smoke test 7). Nothing depends on it |
| 7 | A cloud task's result reaches GitHub (the PR button) correctly when the checkout is a non-colocated jj repository. | Unverified (smoke test 4). Fallback F1 below |
| 8 | `rustup target add` for the OHOS and wasm32 targets works in the VM. The targets exist in rustc and are Tier 2 (OHOS: with host tools; wasm32-unknown-unknown: without). | Targets verified; installing from inside Codex unverified |
| 9 | `cargo check --target *-linux-ohos` needs no OpenHarmony SDK while the checked crates are pure Rust (linking is what needs the SDK). | Unverified: not run yet (the build sandbox could not reach static.rust-lang.org) |

## 2. One-time setup (about 15 minutes)

1. Repository first: it must exist on GitHub with this package's `repo/` contents in it, plus `seeds/` for a brand-new repository (see START-HERE.md).
2. In Codex: **Work in > Cloud > Select environment > Create environment** (or **Settings > Codex Cloud > Environments > Create environment**). Select the Histeries repository, then **Get started**.
3. **Install script.** Paste the whole of `scripts/codex-cloud-setup.sh`. It installs jj 0.46.0, cargo-nextest, cargo-deny and cargo-about from pinned releases (each SHA-256 checked), installs the Rust toolchain named in `rust-toolchain.toml` (or `stable` before that file exists) with the OHOS and wasm32 targets, writes agent-friendly jj defaults, and persists `PATH` through `~/.bashrc` and `~/.profile`. If the new setup flow generates its own script, replace it with this one. If it offers no install-script box at all, tell the setup agent: "Run `bash scripts/codex-cloud-setup.sh` as the install step."
4. **Start skill / start check.** There are no services. Use: *No services to start. Run `bash scripts/check-env.sh`; it must end with `RESULT: ok`.*
5. **Network.** Turn on **Allow Codex to access internet**, choose **Allow domains: Package managers**, add no extra domains. The agent needs GitHub and the Rust and crates hosts (to fetch dependencies and, if a pinned toolchain is missing, to install it). No network secrets are needed.
6. **Secrets and variables:** none. Never paste a token or password into a task, a prompt or a file.
7. Review the setup report, **Save**, then **Publish**. When "Environment published" appears, start the smoke tests.

Plan size matters: Plus and Edu Plus get 2 vCPU, 8 GiB RAM and 8 GiB disk; Pro, Business, Enterprise and Edu get 4 vCPU, 16 GiB RAM and 32 GiB disk. On the small size keep tasks to the crates below `client` (AGENTS.md section 8); the setup script already sets `CARGO_INCREMENTAL=0` and `CARGO_PROFILE_DEV_DEBUG=line-tables-only` to save disk.

## 3. Running a task

**Work in > Cloud**, pick the Histeries environment and the branch (normally `main`; for a review, the PR branch), pick the model (AGENTS.md section 11), and paste a prompt from `docs/prompts/` (`session.md` for work, `review.md` for review). When the task ends, read its final message, then use the PR button; after merging, the next task starts from the new `main` automatically.

## 4. Delivery: how work reaches GitHub

**Path A (default, nothing to configure).** The agent leaves its work as described jj commits and closes with `jj new`. In a non-colocated repository the described jj commits remain in the workspace and can be pushed with the task bookmark. The maintainer reviews the resulting GitHub pull request. No credential is stored in the workspace.

**Fallback F1 (only if smoke test 4 shows an empty PR).** The harness may look only at uncommitted changes. Then end tasks with this instead of `jj new`:
`jj squash --from 'main@origin..@-' --into @ -m "<summary>"`
(tested: all work becomes one uncommitted change on top of main, which is what a plain-git harness expects; the explicit `-m` is required because jj has no editor here). Change step 5 of `.agents/skills/session-finish/SKILL.md` and AGENTS.md section 10 to match, in the same PR.

**Path B (not recommended, untested).** The agent pushes `task/<slug>` itself with `jj git push`. That needs a credential inside the workspace. If you ever do it: a fine-grained token limited to this one repository (contents and pull requests only, short expiry), stored as a network secret restricted to github.com, a branch ruleset on `main`, and a throwaway-repository test first: the secret proxy substitutes placeholders in HTTPS requests, and whether git's Basic-auth header survives that was not tested.

## 5. When to Republish the environment

Republish after changing: any pin in `scripts/codex-cloud-setup.sh`; the `channel` in `rust-toolchain.toml` (so the toolchain is preinstalled instead of fetched on first build); the target list. Not needed for changes to AGENTS.md, skills, docs or code: a repository refresh brings those in at task start. Existing tasks keep their own saved state (recoverable for up to 7 days); saved state is not source control, so merge what matters.

## 6. What the repository files do here

- `AGENTS.md`: always-on rules. The only file every session is guaranteed to read.
- `.agents/skills/`: `session-start`, `session-finish`, `review-change`. AGENTS.md also names their paths, so an agent that cannot see skills can still open them.
- `scripts/check-env.sh`: read-only tool check used by the skills and the start check. `scripts/check_port.py`: checks this setup (AGENTS.md size, end marker and policy sentences; section and file references; skill front matter; JSON and TOML validity; leftovers from the previous agent family; generated files tracked by jj; the hook tests). `scripts/test_hooks.py` and `scripts/test_check_port.py`: the tests for the hooks and for the checker itself.
- `.codex/`: optional guardrails. `guard_git.py` denies shell commands that run `git`; `session_context.py` restates project state on start, resume and compact; `pre_compact.py` writes `.codex/state/compact-snapshot.md` (git-ignored) before compaction; `reviewer-astra.toml` and `reviewer-sol.toml` are subagent definitions. Every hook command fails open (exit 0) when the repository, the script or python3 cannot be found, so a hook can never stall a session. If they never load in cloud, nothing is lost.

## 7. If a smoke test fails

| Test | Failure | What to do |
|---|---|---|
| 1 Tools | a tool is missing | Fix `scripts/codex-cloud-setup.sh` (read the setup log), Republish, rerun. |
| 2 Rules loaded | last line `<!-- END OF AGENTS.md -->` is not quoted | The file was cut or not loaded. `wc -c AGENTS.md` must stay under 28,000; look for other AGENTS.md files in subdirectories (the cap is shared). |
| 2 Rules loaded | skills not listed | Acceptable: AGENTS.md names the skill files by path. |
| 3 Clean room | the agent explains or ports the snippet | Stop. Do not run any build task until fixed. Rerun on the other model; if both fail, tighten AGENTS.md section 3.1 and report it. |
| 4 jj and PR | PR is empty or lacks commits | Use fallback F1. |
| 4 jj and PR | the agent reports that `main@origin` does not exist | The checkout has no remote-tracking refs. The skills tell the agent to use the checked-out branch as the base; if it keeps stumbling, name the branch in the task prompt. |
| 4 jj and PR | PR contains `.jj` files | Add `.jj/` to `.gitignore` (it is self-ignored, so this should not happen). |
| 5 git refusal | the agent ran `git` | AGENTS.md is the control; hooks may not be active. Re-prompt with section 10 quoted; if repeated, add the rule to the task prompt. |
| 6 hooks / 7 subagent | not available | Nothing to do. The workflow does not use them. |

