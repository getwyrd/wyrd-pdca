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
# Cfg-gated test targets (#104): a test whose crate root is `#![cfg(NAME)]` compiles to
# NOTHING without `--cfg NAME` — an empty binary that reports "running 0 tests" and exits 0.
# An exit-status check cannot tell that from a test that ran and passed, so the GREEN leg
# would measure a vacuum and the RED leg would call a correct bundle broken. The gate reads
# the cfg off the test sources it is about to compile and passes the same flags the crate's
# real command uses (for `crates/dst`: RUSTFLAGS=--cfg madsim + MADSIM_TEST_NUM, as
# xtask::run_dst does). $WYRD_VERIFY_MADSIM_SEEDS tunes the seed count.
#
# UNVERIFIABLE (exit 77 -> §6 NEEDS-HUMAN, non-gating): the gate could not MEASURE the
# bundle. Not a verdict on the fix — the absence of one.
#   * Zero tests ran (#114). `cargo test` exits 0 on a target that compiled to nothing, so an
#     exit-status check reads an empty binary as a pass. Scoring that green is a false green in
#     the very gate meant to prove the fix is real; scoring it red accuses a correct bundle of a
#     defect the exit code cannot evidence. Either leg reporting 0 tests is reported as what it
#     is. (#104 removes the dominant cause — a cfg gate — but a test can still vanish behind an
#     unset feature, an #[ignore], or a filter that matches nothing.)
#   * The RED leg failed WITHOUT running a test (Act 2026-08-02). A compile error exits
#     non-zero exactly as a failing assertion does, so "cargo failed" is not evidence of a
#     red; a discriminator that calls production API the patch ADDS cannot build once that
#     production change is reverted. This used to fall through to PASS — the gate meant to
#     prove the fix is real accepting a bundle whose test never built. See _red_verdict.
#   * Non-production (manifest) classification (issue #165, v0.43.0) is N/A for Wyrd: that
#     branch is for repos whose patch must touch a non-behavioral manifest the test can't move
#     (e.g. po/POTFILES.{in,skip}). Wyrd is a pure-Rust workspace with no such manifests.
#
# Isolation: runs in a dedicated `../wyrd-verify` git worktree off the bundle's base —
# `origin/main` by default, or the integration branch the brief targets, so a stacked
# milestone slice validates against its OWN base, not main (#91). The base is single-
# sourced from the brief's "Repo + branch target" field (the same value publish cuts the
# PR from), so C4-verify applies the patch on the same tree the PR opens against. Never the
# live checkout or the cycle worktree. $WYRD_REPO / $WYRD_VERIFY / $WYRD_VERIFY_BASE override.
#
# Driver-named bases (harness v0.54.0): the driver exports AT MOST ONE of
#   * $PDCA_BASE        (#54)  — the brief's `Onto branch` (an existing PR head publish
#                                commits onto), as a full `<remote>/<branch>` ref;
#   * $PDCA_VERIFY_BASE (#273) — the wave's folded integration branch
#                                (`origin/pdca-integration/<base>`) for a wave>0 bundle in a
#                                dependency batch, so a dependent verifies against
#                                base+prereqs instead of false-failing "patch does not
#                                apply" on a file it shares with its prereq.
# Either outranks every local resolution below — the test base must never diverge from the
# base publish commits to. Neither is set for an ordinary wave-0 single bundle.
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

