#!/usr/bin/env bash
# C4-diff-cov (bundle-scoped, ADVISORY): how much of the bundle's patch does its own test
# actually EXECUTE?
#
# Wired from pdca.toml:
#   [[gates.checks]] id="C4-diff-cov" tier="C4" cmd="./engine/scripts/run-diff-cov.sh" scope="bundle"
#
# Check already probes the diff from two sides. C4-verify's red->green proves the shipped test
# catches the ONE bug the fix resolves. C5-mutants finds changed code whose breakage no test
# notices. This row fills the gap between them, and is the cheap one: `cargo mutants` rebuilds
# and retests per mutant (it once held a Check beat for 19h16m — see default_timeout_secs in
# pdca.toml), while coverage is one instrumented build plus one test run, so it can run every
# cycle.
#
# REACH, NOT STRENGTH. A surviving mutant cannot say whether the line was never executed or
# was executed without an assertion binding it. This gate answers only the first: the line ran.
# A covered-but-unasserted line is C5-mutants' finding, not this one's, and the miss lines
# below say so rather than implying a test is adequate because it reached the code.
#
# COVERAGE BOUNDARY (what this gate does and does NOT see — stated because a guard whose real
# reach is narrower than its claimed scope is this instance's `enforcement-reach` class,
# process/act-log.md 2026-07-22, four brand-new guards each shipping with a hole):
#   * Scores lines in `.rs` files under a cargo package (crates/*, xtask) that the patch ADDS
#     or MODIFIES. Deleted lines, docs, CI config and Cargo.toml are outside it.
#   * Excludes test files (*/tests/*.rs) from BOTH numerator and denominator, added or
#     modified — `run-verify.sh --is-test` decides, so there is one spelling of that rule.
#     (cargo-llvm-cov's own --ignore-filename-regex already drops tests/, examples/ and
#     benches/; this is the belt to that braces, and it also drops non-.rs files.)
#   * Excludes any changed line with NO lcov `DA:` record — a comment, a `use`, a bare `}`,
#     a blank. The compiler generated no coverage region there, so it is not a line a test
#     could execute, and counting it would be a false red (the `false-red` class, same Act
#     pass). Measured, not assumed: see engine/tests/test_run_diff_cov.sh case 6.
#   * Measures under the SHIPPED TEST ALONE (the same `-p <pkg> --test <name>` C4-verify
#     computes). A co-located test has no separate target, so the run degrades to `-p <pkg>`
#     — the package's whole test suite — which is a weaker claim and is reported as such.
#   * Does NOT exclude a co-located `#[cfg(test)] mod tests` block. Its lines sit in a
#     production FILE, so the file-level exclusion above cannot see them, and they are covered
#     by definition (they are the code doing the covering) — so a patch shipping its test
#     inline scores GENEROUSLY, by roughly the size of that block. Measured, not assumed: the
#     same planted fixture scores 6/10 with the test in `tests/`, 9/13 with it inline.
#     Excluding it would mean parsing Rust attribute scopes in awk, which is a worse trade than
#     naming the bias — and it points the same way as the advisory default and the
#     `promote_after` ladder: this row informs sign-off, it does not yet block on a number.
#
# UNVERIFIABLE (exit 77 -> SUMMARY §6 NEEDS-HUMAN, non-gating): the gate could not MEASURE the
# bundle. Not a verdict on the fix — the absence of one. `engine/README.md` states the rule the
# whole engine follows: a gate never turns "no evidence" into a verdict. Five ways to get here:
#   * cargo-llvm-cov is not installed (see [[doctor.checks]] 'cargo-llvm-cov');
#   * the toolchain has no llvm-tools component. Checked UP FRONT and deliberately: without
#     it `cargo llvm-cov` PROMPTS on stdin ("I will run `rustup component add
#     llvm-tools-preview ...`. Proceed? [Y/n]") and a gate that blocks on a prompt hangs until
#     the 7200s timeout. Every cargo call below also runs with stdin at /dev/null so no
#     future prompt can do the same. Installing a toolchain component is a networked side
#     effect and is NOT this gate's to perform — it reports and stops.
#   * zero tests ran. `--cfg`-gated targets are the live cause here: every crates/dst test is
#     `#![cfg(madsim)]` and compiles to an EMPTY binary without the flag (#104). Measured
#     during #197: cargo-llvm-cov's wrapper APPENDS its instrumentation to inherited
#     RUSTFLAGS, so `--cfg madsim` does compose and the gated target does run — but the
#     count is checked anyway, because that is a property of the tool, not of this script;
#   * the shipped test did not PASS. Coverage of a failing fix measures nothing about the
#     fix, and whether the test should pass is C4-verify's verdict to give, not this row's;
#   * NONE of the changed production lines carry a `DA:` record. The instrumentation never
#     reached those files, which is a broken measurement, not 0% coverage.
#
# Isolation: its own `../wyrd-cov` git worktree off the bundle's resolved base, on branch
# `pdca-cov`. Separate from C4-verify's `../wyrd-verify` on purpose — llvm-cov builds under
# `-C instrument-coverage` into `target/llvm-cov-target/`, and pointing the two gates at one
# checkout would have each invalidating the other's cache every cycle. $WYRD_COV / $WYRD_REPO
# override the paths; the base ladder is run-verify.sh's, read through `--print-base`.
#
# Lane-safe (docs 09 §parallel lanes, pdca.toml "Any FUTURE gate that adds a container / port /
# scratch dir must likewise $PDCA_LANE-qualify it"): under in-driver concurrency BOTH the
# worktree dir and the branch it checks out are scoped by $PDCA_LANE, so two lanes never
# collide on a checkout nor try to check one branch out twice. Serial -> ../wyrd-cov on
# `pdca-cov`, exactly as C4-verify degrades to ../wyrd-verify.
#
# Footprint (issue #297: stale worktrees once reached >200 GB and false-redded GATING gates
# with 'Disk quota exceeded'): ../wyrd-cov* is ENGINE-owned, so `sweep_worktrees` does not
# know it — the same standing as ../wyrd-verify. The build cache is kept warm deliberately;
# what is reclaimed every run is the profile data (`cargo llvm-cov clean --profraw-only`),
# because a stale .profraw from a previous bundle would be merged into this bundle's numbers.
#
# REUSE, DON'T DUPLICATE. Everything C4-verify already decides is asked of it rather than
# re-spelled here — the base ladder (--print-base), the patch classification (--classify),
# the test-file and crate-dir rules (--is-test / --crate-dir), a net-new crate's package name
# (--pkg-name), crate-root cfg gates (--cfgs), and the executed-test count (--tests-ran).
# Those are pure hooks: no worktree, no cargo, each pinned by engine/tests/test_run_verify.sh.
# Two spellings of one rule drifting apart is the failure eduralph/pdca-harness#387 removed
# from the base parse, and it is not being reintroduced here.
#
#   run-diff-cov.sh --print-isolation          # the lane-scoped COV dir + branch (test hook)
#   run-diff-cov.sh --changed-lines <patch>    # `path:line` per scored line (test hook)
#   run-diff-cov.sh --score <lcov> <lines>     # MISS lines + `TOTAL <cov> <instr>` (test hook)
#   run-diff-cov.sh --verdict <cov> <instr> [min]   # PASS|FAIL|UNVERIFIABLE (test hook)
#   run-diff-cov.sh --act-line <crate> <cov> <instr> # the recurring-signal line (test hook)
set -euo pipefail

