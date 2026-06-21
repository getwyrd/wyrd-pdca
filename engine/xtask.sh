#!/usr/bin/env bash
# Delegate a PDCA gate to Wyrd's single-sourced gate runner, `cargo xtask` (ADR-0016).
#
# PDCA runs gate commands from THIS project's root (`pdca_harness.gates` uses
# `cwd=cfg.root`), but `cargo xtask` must run inside the Wyrd checkout. This wrapper is
# the one place that knows where that checkout is: it `cd`s there and execs
# `cargo xtask "$@"`. It re-declares NO gates — Wyrd owns every gate definition; PDCA
# only orchestrates this runner (`[gates] runner = "./engine/xtask.sh"`, subcmd = "ci").
#
# Where it runs, in priority order:
#   1. $PDCA_WORKTREE — the per-cycle git worktree the driver creates for Do/Check when
#      worktree isolation is on ([driver].worktree, native since eduralph/pdca-harness#94,
#      v0.30.0). The gate MUST test the SAME tree the builder edited, so this wins.
#   2. $WYRD_REPO — explicit override for a bespoke setup.
#   3. ../wyrd — the sibling primary checkout (isolation off, or a --working-tree run).
# A cycle never mutates the human's checkout: with isolation on, (1) is a throwaway
# worktree; only an explicit isolation-off run touches ../wyrd directly.
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
wyrd_repo="${PDCA_WORKTREE:-"${WYRD_REPO:-"$(cd "$here/../../wyrd" 2>/dev/null && pwd || true)"}"}"

if [[ -z "$wyrd_repo" || ! -f "$wyrd_repo/Cargo.toml" ]]; then
  echo "xtask.sh: Wyrd checkout not found (looked for a Cargo.toml at '${wyrd_repo:-<unset>}')." >&2
  echo "          Expected \$PDCA_WORKTREE (set by the driver), \$WYRD_REPO, or a sibling ../wyrd." >&2
  exit 2
fi

cd "$wyrd_repo"
exec cargo xtask "$@"