# --- cfg-gated test targets (#104) ------------------------------------------------
# A test file whose CRATE ROOT is `#![cfg(NAME)]` compiles to NOTHING without `--cfg NAME`:
# the harness links an empty binary, prints "running 0 tests" and exits 0. That is
# indistinguishable, to an exit-status check, from a test that ran and passed — so the GREEN
# leg measures a vacuum and the RED leg concludes "the test PASSES without the fix" and fails
# a correct bundle. Every `crates/dst/tests/*.rs` is `#![cfg(madsim)]` (ADR-0009), which is
# why #258 had to be reconciled by hand.
#
# So: read the cfg off the test sources this run will COMPILE, and hand `cargo test` the same
# flags the crate's real command uses (wyrd `xtask::run_dst` appends `--cfg madsim` to
# RUSTFLAGS and sets MADSIM_TEST_NUM). Pure (file in, names out) so engine/tests can pin it.
_crate_cfgs() { # <source-file>... -> the crate-level cfg names, deduped
  local f
  for f in "$@"; do
    [ -f "$f" ] || continue
    sed -nE 's/^[[:space:]]*#!\[cfg\(([A-Za-z_][A-Za-z0-9_]*)\)\].*/\1/p' "$f"
  done | sort -u
}

# Env a cfg needs to run the way its real command runs it. madsim multiplies each
# `#[madsim::test]` across seeds; xtask::run_dst sets MADSIM_TEST_NUM=50, and the gate matches
# it so a seed-dependent red is not missed by a single-seed run. Override for a faster gate
# with $WYRD_VERIFY_MADSIM_SEEDS.
_cfg_extra_env() { # <cfg-name> -> zero or more KEY=VALUE lines
  case "$1" in
    madsim) printf 'MADSIM_TEST_NUM=%s\n' "${WYRD_VERIFY_MADSIM_SEEDS:-50}" ;;
  esac
}

# RUSTFLAGS with `--cfg <name>` appended for each cfg — APPENDED to any inherited RUSTFLAGS
# (never clobbered), mirroring xtask::run_dst.
_rustflags_with() { # <cfg-name>... -> the RUSTFLAGS value
  local rf="${RUSTFLAGS:-}" c
  for c in "$@"; do rf="${rf:+$rf }--cfg $c"; done
  printf '%s' "$rf"
}

# --- how many tests actually EXECUTED (#114) ---------------------------------------
# `cargo test` exits 0 when it runs ZERO tests, so exit status alone cannot tell "the test
# ran and passed" from "the target compiled to nothing". #104 fixes the dominant cause (a
# cfg gate), but a target still reports zero for other reasons — a `#[cfg(feature = "…")]`
# test whose feature the gate does not enable, every test `#[ignore]`d, a filter matching
# nothing. Sum the harness summaries instead:
#     test result: ok. 3 passed; 0 failed; 0 ignored; ...
# Counts passed+failed only: an `#[ignore]`d test asserted nothing, so it did not run.
_tests_ran() { # <cargo-test-output> -> total tests executed across every target
  printf '%s\n' "$1" \
    | sed -nE 's/^test result:.*[[:space:]]([0-9]+) passed;[[:space:]]([0-9]+) failed;.*/\1 \2/p' \
    | awk '{ t += $1 + $2 } END { print t + 0 }'
}

