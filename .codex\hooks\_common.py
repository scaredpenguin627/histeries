"""Shared helpers for the Histeries Codex hooks (stdlib only, Python 3.9+).

Every helper fails soft: a hook must never break a session. Missing tools, a
missing repo or a slow command just produce empty text.
"""
from __future__ import annotations

import json
import os
import re
import subprocess
import sys
from typing import List, Optional

JJ = ["jj", "--no-pager", "--color", "never"]
TASK_RE = re.compile(r"^\s*[-*]\s+\[ \]\s+(.*\S)")


def read_payload() -> dict:
    """Parse the hook's JSON stdin; return {} when it is empty or malformed."""
    try:
        raw = sys.stdin.read()
        data = json.loads(raw) if raw.strip() else {}
        return data if isinstance(data, dict) else {}
    except Exception:
        return {}


def repo_root(start: Optional[str]) -> str:
    """Nearest ancestor of `start` holding AGENTS.md, .jj or .git (else `start`)."""
    here = os.path.abspath(start or os.getcwd())
    cur = here
    while True:
        if any(os.path.exists(os.path.join(cur, m)) for m in ("AGENTS.md", ".jj", ".git")):
            return cur
        parent = os.path.dirname(cur)
        if parent == cur:
            return here
        cur = parent


def run(cmd: List[str], cwd: str, timeout: int = 8) -> str:
    """Run a command, return stdout (stripped) or '' on any failure."""
    try:
        env = dict(os.environ, NO_COLOR="1")
        done = subprocess.run(
            cmd, cwd=cwd, capture_output=True, text=True, timeout=timeout, env=env
        )
        return done.stdout.strip() if done.returncode == 0 else ""
    except Exception:
        return ""


def current_task(root: str) -> str:
    """First unchecked checkbox in docs/ROADMAP.md."""
    try:
        with open(os.path.join(root, "docs", "ROADMAP.md"), encoding="utf-8") as fh:
            for line in fh:
                m = TASK_RE.match(line)
                if m:
                    return m.group(1)
    except OSError:
        return "(docs/ROADMAP.md not found)"
    return "(no unchecked task in docs/ROADMAP.md)"


def jj_status(root: str, snapshot: bool) -> str:
    """`jj st`; with snapshot=False the working copy is not snapshotted."""
    flags = [] if snapshot else ["--ignore-working-copy"]
    return run(JJ + flags + ["st"], root)


def jj_unlanded(root: str, snapshot: bool) -> str:
    """One line per commit that is on this workspace but not on main@origin."""
    flags = [] if snapshot else ["--ignore-working-copy"]
    tmpl = 'change_id.short() ++ " " ++ description.first_line() ++ "\\n"'
    return run(JJ + flags + ["log", "--no-graph", "-r", "main@origin..@-", "-T", tmpl], root)


def cap_lines(text: str, max_lines: int, max_chars: int) -> str:
    lines = text.splitlines()
    clipped = len(lines) > max_lines
    out = "\n".join(lines[:max_lines])
    if len(out) > max_chars:
        out, clipped = out[:max_chars], True
    return out + ("\n[... truncated]" if clipped else "")
