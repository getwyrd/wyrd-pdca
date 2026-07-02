#!/usr/bin/env bash
# Instance gate-toolchain hook (issue #207, tier 3) — Wyrd's gating C4-ci check is
# `cargo xtask ci`, so the Rust toolchain is a REQUIRED prerequisite the generic
# bootstrap-tools.sh can't know about. It runs this hook in both modes with $CHECK_ONLY
# set; a non-zero exit is treated as a REQUIRED miss. Idempotent — rustup installs
# user-space (~/.cargo, ~/.rustup), no sudo; a present toolchain is a no-op.
set -uo pipefail
CHECK_ONLY="${CHECK_ONLY:-0}"
have() { command -v "$1" >/dev/null 2>&1; }

if have cargo && have rustc; then
  echo "  rust: OK ($(rustc --version 2>&1))"
  exit 0
fi
if [ "$CHECK_ONLY" = 1 ]; then
  echo "  rust: MISSING — rustup (https://rustup.rs); the C4-ci gate 'cargo xtask ci' needs cargo/rustc" >&2
  exit 1
fi
echo "  rust: installing via rustup (user-space ~/.cargo, ~/.rustup)"
if curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain stable; then
  # shellcheck disable=SC1091
  [ -f "$HOME/.cargo/env" ] && . "$HOME/.cargo/env"
  if have cargo; then echo "  rust: installed ($(rustc --version 2>&1))"; exit 0; fi
  echo "  rust: rustup ran but cargo is not on PATH; add \$HOME/.cargo/bin" >&2; exit 1
fi
echo "  rust: rustup install failed (network?); install manually from https://rustup.rs" >&2
exit 1