# --- the RED leg's verdict, from cargo's status AND what actually ran (Act 2026-08-02) ---
# The RED leg reverts production, keeps the added test, and expects a failing assertion.
# Cargo's exit status alone cannot say that happened, in EITHER direction:
#
#   rc  tests  verdict        why
#   --  -----  -----------    -------------------------------------------------------------
#   0     0    UNVERIFIABLE   empty target: exits 0 having asserted nothing (#114)
#   0    >0    FAIL           tests ran and passed without the fix — no red, real defect
#  !=0    0    UNVERIFIABLE   NOTHING RAN. A compile error exits non-zero exactly as a
#                             failing assertion does, and the test that never built proves
#                             nothing. This is the case that used to fall through to PASS.
#  !=0   >0    PASS           a test ran and failed without the fix — the genuine red
#
# The `!=0 / 0` cell is the one this function exists for. Before it, the RED leg's only
# question was "did cargo fail?", so a bundle whose discriminator did not COMPILE against
# the reverted base was recorded as proof that its test catches the bug — an evidence gate
# failing toward *accept*, in the direction that loses the proof rather than the data.
# It was reachable in practice: a test that calls net-new production API cannot build once
# that API is reverted, which is a routine shape here (the `GREEN_ONLY` escape covers only
# a net-new CRATE, not a net-new symbol in an existing one). Three briefs in the 2026-07-25
# multipart batch carried a paragraph telling Do to out-think it; two had a real instance
# caught by cross-vendor plan review. Queued by that review as PROPOSED
# (`process/act-log.md`, getwyrd/wyrd-pdca#178) and applied at the 2026-08-02 Act pass.
#
# UNVERIFIABLE is exit 77 at the call sites — a §6 NEEDS-HUMAN, never a patch defect: the
# leg gave no verdict, so it has none to hold against the fix either way.
_red_verdict() { # <cargo-exit-status> <tests-ran> -> PASS | FAIL | UNVERIFIABLE
  local rc="$1" ran="$2"
  [ "$ran" -eq 0 ] && { printf 'UNVERIFIABLE'; return 0; }
  [ "$rc" -eq 0 ] && { printf 'FAIL'; return 0; }
  printf 'PASS'
}
# The cargo package name from the patch's ADDED `<crate>/Cargo.toml` (a net-new crate the
# patch introduces — there is no pre-patch Cargo.toml to read). Pure patch parsing, "" if
# none — the fallback _pkg_name uses when a test's crate isn't in the worktree yet (#88).
_pkg_from_added_cargo() {
  awk -v want="$1/Cargo.toml" '
    /^\+\+\+ b\// { cur=$0; sub(/^\+\+\+ b\//, "", cur); next }
    cur==want && /^\+name[[:space:]]*=[[:space:]]*"/ {
      s=$0; sub(/^\+name[[:space:]]*=[[:space:]]*"/, "", s); sub(/".*/, "", s); print s; exit
    }
  ' "$2"
}

# The base branch named in the bundle's brief "Repo + branch target: <repo> @ <base>"
# field, or "" if absent. Mirrors publish._clean_ref EXACTLY so C4-verify resolves the SAME
# base publish cuts the PR from — a stacked milestone slice's integration branch, not a
# hardcoded main (#91). Pure brief parse; ALWAYS returns 0 so `set -e`/pipefail never aborts
# a bare `$(_brief_base)` (#88).
#
# The backtick span wins ONLY when it is the START of the field, never anywhere in it
# (#204, mirroring the upstream #235/#262 anchoring of _clean_ref). `main (feature branch
# `feat/x-slice`)` names the base **main**: the span is a parenthetical aside about a
# DIFFERENT branch. This twin kept the pre-#235 rule for a while and inverted the parity it
# exists to maintain — publish opened the PR against main while C4-verify validated against
# feat/x-slice, and the shape a planner actually writes (`main (verified at Plan:
# `origin/main` = `9120f7a`)`) yielded `origin/origin/main`, a ref resolving to nothing.
_brief_base() {
  local bp="${BUNDLE:-}/brief.md" line raw tok
  { [ -n "${BUNDLE:-}" ] && [ -f "$bp" ]; } || return 0
  line="$(grep -iE 'repo \+ branch' "$bp" | head -1 || true)"
  raw="${line#*@}"; [ "$raw" = "$line" ] && return 0     # no '@' → no base named
  raw="${raw#"${raw%%[![:space:]]*}"}"                   # lstrip, as _clean_ref's .strip()
  tok=""
  if [[ "$raw" == '`'*'`'* ]]; then                      # ANCHORED span: `<ref>`…
    tok="${raw#\`}"; tok="${tok%%\`*}"
  fi
  # Empty span (an adjacent `` pair) matches no ref: _clean_ref's `[^`]+` needs content,
  # so it falls through to the first token. Keep the twin agreeing on that edge too.
  [ -n "$tok" ] || tok="${raw%%[[:space:]]*}"
  # token.strip("`").rstrip(",.;:") — both strip REPEATEDLY, so loop rather than trim once.
  while [ -n "$tok" ] && [ "${tok#\`}" != "$tok" ]; do tok="${tok#\`}"; done
  while [ -n "$tok" ] && [ "${tok%\`}" != "$tok" ]; do tok="${tok%\`}"; done
  while [ -n "$tok" ]; do
    case "$tok" in *[,.\;:]) tok="${tok%?}";; *) break;; esac
  done
  printf '%s' "$tok"
}

