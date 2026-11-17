#!/usr/bin/env python3
"""Self-test for the Histeries Codex hooks and agent files.

Run:  python3 scripts/test_hooks.py        (needs only Python 3.11+ and, for the
end-to-end part, jj or git on PATH; those tests are skipped when neither exists)
"""
from __future__ import annotations

import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

try:
    import tomllib
except ModuleNotFoundError:  # Python < 3.11: the TOML test is skipped
    tomllib = None

ROOT = Path(__file__).resolve().parent.parent
HOOKS = ROOT / ".codex" / "hooks"
sys.dont_write_bytecode = True
sys.path.insert(0, str(HOOKS))
import guard_git  # noqa: E402

DENY = [
    "git status",
    'git commit -m "x"',
    "  git   log --oneline",
    "FOO=1 git diff",
    "/usr/bin/git status",
    "\\git status",
    "cd /tmp && git status",
    "echo hi; git reset --hard",
    "ls | git hash-object --stdin",
    "(git status)",
    "{ git status; }",
    "if git diff --quiet; then echo clean; fi",
    "x=$(git rev-parse HEAD)",
    'echo "$(git rev-parse HEAD)"',
    "echo `git log`",
    "bash -c 'git status'",
    'sh -lc "cd x && git add ."',
    "env GIT_DIR=. git status",
    "sudo git push",
    "time git status",
    "xargs git add < list",
    "nohup git gc &",
    "find . -name '*.rs' -exec git add {} +",
    'eval "git status"',
    "python3 -c \"import subprocess; subprocess.run(['git','status'])\"",
    "cargo build && git status",
    "cat <<'EOF' > x\nhello\nEOF\ngit status",
    "cd x\ngit status",
    "jj util exec -- git status",
    "! git diff --quiet",
    "timeout 60 git fetch",
    "sudo -u root git status",
    "timeout 30 bash -c 'git status'",
    "env -i FOO=1 bash -c 'git log'",
    "cargo test 2>&1 | tee out.txt; git diff",
    "echo $(echo $(git status))",
    "bash -c \"bash -c 'git status'\"",
    "sudo bash -c 'git status'",
    "case x in x) git status;; esac",
]

ALLOW = [
    "jj git push --bookmark task/x",
    "jj git fetch",
    "jj st",
    "jj log -r 'main@origin..@'",
    'jj describe -m "fix: handle git remote names" -m "Assisted-by: Codex (gpt-6.1-sol)"',
    'echo "do not run git status"',
    "echo git status",
    "echo 'git'",
    'grep -rn "git" docs/',
    "grep -n 'git commit' AGENTS.md",
    "rg git",
    "cat .gitignore",
    "ls -la .git",
    "cargo install --git https://example.com/x",
    "git-lfs --version",
    "command -v git",
    "which git",
    "type git",
    "cat > notes.md <<'EOF'\ngit status is banned\nEOF",
    "cat <<EOF > a\ngit status\nEOF\ncargo build",
    "gh pr view 1",
    "echo $(date)",
    "FOO=$(pwd) ls",
    "printf '%s\\n' git",
    "awk '/git/ {print}' file",
    "cargo build 2>&1 | tail -5",
    "cmd &> /dev/null",
    "sed -n '1,5p' docs/gitflow.md",
    "export PATH=$PATH:/x && cargo test",
    "tar -tzf a.tgz | head",
    "python3 scripts/foo.py --git",
    "python3 -c \"print('git is great')\"",
    "cargo test git_flow",
    "time cargo test git",
    "sudo apt-get install git",
    "bash scripts/check-env.sh",
    "bash -c 'cargo build'",
    "",
]


def hook_payload(event: str, **extra) -> str:
    base = {"session_id": "t", "transcript_path": None, "cwd": os.getcwd(),
            "hook_event_name": event, "model": "gpt-6.1-sol"}
    base.update(extra)
    return json.dumps(base)


