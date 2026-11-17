#!/usr/bin/env bash
# Histeries — environment check. Read-only: prints tool versions, installs nothing.
# Exit 0 when every REQUIRED tool is present and runs, 1 otherwise.
# Used by the session-start skill and as the Codex Cloud environment's start check.
set -u

export PATH="$HOME/.local/bin:$HOME/.cargo/bin:/usr/local/bin:$PATH"
missing=0

show() { # label, command...   (a tool counts as present only if it runs and exits 0)
  local label=$1; shift
  local out
  if ! command -v "$1" >/dev/null 2>&1; then
    printf '%-14s MISSING\n' "$label"
    return 1
  fi
  if out=$("$@" 2>&1); then
    printf '%-14s %s\n' "$label" "$(printf '%s\n' "$out" | head -n 1)"
  else
    printf '%-14s MISSING (found, but "%s" failed: %s)\n' "$label" "$*" "$(printf '%s\n' "$out" | head -n 1)"
    return 1
  fi
}

echo "== required =="
show jj      jj --version      || missing=1
show cargo   cargo --version   || missing=1
show rustc   rustc --version   || missing=1
show python3 python3 --version || missing=1

if command -v jj >/dev/null 2>&1; then
  if [ "$(jj config get ui.editor 2>/dev/null)" = "false" ] && [ -n "$(jj config get user.email 2>/dev/null)" ]; then
    printf '%-14s ok (editor fails fast, agent identity set)\n' "jj config"
  else
    printf '%-14s NOT SET (session-start step 2 says how to set it)\n' "jj config"
  fi
fi

echo "== recommended (gates need them once the workspace exists) =="
show nextest cargo nextest --version || true
show deny    cargo deny --version    || true
show about   cargo about --version   || true

if command -v rustup >/dev/null 2>&1; then
  installed=$(rustup target list --installed 2>/dev/null | tr '\n' ' ')
  printf '%-14s %s\n' "rust targets" "${installed:-none}"
  lacking=""
  for t in aarch64-unknown-linux-ohos x86_64-unknown-linux-ohos wasm32-unknown-unknown; do
    case " $installed " in *" $t "*) ;; *) lacking="$lacking $t" ;; esac
  done
  if [ -n "$lacking" ]; then
    printf '%-14s MISSING:%s\n' "ohos/wasm32" "$lacking"
  else
    printf '%-14s ok\n' "ohos/wasm32"
  fi
fi

echo "== policy =="
if command -v git >/dev/null 2>&1; then
  echo "git           present for the harness and for jj's internal transport; agents must never run it (AGENTS.md section 10)"
fi
if [ -d .jj ]; then
  echo "jj workspace  .jj present"
else
  echo "jj workspace  no .jj yet: the session-start skill runs 'jj git init'"
fi

if [ "$missing" -ne 0 ]; then
  echo "RESULT: a required tool is missing. Stop and report; do not fall back to git." >&2
  exit 1
fi
echo "RESULT: ok"