# The remote-tracking base ref the patch must apply against, resolving the precedence the
# driver documents (#54/#273):
#   $PDCA_BASE > $PDCA_VERIFY_BASE (driver-named, full refs, at most one set — see header)
#   > $WYRD_VERIFY_BASE (explicit override) > brief target base > origin/main (#91).
# No git access (existence is checked at the call site), so it doubles as the --print-base
# unit hook.
_resolve_base_ref() {
  if [ -n "${PDCA_BASE:-}" ]; then printf '%s' "$PDCA_BASE"; return 0; fi
  if [ -n "${PDCA_VERIFY_BASE:-}" ]; then printf '%s' "$PDCA_VERIFY_BASE"; return 0; fi
  if [ -n "${WYRD_VERIFY_BASE:-}" ]; then printf '%s' "$WYRD_VERIFY_BASE"; return 0; fi
  local b; b="$(_brief_base)"
  if [ -n "$b" ]; then printf 'origin/%s' "$b"; else printf 'origin/main'; fi
}

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

# --pkg-name <crate> <patch>: the package name resolved from the patch's added Cargo.toml
# (the net-new-crate path of _pkg_name). No worktree, no cargo — for engine/tests (#88).
if [ "${1:-}" = "--pkg-name" ]; then
  _pkg_from_added_cargo "${2:?--pkg-name needs a crate dir}" "${3:?--pkg-name needs a patch path}"
  exit 0
fi

# --cfgs <source-file>...: the crate-level cfg gates of those sources (#104). No worktree,
# no cargo — for engine/tests.
if [ "${1:-}" = "--cfgs" ]; then
  shift
  _crate_cfgs "$@"
  exit 0
fi

# --tests-ran <file>: tests actually executed in a captured `cargo test` output (#114). No
# worktree, no cargo — for engine/tests.
if [ "${1:-}" = "--tests-ran" ]; then
  _tests_ran "$(cat "${2:?--tests-ran needs a file of cargo test output}")"
  exit 0
fi

# --red-verdict <rc> <tests-ran>: the RED leg's verdict for that pair, as the leg itself
# computes it. No worktree, no cargo — for engine/tests (Act 2026-08-02).
if [ "${1:-}" = "--red-verdict" ]; then
  _red_verdict "${2:?--red-verdict needs a cargo exit status}" "${3:?--red-verdict needs a tests-ran count}"
  echo
  exit 0
fi

# --print-base: print the resolved verify base ref for the bundle + exit (test hook, #91).
# Pure (env + brief parse, no git); the runtime additionally checks the ref exists.
if [ "${1:-}" = "--print-base" ]; then
  BUNDLE="${PDCA_BUNDLE:?--print-base needs \$PDCA_BUNDLE}"
  _resolve_base_ref; echo
  exit 0
fi

BUNDLE="${PDCA_BUNDLE:?run-verify.sh is bundle-scoped — \$PDCA_BUNDLE must be set}"
PATCH_REL="$BUNDLE/patch.diff"
[ -f "$PATCH_REL" ] || { echo "run-verify.sh: no patch.diff in $BUNDLE" >&2; exit 1; }
PATCH="$(cd "$(dirname "$PATCH_REL")" && pwd)/$(basename "$PATCH_REL")"

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=../lib/ensure-cargo.sh
. "$here/../lib/ensure-cargo.sh"   # defines ensure_cargo; called below, before any cargo use
WYRD_REPO="${WYRD_REPO:-"$(cd "$here/../../../wyrd" 2>/dev/null && pwd || true)"}"
VERIFY="$(_verify_dir)"
VERIFY_BRANCH="$(_verify_branch)"