case "${1:-}" in
  -h | --help) awk 'NR==1{next} /^#/{sub(/^# ?/,"");print;next} {exit}' "$0"; exit 0 ;;
esac

# The share of instrumentable changed lines that must be executed. 80 rather than a strict 100
# because a real patch carries error branches a single regression test does not walk, and a
# threshold no honest bundle can meet would never earn its promote_after promotion — it would
# just train the human to skip the row.
DIFFCOV_MIN="${WYRD_DIFFCOV_MIN:-80}"

# --- lane-scoped coverage worktree + branch (shared by the run and the test hook) ------
# Mirrors run-verify.sh's _verify_dir/_verify_branch, including the deliberate asymmetry of
# the override: $WYRD_COV moves the DIR, but the branch stays lane-scoped either way, because
# the branch is the resource two concurrent lanes would actually fight over.
_here()       ( cd "$(dirname "${BASH_SOURCE[0]}")" && pwd )
_lane_suffix() { printf '%s' "${PDCA_LANE:+-l$PDCA_LANE}"; }
_cov_dir()    { printf '%s' "${WYRD_COV:-"$(_here)/../../../wyrd-cov$(_lane_suffix)"}"; }
_cov_branch() { printf '%s' "pdca-cov$(_lane_suffix)"; }

if [ "${1:-}" = "--print-isolation" ]; then
  echo "COV $(basename "$(_cov_dir)")"
  echo "BRANCH $(_cov_branch)"
  exit 0
fi

RV="$(_here)/run-verify.sh"

