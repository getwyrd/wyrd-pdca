#!/usr/bin/env bash
# C4-verify (bundle-scoped): prove the bundle's patch is a REAL fix for Wyrd.
#
# Wired from pdca.toml:
#   [[gates.checks]] id="C4-verify" tier="C4" cmd="./engine/scripts/run-verify.sh" scope="bundle"
#
# Unlike the whole-tree C4-ci gate (`cargo xtask ci` on the working tree), this
# applies $PDCA_BUNDLE/patch.diff to a CLEAN checkout and runs ONLY the test the
# patch ships, asserting the regression contract:
#   * GREEN  with the fix applied, and
#   * RED    with the production change reverted (the added test kept) — i.e. the
#     test really catches the bug the fix resolves.
# Passes iff green-with-fix AND red-without-fix.
#
# Co-located test (the test lives INSIDE a modified production file, so the patch
# adds no separate `*/tests/*.rs`): the fix and test can't be split, so the gate
# runs GREEN-ONLY with a warning and passes on green — the whole-tree C4-ci still
# gates it. Ship the test as its own file (crates/<c>/tests/<t>.rs) to earn the
# full red->green.
#
# Isolation: runs in a dedicated `../wyrd-verify` git worktree off origin/main —
# never the live checkout or the cycle worktree. $WYRD_REPO / $WYRD_VERIFY override.
#
# Lane-safe (docs 09 §parallel lanes): under in-driver concurrency the driver pins each
# worker to a slot and exports $PDCA_LANE (0..N-1); a serial run leaves it unset. The
# per-fix verify worktree AND the branch it checks out are a shared mutable resource, so
# BOTH are scoped per lane — two concurrent lanes never collide on the same checkout dir
# nor try to check out one branch in two worktrees. Mirrors the driver's own
# `<name>.pdca-wt-l<slot>` worktree naming (worktree.py). Serial → `../wyrd-verify` on
# branch `pdca-verify`, unchanged.
#
#   run-verify.sh --classify <patch>     # print the file classification + exit (test hook)
#   run-verify.sh --print-isolation      # print the lane-scoped VERIFY dir + branch (test hook)
set -euo pipefail

case "${1:-}" in
  -h | --help) awk 'NR==1{next} /^#/{sub(/^# ?/,"");print;next} {exit}' "$0"; exit 0 ;;
esac

# --- lane-scoped verify worktree + branch (shared by the run and the test hook) -------
_here()          ( cd "$(dirname "${BASH_SOURCE[0]}")" && pwd )
_lane_suffix()   { printf '%s' "${PDCA_LANE:+-l$PDCA_LANE}"; }
_verify_dir()    { printf '%s' "${WYRD_VERIFY:-"$(_here)/../../../wyrd-verify$(_lane_suffix)"}"; }
_verify_branch() { printf '%s' "pdca-verify$(_lane_suffix)"; }

if [ "${1:-}" = "--print-isolation" ]; then
  echo "VERIFY $(basename "$(_verify_dir)")"
  echo "BRANCH $(_verify_branch)"
  exit 0
fi

# --- pure patch-classification helpers (unit-tested via --classify) ---------------
# Every `+++ b/<path>` is a changed file; a `--- /dev/null` immediately before it
# means the patch ADDS that file (untracked after `git apply` — revert by `rm`, not
# `git checkout`). An added file under a `tests/` dir ending `.rs` is the discriminator.
_all_files()    { awk '/^\+\+\+ b\//{p=$0;sub(/^\+\+\+ b\//,"",p);print p}' "$1"; }
_added_files()  { awk '/^--- /{prev=$0;next} /^\+\+\+ b\//{p=$0;sub(/^\+\+\+ b\//,"",p); if(prev=="--- /dev/null")print p}' "$1"; }
_is_test_file() { case "$1" in */tests/*.rs | tests/*.rs) return 0 ;; *) return 1 ;; esac; }
# Wyrd layout: packages live at `crates/<name>/` and `xtask/`. A file's crate dir
# (empty for root-level docs/CI files) maps to its cargo package.
_crate_dir()    { case "$1" in crates/*/*) echo "crates/$(echo "$1" | cut -d/ -f2)" ;; xtask/*) echo "xtask" ;; *) echo "" ;; esac; }
_in()           { local x="$1"; shift; local e; for e in "$@"; do [ "$e" = "$x" ] && return 0; done; return 1; }

# --classify <patch>: emit `ADDED_TEST <f>` per discriminator test and `CRATE <dir>`
# per affected crate dir (deduped, in order). No worktree, no cargo — for engine/tests.
if [ "${1:-}" = "--classify" ]; then
  cp="${2:?--classify needs a patch path}"
  while IFS= read -r f; do [ -n "$f" ] && _is_test_file "$f" && echo "ADDED_TEST $f"; done < <(_added_files "$cp")
  declare -A _seen=()
  while IFS= read -r f; do
    c="$(_crate_dir "$f")"; [ -n "$c" ] || continue
    [ -n "${_seen[$c]:-}" ] && continue
    echo "CRATE $c"; _seen["$c"]=1
  done < <(_all_files "$cp")
  exit 0
fi

BUNDLE="${PDCA_BUNDLE:?run-verify.sh is bundle-scoped — \$PDCA_BUNDLE must be set}"
PATCH_REL="$BUNDLE/patch.diff"
[ -f "$PATCH_REL" ] || { echo "run-verify.sh: no patch.diff in $BUNDLE" >&2; exit 1; }
PATCH="$(cd "$(dirname "$PATCH_REL")" && pwd)/$(basename "$PATCH_REL")"

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
WYRD_REPO="${WYRD_REPO:-"$(cd "$here/../../../wyrd" 2>/dev/null && pwd || true)"}"
VERIFY="$(_verify_dir)"
VERIFY_BRANCH="$(_verify_branch)"

if [ -z "$WYRD_REPO" ] || [ ! -f "$WYRD_REPO/Cargo.toml" ]; then
  echo "run-verify.sh: live Wyrd repo not found (set WYRD_REPO, or place this project beside ~/wyrd/wyrd)." >&2
  exit 2
fi

# --- dedicated verification worktree, clean at origin/main every run --------------
git -C "$WYRD_REPO" fetch -q origin 2>/dev/null || true
git -C "$WYRD_REPO" worktree prune
if [ ! -e "$VERIFY/Cargo.toml" ]; then
  git -C "$WYRD_REPO" worktree add -q -B "$VERIFY_BRANCH" "$VERIFY" origin/main
fi
git -C "$VERIFY" reset -q --hard origin/main
git -C "$VERIFY" clean -fdq
VERIFY="$(cd "$VERIFY" && pwd)"

_pkg_name() { local c="$1"; [ -f "$VERIFY/$c/Cargo.toml" ] && sed -n 's/^name *= *"\(.*\)".*/\1/p' "$VERIFY/$c/Cargo.toml" | head -1; }

