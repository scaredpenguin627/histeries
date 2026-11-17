#!/usr/bin/env python3
"""Consistency check for the Histeries Codex Cloud instruction pack.

Read-only and standard-library only. It answers one question: is the pack that
tells agents how to work here still intact? AGENTS.md not truncated, the policy
text unchanged, skills and hooks loadable, every cross-reference resolving, no
leftovers from the previous agent family.

    python3 scripts/check_port.py            full check
    python3 scripts/check_port.py --quick    skip the hook test suite and the jj lookup
    python3 scripts/check_port.py --strict   warnings count as failures
    python3 scripts/check_port.py --root DIR check another checkout

Exit status 0 means no errors. The last line is "RESULT: ok" or "RESULT: FAILED".
Run it before declaring any task done (the session-finish skill says so).
"""
from __future__ import annotations

import argparse
import json
import os
import re
import shutil
import subprocess
import sys
from pathlib import Path

try:
    import tomllib
except ModuleNotFoundError:  # Python < 3.11: TOML files are not parsed
    tomllib = None

# Codex reads at most project_doc_max_bytes (default 32,768) across every AGENTS.md
# from the repository root down to the working directory, and drops the rest silently.
CODEX_DOC_CAP = 32_768
AGENTS_ERROR = 28_000
AGENTS_WARN = 24_000
END_MARKER = "<!-- END OF AGENTS.md -->"

REQUIRED = [
    "AGENTS.md",
    ".agents/skills/session-start/SKILL.md",
    ".agents/skills/session-finish/SKILL.md",
    ".agents/skills/review-change/SKILL.md",
    ".codex/hooks.json",
    ".codex/hooks/_common.py",
    ".codex/hooks/guard_git.py",
    ".codex/hooks/pre_compact.py",
    ".codex/hooks/session_context.py",
    ".codex/agents/reviewer-astra.toml",
    ".codex/agents/reviewer-sol.toml",
    "docs/codex-cloud.md",
    "docs/ARCHITECTURE.md",
    "docs/ROADMAP.md",
    "docs/prompts/session.md",
    "docs/prompts/review.md",
    "docs/prompts/smoke-tests.md",
    "docs/sessions/TEMPLATE.md",
    "docs/sessions/TEMPLATE-handover.md",
    "scripts/codex-cloud-setup.sh",
    "scripts/check-env.sh",
    "scripts/check_port.py",
    "scripts/test_check_port.py",
    "scripts/test_hooks.py",
]
# Used once and then safe to delete: absence is fine, and references to them are tolerated.
ONE_SHOT = ["docs/prompts/bootstrap.md", "docs/prompts/port.md"]
# Files that name stale strings on purpose; never scanned for them.
STALE_EXEMPT = {"docs/prompts/port.md", "scripts/check_port.py"}

SECTIONS_REQUIRED = [str(n) for n in range(16)] + ["3.1", "3.2", "3.3", "3.4"]

# Policy text that must survive every edit of AGENTS.md word for word.
MUST_CONTAIN = [
    "Never request, read, summarize or reason from decompiled or disassembled code of any commercial game; "
    "from any game's shipped data files or scripts (XML, SQL, Lua, etc.); "
    "from Steam Workshop or other third-party mods (their code, data or text); "
    "or from a game's proprietary text.",
    "If any such material appears — pasted, linked, or found on disk — stop, say so, do not use it, "
    "and ask for a behavioral description in the maintainer's own words.",
    "Computer use: off. Never browse, open or read an installed game's files. "
    "Never attach or clone repositories containing decompiled, ripped or mod material.",
    "Fixtures are produced by the maintainer from their own play; agents only consume them.",
    "Never ask for, store or print credentials or tokens.",
    "You never run `git` yourself",
]