# --- the patch's added/modified lines, as `path:line` ------------------------------------
# Walks the unified diff and reports the NEW-file line number of every `+` line. A modified
# line is a `+` line too, so "added or modified" needs no second rule.
#
# The hunk body is tracked by COUNT, not by pattern: a hunk header declares how many old and
# new lines follow, ` ` consumes one of each, `+` one new, `-` one old, and the hunk ends when
# both reach zero. That is what makes a patch containing a patch parse correctly — a `+++ b/x`
# or `@@ ... @@` line INSIDE a hunk body is content the diff carries, not structure, and a
# parser that keys on the pattern alone would silently re-anchor onto it and misnumber every
# line after. `\ No newline at end of file` consumes nothing.
_line_records() { # <patch> -> `path:line` for every + line, unfiltered
  awk '
    # Structural lines are only structural OUTSIDE a hunk body.
    rem_new <= 0 && rem_old <= 0 {
      if ($0 ~ /^\+\+\+ /) {
        path = substr($0, 5)
        sub(/\t.*$/, "", path)                 # git appends a timestamp on some diff flavours
        if (path == "/dev/null") { path = "" } # a deleted file has no new-side lines
        else { sub(/^b\//, "", path) }
        next
      }
      if ($0 ~ /^@@ /) {
        # @@ -<os>[,<oc>] +<ns>[,<nc>] @@
        hdr = $0
        sub(/^@@ -/, "", hdr)
        split(hdr, part, / \+/)
        split(part[1], o, /,/); split(part[2], n, /,/)
        rem_old = (o[2] == "" ? 1 : o[2] + 0)
        new_at  = n[1] + 0
        sub(/ @@.*$/, "", n[2])
        rem_new = (n[2] == "" ? 1 : n[2] + 0)
        next
      }
      next
    }
    # Inside a hunk body.
    /^\\/  { next }                                        # \ No newline at end of file
    /^\+/  { if (path != "") print path ":" new_at; new_at++; rem_new--; next }
    /^-/   { rem_old--; next }
    { new_at++; rem_new--; rem_old-- }                      # context (leading space, or empty)
  ' "$1"
}

# The scored subset: production Rust under a cargo package. The two predicates come from
# run-verify.sh so there is one spelling of each; they are asked once per FILE, not per line.
_changed_lines() { # <patch> -> `path:line` for every scored line
  local rec path prev="" keep=0
  while IFS= read -r rec; do
    [ -n "$rec" ] || continue
    path="${rec%:*}"
    if [ "$path" != "$prev" ]; then
      prev="$path"; keep=0
      case "$path" in *.rs) ;; *) continue ;; esac
      [ "$("$RV" --is-test "$path")" = "no" ] || continue
      [ -n "$("$RV" --crate-dir "$path")" ] || continue
      keep=1
    fi
    [ "$keep" = 1 ] && printf '%s\n' "$rec"
  done < <(_line_records "$1")
  return 0
}

# --- scoring the changed lines against an lcov report -------------------------------------
# lcov records are `SF:<file>` then `DA:<line>,<hits>` until `end_of_record`. Two properties
# decide the arithmetic:
#   * `SF:` is an ABSOLUTE path (measured: it points into whichever worktree built the report),
#     while the patch's paths are repo-relative. So a record matches a changed file when the
#     SF equals it or ends with `/<changed path>` — a whole path component, never a bare
#     substring, or `core/src/lib.rs` would also claim `vendor/notcore/src/lib.rs`.
#   * A changed line with NO `DA:` record is NOT instrumentable and leaves the DENOMINATOR.
#     Comments, `use` lines, blanks and bare delimiters have no coverage region; scoring them
#     as uncovered would red a patch for lines no test could ever execute.
_score() { # <lcov> <lines-file> -> `MISS path:line`... then `TOTAL <covered> <instrumentable>`
  awk '
    FNR == NR {                                   # pass 1: the changed lines
      split($0, p, /:/); f = p[1]; l = p[2]
      if (f == "") next
      want[f "\t" l] = 1
      if (!(f in seenfile)) { seenfile[f] = 1; files[++nf] = f }
      order[++nrec] = f "\t" l
      next
    }
    /^SF:/ {                                      # pass 2: the report
      sf = substr($0, 4); cur = ""
      for (i = 1; i <= nf; i++) {
        f = files[i]
        if (sf == f || index(sf, "/" f) == length(sf) - length(f)) { cur = f; break }
      }
      next
    }
    /^DA:/ {
      if (cur == "") next
      rec = substr($0, 4)
      split(rec, d, /,/)
      k = cur "\t" d[1]
      if (!(k in want)) next
      instr[k] = 1
      if (d[2] + 0 > 0) hit[k] = 1
      next
    }
    /^end_of_record/ { cur = ""; next }
    END {
      c = 0; n = 0
      for (i = 1; i <= nrec; i++) {
        k = order[i]
        if (!(k in instr)) continue             # no region here — out of the denominator
        if (k in seenk) continue                # the same line twice in one patch
        seenk[k] = 1
        n++
        if (k in hit) { c++ } else { split(k, s, /\t/); print "MISS " s[1] ":" s[2] }
      }
      print "TOTAL " c " " n
    }
  ' "$2" "$1"
}