if [ -z "$WYRD_REPO" ] || [ ! -f "$WYRD_REPO/Cargo.toml" ]; then
  echo "run-verify.sh: live Wyrd repo not found (set WYRD_REPO, or place this project beside ~/wyrd/wyrd)." >&2
  exit 2
fi

# --- resolve the base (origin/main, or the brief's integration branch for a stacked
#     slice; #91) and prepare a dedicated worktree clean at that base every run ----------
git -C "$WYRD_REPO" fetch -q origin 2>/dev/null || true
git -C "$WYRD_REPO" worktree prune
BASE_REF="$(_resolve_base_ref)"
if ! git -C "$WYRD_REPO" rev-parse --verify --quiet "${BASE_REF}^{commit}" >/dev/null 2>&1; then
  echo "run-verify.sh: base '$BASE_REF' not found on origin — falling back to origin/main;" >&2
  echo "               C4-verify may be unreliable for a stacked slice (#91)." >&2
  BASE_REF="origin/main"
fi
if [ ! -e "$VERIFY/Cargo.toml" ]; then
  git -C "$WYRD_REPO" worktree add -q -B "$VERIFY_BRANCH" "$VERIFY" "$BASE_REF"
fi
git -C "$VERIFY" reset -q --hard "$BASE_REF"   # re-points an existing worktree if the base changed
git -C "$VERIFY" clean -fdq
VERIFY="$(cd "$VERIFY" && pwd)"

# Cargo package name for crate dir `c`: from the worktree's Cargo.toml (an existing crate)
# or, for a crate the PATCH itself adds (net-new — no pre-patch Cargo.toml), from the added
# Cargo.toml carried in patch.diff. ALWAYS returns 0 (empty output ⇒ unresolved) so `set -e`
# never aborts a bare `pkg="$(_pkg_name ...)"` assignment (issue #88).
_pkg_name() {
  local c="$1"
  if [ -f "$VERIFY/$c/Cargo.toml" ]; then
    sed -n 's/^name *= *"\(.*\)".*/\1/p' "$VERIFY/$c/Cargo.toml" | head -1
    return 0
  fi
  _pkg_from_added_cargo "$c" "$PATCH"   # net-new crate: name from the patch's added Cargo.toml
  return 0
}

mapfile -t ALL   < <(_all_files "$PATCH" | sort -u)
mapfile -t ADDED < <(_added_files "$PATCH" | sort -u)
ADDED_TESTS=()
for f in "${ADDED[@]:-}"; do [ -n "$f" ] && _is_test_file "$f" && ADDED_TESTS+=("$f"); done

# --- map changed files -> the cargo test targets to run --------------------------
# Alongside the cargo args, record the test SOURCES this invocation will compile, so the
# cfg gate they sit behind can be read off them once the patch is applied (#104):
#   * added-test path — exactly the file we `--test`;
#   * fallback path   — the crate's whole tests/ dir, since a bare `-p <pkg>` compiles all.
declare -A SEEN_PKG=()
TEST_ARGS=()
TEST_SRC_FILES=()   # explicit test sources (relative to $VERIFY)
TEST_SRC_CRATES=()  # crate dirs whose tests/*.rs are all compiled
GREEN_ONLY=0
if [ "${#ADDED_TESTS[@]}" -gt 0 ]; then
  for t in "${ADDED_TESTS[@]}"; do
    c="$(_crate_dir "$t")"; [ -n "$c" ] || continue
    pkg="$(_pkg_name "$c")"; [ -n "$pkg" ] || continue
    TEST_ARGS+=("-p" "$pkg" "--test" "$(basename "$t" .rs)"); SEEN_PKG["$pkg"]=1
    TEST_SRC_FILES+=("$t")
    # A crate the patch itself CREATES has no pre-patch state, so its test is born green —
    # there is no per-fix RED to isolate (like a co-located test). Run green-only (#88).
    [ -f "$VERIFY/$c/Cargo.toml" ] || GREEN_ONLY=1
  done