# Leftovers of the previous agent family and of wrong jj advice.
STALE = [
    (r"\.claude/", "a Claude Code directory"),
    (r"CLAUDE\.md", "a Claude Code memory file"),
    (r"reviewer-(?:fable|opus)", "a Claude reviewer agent"),
    (r"\b(?:Fable|Opus|Sonnet)\b", "a Claude model name (use task classes A, B, C: AGENTS.md section 11)"),
    (r"--allow-new", "a jj flag that does not exist (new bookmarks push without it)"),
    (r"git_head\(", "a revset that does not exist in jj"),
    (r"check-port\.sh", "the old checker name (it is scripts/check_port.py)"),
    (r"non-colocated", "outdated wording (jj colocates by default)"),
]

SKIP_DIRS = {".git", ".jj", "target", "node_modules", "__pycache__", ".venv", "venv"}


class Report:
    def __init__(self) -> None:
        self.errors = 0
        self.warnings = 0

    def section(self, title: str) -> None:
        print(f"\n== {title}")

    def ok(self, msg: str) -> None:
        print(f"  ok    {msg}")

    def info(self, msg: str) -> None:
        print(f"  info  {msg}")

    def warn(self, msg: str) -> None:
        self.warnings += 1
        print(f"  WARN  {msg}")

    def error(self, msg: str) -> None:
        self.errors += 1
        print(f"  FAIL  {msg}")


# ------------------------------------------------------------------ helpers ---

def list_files(root: Path) -> list[str]:
    out: list[str] = []
    for dirpath, dirnames, filenames in os.walk(root):
        rel_dir = Path(dirpath).relative_to(root).as_posix()
        rel_dir = "" if rel_dir == "." else rel_dir
        dirnames[:] = sorted(
            d for d in dirnames
            if d not in SKIP_DIRS and f"{rel_dir}/{d}".lstrip("/") != ".codex/state"
        )
        for name in sorted(filenames):
            out.append(f"{rel_dir}/{name}".lstrip("/"))
    return out


def read(root: Path, rel: str) -> str:
    return (root / rel).read_text(encoding="utf-8", errors="replace")


def normalize(text: str) -> str:
    text = text.replace("’", "'").replace("‘", "'")
    return re.sub(r"\s+", " ", text)


def frontmatter(text: str) -> dict[str, str] | None:
    text = text.lstrip("﻿")
    if not text.startswith("---"):
        return None
    end = text.find("\n---", 3)
    if end == -1:
        return None
    fields: dict[str, str] = {}
    for line in text[3:end].splitlines():
        m = re.match(r"^([A-Za-z_][\w-]*):\s*(.*)$", line)
        if m:
            fields[m.group(1)] = m.group(2).strip()
    return fields


def classify(rel: str) -> str | None:
    """'error' for instruction files, 'warn' for other prose, None for files not scanned."""
    if rel in STALE_EXEMPT:
        return None
    if rel.startswith("docs/sessions/TEMPLATE"):
        return "error"
    if rel.startswith("docs/sessions/"):
        return None  # historical reports may name anything
    if rel == "AGENTS.md" or rel == "docs/codex-cloud.md":
        return "error"
    if rel.startswith((".agents/", ".codex/", "docs/prompts/")):
        return "error"
    if rel.startswith("scripts/") and rel.endswith(".sh"):
        return "error"
    if rel.endswith((".md", ".yml", ".yaml", ".toml", ".json")):
        if "/" not in rel or rel.startswith(("docs/", ".github/", ".vscode/")):
            return "warn"
    return None


def agents_section_ids(text: str) -> set[str]:
    ids = set(re.findall(r"^#{2,4}\s+(\d+(?:\.\d+)?)\.?\s", text, re.M))
    ids |= set(re.findall(r"\*\*(\d+\.\d+)\b", text))
    return ids


def doc_section_ids(text: str) -> set[str]:
    return set(re.findall(r"^#{2,4}\s+(\d+(?:\.\d+)?)\.?\s", text, re.M))


# ------------------------------------------------------------------- checks ---