class GuardUnit(unittest.TestCase):
    def test_denied_commands(self):
        for cmd in DENY:
            with self.subTest(cmd=cmd):
                self.assertIsNotNone(guard_git.analyze(cmd), f"should deny: {cmd!r}")

    def test_allowed_commands(self):
        for cmd in ALLOW:
            with self.subTest(cmd=cmd):
                self.assertIsNone(guard_git.analyze(cmd), f"should allow: {cmd!r}")

    def test_malformed_input_fails_open(self):
        for cmd in ['echo "unterminated; git status', "((((", "$(", "`", "<<"]:
            with self.subTest(cmd=cmd):
                guard_git.analyze(cmd)  # must not raise


class GuardProcess(unittest.TestCase):
    def run_guard(self, stdin: str):
        return subprocess.run([sys.executable, str(HOOKS / "guard_git.py")], input=stdin,
                              capture_output=True, text=True, timeout=20)

    def test_deny_json_contract(self):
        done = self.run_guard(hook_payload("PreToolUse", tool_name="Bash", tool_use_id="x", turn_id="y",
                                           tool_input={"command": "git status"}))
        self.assertEqual(done.returncode, 0)
        out = json.loads(done.stdout)["hookSpecificOutput"]
        self.assertEqual(out["hookEventName"], "PreToolUse")
        self.assertEqual(out["permissionDecision"], "deny")
        self.assertIn("jj st", out["permissionDecisionReason"])

    def test_allow_prints_nothing(self):
        done = self.run_guard(hook_payload("PreToolUse", tool_name="Bash", tool_input={"command": "jj st"}))
        self.assertEqual((done.returncode, done.stdout.strip()), (0, ""))

    def test_other_tools_are_ignored(self):
        patch = "*** Begin Patch\n+git status is banned\n*** End Patch"
        for tool, ti in (("apply_patch", {"command": patch}), ("mcp__x__y", {"command": "git status"})):
            done = self.run_guard(hook_payload("PreToolUse", tool_name=tool, tool_input=ti))
            self.assertEqual(done.stdout.strip(), "", tool)

    def test_garbage_input_fails_open(self):
        for stdin in ("", "not json", "[]", "{}", '{"tool_name":"Bash"}', '{"tool_name":"Bash","tool_input":5}'):
            done = self.run_guard(stdin)
            self.assertEqual((done.returncode, done.stdout.strip()), (0, ""), stdin)

    def test_command_given_as_list(self):
        done = self.run_guard(hook_payload("PreToolUse", tool_name="Bash",
                                           tool_input={"command": ["bash", "-lc", "git status"]}))
        self.assertIn("deny", done.stdout)


def make_repo(tmp: Path) -> str:
    """Create a throwaway repo skeleton; returns 'jj' or 'git' (whichever backs it) or ''."""
    (tmp / "docs").mkdir()
    (tmp / "AGENTS.md").write_text("# test\n", encoding="utf-8")
    (tmp / ".gitignore").write_text(".codex/state/\n__pycache__/\n", encoding="utf-8")
    (tmp / "docs" / "ROADMAP.md").write_text(
        "# Roadmap\n- [x] P0-01 done thing\n- [ ] P0-02 scaffold the crates (class B)\n- [ ] P0-03 later\n",
        encoding="utf-8")
    shutil.copytree(ROOT / ".codex", tmp / ".codex", ignore=shutil.ignore_patterns("state", "__pycache__"))
    env = dict(os.environ, HOME=str(tmp / "_home"), JJ_CONFIG=str(tmp / "_home" / "jj.toml"))
    (tmp / "_home").mkdir()
    (tmp / "_home" / "jj.toml").write_text('[user]\nname="t"\nemail="t@x.invalid"\n[ui]\npaginate="never"\n')
    if shutil.which("jj"):
        subprocess.run(["jj", "git", "init"], cwd=tmp, env=env, capture_output=True, check=True)
        return "jj"
    if shutil.which("git"):
        subprocess.run(["git", "init", "-q"], cwd=tmp, capture_output=True, check=True)
        return "git"
    return ""