fi
# Fallback / co-located: scope to the affected packages and run their tests.
if [ "${#TEST_ARGS[@]}" -eq 0 ]; then
  for f in "${ALL[@]}"; do
    c="$(_crate_dir "$f")"; [ -n "$c" ] || continue
    pkg="$(_pkg_name "$c")"; [ -n "$pkg" ] || continue
    [ -n "${SEEN_PKG[$pkg]:-}" ] && continue
    TEST_ARGS+=("-p" "$pkg"); SEEN_PKG["$pkg"]=1
    TEST_SRC_CRATES+=("$c")
  done
fi
if [ "${#TEST_ARGS[@]}" -eq 0 ]; then
  echo "run-verify.sh: patch touches no Wyrd crate (docs/CI only) — nothing to verify per-fix; the C4-ci gate covers it." >&2
  exit 0
fi

# Cargo is genuinely needed from here on — resolve it even under a bare PATH (CI/cron/the
# driver's subprocess). Placed AFTER the docs-only early-exit so a no-crate patch never
# requires a toolchain. See engine/lib/ensure-cargo.sh.
ensure_cargo || exit $?

TEST_ENV=()
TESTS_RAN=0
# Captures the harness output so a zero-test run can be told apart from a real pass (#114) —
# the output still reaches the operator on stderr. Sets $TESTS_RAN; returns cargo's status.
run_test() {
  local out rc=0
  out="$( ( cd "$VERIFY" && env "${TEST_ENV[@]+"${TEST_ENV[@]}"}" cargo test --quiet "${TEST_ARGS[@]}" ) 2>&1 )" || rc=$?
  printf '%s\n' "$out" >&2
  TESTS_RAN="$(_tests_ran "$out")"
  return "$rc"
}

