# Prompt: Phase 0 bootstrap (run once, on a new project)

Start a task with **Work in > Cloud**, the Histeries environment, branch `main`, model **gpt-6-astra** (this task includes the architecture ADRs). Paste everything inside the block.

```text
Histeries Phase 0 bootstrap: scaffold the repository and stop before any feature code.

Context: this is a fresh Codex Cloud task in the published Histeries environment, and the repository is checked out. AGENTS.md at its root is in force; read it in full first, then follow its section 0 (run the session-start skill). Version control is jj only (AGENTS.md section 10): never run git; the maintainer opens the pull request from your jj commits.

Already in the repository, do not rewrite except where this prompt says: AGENTS.md; .agents/skills/; .codex/; scripts/ (codex-cloud-setup.sh, check-env.sh, check_port.py, test_hooks.py, test_check_port.py); docs/codex-cloud.md; docs/ARCHITECTURE.md and docs/ROADMAP.md (seeds you will complete); docs/prompts/; docs/sessions/TEMPLATE.md and TEMPLATE-handover.md.

Do these in order, each as its own described jj commit:
1. Workspace. A Cargo workspace with the nine empty crates from AGENTS.md section 5 (directories crates/hex, rules-core, ruleset, sim, ai, net, mcp, cli, client; package names histeries-hex, histeries-rules-core, and so on), `#![forbid(unsafe_code)]` in all but client. rust-toolchain.toml pinned to the version `rustc --version` reports in this workspace, with the targets aarch64-unknown-linux-ohos, x86_64-unknown-linux-ohos and wasm32-unknown-unknown listed. rustfmt.toml, clippy.toml, deny.toml (license classes from AGENTS.md section 3.3), about.toml, .cargo/config.toml (an `xtask` alias), .config/nextest.toml, .editorconfig. .gitignore containing at least: target/, .codex/state/, __pycache__/, *.pyc, .DS_Store. .gitattributes containing: * text=auto eol=lf
2. xtask. `cargo xtask layers`: parses the workspace graph and fails on any dependency that breaks the direction in AGENTS.md section 5, on any windowing or GPU crate below client, and on any -sys crate below client that is not on an allow-list file; implemented, with tests, and green. `cargo xtask ci-fast`: layers + fmt + clippy + deny + nextest + cargo check of the OHOS and wasm32 targets for the crates AGENTS.md section 4 names; it skips the client crate unless given --all.
3. CI. .github/workflows/pr.yml, nightly.yml, weekly.yml, release.yml per AGENTS.md section 8 (nightly and weekly may only run the fast gate for now), CODEOWNERS, pull-request and issue templates.
4. Docs. Complete docs/ARCHITECTURE.md (data flow, determinism and threading model; keep the seeded layout). Write docs/CLEANROOM.md (AGENTS.md section 3.1 as an auditable checklist), docs/scorecard.md (empty table with the CI-measured columns), docs/budgets.md (initial budgets marked provisional), docs/tooling.md (the pins from scripts/codex-cloud-setup.sh and how to rerun it; VS Code for local reading with rust-analyzer, Even Better TOML, Dependi, CodeLLDB, Error Lens, markdownlint and a Jujutsu extension whose Marketplace ID you confirm; Windows contributors need only the MSVC Build Tools), docs/assets.md (Blender 5.2 LTS, glTF 2.0, licensing), and .vscode/extensions.json.
5. Repository files. README.md, CONTRIBUTING.md (clean-room rules; agents add `Assisted-by`, the maintainer adds `Signed-off-by` when merging), CODE_OF_CONDUCT.md, SECURITY.md, CHANGELOG.md, ATTRIBUTION.md, LICENSE-MIT, LICENSE-APACHE, THIRD_PARTY_LICENSES.md generated with cargo-about, data/LICENSE (CC-BY-4.0), assets/LICENSES.md.
6. Roadmap. Keep the phases in docs/ROADMAP.md; break Phase 1 into PR-sized tasks, each labelled class A, B or C (AGENTS.md section 11); tick the Phase 0 tasks you complete.
7. ADRs. docs/adr/0001 to 0005, status Proposed, each with options, a recommendation and consequences: topology trait shape and coordinate policy; RNG; fixed-point format; entity storage; ruleset and modifier language.
8. Finish with the session-finish skill: run `cargo xtask ci-fast` and `python3 scripts/check_port.py`, write the session report, close with `jj new`, and end with the final message the skill describes.

Decide everything else yourself and record it. Ask only if blocked. If a gate cannot run in this workspace (disk, time, network), say which and why instead of weakening it.
```

After it finishes: open the pull request, run a review task on a different model (`docs/prompts/review.md`), merge, then **Republish** the environment (the pinned toolchain in `rust-toolchain.toml` is then preinstalled).
