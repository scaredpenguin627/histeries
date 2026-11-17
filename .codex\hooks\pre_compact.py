#!/usr/bin/env python3
"""Histeries PreCompact hook: write a state snapshot to a file before compaction.

PreCompact stdout is ignored by Codex, so the snapshot goes to
.codex/state/compact-snapshot.md (git-ignored); the SessionStart hook reads it
back when the session restarts with source "compact" or "resume".
This hook never stops compaction (it does not return continue:false).
"""
from __future__ import annotations

import datetime
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import _common as c  # noqa: E402


def snapshot(payload: dict) -> str:
    root = c.repo_root(payload.get("cwd"))
    trigger = str(payload.get("trigger") or "unknown")
    now = datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")
    # snapshot=True: asking jj for its status also snapshots the working copy,
    # so uncommitted edits are captured in the operation log before compaction.
    parts = [
        f"# Compaction snapshot {now} (trigger: {trigger})",
        "## Current task\n" + c.current_task(root),
        "## jj status\n" + (c.cap_lines(c.jj_status(root, snapshot=True), 25, 1800) or "(jj unavailable)"),
        "## Commits not on main@origin\n" + (c.cap_lines(c.jj_unlanded(root, snapshot=True), 25, 1800) or "(none)"),
        "## Changed files\n"
        + (c.cap_lines(c.run(c.JJ + ["diff", "--stat"], root), 30, 1800) or "(none)"),
    ]
    return "\n\n".join(parts) + "\n"


def main() -> int:
    try:
        payload = c.read_payload()
        root = c.repo_root(payload.get("cwd"))
        state_dir = os.path.join(root, ".codex", "state")
        os.makedirs(state_dir, exist_ok=True)
        with open(os.path.join(state_dir, "compact-snapshot.md"), "w", encoding="utf-8") as fh:
            fh.write(snapshot(payload))
    except Exception as exc:  # never break a session over a snapshot
        print(f"pre_compact: ignored internal error: {exc}", file=sys.stderr)
    return 0


if __name__ == "__main__":
    sys.exit(main())
