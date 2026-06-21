#!/usr/bin/env bash
# Delegate a PDCA gate to Wyrd's single-sourced gate runner, `cargo xtask` (ADR-0016).
#
# PDCA runs gate commands from THIS project's root (`pdca_harness.gates` uses
# `cwd=cfg.root`), but `cargo xtask` must run inside the Wyrd checkout. This wrapper is
# the one place that knows where that checkout is: it `cd`s there and execs
# `cargo xtask "$@"`. It re-declares NO gates — Wyrd owns every gate definition; PDCA
# only orchestrates this runner (`[gates] runner = "./engine/xtask.sh"`, subcmd = "ci").
#
# The gate runs in the dedicated **cycle worktree** ($WYRD_CYCLE or ../../wyrd-cycle),
# NOT the live Wyrd checkout — it must test the SAME tree the builder edited, and a
# cycle must never mutate the human's working checkout (engine/cycle-worktree.sh
# creates/refreshes it; eduralph/pdca-harness#94). $WYRD_REPO still overrides for a
# bespoke setup.
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
wyrd_repo="${WYRD_REPO:-"${WYRD_CYCLE:-"$(cd "$here/../../wyrd-cycle" 2>/dev/null && pwd || true)"}"}"

if [[ -z "$wyrd_repo" || ! -f "$wyrd_repo/Cargo.toml" ]]; then
  echo "xtask.sh: Wyrd cycle worktree not found (looked for a Cargo.toml at '${wyrd_repo:-<unset>}')." >&2
  echo "          Run engine/cycle-worktree.sh to create it, or set WYRD_REPO/WYRD_CYCLE." >&2
  exit 2
fi

cd "$wyrd_repo"
exec cargo xtask "$@"