def check_required(root: Path, rep: Report, files: list[str]) -> None:
    rep.section("Required files")
    have = set(files)
    missing = [p for p in REQUIRED if p not in have]
    for p in missing:
        rep.error(f"missing {p}")
    if not missing:
        rep.ok(f"all {len(REQUIRED)} required files are present")
    gone = [p for p in ONE_SHOT if p not in have]
    if gone:
        rep.info(f"one-shot prompt(s) not present: {', '.join(gone)} (fine once used)")


def check_agents(root: Path, rep: Report, files: list[str]) -> str:
    rep.section("AGENTS.md")
    if "AGENTS.md" not in files:
        return ""
    raw = (root / "AGENTS.md").read_bytes()
    text = raw.decode("utf-8", errors="replace")
    size = len(raw)
    detail = f"{size:,} bytes (Codex keeps at most {CODEX_DOC_CAP:,} across all AGENTS.md files)"
    if size > AGENTS_ERROR:
        rep.error(f"{detail}; limit for this file is {AGENTS_ERROR:,}. Move detail into docs/ or a skill")
    elif size > AGENTS_WARN:
        rep.warn(f"{detail}; getting close to the {AGENTS_ERROR:,} limit")
    else:
        rep.ok(detail)

    others = [p for p in files if p != "AGENTS.md" and Path(p).name in ("AGENTS.md", "AGENTS.override.md")]
    if others:
        total = size + sum((root / p).stat().st_size for p in others)
        msg = f"{len(others)} other AGENTS file(s) ({', '.join(others[:3])}); combined {total:,} bytes"
        (rep.warn if total > AGENTS_ERROR else rep.info)(msg)

    last = [ln.strip() for ln in text.splitlines() if ln.strip()]
    if last and last[-1] == END_MARKER:
        rep.ok("ends with the end marker (not truncated)")
    else:
        rep.error(f"last line must be {END_MARKER}; the file may have been cut or edited after the marker")

    ids = agents_section_ids(text)
    missing = [s for s in SECTIONS_REQUIRED if s not in ids]
    if missing:
        rep.error(f"missing section(s): {', '.join(missing)}")
    else:
        rep.ok("sections 0 to 15 and 3.1 to 3.4 present")

    flat = normalize(text)
    lost = [n for n in MUST_CONTAIN if normalize(n) not in flat]
    for n in lost:
        rep.error(f"policy text changed or missing: \"{n[:70]}...\"")
    if not lost:
        rep.ok(f"{len(MUST_CONTAIN)} policy sentences (clean room, tool policy, no git) are intact")
    return text


def check_skills(root: Path, rep: Report, files: list[str]) -> None:
    rep.section("Skills")
    skills = sorted(p for p in files if re.fullmatch(r"\.agents/skills/[^/]+/SKILL\.md", p))
    for name in ("session-start", "session-finish", "review-change"):
        if f".agents/skills/{name}/SKILL.md" not in skills:
            rep.error(f"skill {name} is missing")
    bad = 0
    for p in skills:
        directory = p.split("/")[2]
        fm = frontmatter(read(root, p))
        if fm is None:
            rep.error(f"{p}: no YAML frontmatter between --- lines")
            bad += 1
            continue
        if fm.get("name") != directory:
            rep.error(f"{p}: name is {fm.get('name')!r}, must equal the directory name {directory!r}")
            bad += 1
        if not fm.get("description"):
            rep.error(f"{p}: description is empty")
            bad += 1
    if skills and not bad:
        rep.ok(f"{len(skills)} skills have valid frontmatter")