@unittest.skipUnless(shutil.which("jj") or shutil.which("git"), "needs jj or git for the end-to-end test")
class EndToEnd(unittest.TestCase):
    """Runs the exact command strings from hooks.json, started from a subdirectory."""

    def setUp(self):
        self._tmp = tempfile.TemporaryDirectory()
        self.tmp = Path(self._tmp.name).resolve()
        self.vcs = make_repo(self.tmp)
        (self.tmp / "sub" / "dir").mkdir(parents=True)
        self.cwd = str(self.tmp / "sub" / "dir")
        self.env = dict(os.environ, HOME=str(self.tmp / "_home"), JJ_CONFIG=str(self.tmp / "_home" / "jj.toml"))
        self.hooks = json.loads((ROOT / ".codex" / "hooks.json").read_text(encoding="utf-8"))["hooks"]

    def tearDown(self):
        self._tmp.cleanup()

    def command(self, event: str) -> str:
        return self.hooks[event][0]["hooks"][0]["command"]

    def run_hook(self, event: str, **payload):
        stdin = json.dumps(dict(payload, cwd=self.cwd, hook_event_name=event))
        return subprocess.run(["sh", "-c", self.command(event)], input=stdin, cwd=self.cwd, env=self.env,
                              capture_output=True, text=True, timeout=60)

    def test_session_start_startup(self):
        done = self.run_hook("SessionStart", source="startup")
        self.assertEqual(done.returncode, 0, done.stderr)
        self.assertIn("P0-02 scaffold the crates", done.stdout)
        self.assertNotIn("P0-03", done.stdout)
        self.assertLess(len(done.stdout), 3700)

    def test_pre_compact_then_session_start_compact(self):
        done = self.run_hook("PreCompact", trigger="auto", turn_id="1")
        self.assertEqual((done.returncode, done.stdout.strip()), (0, ""), done.stderr)
        snap = self.tmp / ".codex" / "state" / "compact-snapshot.md"
        self.assertTrue(snap.is_file())
        self.assertIn("P0-02 scaffold the crates", snap.read_text(encoding="utf-8"))
        again = self.run_hook("SessionStart", source="compact")
        self.assertIn("re-read AGENTS.md section 0", again.stdout)
        self.assertIn("Last pre-compaction snapshot", again.stdout)
        self.assertLess(len(again.stdout), 3700)

    @unittest.skipUnless(shutil.which("jj"), "needs jj")
    def test_state_directory_is_invisible_to_jj(self):
        self.run_hook("PreCompact", trigger="manual", turn_id="1")
        self.assertTrue((self.tmp / ".codex" / "state" / "compact-snapshot.md").is_file())
        listed = subprocess.run(["jj", "file", "list"], cwd=self.tmp, env=self.env,
                                capture_output=True, text=True).stdout
        self.assertIn(".codex/hooks/guard_git.py", listed)  # control: ordinary files are tracked
        self.assertNotIn("compact-snapshot.md", listed)     # the git-ignored state file is not

    def test_guard_through_hooks_json(self):
        deny = self.run_hook("PreToolUse", tool_name="Bash", tool_input={"command": "git commit -am x"})
        self.assertIn('"permissionDecision": "deny"', deny.stdout, deny.stderr)
        ok = self.run_hook("PreToolUse", tool_name="Bash", tool_input={"command": "jj st"})
        self.assertEqual((ok.returncode, ok.stdout.strip()), (0, ""), ok.stderr)