# --- the verdict, from the two counts -----------------------------------------------------
#   covered/instrumentable >= min  -> PASS
#   instrumentable == 0            -> UNVERIFIABLE. The caller only reaches this having found
#                                     changed production lines, so zero instrumentable means
#                                     the report covered none of the changed FILES at all —
#                                     a measurement that did not happen, not 0% coverage.
#                                     Reporting it as a fail would accuse a patch of a defect
#                                     the evidence cannot support.
#   otherwise                      -> FAIL
# Integer arithmetic (covered*100 >= min*instrumentable) — no bc dependency, and no float
# rounding deciding a boundary case.
_verdict() { # <covered> <instrumentable> [min] -> PASS | FAIL | UNVERIFIABLE
  local c="$1" n="$2" min="${3:-$DIFFCOV_MIN}"
  [ "$n" -eq 0 ] && { printf 'UNVERIFIABLE'; return 0; }
  [ $((c * 100)) -ge $((min * n)) ] && { printf 'PASS'; return 0; }
  printf 'FAIL'
}

_pct() { # <covered> <instrumentable> -> the percentage, one decimal
  awk -v c="$1" -v n="$2" 'BEGIN { printf "%.1f", (n == 0 ? 0 : c * 100 / n) }'
}

# --- the recurring-signal line (SUMMARY §10 shape) ----------------------------------------
# Act pools §10 Act-candidates and §6 items and calls two of them "the same" when their first
# EIGHT WORDS match, lowercased (act._norm). So a line that carries its numbers early can
# never match itself across cycles, and the recurrence this row exists to expose — one crate
# repeatedly shipping fixes its tests do not reach — would never register. Invariant class and
# crate first, every digit after word eight. engine/tests/test_run_diff_cov.sh pins that,
# because getting it wrong fails silently.
#
# Nothing writes §10 automatically: assemble.py builds it from the plan-advisory record alone,
# and the harness is out of scope for this instance-side row. The line is printed into the
# gate's own output (frozen at gate-logs/C4-diff-cov.log) for the human to lift at sign-off.
# eduralph/pdca-harness#406 is the upstream issue that would carry the numbers instead.
_act_line() { # <crate> <covered> <instrumentable> -> the normalized signal line
  local crate="$1" c="$2" n="$3"
  printf 'C4-diff-cov: uncovered changed lines in crate %s — %s of %s not executed by the shipped test (%s%%)' \
    "$crate" "$((n - c))" "$n" "$(_pct "$c" "$n")"
}

# --changed-lines <patch>: the scored `path:line` set. Calls run-verify.sh's pure hooks only —
# no worktree, no cargo, no git. For engine/tests.
if [ "${1:-}" = "--changed-lines" ]; then
  _changed_lines "${2:?--changed-lines needs a patch path}"
  exit 0
fi

# --score <lcov> <lines-file>: MISS lines + the TOTAL pair. No worktree, no cargo — for
# engine/tests.
if [ "${1:-}" = "--score" ]; then
  _score "${2:?--score needs an lcov path}" "${3:?--score needs a changed-lines file}"
  exit 0
fi

# --verdict <covered> <instrumentable> [min]: the verdict for that triple, as the run computes
# it. No worktree, no cargo — for engine/tests.
if [ "${1:-}" = "--verdict" ]; then
  _verdict "${2:?--verdict needs a covered count}" "${3:?--verdict needs an instrumentable count}" \
           "${4:-$DIFFCOV_MIN}"
  echo
  exit 0
fi

# --act-line <crate> <covered> <instrumentable>: the recurring-signal line. No worktree, no
# cargo — for engine/tests.
if [ "${1:-}" = "--act-line" ]; then
  _act_line "${2:?--act-line needs a crate}" "${3:?--act-line needs a covered count}" \
            "${4:?--act-line needs an instrumentable count}"
  echo
  exit 0
fi

# ==========================================================================================
# runtime
# ==========================================================================================
BUNDLE="${PDCA_BUNDLE:?run-diff-cov.sh is bundle-scoped — \$PDCA_BUNDLE must be set}"
PATCH_REL="$BUNDLE/patch.diff"
[ -f "$PATCH_REL" ] || { echo "run-diff-cov.sh: no patch.diff in $BUNDLE" >&2; exit 1; }
PATCH="$(cd "$(dirname "$PATCH_REL")" && pwd)/$(basename "$PATCH_REL")"
BUNDLE_ABS="$(cd "$BUNDLE" && pwd)"

