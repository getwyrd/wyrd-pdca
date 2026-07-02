#!/usr/bin/env bash
# Bootstrap EVERY tool `wyrd-pdca flow` needs, idempotently, on a machine of
# unknown state — the thing `make install` now delegates to. Safe to re-run: each
# tool is `command -v`-probed first and only installed if absent.
#
#   ./scripts/bootstrap-tools.sh          install what's missing, then the venv + console script
#   ./scripts/bootstrap-tools.sh --check  report each tool's status and exit (installs nothing)
#
# Tool classes (chosen in issue discussion):
#   system   git, make, python3-venv/ensurepip  -> `sudo apt-get` when available, else print
#            exact install commands and exit non-zero (never silently degrade a REQUIRED tool).
#   user     rustup->cargo/rustc, claude, codex  -> official curl installers, no sudo (~/.cargo,
#            ~/.local/bin). Node is NOT required.
#
# REQUIRED (flow cannot complete without them): python3+venv, git, gh, claude, cargo/rustc
#   (the gating C4-ci gate is `cargo xtask ci`). OPTIONAL: codex (advisory leaf is non-gating —
#   a miss degrades to a §6 note, so its install is best-effort and never fails the run).
set -uo pipefail

PYTHON="${PYTHON:-python3}"
CHECK_ONLY=0
[ "${1:-}" = "--check" ] && CHECK_ONLY=1
MISSING_REQUIRED=0

c_blue='\033[1;34m'; c_yellow='\033[1;33m'; c_red='\033[1;31m'; c_green='\033[1;32m'; c_off='\033[0m'
log()  { printf "${c_blue}[bootstrap]${c_off} %s\n"   "$*"; }
ok()   { printf "${c_green}[bootstrap] ok:${c_off} %s\n"   "$*"; }
warn() { printf "${c_yellow}[bootstrap] warn:${c_off} %s\n" "$*" >&2; }
err()  { printf "${c_red}[bootstrap] error:${c_off} %s\n"  "$*" >&2; }
have() { command -v "$1" >/dev/null 2>&1; }

# --- apt / sudo plumbing (system packages) ----------------------------------
SUDO=""; have sudo && [ "$(id -u)" -ne 0 ] && SUDO="sudo"
APT_UPDATED=0
apt_install() {                       # apt_install <pkg…> — 0 on success, 1 if apt unusable
  have apt-get || return 1
  if [ "$APT_UPDATED" -eq 0 ]; then $SUDO apt-get update -qq || return 1; APT_UPDATED=1; fi
  $SUDO apt-get install -y "$@"
}

# ensure_system <cmd> <apt-pkgs> <human hint>  — REQUIRED system tool via apt, else instruct+flag
ensure_system() {
  local cmd="$1" pkgs="$2" hint="$3"
  if have "$cmd"; then ok "$cmd present ($("$cmd" --version 2>&1 | head -1))"; return 0; fi
  if [ "$CHECK_ONLY" -eq 1 ]; then err "$cmd MISSING (install: $hint)"; MISSING_REQUIRED=1; return 0; fi
  log "installing $cmd via apt ($pkgs)"
  if apt_install $pkgs && have "$cmd"; then ok "$cmd installed"; else
    err "could not install $cmd automatically. Install it manually: $hint"; MISSING_REQUIRED=1
  fi
}

# --- REQUIRED: git, make, gh -------------------------------------------------
ensure_system git  "git"       "sudo apt-get install -y git"
ensure_system make "make"      "sudo apt-get install -y make"
# gh: flow scrapes the tracker (engine/scripts/scrape-notes.sh) and opens PRs through it.
ensure_system gh   "gh"        "https://github.com/cli/cli/blob/trunk/docs/install_linux.md — then: gh auth login"
if have gh && ! gh auth status >/dev/null 2>&1; then
  warn "gh is installed but NOT authenticated — run 'gh auth login' before a real flow (tracker scrape + PRs need it)."
fi

# --- REQUIRED: python3 + a venv that can pip (ensurepip) ---------------------
if have "$PYTHON"; then ok "$PYTHON present ($($PYTHON --version 2>&1))"; else
  err "$PYTHON MISSING — install Python >=3.11 (sudo apt-get install -y python3)"; MISSING_REQUIRED=1
fi
# ensurepip lives in the python3-venv apt package on Debian/Ubuntu; probe and repair it.
PYV="$($PYTHON -c 'import sys;print(f"{sys.version_info[0]}.{sys.version_info[1]}")' 2>/dev/null || echo 3)"
if ! $PYTHON -c 'import ensurepip' >/dev/null 2>&1; then
  if [ "$CHECK_ONLY" -eq 1 ]; then
    err "python venv/ensurepip MISSING (install: sudo apt-get install -y python3-venv python${PYV}-venv)"; MISSING_REQUIRED=1
  else
    log "python 'ensurepip' unavailable — installing python3-venv (fallback: get-pip at venv time)"
    apt_install python3-venv "python${PYV}-venv" || warn "apt could not add python3-venv; will bootstrap pip via get-pip.py instead"
  fi
fi

