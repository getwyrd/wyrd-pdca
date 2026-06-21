#!/usr/bin/env bash
# Create / refresh the dedicated Wyrd **cycle worktree**, so a PDCA cycle's Do (and
# the Check gate) run in an ISOLATED git worktree off `main` — never mutating the
# live Wyrd checkout a human is working in.
#
# This is the render-level stopgap for the harness's in-place-edit default
# (the builder runs `--add-dir <checkout>` with acceptEdits against the shared
# checkout): see eduralph/pdca-harness#94. When the harness adopts per-cycle
# worktrees by default, this script goes away.
#
# Paths (sibling convention): the live Wyrd repo is $WYRD_REPO or ../../wyrd; the
# cycle worktree is $WYRD_CYCLE or ../../wyrd-cycle, on branch `pdca-cycle`.
#
# Usage:
#   engine/cycle-worktree.sh           # ensure the worktree exists (create if absent)
#   engine/cycle-worktree.sh --reset   # also reset it clean to origin/main (run before a cycle)
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
wyrd_repo="${WYRD_REPO:-"$(cd "$here/../../wyrd" 2>/dev/null && pwd || true)"}"
cycle="${WYRD_CYCLE:-"$here/../../wyrd-cycle"}"
base="origin/main"

if [[ -z "$wyrd_repo" || ! -f "$wyrd_repo/Cargo.toml" ]]; then
  echo "cycle-worktree.sh: live Wyrd checkout not found (set WYRD_REPO, or place this" >&2
  echo "                   project beside it at ~/wyrd/wyrd)." >&2
  exit 2
fi

git -C "$wyrd_repo" fetch --quiet origin || true
git -C "$wyrd_repo" worktree prune

if [[ ! -e "$cycle/Cargo.toml" ]]; then
  echo "cycle-worktree.sh: creating worktree at $cycle (branch pdca-cycle off $base)" >&2
  git -C "$wyrd_repo" worktree add -B pdca-cycle "$cycle" "$base"
fi

if [[ "${1:-}" == "--reset" ]]; then
  echo "cycle-worktree.sh: resetting $cycle clean to $base" >&2
  git -C "$cycle" reset --hard "$base"
  git -C "$cycle" clean -fdq
fi

echo "cycle worktree ready: $(cd "$cycle" && pwd) [$(git -C "$cycle" rev-parse --abbrev-ref HEAD) @ $(git -C "$cycle" rev-parse --short HEAD)]" >&2