mapfile -t ALL   < <(_all_files "$PATCH" | sort -u)
mapfile -t ADDED < <(_added_files "$PATCH" | sort -u)
ADDED_TESTS=()
for f in "${ADDED[@]:-}"; do [ -n "$f" ] && _is_test_file "$f" && ADDED_TESTS+=("$f"); done

# --- map changed files -> the cargo test targets to run --------------------------
declare -A SEEN_PKG=()
TEST_ARGS=()
if [ "${#ADDED_TESTS[@]}" -gt 0 ]; then
  for t in "${ADDED_TESTS[@]}"; do
    c="$(_crate_dir "$t")"; [ -n "$c" ] || continue
    pkg="$(_pkg_name "$c")"; [ -n "$pkg" ] || continue
    TEST_ARGS+=("-p" "$pkg" "--test" "$(basename "$t" .rs)"); SEEN_PKG["$pkg"]=1
  done
fi
# Fallback / co-located: scope to the affected packages and run their tests.
if [ "${#TEST_ARGS[@]}" -eq 0 ]; then
  for f in "${ALL[@]}"; do
    c="$(_crate_dir "$f")"; [ -n "$c" ] || continue
    pkg="$(_pkg_name "$c")"; [ -n "$pkg" ] || continue
    [ -n "${SEEN_PKG[$pkg]:-}" ] && continue
    TEST_ARGS+=("-p" "$pkg"); SEEN_PKG["$pkg"]=1
  done
fi
if [ "${#TEST_ARGS[@]}" -eq 0 ]; then
  echo "run-verify.sh: patch touches no Wyrd crate (docs/CI only) — nothing to verify per-fix; the C4-ci gate covers it." >&2
  exit 0
fi

run_test() { ( cd "$VERIFY" && cargo test --quiet "${TEST_ARGS[@]}" ); }

# --- GREEN: with the fix applied, the test passes --------------------------------
if ! git -C "$VERIFY" apply "$PATCH" 2>/dev/null; then
  echo "run-verify.sh: patch.diff does not apply on origin/main — the bundle is stale; rebase Do." >&2
  exit 1
fi
echo "run-verify.sh: GREEN — cargo test ${TEST_ARGS[*]} (fix applied)" >&2
if ! run_test; then
  echo "run-verify.sh: FAIL — the bundle's test is RED *with* the fix applied (not green)." >&2
  exit 1
fi

# --- RED: revert the production change, keep the added test, the test must fail ----
if [ "${#ADDED_TESTS[@]}" -eq 0 ]; then
  echo "run-verify.sh: PASS (green-only) — test is co-located with the fix (no separate */tests/*.rs)," >&2
  echo "               so the per-fix RED can't be isolated; C4-ci gates the whole tree. Ship the test" >&2
  echo "               as its own file to earn the full red->green." >&2
  exit 0
fi

git -C "$VERIFY" reset -q --hard origin/main
git -C "$VERIFY" clean -fdq
git -C "$VERIFY" apply "$PATCH"
for f in "${ALL[@]}"; do
  _in "$f" "${ADDED_TESTS[@]}" && continue          # keep the discriminator test(s)
  if _in "$f" "${ADDED[@]}"; then
    rm -f "$VERIFY/$f"                               # added non-test file -> remove
  else
    git -C "$VERIFY" checkout -q -- "$f"             # modified production file -> revert the fix
  fi
done
echo "run-verify.sh: RED — cargo test ${TEST_ARGS[*]} (production reverted, test kept)" >&2
if run_test; then
  echo "run-verify.sh: FAIL — the test PASSES without the fix, so it does not catch the bug (no red)." >&2
  exit 1
fi

echo "run-verify.sh: PASS — red without the fix, green with it." >&2
exit 0
