#!/usr/bin/env bash
# Histeries — Codex Cloud environment install script.
# ---------------------------------------------------------------------------
# Paste this whole file into the environment's install script, or run it from a
# checkout with:  bash scripts/codex-cloud-setup.sh
# It is idempotent. Re-run (and Republish the environment) after changing a pin:
# a repo refresh at task start does NOT re-run the install script.
#
# Every download is pinned and verified against a SHA-256 recorded on 2026-10-24;
# a mismatch aborts. Hosts needed: github.com, release-assets.githubusercontent.com,
# static.rust-lang.org, index.crates.io, static.crates.io (the "Package managers"
# network preset covers all of them).
#
# Test switches: HISTERIES_BIN_DIR=<dir>  SKIP_RUST=1  HISTERIES_SOURCE_ONLY=1
# ---------------------------------------------------------------------------
set -euo pipefail

# ---- pins ------------------------------------------------------------------
JJ_VERSION="0.46.0"
JJ_SHA256_X86_64="fce0271158e665ceb82dc66c5eb95b1728649094e9133074f3b5baece345fc08"
JJ_SHA256_AARCH64="80e28f7501c11e50e0ca06cd6a1fa1d982cff9cc59bfe4411a9d36aa501d867e"
NEXTEST_VERSION="0.9.149"
NEXTEST_SHA256_X86_64="d65c7f14bcb5c1566d1ecd534c5da8dedb81ba58586e478b0ab81c877d2bd8ad"
DENY_VERSION="0.20.2"
DENY_SHA256_X86_64="9f12ed4c49936e09b48bf862b595cde2fe64fcbd9d74dfacac6131ca824c8d5f"
ABOUT_VERSION="0.9.2"
ABOUT_SHA256_X86_64="9099a59e820c38a68b9d65f300662a567d56562f9a10f6aa4c7e86c17c2566af"
RUST_TARGETS="aarch64-unknown-linux-ohos x86_64-unknown-linux-ohos wasm32-unknown-unknown"
# ---------------------------------------------------------------------------

SKIP_RUST="${SKIP_RUST:-0}"
BIN_DIR="${HISTERIES_BIN_DIR:-/usr/local/bin}"
ARCH=""
TMP=""

log()  { printf '[histeries-setup] %s\n' "$*"; }
warn() { printf '[histeries-setup] WARNING: %s\n' "$*" >&2; }
die()  { printf '[histeries-setup] ERROR: %s\n' "$*" >&2; exit 1; }

cleanup() { [ -n "$TMP" ] && rm -rf "$TMP"; return 0; }

detect_arch() {
  case "$(uname -m)" in
    x86_64 | amd64) ARCH="x86_64" ;;
    aarch64 | arm64) ARCH="aarch64" ;;
    *) die "unsupported CPU architecture: $(uname -m)" ;;
  esac
  [ "$(uname -s)" = "Linux" ] || die "this script supports Linux only"
}

pick_bin_dir() {
  mkdir -p "$BIN_DIR" 2>/dev/null || true
  if [ ! -w "$BIN_DIR" ]; then
    BIN_DIR="$HOME/.local/bin"
    mkdir -p "$BIN_DIR"
    log "no write access to the default bin dir; using $BIN_DIR"
  fi
}

# fetch_verify <url> <sha256> <dest>
fetch_verify() {
  local url=$1 want=$2 dest=$3 got
  log "download $(basename "$url")"
  curl -fsSL --retry 3 --retry-delay 2 --connect-timeout 20 --max-time 300 \
    -o "$dest" "$url" || die "download failed: $url"
  got=$(sha256sum "$dest" | cut -d' ' -f1)
  [ "$got" = "$want" ] || die "sha256 mismatch for $(basename "$url"): expected $want, got $got"
}

# have_version <expected-substring> <command...>
have_version() {
  local want=$1; shift
  command -v "$1" >/dev/null 2>&1 || return 1
  "$@" 2>&1 | head -3 | grep -qF "$want"
}

