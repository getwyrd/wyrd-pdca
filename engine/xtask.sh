#!/usr/bin/env bash
# Delegate a PDCA gate to Wyrd's single-sourced gate runner, `cargo xtask` (ADR-0016).
#
# PDCA runs gate commands from THIS project's root (`pdca_harness.gates` uses
# `cwd=cfg.root`), but `cargo xtask` must run inside the Wyrd checkout. This wrapper is
# the one place that knows where that checkout is: it `cd`s there and execs
# `cargo xtask "$@"`. It re-declares NO gates — Wyrd owns every gate definition; PDCA
# only orchestrates this runner (`[gates] runner = "./engine/xtask.sh"`, subcmd = "ci").
#
# Checkout resolution: $WYRD_REPO if set, else the sibling default ~/wyrd/wyrd
# (this file lives at ~/wyrd/wyrd-pdca/engine/xtask.sh, so ../../wyrd).
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
wyrd_repo="${WYRD_REPO:-"$(cd "$here/../../wyrd" 2>/dev/null && pwd || true)"}"

if [[ -z "$wyrd_repo" || ! -f "$wyrd_repo/Cargo.toml" ]]; then
  echo "xtask.sh: Wyrd checkout not found (looked for a Cargo.toml at '${wyrd_repo:-<unset>}')." >&2
  echo "          Set WYRD_REPO to the Wyrd repo root, or place this project beside it (~/wyrd/wyrd)." >&2
  exit 2
fi

cd "$wyrd_repo"
exec cargo xtask "$@"