# The bundle's coverage artifacts, rebuilt from scratch every run. `coverage/` is NOT in
# state.DOWNSTREAM_OF_BRIEF, so an iterate does not archive it with the attempt it describes —
# left in place, round N's numbers would be read as round N+1's own. Wiping here (rather than
# only writing on success) means every exit path below leaves a diff-cov.json describing THIS
# attempt, including the ones that measured nothing.
COV_OUT="$BUNDLE_ABS/coverage"
rm -rf "$COV_OUT"
mkdir -p "$COV_OUT"
LCOV="$COV_OUT/lcov.info"
JSON="$COV_OUT/diff-cov.json"

# Working files live OUTSIDE the bundle. Only the two artifacts above are the bundle's — the
# rest is this run's arithmetic, and a bundle is a frozen record that `pdca record` commits
# wholesale, so an intermediate left behind on any exit path becomes permanent noise in it.
TMPD="$(mktemp -d)"
trap 'rm -rf "$TMPD"' EXIT
CHANGED="$TMPD/changed-lines.txt"
MISS_FILE="$TMPD/missed"

# One JSON writer for every exit path. No jq: the harness does not depend on it, and a gate
# that needs a tool to report its own verdict has one more way to give none.
_json() { # <status> <reason> [covered] [instrumentable] [tests_ran] [base] [test_args]
  local status="$1" reason="$2" c="${3:-0}" n="${4:-0}" ran="${5:-0}" base="${6:-}" targs="${7:-}"
  {
    printf '{\n'
    printf '  "gate": "C4-diff-cov",\n'
    printf '  "status": "%s",\n' "$status"
    printf '  "reason": "%s",\n' "$reason"
    printf '  "min_pct": %s,\n' "$DIFFCOV_MIN"
    printf '  "diff_cov_pct": %s,\n' "$(_pct "$c" "$n")"
    printf '  "covered": %s,\n' "$c"
    printf '  "instrumentable": %s,\n' "$n"
    printf '  "changed_lines": %s,\n' "${CHANGED_TOTAL:-0}"
    printf '  "tests_ran": %s,\n' "$ran"
    printf '  "base_ref": "%s",\n' "$base"
    printf '  "test_args": "%s",\n' "$targs"
    printf '  "missed": ['
    if [ -s "${MISS_FILE:-/dev/null}" ]; then
      printf '\n'
      sed 's/^MISS //' "$MISS_FILE" | awk '{ printf "%s    \"%s\"", (NR>1 ? ",\n" : ""), $0 } END { printf "\n  " }'
    fi
    printf ']\n'
    printf '}\n'
  } > "$JSON"
}

# --- what to score -----------------------------------------------------------------------
_changed_lines "$PATCH" > "$CHANGED"
CHANGED_TOTAL="$(wc -l < "$CHANGED" | tr -d ' ')"

# Docs / CI-only, or a test-only patch: there is no production line to measure. A genuine N/A,
# not a missing measurement — and placed BEFORE ensure_cargo so a no-crate patch never needs a
# toolchain, the same ordering run-verify.sh uses for its own docs-only exit.
if [ "$CHANGED_TOTAL" -eq 0 ]; then
  _json "n/a" "patch changes no production Rust line under a cargo package"
  echo "run-diff-cov.sh: nothing to measure — the patch changes no production Rust line under a" >&2
  echo "                 cargo package (docs / CI / test-only). C4-ci and C4-verify cover it." >&2
  echo "PDCA-EVIDENCE: diff coverage n/a — no production Rust lines changed" >&2
  exit 0
fi

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=../lib/ensure-cargo.sh
. "$here/../lib/ensure-cargo.sh"   # defines ensure_cargo; called below, before any cargo use
ensure_cargo || exit $?

if ! cargo llvm-cov --version >/dev/null 2>&1; then
  _json "unverifiable" "cargo-llvm-cov is not installed"
  echo "run-diff-cov.sh: cargo-llvm-cov not installed — see [[doctor.checks]] 'cargo-llvm-cov'" >&2
  echo "                 (cargo install cargo-llvm-cov --locked)" >&2
  echo "PDCA-UNVERIFIABLE: cargo-llvm-cov is not installed, so diff coverage was not measured" >&2
  exit 77
fi

WYRD_REPO="${WYRD_REPO:-"$(cd "$here/../../../wyrd" 2>/dev/null && pwd || true)"}"
COV="$(_cov_dir)"
COV_BRANCH="$(_cov_branch)"

if [ -z "$WYRD_REPO" ] || [ ! -f "$WYRD_REPO/Cargo.toml" ]; then
  echo "run-diff-cov.sh: live Wyrd repo not found (set WYRD_REPO, or place this project beside ~/wyrd/wyrd)." >&2
  exit 2
fi