# --- REQUIRED: Rust toolchain (the gating C4-ci gate is `cargo xtask ci`) -----
ensure_rust() {
  if have cargo && have rustc; then ok "rust present ($(rustc --version 2>&1))"; return 0; fi
  if [ "$CHECK_ONLY" -eq 1 ]; then err "cargo/rustc MISSING (install: rustup — https://rustup.rs)"; MISSING_REQUIRED=1; return 0; fi
  log "installing Rust via rustup (user-space: ~/.rustup, ~/.cargo)"
  if curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain stable; then
    # shellcheck disable=SC1091
    [ -f "$HOME/.cargo/env" ] && . "$HOME/.cargo/env"
    if have cargo; then ok "rust installed ($(rustc --version 2>&1)) — new shells pick it up via ~/.cargo/env"; else
      err "rustup ran but cargo is not on PATH; add \$HOME/.cargo/bin to PATH"; MISSING_REQUIRED=1
    fi
  else err "rustup install failed (network?). Install manually from https://rustup.rs"; MISSING_REQUIRED=1; fi
}
ensure_rust

# --- REQUIRED: Claude Code CLI (all six leaves are family=\"claude\") ----------
ensure_claude() {
  if have claude; then ok "claude present ($(claude --version 2>&1 | head -1))"; return 0; fi
  if [ "$CHECK_ONLY" -eq 1 ]; then err "claude MISSING (install: curl -fsSL https://claude.ai/install.sh | bash)"; MISSING_REQUIRED=1; return 0; fi
  log "installing Claude Code CLI (official installer -> ~/.local/bin)"
  mkdir -p "$HOME/.local/bin"
  if curl -fsSL https://claude.ai/install.sh | bash; then
    have claude || export PATH="$HOME/.local/bin:$PATH"
    if have claude; then ok "claude installed ($(claude --version 2>&1 | head -1))"; else
      err "claude installed but not on PATH; add \$HOME/.local/bin to PATH"; MISSING_REQUIRED=1
    fi
  else err "claude install failed. Install manually: https://docs.claude.com/claude-code"; MISSING_REQUIRED=1; fi
}
ensure_claude

# --- OPTIONAL: OpenAI Codex CLI (advisory leaf, non-gating -> best-effort) ----
ensure_codex() {
  if have codex; then ok "codex present ($(codex --version 2>&1 | head -1))"; return 0; fi
  if [ "$CHECK_ONLY" -eq 1 ]; then warn "codex MISSING (optional; advisory review degrades to a §6 note)"; return 0; fi
  log "installing Codex CLI from the latest GitHub release (optional)"
  mkdir -p "$HOME/.local/bin"
  local arch tgt url tmp
  arch="$(uname -m)"; tgt="${arch}-unknown-linux-musl"
  url="$(curl -fsSL https://api.github.com/repos/openai/codex/releases/latest 2>/dev/null \
        | grep -oE '"browser_download_url"[[:space:]]*:[[:space:]]*"[^"]+"' \
        | sed -E 's/.*"(https[^"]+)".*/\1/' \
        | grep -E "${tgt}.*(\.tar\.gz|\.tgz)$" | head -1)"
  if [ -z "$url" ]; then
    warn "no codex ${tgt} release asset found — install manually (npm i -g @openai/codex) if you want the cross-vendor advisory. Continuing."
    return 0
  fi
  tmp="$(mktemp -d)"
  if curl -fsSL "$url" -o "$tmp/codex.tgz" && tar -xzf "$tmp/codex.tgz" -C "$tmp"; then
    local bin; bin="$(find "$tmp" -type f -name 'codex*' -perm -u+x | head -1)"
    [ -z "$bin" ] && bin="$(find "$tmp" -type f -name 'codex' | head -1)"
    if [ -n "$bin" ]; then install -m 0755 "$bin" "$HOME/.local/bin/codex"; have codex || export PATH="$HOME/.local/bin:$PATH"; ok "codex installed ($(codex --version 2>&1 | head -1))"
    else warn "codex archive had no codex binary; skipping (optional)."; fi
  else warn "codex download/extract failed; skipping (optional)."; fi
  rm -rf "$tmp"
}
ensure_codex

# --- console script: venv + editable install (robust to a pip-less stdlib) ---
install_console_script() {
  if [ "$CHECK_ONLY" -eq 1 ]; then
    if [ -x .venv/bin/wyrd-pdca ]; then ok "console script present (.venv/bin/wyrd-pdca)"; else err "console script MISSING (run without --check)"; MISSING_REQUIRED=1; fi
    return 0
  fi
  if [ ! -x .venv/bin/pip ]; then
    log "creating .venv + installing the console script (pip install -e .)"
    if "$PYTHON" -m venv .venv 2>/dev/null && [ -x .venv/bin/pip ]; then :; else
      warn "stdlib venv has no pip (ensurepip missing) — bootstrapping pip via get-pip.py"
      rm -rf .venv; "$PYTHON" -m venv --without-pip .venv
      curl -fsSL https://bootstrap.pypa.io/get-pip.py | .venv/bin/python -
    fi
  fi
  .venv/bin/pip install -q -e . && ok "console script installed (.venv/bin/wyrd-pdca)"
}
install_console_script

# --- summary ----------------------------------------------------------------
echo
if [ "$MISSING_REQUIRED" -ne 0 ]; then
  err "one or more REQUIRED tools are missing — see the lines above. Resolve them, then re-run."
  exit 1
fi
if [ "$CHECK_ONLY" -eq 1 ]; then ok "all REQUIRED tools present."; else
  ok "bootstrap complete — run the cycle via .venv/bin/wyrd-pdca (or 'wyrd-pdca' once .venv/bin is on PATH)."
  log "next: 'make setup' grants Claude workspace read so the interactive leaves don't prompt."
fi