def check_hooks_json(root: Path, rep: Report, files: list[str]) -> None:
    rep.section("Hooks configuration")
    rel = ".codex/hooks.json"
    if rel not in files:
        return
    try:
        data = json.loads(read(root, rel))
    except ValueError as exc:
        rep.error(f"{rel} is not valid JSON: {exc}")
        return
    hooks = data.get("hooks") if isinstance(data, dict) else None
    if not isinstance(hooks, dict) or not hooks:
        rep.error(f"{rel} has no \"hooks\" object")
        return
    count = 0
    problems = 0
    for event, groups in hooks.items():
        for group in groups if isinstance(groups, list) else []:
            for hook in group.get("hooks", []):
                count += 1
                command = hook.get("command", "")
                scripts = re.findall(r"\.codex/hooks/([\w.-]+\.py)", command)
                if not scripts:
                    rep.error(f"{event}: command does not run a script from .codex/hooks/")
                    problems += 1
                for script in scripts:
                    if f".codex/hooks/{script}" not in files:
                        rep.error(f"{event}: {script} does not exist")
                        problems += 1
                if not isinstance(hook.get("timeout"), int) or hook["timeout"] <= 0:
                    rep.warn(f"{event}: {Path(scripts[0]).name if scripts else 'hook'} has no positive integer timeout")
    if count and not problems:
        rep.ok(f"{count} hook commands, every script exists")


def check_toml(root: Path, rep: Report, files: list[str]) -> None:
    rep.section("TOML files")
    tomls = [p for p in files if p.endswith(".toml")]
    if tomllib is None:
        rep.info("Python 3.11+ is needed to parse TOML; skipped")
        return
    parsed: dict[str, dict] = {}
    for p in tomls:
        try:
            parsed[p] = tomllib.loads(read(root, p))
        except Exception as exc:  # tomllib.TOMLDecodeError and decode errors
            rep.error(f"{p}: {exc}")
    for p, data in parsed.items():
        if p.startswith(".codex/agents/"):
            for key in ("name", "description", "developer_instructions"):
                if not isinstance(data.get(key), str) or not data[key].strip():
                    rep.error(f"{p}: required field {key!r} is missing or empty")
    if tomls and len(parsed) == len(tomls):
        rep.ok(f"{len(tomls)} TOML files parse")


def check_scripts(root: Path, rep: Report, files: list[str]) -> None:
    rep.section("Scripts")
    py = [p for p in files if p.endswith(".py") and p.startswith((".codex/", "scripts/"))]
    sh = [p for p in files if p.endswith(".sh") and p.startswith(("scripts/", ".agents/", ".codex/"))]
    failed = 0
    for p in py:
        try:
            compile(read(root, p), p, "exec")
        except SyntaxError as exc:
            rep.error(f"{p}: {exc}")
            failed += 1
    if py and not failed:
        rep.ok(f"{len(py)} Python files compile")
    bash = shutil.which("bash")
    failed = 0
    for p in sh:
        if b"\r\n" in (root / p).read_bytes():
            rep.error(f"{p}: has Windows (CRLF) line endings, which break bash; save with LF")
            failed += 1
            continue
        if bash:
            result = subprocess.run([bash, "-n", str(root / p)], capture_output=True, text=True)
            if result.returncode != 0:
                rep.error(f"{p}: bash -n failed: {result.stderr.strip()[:200]}")
                failed += 1
    if sh and not failed:
        rep.ok(f"{len(sh)} shell scripts pass bash -n and use LF line endings" if bash
               else f"{len(sh)} shell scripts use LF line endings (bash not found, syntax not checked)")


def check_stale(root: Path, rep: Report, files: list[str]) -> None:
    rep.section("Leftovers")
    patterns = [(re.compile(p), why) for p, why in STALE]
    hits = 0
    scanned = 0
    for rel in files:
        level = classify(rel)
        if level is None:
            continue
        try:
            if (root / rel).stat().st_size > 1_000_000:
                continue
            text = read(root, rel)
        except OSError:
            continue
        scanned += 1
        for number, line in enumerate(text.splitlines(), 1):
            for pattern, why in patterns:
                m = pattern.search(line)
                if m:
                    hits += 1
                    (rep.error if level == "error" else rep.warn)(f"{rel}:{number}: '{m.group(0)}' is {why}")
    if not hits:
        rep.ok(f"no stale strings in {scanned} files")


