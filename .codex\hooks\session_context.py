#!/usr/bin/env python3
"""Histeries SessionStart hook: put the project state back in front of the agent.

Plain stdout from a SessionStart hook is added to the session as developer
context. The text is kept small (Codex caps hook context at roughly 2,500 tokens).
Fires for startup, resume, clear, compact and fork; richer output after a
compaction or resume, when the previous context may have been summarized away.

Never relied on: it only restates what AGENTS.md and the repo files already say.
"""
from __future__ import annotations

import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import _common as c  # noqa: E402

MAX_CHARS = 3500
REREAD = "AGENTS.md section 0 (workflow), section 3 (clean-room), section 10 (jj only), section 15 (context hygiene)"


def build(payload: dict) -> str:
    source = str(payload.get("source") or "startup")
    root = c.repo_root(payload.get("cwd"))
    lost_context = source in ("compact", "resume", "clear", "fork")

    out = [f"Histeries state ({source}). AGENTS.md is in force; never run git, use jj."]
    if lost_context:
        out.append(f"Context may have been compacted or reset. Before your next action re-read {REREAD}, "
                   "then the current task, and re-verify any fact you were relying on.")
    out.append(f"Current task (first unchecked item in docs/ROADMAP.md): {c.current_task(root)}")

    status = c.jj_status(root, snapshot=False)
    if status:
        out.append("Working copy (jj st):\n" + c.cap_lines(status, 12, 900))
    unlanded = c.jj_unlanded(root, snapshot=False)
    if unlanded:
        out.append("Commits not on main@origin:\n" + c.cap_lines(unlanded, 12, 900))

    snap = os.path.join(root, ".codex", "state", "compact-snapshot.md")
    if lost_context and os.path.isfile(snap):
        try:
            with open(snap, encoding="utf-8") as fh:
                out.append("Last pre-compaction snapshot:\n" + c.cap_lines(fh.read(), 30, 1400))
        except OSError:
            pass
    return c.cap_lines("\n\n".join(out), 80, MAX_CHARS)


def main() -> int:
    try:
        print(build(c.read_payload()))
    except Exception as exc:  # never break a session over a context hook
        print(f"session_context: ignored internal error: {exc}", file=sys.stderr)
    return 0


if __name__ == "__main__":
    sys.exit(main())