# install_from_archive <url> <sha256> <file-name-in-archive> <dest-name>
install_from_archive() {
  local url=$1 sha=$2 inner=$3 dest=$4 found
  fetch_verify "$url" "$sha" "$TMP/archive.tgz"
  rm -rf "$TMP/x" && mkdir -p "$TMP/x"
  tar -xzf "$TMP/archive.tgz" -C "$TMP/x"
  found=$(find "$TMP/x" -type f -name "$inner" | head -1)
  [ -n "$found" ] || die "$inner not found inside $(basename "$url")"
  install -m 0755 "$found" "$BIN_DIR/$dest"
}

install_jj() {
  if have_version "jj $JJ_VERSION" "$BIN_DIR/jj" --version; then
    log "jj $JJ_VERSION already installed"; return
  fi
  local sha
  if [ "$ARCH" = "x86_64" ]; then sha=$JJ_SHA256_X86_64; else sha=$JJ_SHA256_AARCH64; fi
  install_from_archive \
    "https://github.com/jj-vcs/jj/releases/download/v${JJ_VERSION}/jj-v${JJ_VERSION}-${ARCH}-unknown-linux-musl.tar.gz" \
    "$sha" jj jj
}

# Agent-friendly jj defaults: no pager, no color, and an editor that fails fast
# (a bare `jj describe` must error, never hang waiting for an editor).
configure_jj() {
  local jj="$BIN_DIR/jj"
  "$jj" config set --user user.name  "Histeries Agent"
  "$jj" config set --user user.email "agent@histeries.invalid"
  "$jj" config set --user ui.paginate "never"
  # The value must be the TOML *string* "false" (the `false` command), not the boolean.
  "$jj" config set --user ui.editor   '"false"'
  "$jj" config set --user ui.color    "never"
}

find_toolchain_channel() {
  local f ch
  for f in ./rust-toolchain.toml "${HISTERIES_REPO:-.}/rust-toolchain.toml" \
           /workspace/*/rust-toolchain.toml "$HOME"/*/rust-toolchain.toml; do
    [ -f "$f" ] || continue
    ch=$(sed -n 's/^[[:space:]]*channel[[:space:]]*=[[:space:]]*"\([^"]*\)".*/\1/p' "$f" | head -1)
    if [ -n "$ch" ]; then printf '%s\n' "$ch"; return; fi
  done
  printf 'stable\n'
}

install_rust() {
  local ch host
  if ! command -v rustup >/dev/null 2>&1 && [ ! -x "$HOME/.cargo/bin/rustup" ]; then
    host="${ARCH}-unknown-linux-gnu"
    log "installing rustup-init for $host"
    curl -fsSL --retry 3 --max-time 300 -o "$TMP/rustup-init" \
      "https://static.rust-lang.org/rustup/dist/${host}/rustup-init" || die "rustup-init download failed"
    curl -fsSL --retry 3 --max-time 60 -o "$TMP/rustup-init.sha256" \
      "https://static.rust-lang.org/rustup/dist/${host}/rustup-init.sha256" || die "rustup-init checksum download failed"
    [ "$(sha256sum "$TMP/rustup-init" | cut -d' ' -f1)" = "$(cut -d' ' -f1 "$TMP/rustup-init.sha256")" ] \
      || die "rustup-init checksum mismatch"
    chmod +x "$TMP/rustup-init"
    "$TMP/rustup-init" -y --profile minimal --default-toolchain none --no-modify-path
  fi
  export PATH="$HOME/.cargo/bin:$PATH"
  ch=$(find_toolchain_channel)
  log "rust toolchain channel: $ch"
  rustup toolchain install "$ch" --profile minimal --component rustfmt --component clippy
  rustup default "$ch"
  for t in $RUST_TARGETS; do
    rustup target add --toolchain "$ch" "$t" || warn "could not add target $t (continuing)"
  done
}