def check_refs(root: Path, rep: Report, files: list[str], agents_text: str) -> None:
    rep.section("Cross-references")
    have = set(files)
    agents_ids = agents_section_ids(agents_text) if agents_text else set()
    cloud_ids = doc_section_ids(read(root, "docs/codex-cloud.md")) if "docs/codex-cloud.md" in have else set()
    union = agents_ids | cloud_ids
    ref_re = re.compile(
        r"(?:§|\b[Ss]ections?\s)\s*(\d+(?:\.\d+)?(?:(?:\s*,\s*and\s+|\s*,\s*|\s+and\s+)\d+(?:\.\d+)?)*)"
    )
    path_re = re.compile(
        r"(?<![\w/.-])("
        r"\.agents/skills/[\w-]+/SKILL\.md|docs/prompts/[\w-]+\.md|scripts/[\w.-]+\.(?:sh|py)|"
        r"\.codex/[\w/.-]*\.(?:py|json|toml)"
        r")"
    )
    scope = [p for p in files if classify(p) == "error" or p in ("docs/ARCHITECTURE.md", "docs/ROADMAP.md")]
    bad = 0
    checked = 0
    for rel in scope:
        text = read(root, rel)
        for number, line in enumerate(text.splitlines(), 1):
            for m in ref_re.finditer(line):
                window = line[max(0, m.start() - 90):m.start()]
                a, c = window.rfind("AGENTS.md"), window.rfind("codex-cloud.md")
                if a == -1 and c == -1:
                    target, valid = ("AGENTS.md" if rel == "AGENTS.md" else "AGENTS.md or codex-cloud.md"), (
                        agents_ids if rel == "AGENTS.md" else union)
                elif a > c:
                    target, valid = "AGENTS.md", agents_ids
                else:
                    target, valid = "docs/codex-cloud.md", cloud_ids
                if not valid:
                    continue
                for num in re.findall(r"\d+(?:\.\d+)?", m.group(1)):
                    checked += 1
                    if num not in valid:
                        bad += 1
                        rep.error(f"{rel}:{number}: section {num} does not exist in {target}")
            if rel == "AGENTS.md":
                for m in re.finditer(r"\((\d{1,2}\.\d{1,2})\)", line):
                    checked += 1
                    if m.group(1) not in agents_ids:
                        bad += 1
                        rep.error(f"{rel}:{number}: ({m.group(1)}) is not a section of AGENTS.md")
            for m in path_re.finditer(line):
                target = m.group(1)
                if target in have or target in ONE_SHOT:
                    continue
                if target.startswith("docs/prompts/") and any(p.endswith(target.rsplit("/", 1)[1]) for p in ONE_SHOT):
                    continue
                bad += 1
                rep.error(f"{rel}:{number}: refers to {target}, which does not exist")
    if not bad:
        rep.ok(f"{checked} section references resolve; every referenced skill, prompt, script and hook file exists")


