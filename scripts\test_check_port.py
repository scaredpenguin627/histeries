#!/usr/bin/env python3
"""Tests for scripts/check_port.py and scripts/check-env.sh.

The checker tests: each case breaks a copy of the instruction pack in
one way and expects the checker to notice (or, for the harmless cases, not to).
The copy holds only the pack's own files, so the tests do not depend on what else
the repository contains (a .gitignore, a README, build output).

    python3 scripts/test_check_port.py
"""
from __future__ import annotations

import json
import os
import shutil
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
sys.dont_write_bytecode = True
sys.path.insert(0, str(ROOT / "scripts"))
import check_port  # noqa: E402

END = "<!-- END OF AGENTS.md -->"

try:
    import tomllib  # noqa: F401
    HAVE_TOML = True
except ModuleNotFoundError:
    HAVE_TOML = False


class CheckerTest(unittest.TestCase):
    def setUp(self) -> None:
        self._tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self._tmp.cleanup)
        self.repo = Path(self._tmp.name) / "repo"
        for rel in check_port.REQUIRED + check_port.ONE_SHOT:
            src = ROOT / rel
            if src.is_file():
                (self.repo / rel).parent.mkdir(parents=True, exist_ok=True)
                shutil.copy2(src, self.repo / rel)

    # -- helpers -------------------------------------------------------------
    def run_check(self, *flags: str, quick: bool = True) -> tuple[int, str]:
        cmd = [sys.executable, str(self.repo / "scripts" / "check_port.py"), "--root", str(self.repo), *flags]
        if quick:
            cmd.append("--quick")
        env = dict(os.environ, PYTHONDONTWRITEBYTECODE="1")
        result = subprocess.run(cmd, capture_output=True, text=True, env=env, timeout=300)
        return result.returncode, result.stdout

    def edit(self, rel: str, fn) -> None:
        path = self.repo / rel
        path.write_text(fn(path.read_text(encoding="utf-8")), encoding="utf-8", newline="")

    def append(self, rel: str, text: str) -> None:
        self.edit(rel, lambda s: s + text)

    def assertFails(self, needle: str, *flags: str) -> None:
        code, out = self.run_check(*flags)
        self.assertEqual(code, 1, out)
        self.assertIn(needle, out)
        self.assertIn("RESULT: FAILED", out)

    # -- the clean tree ------------------------------------------------------
    def test_clean_tree_passes(self) -> None:
        code, out = self.run_check()
        self.assertEqual(code, 0, out)
        self.assertIn("RESULT: ok", out)
        self.assertNotIn("FAIL", out)

    def test_strict_turns_warnings_into_failures(self) -> None:
        code, out = self.run_check("--strict")  # .gitignore is not shipped, so there are warnings
        self.assertEqual(code, 1, out)
        self.assertIn(".gitignore is missing", out)

    def test_gitignore_and_gitattributes_silence_the_warnings(self) -> None:
        (self.repo / ".gitignore").write_text("target/\n.codex/state/\n__pycache__/\n*.pyc\n")
        (self.repo / ".gitattributes").write_text("* text=auto eol=lf\n")
        code, out = self.run_check("--strict")
        self.assertEqual(code, 0, out)

    # -- AGENTS.md -----------------------------------------------------------
    def test_missing_end_marker(self) -> None:
        self.edit("AGENTS.md", lambda s: s.replace(END, ""))
        self.assertFails("last line must be")

    def test_oversize_agents_md(self) -> None:
        self.edit("AGENTS.md", lambda s: s.replace(END, "x" * 7600 + "\n\n" + END))
        self.assertFails("limit for this file")

    def test_large_agents_md_only_warns(self) -> None:
        self.edit("AGENTS.md", lambda s: s.replace(END, "x" * 4100 + "\n\n" + END))
        code, out = self.run_check()
        self.assertEqual(code, 0, out)
        self.assertIn("getting close", out)

    def test_clean_room_sentence_changed(self) -> None:
        self.edit("AGENTS.md", lambda s: s.replace("decompiled or disassembled code", "decompiled code"))
        self.assertFails("policy text changed")

    def test_no_git_sentence_removed(self) -> None:
        self.edit("AGENTS.md", lambda s: s.replace("You never run `git` yourself", "Avoid git"))
        self.assertFails("policy text changed")

    def test_section_removed(self) -> None:
        self.edit("AGENTS.md", lambda s: s.replace("## 15. Context hygiene", "## Context hygiene"))
        self.assertFails("missing section(s): 15")

    # -- skills, hooks, agents -----------------------------------------------
    def test_missing_skill(self) -> None:
        shutil.rmtree(self.repo / ".agents" / "skills" / "review-change")
        self.assertFails("review-change")

    def test_skill_name_must_match_directory(self) -> None:
        self.edit(".agents/skills/session-start/SKILL.md", lambda s: s.replace("name: session-start", "name: start", 1))
        self.assertFails("must equal the directory name")

    def test_skill_without_frontmatter(self) -> None:
        self.edit(".agents/skills/session-start/SKILL.md", lambda s: s.split("---", 2)[2])
        self.assertFails("no YAML frontmatter")

    def test_invalid_hooks_json(self) -> None:
        self.append(".codex/hooks.json", "}")
        self.assertFails("is not valid JSON")

    def test_hook_script_missing(self) -> None:
        (self.repo / ".codex" / "hooks" / "guard_git.py").unlink()
        self.assertFails("guard_git.py does not exist")

    def test_hook_command_must_name_a_script(self) -> None:
        path = self.repo / ".codex" / "hooks.json"
        data = json.loads(path.read_text())
        data["hooks"]["SessionStart"][0]["hooks"][0]["command"] = "echo hi"
        path.write_text(json.dumps(data))
        self.assertFails("does not run a script")

    @unittest.skipUnless(HAVE_TOML, "needs Python 3.11+ for tomllib")
    def test_agent_toml_missing_field(self) -> None:
        self.edit(".codex/agents/reviewer-sol.toml",
                  lambda s: s.replace("developer_instructions", "developer_notes"))
        self.assertFails("developer_instructions")

    @unittest.skipUnless(HAVE_TOML, "needs Python 3.11+ for tomllib")
    def test_invalid_toml(self) -> None:
        self.append(".codex/agents/reviewer-sol.toml", "\n[[[\n")
        self.assertFails("reviewer-sol.toml")

    # -- scripts -------------------------------------------------------------
    def test_crlf_shell_script(self) -> None:
        path = self.repo / "scripts" / "check-env.sh"
        path.write_bytes(path.read_bytes().replace(b"\n", b"\r\n"))
        self.assertFails("CRLF")

    def test_python_syntax_error(self) -> None:
        self.append(".codex/hooks/pre_compact.py", "\ndef broken(:\n")
        self.assertFails("pre_compact.py")

    @unittest.skipUnless(shutil.which("bash"), "bash not available")
    def test_shell_syntax_error(self) -> None:
        self.append("scripts/check-env.sh", "\nif then fi\n")
        self.assertFails("bash -n failed")

    # -- leftovers and references --------------------------------------------
    def test_stale_model_name_in_instructions(self) -> None:
        self.append("docs/prompts/session.md", "\nUse Fable for class A.\n")
        self.assertFails("'Fable'")

    def test_stale_jj_flag(self) -> None:
        self.append(".agents/skills/session-finish/SKILL.md", "\njj git push --allow-new\n")
        self.assertFails("'--allow-new'")

    def test_stale_string_outside_instructions_only_warns(self) -> None:
        (self.repo / "README.md").write_text("Built with Opus.\n")
        code, out = self.run_check()
        self.assertEqual(code, 0, out)
        self.assertIn("README.md:1: 'Opus'", out)

    def test_historic_session_reports_are_not_scanned(self) -> None:
        (self.repo / "docs" / "sessions" / "2026-10-01-old.md").write_text("Written by Fable.\n")
        code, out = self.run_check()
        self.assertEqual(code, 0, out)
        self.assertNotIn("2026-10-01-old", out)

    def test_dangling_section_reference(self) -> None:
        self.append("docs/prompts/session.md", "\nSee AGENTS.md section 99.\n")
        self.assertFails("section 99 does not exist in AGENTS.md")

    def test_dangling_reference_to_codex_cloud_doc(self) -> None:
        self.append("docs/prompts/session.md", "\nSee docs/codex-cloud.md section 12.\n")
        self.assertFails("section 12 does not exist in docs/codex-cloud.md")

    def test_dangling_subsection_in_a_list(self) -> None:
        self.append("docs/prompts/session.md", "\nRead AGENTS.md sections 3, 10 and 3.9.\n")
        self.assertFails("section 3.9 does not exist")

    def test_dangling_path_reference(self) -> None:
        self.edit("AGENTS.md", lambda s: s.replace(END, "See `.agents/skills/nope/SKILL.md`.\n\n" + END))
        self.assertFails(".agents/skills/nope/SKILL.md, which does not exist")

    def test_one_shot_prompts_may_be_deleted(self) -> None:
        for name in ("bootstrap.md", "port.md"):
            (self.repo / "docs" / "prompts" / name).unlink(missing_ok=True)
        code, out = self.run_check()
        self.assertEqual(code, 0, out)

    def test_required_file_missing(self) -> None:
        (self.repo / "docs" / "sessions" / "TEMPLATE.md").unlink()
        self.assertFails("missing docs/sessions/TEMPLATE.md")

    # -- jj ------------------------------------------------------------------
    @unittest.skipUnless(shutil.which("jj"), "jj not available")
    def test_generated_state_tracked_by_jj(self) -> None:
        subprocess.run(["jj", "git", "init"], cwd=self.repo, capture_output=True, check=True)
        state = self.repo / ".codex" / "state"
        state.mkdir(parents=True, exist_ok=True)
        (state / "compact-snapshot.md").write_text("x\n")
        # the full run (not --quick) looks at what jj tracks; hook tests also run
        code, out = self.run_check(quick=False)
        self.assertEqual(code, 1, out)
        self.assertIn("tracked by jj but must not be committed", out)
        (self.repo / ".gitignore").write_text(".codex/state/\n__pycache__/\n")
        subprocess.run(["jj", "file", "untrack", ".codex/state/compact-snapshot.md"],
                       cwd=self.repo, capture_output=True)
        code, out = self.run_check(quick=False)
        self.assertEqual(code, 0, out)


