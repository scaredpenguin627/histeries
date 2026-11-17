#!/usr/bin/env python3
"""Histeries PreToolUse hook: deny shell commands that run `git`.

Policy (AGENTS.md section 10): agents use jj and never run git. This is a
seatbelt, not a security boundary: it catches the usual ways an agent types
`git` (plain, env-prefixed, chained, wrapped in sudo/xargs/bash -c, inside
$(...) or backticks) and answers with the jj equivalent. A script that shells
out to git cannot be seen from here; review and CI cover that.

Fails open: if the payload or the command cannot be parsed, the call proceeds.
Only the Bash tool is inspected (apply_patch bodies must never be scanned).
"""
from __future__ import annotations

import json
import os
import re
import shlex
import sys
from typing import List, Optional, Tuple

MAX_DEPTH = 4

# Commands that take another command as their argument.
WRAPPERS = {
    "env", "sudo", "doas", "command", "exec", "time", "nice", "nohup", "setsid",
    "stdbuf", "ionice", "xargs", "timeout", "watch", "strace", "ltrace", "unbuffer",
    "chronic", "builtin",
}
SHELLS = {"bash", "sh", "zsh", "dash", "ksh"}
INTERPRETERS = {"python", "python3", "node", "bun", "perl", "ruby"}
RESERVED = {"{", "}", "!", "if", "then", "else", "elif", "do", "while", "until"}

ASSIGN_RE = re.compile(r"^[A-Za-z_][A-Za-z0-9_]*=")
REDIR_RE = re.compile(r"^\d*(>>?|<<?<?|>&|<&|&>>?)")
REDIR_ONLY_RE = re.compile(r"^\d*(>>?|<<?<?|>&|<&|&>>?)$")
HEREDOC_RE = re.compile(r"<<(-?)\s*(['\"]?)([A-Za-z0-9_]+)\2")
SHELL_C_RE = re.compile(r"^-[A-Za-z]*c[A-Za-z]*$")
INTERP_GIT_RE = re.compile(
    r"""['"]git['"]|\bgit\s+(status|add|commit|checkout|switch|reset|rebase|merge|push|"""
    r"""pull|fetch|clone|stash|branch|log|diff|show|restore|clean|tag|cherry-pick|"""
    r"""revert|rm|mv|init|remote|config)\b"""
)

DENY_MESSAGE = (
    "Histeries policy (AGENTS.md section 10): agents never run git; use jj. "
    "git status -> jj st | git diff -> jj diff | git log -> jj log | "
    "git add/commit -> nothing to stage, jj snapshots the working copy; "
    "describe it with: jj describe -m \"<type>(<scope>): <summary>\" -m \"Assisted-by: Codex (<model>)\" | "
    "git checkout/switch -> jj new <rev> or jj edit <rev> | git fetch -> jj git fetch | "
    "git push -> do not push unless the task prompt says so (then: jj git push --bookmark task/<slug>). "
    "Blocked command: {snippet}"
)


# ---------------------------------------------------------------- scanning ---
def _find_close(s: str, i: int) -> int:
    """s[i-1] is '('. Return the index of the matching ')', or len(s)."""
    depth, quote = 1, ""
    while i < len(s):
        c = s[i]
        if c == "\\" and quote != "'":
            i += 2
            continue
        if quote:
            if c == quote:
                quote = ""
        elif c in "'\"":
            quote = c
        elif c == "(":
            depth += 1
        elif c == ")":
            depth -= 1
            if depth == 0:
                return i
        i += 1
    return len(s)


def _find_backtick(s: str, i: int) -> int:
    while i < len(s):
        if s[i] == "\\":
            i += 2
            continue
        if s[i] == "`":
            return i
        i += 1
    return len(s)


def _skip_heredocs(s: str, i: int, pending: List[Tuple[str, bool]]) -> int:
    """Skip heredoc bodies that start at s[i]; return the index after them."""
    for delim, strip_tabs in pending:
        while i < len(s):
            end = s.find("\n", i)
            line_end = len(s) if end == -1 else end
            line = s[i:line_end].rstrip("\r")
            i = len(s) if end == -1 else end + 1
            if (line.lstrip("\t") if strip_tabs else line) == delim:
                break
    return i


def scan(s: str) -> Tuple[List[str], List[str]]:
    """Split a shell command line into simple-command segments.

    Returns (segments, nested): `nested` holds the text of $(...), `...`,
    <(...) and >(...) substitutions, which run as commands of their own.
    Heredoc bodies are skipped; quoted text is kept inside its segment.
    """
    segments: List[str] = []
    nested: List[str] = []
    buf: List[str] = []
    heredocs: List[Tuple[str, bool]] = []
    quote = ""
    i, n = 0, len(s)

    def flush() -> None:
        seg = "".join(buf).strip()
        buf.clear()
        if seg:
            segments.append(seg)

    while i < n:
        c = s[i]
        if quote == "'":
            buf.append(c)
            if c == "'":
                quote = ""
            i += 1
            continue
        if c == "\\":
            buf.append(s[i:i + 2])
            i += 2
            continue
        if quote == '"':
            if s.startswith("$(", i):
                j = _find_close(s, i + 2)
                nested.append(s[i + 2:j])
                buf.append("SUBST")
                i = j + 1
            elif c == "`":
                j = _find_backtick(s, i + 1)
                nested.append(s[i + 1:j])
                buf.append("SUBST")
                i = j + 1
            else:
                buf.append(c)
                if c == '"':
                    quote = ""
                i += 1
            continue
        # ---- unquoted ----
        if c in "'\"":
            quote = c
            buf.append(c)
            i += 1
            continue
        if s.startswith(("$(", "<(", ">("), i):
            j = _find_close(s, i + 2)
            nested.append(s[i + 2:j])
            buf.append("SUBST")
            i = j + 1
            continue
        if c == "`":
            j = _find_backtick(s, i + 1)
            nested.append(s[i + 1:j])
            buf.append("SUBST")
            i = j + 1
            continue
        if s.startswith("<<", i) and not s.startswith("<<<", i):
            m = HEREDOC_RE.match(s, i)
            if m:
                heredocs.append((m.group(3), m.group(1) == "-"))
                buf.append(m.group(0))
                i = m.end()
                continue
        if c == "\n":
            flush()
            i += 1
            if heredocs:
                i = _skip_heredocs(s, i, heredocs)
                heredocs = []
            continue
        if c in ";()":
            flush()
            i += 1
            continue
        if c == "|":
            flush()
            i += 2 if s[i + 1:i + 2] in ("|", "&") else 1
            continue
        if c == "&":
            prev, nxt = s[i - 1:i], s[i + 1:i + 2]
            if prev in (">", "<") or nxt == ">":  # 2>&1, >&2, &>file
                buf.append(c)
                i += 1
                continue
            flush()
            i += 2 if nxt == "&" else 1
            continue
        buf.append(c)
        i += 1
    flush()
    return segments, nested