# The cfg gate the test sources sit behind, and the env that satisfies it (#104). Computed
# once, AFTER the patch is applied (an added test does not exist in the worktree before
# that), and reused by the RED leg — which keeps the same test files.
_build_test_env() {
  local srcs=() c f kv
  for f in "${TEST_SRC_FILES[@]+"${TEST_SRC_FILES[@]}"}"; do [ -n "$f" ] && srcs+=("$VERIFY/$f"); done
  for c in "${TEST_SRC_CRATES[@]+"${TEST_SRC_CRATES[@]}"}"; do
    [ -n "$c" ] || continue
    for f in "$VERIFY/$c"/tests/*.rs; do [ -f "$f" ] && srcs+=("$f"); done
  done
  [ "${#srcs[@]}" -gt 0 ] || return 0

  local cfgs=()
  mapfile -t cfgs < <(_crate_cfgs "${srcs[@]}")
  [ "${#cfgs[@]}" -gt 0 ] && [ -n "${cfgs[0]:-}" ] || return 0

  TEST_ENV+=("RUSTFLAGS=$(_rustflags_with "${cfgs[@]}")")
  for c in "${cfgs[@]}"; do
    while IFS= read -r kv; do [ -n "$kv" ] && TEST_ENV+=("$kv"); done < <(_cfg_extra_env "$c")
  done
  echo "run-verify.sh: cfg-gated test target (${cfgs[*]}) — without the flag it compiles to 0" >&2
  echo "               tests and the gate measures nothing; running with ${TEST_ENV[*]} (#104)." >&2
}

# --- GREEN: with the fix applied, the test passes --------------------------------
if ! git -C "$VERIFY" apply "$PATCH" 2>/dev/null; then
  echo "run-verify.sh: patch.diff does not apply on $BASE_REF — the bundle is stale; rebase Do." >&2
  exit 1
fi
_build_test_env
echo "run-verify.sh: GREEN — cargo test ${TEST_ARGS[*]} (fix applied)" >&2
if ! run_test; then
  echo "run-verify.sh: FAIL — the bundle's test does not pass with the fix applied (it failed to" >&2
  echo "               build, or it ran and failed)." >&2
  exit 1
fi
# A target that compiled to nothing exits 0 (#114). That is not a green — it is the absence of
# a measurement, and passing it would be a false green in the very gate meant to prove the fix
# is real. Report it as what it is rather than inventing a verdict.
if [ "$TESTS_RAN" -eq 0 ]; then
  echo "run-verify.sh: UNVERIFIABLE — the target ran 0 tests with the fix applied, so the GREEN" >&2
  echo "               leg asserted nothing (\`cargo test\` exits 0 on an empty target). The test is" >&2
  echo "               compiled out: a cfg the gate does not set (#104), a feature it does not" >&2
  echo "               enable, every test #[ignore]d, or a filter that matches nothing." >&2
  exit 77
fi

# --- RED: revert the production change, keep the added test, the test must fail ----
if [ "${#ADDED_TESTS[@]}" -eq 0 ] || [ "$GREEN_ONLY" = 1 ]; then
  if [ "$GREEN_ONLY" = 1 ]; then
    echo "run-verify.sh: PASS (green-only) — the test lives in a crate this patch CREATES, so it has" >&2
    echo "               no pre-patch state to isolate a RED against; C4-ci gates the whole tree (#88)." >&2
  else
    echo "run-verify.sh: PASS (green-only) — test is co-located with the fix (no separate */tests/*.rs)," >&2
    echo "               so the per-fix RED can't be isolated; C4-ci gates the whole tree. Ship the test" >&2
    echo "               as its own file to earn the full red->green." >&2
  fi
  exit 0
fi

git -C "$VERIFY" reset -q --hard "$BASE_REF"
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
RED_RC=0
run_test || RED_RC=$?
case "$(_red_verdict "$RED_RC" "$TESTS_RAN")" in
  UNVERIFIABLE)
    # Neither direction is claimable: nothing executed. Which of the two ways it got here
    # matters to the human, so name it (cargo's status is the only thing that tells them
    # apart) — but both are exit 77, a §6 item, never a verdict on the patch.
    if [ "$RED_RC" -eq 0 ]; then
      echo "run-verify.sh: UNVERIFIABLE — with production reverted the target ran 0 tests, so no RED" >&2
      echo "               could be established. This is NOT 'the test passes without the fix' — the" >&2
      echo "               test never ran. It is compiled out: a cfg the gate does not set (#104), a" >&2
      echo "               feature it does not enable, every test #[ignore]d, or a filter matching" >&2
      echo "               nothing." >&2
    else
      echo "run-verify.sh: UNVERIFIABLE — the RED leg's cargo run failed (status $RED_RC) WITHOUT" >&2
      echo "               running a test, so no RED was established. This is NOT 'red without the" >&2
      echo "               fix' — the discriminator never executed. The usual cause is that it does" >&2
      echo "               not COMPILE against the reverted base: it calls production API this patch" >&2
      echo "               adds, so reverting the fix removes the symbol it needs. Split the test so" >&2
      echo "               it exercises the behaviour through pre-existing API, or record at sign-off" >&2
      echo "               why this slice has no isolable red (the cargo output is above)." >&2
    fi
    exit 77
    ;;
  FAIL)
    echo "run-verify.sh: FAIL — the test PASSES without the fix ($TESTS_RAN test(s) ran), so it does" >&2
    echo "               not catch the bug (no red)." >&2
    exit 1
    ;;
esac

echo "run-verify.sh: PASS — red without the fix, green with it ($TESTS_RAN test(s) ran red)." >&2
exit 0
