# Prompt: port an existing repository from Claude to Codex (run once)

Use this instead of `bootstrap.md` when the repository was already scaffolded under Claude. First upload this package's `repo/` contents over the repository root on GitHub, **except** `.gitignore` and `.gitattributes` if you have your own (this package ships neither). Then start a task with **Work in > Cloud**, the Histeries environment, branch `main`, model **gpt-6.1-sol**. Paste everything inside the block.

```text
Port task for Histeries: the repository was scaffolded under another agent family. Switch its instructions to Codex Cloud, and change nothing else.

AGENTS.md has just been replaced by the Codex version; follow its section 0 (session-start skill) and section 10 (jj only; never run git).

1. Delete the .claude/ directory. Keep CLAUDE.md only if its sole content is the single line `@AGENTS.md`; otherwise delete it.
2. .gitignore: add only the lines that are missing from this list: .codex/state/, __pycache__/, *.pyc. .gitattributes: add `* text=auto eol=lf` if no eol rule exists.
3. docs/tooling.md: replace any git-based install or workflow text for agents with a pointer to scripts/codex-cloud-setup.sh (which pins jj) and AGENTS.md section 10; keep everything else. CONTRIBUTING.md: agents add the trailer `Assisted-by: Codex (<model slug>)`; the maintainer adds `Signed-off-by` when merging; remove any instruction that tells agents to add Signed-off-by.
4. docs/ROADMAP.md and the docs/adr files: replace model names (Fable, Opus, Sonnet) with task classes A, B and C as defined in AGENTS.md section 11. Do not change task content.
5. Search the whole repository for leftovers: ".claude", "CLAUDE.md", "Fable", "Opus", "Sonnet", "allow-new", "git clone", "non-colocated", "reviewer-fable", "reviewer-opus". Ignore matches inside docs/prompts/port.md and scripts/check_port.py, which name them on purpose. Fix each other match, or list it in the session report with the reason it stays. Check .github/workflows for steps that reference the removed files.
6. Run `python3 scripts/check_port.py` and, if it exists, `cargo xtask ci-fast`. Write the session report (docs/sessions/TEMPLATE.md), close with `jj new`, and end with the final message the session-finish skill describes.
```

Then review the result with `docs/prompts/review.md` on the other model, merge, and **Republish** the environment.