# ---------------------------------------------------------------- analysis ---
def _tokens(segment: str) -> List[str]:
    try:
        return shlex.split(segment, posix=True)
    except ValueError:
        return segment.split()


def _is_git(token: str) -> bool:
    return os.path.basename(token) == "git"


def _wrapped_git(rest: List[str]) -> bool:
    """Heuristic: is `git` the first or second real argument after a wrapper?"""
    seen = 0
    for tok in rest:
        if tok.startswith("-") or ASSIGN_RE.match(tok) or re.fullmatch(r"\d+[smhd]?", tok):
            continue
        if _is_git(tok):
            return True
        seen += 1
        if seen >= 2:
            return False
    return False


def _strip_wrapper_options(rest: List[str]) -> List[str]:
    """Drop the leading options/durations of a wrapper (`timeout -s KILL 60 ...`)."""
    i = 0
    while i < len(rest) and (rest[i].startswith("-") or re.fullmatch(r"\d+[smhd]?", rest[i])):
        i += 1
    return rest[i:]


def _check_tokens(tokens: List[str], depth: int) -> Optional[str]:
    i = 0
    while i < len(tokens):
        tok = tokens[i]
        if tok in RESERVED or ASSIGN_RE.match(tok):
            i += 1
        elif REDIR_RE.match(tok):
            i += 2 if REDIR_ONLY_RE.match(tok) else 1
        else:
            break
    if i >= len(tokens):
        return None
    word = os.path.basename(tokens[i])
    rest = tokens[i + 1:]
    snippet = " ".join(tokens[i:i + 5])

    if word == "git":
        return snippet
    if word in WRAPPERS:
        if word == "command" and rest[:1] in (["-v"], ["-V"]):
            return None
        if _wrapped_git(rest):
            return snippet
        return _check_tokens(_strip_wrapper_options(rest), depth + 1) if depth < MAX_DEPTH else None
    if word in SHELLS:
        for idx, tok in enumerate(rest):
            if SHELL_C_RE.match(tok) and idx + 1 < len(rest):
                return analyze(rest[idx + 1], depth + 1)
        return None
    if word == "eval":
        return analyze(" ".join(rest), depth + 1)
    if word in INTERPRETERS:
        for idx, tok in enumerate(rest):
            if tok in ("-c", "-e", "-p", "--eval", "--print") and idx + 1 < len(rest):
                if INTERP_GIT_RE.search(rest[idx + 1]):
                    return snippet + " (runs git)"
        return None
    if word == "find":
        for idx, tok in enumerate(rest):
            if tok in ("-exec", "-execdir", "-ok", "-okdir") and idx + 1 < len(rest) and _is_git(rest[idx + 1]):
                return snippet
        return None
    if word == "jj" and "util" in rest and "exec" in rest:
        if "--" in rest:
            after = rest[rest.index("--") + 1:]
            if after and _is_git(after[0]):
                return snippet
        return None
    return None


def analyze(command: str, depth: int = 0) -> Optional[str]:
    """Return the offending snippet if `command` runs git, else None."""
    if depth > MAX_DEPTH or not command.strip():
        return None
    segments, nested = scan(command)
    for seg in segments:
        hit = _check_tokens(_tokens(seg), depth)
        if hit:
            return hit
    for sub in nested:
        hit = analyze(sub, depth + 1)
        if hit:
            return hit
    return None


# -------------------------------------------------------------------- main ---
def extract_command(payload: dict) -> Optional[str]:
    if payload.get("tool_name") != "Bash":
        return None
    tool_input = payload.get("tool_input")
    cmd = tool_input.get("command") if isinstance(tool_input, dict) else None
    if isinstance(cmd, list):
        cmd = shlex.join(str(x) for x in cmd)
    return cmd if isinstance(cmd, str) else None


def main() -> int:
    try:
        raw = sys.stdin.read()
        payload = json.loads(raw) if raw.strip() else {}
        command = extract_command(payload) if isinstance(payload, dict) else None
        if not command:
            return 0
        hit = analyze(command)
        if hit:
            shown = hit if len(hit) <= 120 else hit[:117] + "..."
            print(json.dumps({
                "hookSpecificOutput": {
                    "hookEventName": "PreToolUse",
                    "permissionDecision": "deny",
                    "permissionDecisionReason": DENY_MESSAGE.format(snippet=shown),
                }
            }))
    except Exception as exc:  # fail open: never block work because the guard broke
        print(f"guard_git: ignored internal error: {exc}", file=sys.stderr)
    return 0


if __name__ == "__main__":
    sys.exit(main())