install_cargo_tools() {
  if [ "$ARCH" = "x86_64" ]; then
    have_version "$NEXTEST_VERSION" "$BIN_DIR/cargo-nextest" nextest --version || install_from_archive \
      "https://github.com/nextest-rs/nextest/releases/download/cargo-nextest-${NEXTEST_VERSION}/cargo-nextest-${NEXTEST_VERSION}-x86_64-unknown-linux-musl.tar.gz" \
      "$NEXTEST_SHA256_X86_64" cargo-nextest cargo-nextest
    have_version "$DENY_VERSION" "$BIN_DIR/cargo-deny" --version || install_from_archive \
      "https://github.com/EmbarkStudios/cargo-deny/releases/download/${DENY_VERSION}/cargo-deny-${DENY_VERSION}-x86_64-unknown-linux-musl.tar.gz" \
      "$DENY_SHA256_X86_64" cargo-deny cargo-deny
    have_version "$ABOUT_VERSION" "$BIN_DIR/cargo-about" --version || install_from_archive \
      "https://github.com/EmbarkStudios/cargo-about/releases/download/${ABOUT_VERSION}/cargo-about-${ABOUT_VERSION}-x86_64-unknown-linux-musl.tar.gz" \
      "$ABOUT_SHA256_X86_64" cargo-about cargo-about
  else
    # No pinned prebuilt binaries for this architecture: build from source (slow).
    command -v cargo >/dev/null 2>&1 || die "cargo is required to build the cargo tools on $ARCH"
    cargo install --locked --version "$NEXTEST_VERSION" cargo-nextest
    cargo install --locked --version "$DENY_VERSION" cargo-deny
    cargo install --locked --version "$ABOUT_VERSION" cargo-about
  fi
}

# The install script and the agent run in separate shells, so exports do not
# carry over; persist what the agent needs in the shell start-up files.
persist_env() {
  local f
  # ~/.bash_profile and ~/.bash_login shadow ~/.profile for bash login shells, but creating
  # them where they do not exist would shadow it too, so touch them only if present.
  for f in "$HOME/.bashrc" "$HOME/.profile" "$HOME/.bash_profile" "$HOME/.bash_login"; do
    case "$f" in */.bash_profile | */.bash_login) [ -f "$f" ] || continue ;; esac
    touch "$f"
    sed -i '/# >>> histeries environment >>>/,/# <<< histeries environment <<</d' "$f"
    {
      printf '%s\n' '# >>> histeries environment >>>'
      # shellcheck disable=SC2016  # $HOME and $PATH must expand when the file is read, not now
      printf 'export PATH="%s:$HOME/.cargo/bin:$PATH"\n' "$BIN_DIR"
      printf '%s\n' 'export CARGO_INCREMENTAL=0'
      printf '%s\n' 'export CARGO_PROFILE_DEV_DEBUG=line-tables-only'
      printf '%s\n' 'export NO_COLOR=1'
      printf '%s\n' '# <<< histeries environment <<<'
    } >> "$f"
  done
}

report() {
  export PATH="$BIN_DIR:$HOME/.cargo/bin:$PATH"
  log "---- installed ----"
  "$BIN_DIR/jj" --version
  if command -v cargo >/dev/null 2>&1; then cargo --version; rustc --version; fi
  "$BIN_DIR/cargo-nextest" nextest --version 2>&1 | head -1 || true
  "$BIN_DIR/cargo-deny" --version || true
  "$BIN_DIR/cargo-about" --version || true
  if command -v rustup >/dev/null 2>&1; then
    log "rust targets: $(rustup target list --installed | tr '\n' ' ')"
  fi
  log "done"
}

main() {
  detect_arch
  pick_bin_dir
  TMP=$(mktemp -d)
  trap cleanup EXIT
  install_jj
  configure_jj
  if [ "$SKIP_RUST" != "1" ]; then install_rust; fi
  install_cargo_tools
  persist_env
  report
}

if [ "${HISTERIES_SOURCE_ONLY:-0}" != "1" ]; then main "$@"; fi