BASH = shutil.which("bash")


@unittest.skipUnless(BASH, "bash not available")
class CheckEnvTest(unittest.TestCase):
    """scripts/check-env.sh must never print RESULT: ok while a required tool is absent or broken."""

    SCRIPT = ROOT / "scripts" / "check-env.sh"

    def setUp(self) -> None:
        self._tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self._tmp.cleanup)
        self.base = Path(self._tmp.name)
        self.home = self.base / "home"
        self.bin = self.home / ".local" / "bin"  # the script puts this first on PATH
        self.bin.mkdir(parents=True)

    def fake(self, name: str, body: str, directory: Path | None = None) -> None:
        path = (directory or self.bin) / name
        path.write_text("#!/bin/sh\n" + body + "\n")
        path.chmod(0o755)

    def run_script(self, script: Path | None = None, path: str | None = None):
        env = dict(os.environ, HOME=str(self.home))
        if path is not None:
            env["PATH"] = path
        return subprocess.run([BASH, str(script or self.SCRIPT)], capture_output=True, text=True,
                              env=env, cwd=self.base, timeout=60)

    def fake_toolchain(self, rustc: str = 'echo "rustc 1.99.0"', jj_configured: bool = True) -> None:
        config = ('if [ "$1" = config ] && [ "$2" = get ]; then case "$3" in ui.editor) echo false;; '
                  'user.email) echo agent@histeries.invalid;; esac; exit 0; fi; ') if jj_configured else \
                 'if [ "$1" = config ]; then exit 1; fi; '
        self.fake("jj", config + 'echo "jj 0.46.0"')
        self.fake("python3", 'echo "Python 3.12.0"')
        self.fake("rustc", rustc)
        self.fake("cargo", 'case "$1" in nextest|deny|about) echo "error: no such command: $1" >&2; exit 101;; '
                           '*) echo "cargo 1.99.0";; esac')

    def test_all_required_present(self) -> None:
        self.fake_toolchain()
        result = self.run_script()
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual(result.stdout.strip().splitlines()[-1], "RESULT: ok")
        self.assertIn("rustc          rustc 1.99.0", result.stdout)
        self.assertIn("nextest        MISSING", result.stdout)  # recommended tools do not fail the check

    def test_jj_config_line(self) -> None:
        self.fake_toolchain()
        self.assertIn("jj config      ok", self.run_script().stdout)
        self.fake_toolchain(jj_configured=False)
        result = self.run_script()
        self.assertIn("jj config      NOT SET", result.stdout)
        self.assertEqual(result.returncode, 0, "an unconfigured jj is reported, not fatal")

    def test_a_required_tool_that_fails_is_missing(self) -> None:
        self.fake_toolchain(rustc='echo "rustup could not choose a version of rustc to run" >&2; exit 1')
        result = self.run_script()
        self.assertEqual(result.returncode, 1, result.stdout)
        self.assertIn("rustc          MISSING", result.stdout)
        self.assertIn("a required tool is missing", result.stderr)
        self.assertNotIn("RESULT: ok", result.stdout)

    def test_an_absent_required_tool_is_missing(self) -> None:
        # Run with a PATH that holds only fakes plus head and tr, and without the script's PATH line,
        # so that cargo really is absent whatever is installed on this machine.
        bare = self.base / "bare"
        bare.mkdir()
        try:
            for tool in ("head", "tr"):
                (bare / tool).symlink_to(shutil.which(tool))
        except (OSError, TypeError):
            self.skipTest("cannot link coreutils into a bare PATH here")
        for name, body in (("jj", 'echo "jj 0.46.0"'), ("rustc", 'echo "rustc 1.99.0"'),
                           ("python3", 'echo "Python 3.12.0"')):
            self.fake(name, body, bare)
        text = self.SCRIPT.read_text()
        stripped = text.replace('export PATH="$HOME/.local/bin:$HOME/.cargo/bin:/usr/local/bin:$PATH"', ":")
        self.assertNotEqual(text, stripped, "the PATH line moved; update this test")
        script = self.base / "check-env-bare.sh"
        script.write_text(stripped)
        result = self.run_script(script, path=str(bare))
        self.assertEqual(result.returncode, 1, result.stdout)
        self.assertIn("cargo          MISSING", result.stdout)
        self.assertNotIn("not found", result.stdout)  # an error message is not a version
        self.assertNotIn("RESULT: ok", result.stdout)

    def test_target_line_reports_what_is_lacking(self) -> None:
        self.fake_toolchain()
        self.fake("rustup", 'echo "x86_64-unknown-linux-gnu"; echo "wasm32-unknown-unknown"')
        result = self.run_script()
        self.assertEqual(result.returncode, 0, result.stdout)
        self.assertIn("ohos/wasm32    MISSING: aarch64-unknown-linux-ohos x86_64-unknown-linux-ohos", result.stdout)
        self.fake("rustup", 'echo "aarch64-unknown-linux-ohos"; echo "x86_64-unknown-linux-ohos"; echo "wasm32-unknown-unknown"')
        result = self.run_script()
        self.assertIn("ohos/wasm32    ok", result.stdout)


if __name__ == "__main__":
    unittest.main(verbosity=1)