class HookLauncher(unittest.TestCase):
    """The commands in hooks.json must fail open: a hook that cannot start must never block a tool call."""

    PAYLOAD = '{"tool_name": "Bash", "tool_input": {"command": "git status"}, "source": "startup", "trigger": "auto"}'

    def setUp(self):
        self._tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self._tmp.cleanup)
        self.tmp = Path(self._tmp.name).resolve()
        self.hooks = json.loads((ROOT / ".codex" / "hooks.json").read_text(encoding="utf-8"))["hooks"]

    def commands(self):
        for event, groups in self.hooks.items():
            yield event, groups[0]["hooks"][0]["command"]

    def run_sh(self, command, cwd, env):
        return subprocess.run(["/bin/sh", "-c", command], input=self.PAYLOAD, cwd=cwd, env=env,
                              capture_output=True, text=True, timeout=30)

    def test_outside_any_repository_exits_zero_and_silent(self):
        outside = self.tmp / "outside"
        outside.mkdir()
        env = dict(os.environ, GIT_CEILING_DIRECTORIES=str(self.tmp))
        for event, command in self.commands():
            done = self.run_sh(command, outside, env)
            self.assertEqual((done.returncode, done.stdout.strip()), (0, ""), f"{event}: {done.stderr}")

    def test_missing_python_exits_zero(self):
        repo = self.tmp / "repo"
        (repo / ".codex" / "hooks").mkdir(parents=True)
        for _event, command in self.commands():
            script = re.search(r"/\.codex/hooks/([\w.-]+\.py)", command).group(1)
            (repo / ".codex" / "hooks" / script).write_text("raise SystemExit(2)\n")  # would block if it ran
        bare = self.tmp / "bare"
        bare.mkdir()
        fake_jj = bare / "jj"
        fake_jj.write_text(f'#!/bin/sh\necho "{repo}"\n')
        fake_jj.chmod(0o755)
        env = dict(os.environ, PATH=str(bare))
        for event, command in self.commands():
            done = self.run_sh(command, repo, env)
            self.assertEqual(done.returncode, 0, f"{event}: {done.stderr}")

    def test_missing_script_exits_zero(self):
        repo = self.tmp / "repo2"
        repo.mkdir()
        bare = self.tmp / "bare2"
        bare.mkdir()
        fake_jj = bare / "jj"
        fake_jj.write_text(f'#!/bin/sh\necho "{repo}"\n')
        fake_jj.chmod(0o755)
        env = dict(os.environ, PATH=os.pathsep.join([str(bare), os.environ.get("PATH", "")]))
        for event, command in self.commands():
            done = self.run_sh(command, repo, env)
            self.assertEqual((done.returncode, done.stdout.strip()), (0, ""), f"{event}: {done.stderr}")


class StaticFiles(unittest.TestCase):
    def test_hooks_json_shape(self):
        data = json.loads((ROOT / ".codex" / "hooks.json").read_text(encoding="utf-8"))
        for event, groups in data["hooks"].items():
            for group in groups:
                for handler in group["hooks"]:
                    self.assertEqual(handler["type"], "command")
                    self.assertIsInstance(handler["timeout"], int)
                    script = handler["command"].split("/.codex/hooks/")[1].split('"')[0]
                    self.assertTrue((HOOKS / script).is_file(), f"{event}: missing {script}")

    @unittest.skipIf(tomllib is None, "needs Python 3.11+ for tomllib")
    def test_agent_files(self):
        files = sorted((ROOT / ".codex" / "agents").glob("*.toml"))
        self.assertTrue(files, "no custom agent files")
        names = set()
        for path in files:
            data = tomllib.loads(path.read_text(encoding="utf-8"))
            for key in ("name", "description", "developer_instructions"):
                self.assertTrue(data.get(key), f"{path.name}: missing {key}")
            self.assertNotIn(data["name"], names)
            names.add(data["name"])

    def test_skills_have_frontmatter(self):
        for path in sorted((ROOT / ".agents" / "skills").glob("*/SKILL.md")):
            lines = path.read_text(encoding="utf-8").splitlines()
            self.assertEqual(lines[0], "---", path)
            end = lines[1:].index("---") + 1
            fields = dict(line.split(":", 1) for line in lines[1:end] if ":" in line)
            self.assertEqual(fields["name"].strip(), path.parent.name, path)
            self.assertTrue(fields["description"].strip(), path)


if __name__ == "__main__":
    unittest.main(verbosity=1)