# --- a dedicated worktree, clean at the bundle's base every run ---------------------------
git -C "$WYRD_REPO" fetch -q origin 2>/dev/null || true
git -C "$WYRD_REPO" worktree prune
BASE_REF="$(PDCA_BUNDLE="$BUNDLE" "$RV" --print-base)"
if ! git -C "$WYRD_REPO" rev-parse --verify --quiet "${BASE_REF}^{commit}" >/dev/null 2>&1; then
  echo "run-diff-cov.sh: base '$BASE_REF' not found on origin — falling back to origin/main;" >&2
  echo "                 the coverage figure may not describe the base the PR opens against." >&2
  BASE_REF="origin/main"
fi
if [ ! -e "$COV/Cargo.toml" ]; then
  git -C "$WYRD_REPO" worktree add -q -B "$COV_BRANCH" "$COV" "$BASE_REF"
fi
git -C "$COV" reset -q --hard "$BASE_REF"
git -C "$COV" clean -fdq
COV="$(cd "$COV" && pwd)"

# The toolchain component llvm-cov needs, for the toolchain the WORKTREE resolves (the target
# pins one in rust-toolchain.toml, which this out-of-tree project must not edit). Probed here
# rather than left to cargo, because cargo-llvm-cov's own handling is an interactive prompt —
# a gate that blocks on stdin hangs until the timeout kills it.
#
# Only where rustup MANAGES the toolchain, though. On a distro//nix Rust there is no rustup to
# ask, and treating "cannot ask" as "the component is missing" would report every such host
# UNVERIFIABLE with a fix instruction it cannot follow — a guard whose reach exceeds its
# subject. There, fall through and let cargo decide; the </dev/null on every cargo call below
# is what keeps the prompt from hanging the gate in that case.
if command -v rustup >/dev/null 2>&1 \
   && ! (cd "$COV" && rustup component list --installed 2>/dev/null | grep -q '^llvm-tools'); then
  _tc="$( (cd "$COV" && rustup show active-toolchain 2>/dev/null | awk '{print $1; exit}') || true)"
  _json "unverifiable" "the ${_tc:-active} toolchain has no llvm-tools component"
  echo "run-diff-cov.sh: the ${_tc:-active} toolchain has no llvm-tools component, which" >&2
  echo "                 cargo-llvm-cov needs. Installing it is a networked change to the" >&2
  echo "                 host toolchain and is not this gate's to make:" >&2
  echo "                     rustup component add llvm-tools-preview --toolchain ${_tc:-<channel>}" >&2
  echo "PDCA-UNVERIFIABLE: no llvm-tools component for ${_tc:-the active toolchain}, so diff coverage was not measured" >&2
  exit 77
fi

if ! git -C "$COV" apply "$PATCH" 2>/dev/null; then
  _json "fail" "patch.diff does not apply on $BASE_REF" 0 0 0 "$BASE_REF"
  echo "run-diff-cov.sh: patch.diff does not apply on $BASE_REF — the bundle is stale; rebase Do." >&2
  echo "PDCA-EVIDENCE: diff coverage not measured — patch.diff does not apply on $BASE_REF" >&2
  exit 1
fi

# --- the same test targets C4-verify runs -------------------------------------------------
# Read off run-verify.sh's --classify, so "which test proves this patch" has one answer for
# both C4 rows. Added-test path first (`-p <pkg> --test <name>`), then the co-located /
# fallback path (`-p <pkg>`, the package's whole suite — a weaker claim, reported as one).
_pkg_name() { # <crate dir> -> the cargo package name, "" if unresolved
  local c="$1"
  if [ -f "$COV/$c/Cargo.toml" ]; then
    sed -n 's/^name *= *"\(.*\)".*/\1/p' "$COV/$c/Cargo.toml" | head -1
    return 0
  fi
  "$RV" --pkg-name "$c" "$PATCH"   # net-new crate: the name from the patch's added Cargo.toml
  return 0
}

ADDED_TESTS=(); CRATES=()
while IFS= read -r line; do
  case "$line" in
    "ADDED_TEST "*) ADDED_TESTS+=("${line#ADDED_TEST }") ;;
    "CRATE "*)      CRATES+=("${line#CRATE }") ;;
  esac
done < <("$RV" --classify "$PATCH")

declare -A SEEN_PKG=()
TEST_ARGS=(); TEST_SRC_FILES=(); TEST_SRC_CRATES=(); PKGS=()
for t in "${ADDED_TESTS[@]+"${ADDED_TESTS[@]}"}"; do
  c="$("$RV" --crate-dir "$t")"; [ -n "$c" ] || continue
  pkg="$(_pkg_name "$c")"; [ -n "$pkg" ] || continue
  TEST_ARGS+=("-p" "$pkg" "--test" "$(basename "$t" .rs)"); SEEN_PKG["$pkg"]=1; PKGS+=("$pkg")
  TEST_SRC_FILES+=("$t")