def check_hygiene(root: Path, rep: Report, files: list[str], quick: bool) -> None:
    rep.section("Repository hygiene")
    gi = root / ".gitignore"
    if not gi.is_file():
        rep.warn(".gitignore is missing (the Phase 0 bootstrap creates it)")
    else:
        entries = {ln.strip().strip("/") for ln in read(root, ".gitignore").splitlines()
                   if ln.strip() and not ln.lstrip().startswith("#")}
        wanted = [".codex/state", "__pycache__"] + (["target"] if "Cargo.toml" in files else [])
        missing = [w for w in wanted if w not in entries and f"{w}/*" not in entries]
        if missing:
            rep.warn(f".gitignore lacks: {', '.join(m + '/' for m in missing)}")
        else:
            rep.ok(".gitignore covers generated state, bytecode and build output")
    ga = root / ".gitattributes"
    if not ga.is_file() or "eol=lf" not in read(root, ".gitattributes"):
        rep.warn(".gitattributes should contain: * text=auto eol=lf   (CRLF breaks shell scripts)")
    else:
        rep.ok(".gitattributes pins LF line endings")
    if quick:
        rep.info("jj lookup skipped (--quick)")
        return
    jj = shutil.which("jj")
    if jj and (root / ".jj").is_dir():
        result = subprocess.run([jj, "file", "list"], cwd=root, capture_output=True, text=True)
        if result.returncode != 0:
            rep.info("jj file list failed; tracked files not checked")
            return
        tracked = result.stdout.splitlines()
        junk = [p for p in tracked
                if p.startswith((".codex/state/", "target/")) or "__pycache__/" in p or p.endswith(".pyc")]
        if junk:
            rep.error(f"tracked by jj but must not be committed: {', '.join(junk[:5])}"
                      f"{' ...' if len(junk) > 5 else ''} (fix .gitignore, then `jj file untrack <path>`)")
        else:
            rep.ok("jj tracks no generated state, bytecode or build output")
    else:
        rep.info("no jj repository here; tracked files not checked")


def check_hook_tests(root: Path, rep: Report, files: list[str], quick: bool) -> None:
    rep.section("Hook and agent tests")
    if quick:
        rep.info("skipped (--quick)")
        return
    if "scripts/test_hooks.py" not in files:
        return
    env = dict(os.environ, PYTHONDONTWRITEBYTECODE="1")
    try:
        result = subprocess.run([sys.executable, str(root / "scripts/test_hooks.py")], cwd=root,
                                capture_output=True, text=True, timeout=300, env=env)
    except subprocess.TimeoutExpired:
        rep.error("scripts/test_hooks.py timed out after 300 s")
        return
    tail = [ln for ln in (result.stderr + result.stdout).strip().splitlines() if ln.strip()]
    if result.returncode == 0:
        rep.ok(next((ln for ln in reversed(tail) if ln.startswith(("Ran ", "OK"))), "passed") + " (scripts/test_hooks.py)")
    else:
        rep.error("scripts/test_hooks.py failed; last lines:")
        for ln in tail[-12:]:
            print(f"        {ln}")


# --------------------------------------------------------------------- main ---

def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="Check the Histeries Codex instruction pack.")
    parser.add_argument("--root", help="repository root (default: the parent of scripts/)")
    parser.add_argument("--quick", action="store_true", help="skip the hook test suite and the jj lookup")
    parser.add_argument("--strict", action="store_true", help="treat warnings as failures")
    args = parser.parse_args(argv)

    root = Path(args.root).resolve() if args.root else Path(__file__).resolve().parent.parent
    if not root.is_dir():
        print(f"RESULT: FAILED (no such directory: {root})")
        return 1
    files = list_files(root)
    rep = Report()
    print(f"Histeries instruction-pack check: {root}")

    check_required(root, rep, files)
    agents_text = check_agents(root, rep, files)
    check_skills(root, rep, files)
    check_hooks_json(root, rep, files)
    check_toml(root, rep, files)
    check_scripts(root, rep, files)
    check_stale(root, rep, files)
    check_refs(root, rep, files, agents_text)
    check_hygiene(root, rep, files, args.quick)
    check_hook_tests(root, rep, files, args.quick)

    failed = rep.errors > 0 or (args.strict and rep.warnings > 0)
    print()
    def count(n: int, word: str) -> str:
        return f"{n} {word}{'' if n == 1 else 's'}"

    if failed:
        print(f"RESULT: FAILED ({count(rep.errors, 'error')}, {count(rep.warnings, 'warning')})")
        return 1
    print(f"RESULT: ok ({count(rep.warnings, 'warning')})" if rep.warnings else "RESULT: ok")
    return 0


if __name__ == "__main__":
    sys.exit(main())