done
WHOLE_SUITE=0
if [ "${#TEST_ARGS[@]}" -eq 0 ]; then
  WHOLE_SUITE=1
  for c in "${CRATES[@]+"${CRATES[@]}"}"; do
    pkg="$(_pkg_name "$c")"; [ -n "$pkg" ] || continue
    [ -n "${SEEN_PKG[$pkg]:-}" ] && continue
    TEST_ARGS+=("-p" "$pkg"); SEEN_PKG["$pkg"]=1; PKGS+=("$pkg")
    TEST_SRC_CRATES+=("$c")
  done
fi
if [ "${#TEST_ARGS[@]}" -eq 0 ]; then
  # Changed production lines exist but no test target maps to them — nothing to measure them
  # WITH. Not a 0% score: no test ran, so nothing was measured.
  _json "unverifiable" "no cargo test target maps to the patch's crates" 0 0 0 "$BASE_REF"
  echo "run-diff-cov.sh: UNVERIFIABLE — the patch changes production lines but no cargo test" >&2
  echo "                 target maps to their crates, so there is no test whose reach could be" >&2
  echo "                 measured." >&2
  echo "PDCA-UNVERIFIABLE: no test target maps to the patch's crates, so diff coverage was not measured" >&2
  exit 77
fi

# The cfg gate the test sources sit behind (#104), read AFTER the patch is applied — an added
# test does not exist in the worktree before that. RUSTFLAGS is APPENDED to, never clobbered;
# cargo-llvm-cov then appends its own instrumentation on top (measured in #197).
TEST_ENV=()
_srcs=()
for f in "${TEST_SRC_FILES[@]+"${TEST_SRC_FILES[@]}"}"; do [ -n "$f" ] && _srcs+=("$COV/$f"); done
for c in "${TEST_SRC_CRATES[@]+"${TEST_SRC_CRATES[@]}"}"; do
  [ -n "$c" ] || continue
  for f in "$COV/$c"/tests/*.rs; do [ -f "$f" ] && _srcs+=("$f"); done
done
if [ "${#_srcs[@]}" -gt 0 ]; then
  mapfile -t _cfgs < <("$RV" --cfgs "${_srcs[@]}")
  if [ "${#_cfgs[@]}" -gt 0 ] && [ -n "${_cfgs[0]:-}" ]; then
    _rf="${RUSTFLAGS:-}"
    for c in "${_cfgs[@]}"; do _rf="${_rf:+$_rf }--cfg $c"; done
    TEST_ENV+=("RUSTFLAGS=$_rf")
    for c in "${_cfgs[@]}"; do
      [ "$c" = "madsim" ] && TEST_ENV+=("MADSIM_TEST_NUM=${WYRD_VERIFY_MADSIM_SEEDS:-50}")
    done
    echo "run-diff-cov.sh: cfg-gated test target (${_cfgs[*]}) — running with ${TEST_ENV[*]} (#104)." >&2
  fi
fi

# --- measure -------------------------------------------------------------------------------
# Profile data only: a .profraw left by a previous bundle would be merged into this bundle's
# numbers. The BUILD cache is kept (that is the point of a durable worktree), so this is not
# `clean --workspace`, which would also drop the workspace's build artifacts every run.
(cd "$COV" && cargo llvm-cov clean --profraw-only) </dev/null >/dev/null 2>&1 || true

echo "run-diff-cov.sh: MEASURE — cargo llvm-cov test ${TEST_ARGS[*]} (fix applied)" >&2
[ "$WHOLE_SUITE" = 1 ] && \
  echo "                 (no separate test file in this patch — scoring against the package's whole" >&2
  echo "                 suite, and any co-located #[cfg(test)] lines the patch adds count as" >&2
  echo "                 covered, so this figure is the generous one; see the header's boundary)" >&2
COV_RC=0
COV_OUTPUT="$( ( cd "$COV" && env "${TEST_ENV[@]+"${TEST_ENV[@]}"}" \
    cargo llvm-cov test --lcov --output-path "$LCOV" "${TEST_ARGS[@]}" </dev/null ) 2>&1 )" || COV_RC=$?
printf '%s\n' "$COV_OUTPUT" >&2
printf '%s' "$COV_OUTPUT" > "$TMPD/cargo-out"
TESTS_RAN="$("$RV" --tests-ran "$TMPD/cargo-out")"

# Judge by BOTH facts, exactly as C4-verify's legs do: the runner's status AND how many tests
# actually executed. Each of the three bad cells is a missing measurement, never a verdict.
if [ "$TESTS_RAN" -eq 0 ]; then
  _json "unverifiable" "the target ran 0 tests" 0 0 0 "$BASE_REF" "${TEST_ARGS[*]}"
  echo "run-diff-cov.sh: UNVERIFIABLE — the target ran 0 tests, so no reach was measured." >&2
  echo "                 The test is compiled out: a cfg the gate does not set (#104), a feature" >&2
  echo "                 it does not enable, every test #[ignore]d, or a filter matching nothing." >&2
  echo "PDCA-UNVERIFIABLE: the target ran 0 tests, so diff coverage was not measured" >&2
  exit 77
fi
if [ "$COV_RC" -ne 0 ] || [ ! -s "$LCOV" ]; then
  _json "unverifiable" "the shipped test did not pass under llvm-cov (status $COV_RC)" \
        0 0 "$TESTS_RAN" "$BASE_REF" "${TEST_ARGS[*]}"
  echo "run-diff-cov.sh: UNVERIFIABLE — the run exited $COV_RC and produced no usable report" >&2
  echo "                 ($TESTS_RAN test(s) ran). Coverage of a fix whose own test does not pass" >&2
  echo "                 measures nothing about the fix; whether that test SHOULD pass is" >&2
  echo "                 C4-verify's verdict, not this row's (the output is above)." >&2
  echo "PDCA-UNVERIFIABLE: the shipped test did not pass under llvm-cov, so diff coverage was not measured" >&2
  exit 77
fi

# --- score ---------------------------------------------------------------------------------
SCORED="$(_score "$LCOV" "$CHANGED")"
printf '%s\n' "$SCORED" | grep '^MISS ' > "$MISS_FILE" || true
COVERED="$(printf '%s\n' "$SCORED" | awk '/^TOTAL /{print $2; exit}')"
INSTR="$(printf '%s\n' "$SCORED" | awk '/^TOTAL /{print $3; exit}')"
PCT="$(_pct "$COVERED" "$INSTR")"
# NB the default has no apostrophe on purpose: inside ${var:-word} bash still processes quotes,
# so a `'` here opens a string that never closes and the whole script fails to parse.
CRATE_LABEL="${PKGS[0]:-unknown}"
VERDICT="$(_verdict "$COVERED" "$INSTR" "$DIFFCOV_MIN")"

if [ "$VERDICT" = "UNVERIFIABLE" ]; then
  _json "unverifiable" "no changed line carries an lcov DA: record" 0 0 "$TESTS_RAN" \
        "$BASE_REF" "${TEST_ARGS[*]}"
  echo "run-diff-cov.sh: UNVERIFIABLE — none of the $CHANGED_TOTAL changed production lines carry" >&2
  echo "                 a coverage record, so the instrumentation never reached those files. That" >&2
  echo "                 is a broken measurement, not 0% coverage." >&2
  echo "PDCA-UNVERIFIABLE: no changed line carries an lcov record, so diff coverage was not measured" >&2
  exit 77
fi

sed 's/^MISS /run-diff-cov.sh: MISS /' "$MISS_FILE" >&2
_json "$( [ "$VERDICT" = PASS ] && echo pass || echo fail )" \
      "$VERDICT at min ${DIFFCOV_MIN}%" "$COVERED" "$INSTR" "$TESTS_RAN" "$BASE_REF" "${TEST_ARGS[*]}"

# Forward-compat with eduralph/pdca-harness#406 (gate metrics into the Act index). Nothing
# consumes it yet; it is frozen in gate-logs/C4-diff-cov.log so the trend is reconstructable.
echo "METRIC diff_cov=$PCT covered=$COVERED instrumentable=$INSTR" >&2

if [ "$VERDICT" = "FAIL" ]; then
  echo "run-diff-cov.sh: $(_act_line "$CRATE_LABEL" "$COVERED" "$INSTR")" >&2
  echo "run-diff-cov.sh: FAIL — the shipped test executes $PCT% of this patch's instrumentable" >&2
  echo "                 changed lines, below the ${DIFFCOV_MIN}% floor. Each MISS above is a REACH" >&2
  echo "                 gap: the line never ran. (A line that DID run may still be unasserted —" >&2
  echo "                 that is C5-mutants' question, not this row's.)" >&2
  echo "PDCA-EVIDENCE: diff coverage $PCT% ($COVERED/$INSTR changed lines, below the ${DIFFCOV_MIN}% floor)" >&2
  exit 1
fi

echo "run-diff-cov.sh: PASS — the shipped test executes $PCT% of this patch's $INSTR instrumentable" >&2
echo "                 changed lines (floor ${DIFFCOV_MIN}%, $TESTS_RAN test(s) ran)." >&2
echo "PDCA-EVIDENCE: diff coverage $PCT% ($COVERED/$INSTR changed lines, floor ${DIFFCOV_MIN}%)" >&2
exit 0
