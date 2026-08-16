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
#   * …and CANNOT SEE the difference between that and real code the build compiled out behind
#     a `#[cfg(...)]` your feature selection does not enable. Both are simply absent from the
#     report. This is the gate's last blind spot and it is live in the target:
#     crates/metadata-tikv (`feature = "tikv"`) and crates/metadata-fdb (`feature = "fdb"`)
#     carry hundreds of such lines. Demonstrated in the #197 adversarial review: 19 of 23
#     changed lines inside a gated block left the denominator and the run reported 100%.
#     Three discriminators were tried and all failed — a ratio (a legitimate patch sat at 26%
#     instrumentable, the gated one at 17%), a contiguous unscored run (78 lines of ordinary
#     doc comment beat the 16-line gated block), and an added `#[cfg(` attribute (false both
#     ways, since lines added INSIDE an existing gated block carry no attribute of their own).
#     So the gate reports the gap rather than guessing at it, and reports it in a form a human
#     can act on (#222): every run prints how many changed lines were instrumentable out of how
#     many changed AND the largest unscored SPANS, with all of them in `diff-cov.json`. A bare
#     count says nothing; `crates/metadata-tikv/src/lib.rs:1605-1620 (16 lines)` is recognised
#     on sight as a gated block, where `crates/core/src/multipart.rs:1458-1570 (113 lines)` is
#     as plainly a doc-comment run. The gate does not classify them — it shows them, and the
#     judgement is the human's at sign-off.
#     This is also why the row carries NO `promote_after` and is not on the promotion ladder
#     (#222): a patch can be 100% green here having measured a fraction of itself, and passing
#     three clean cycles cannot retire that — it is a property of the measurement, not of the
#     streak. The row informs sign-off; it is not a candidate to block.
#   * Measures under the patch's OWN test where there is one — the same `-p <pkg> --test <name>`
#     C4-verify computes — PLUS `-p <pkg>` (the existing suite) for every other changed crate.
#     Not only the test-owning crate: the denominator spans every changed crate, so a run that
#     built one of them and dropped the rest reports part of a patch as though it were all of
#     it. That was a real false green here — a two-crate patch scored 100% (5/5) while the
#     second crate's uncovered new function was never compiled (#197 codex review) — and it is
#     why a changed file whose package never entered the run is UNVERIFIABLE, not a silent
#     omission. A co-located test has no separate target either, so its crate also degrades to
#     `-p <pkg>`. Both degradations are a weaker claim and are reported as such.
#   * Does NOT exclude a co-located `#[cfg(test)] mod tests` block. Its lines sit in a
#     production FILE, so the file-level exclusion above cannot see them, and they are covered
#     by definition (they are the code doing the covering) — so a patch shipping its test
#     inline scores GENEROUSLY, by roughly the size of that block. Measured, not assumed: the
#     same planted fixture scores 6/10 with the test in `tests/`, 9/13 with it inline.
#     Excluding it would mean parsing Rust attribute scopes in awk, which is a worse trade than
#     naming the bias — and it points the same way as the advisory default: this row informs
#     sign-off, it does not block on a number.
#
# UNVERIFIABLE (exit 77 -> SUMMARY §6 NEEDS-HUMAN, non-gating): the gate could not MEASURE the
# bundle. Not a verdict on the fix — the absence of one. `engine/README.md` states the rule the
# whole engine follows: a gate never turns "no evidence" into a verdict. Ten routes, and the
# HOST ones matter as much as the measurement ones — `gates.py` decides pass/fail on the exit
# code alone, so anything that merely exits non-zero files a red row against the patch and
# resets the promote_after streak (#197 review found three doing exactly that):
#   * the patch names .rs files but no changed line resolved — a diff shape or path layout this
#     gate cannot read. NOT the same as having nothing to measure, which exits 0;
#   * cargo is not on PATH at all (ensure_cargo's 127 is not propagated);
#   * cargo-llvm-cov is not installed (see [[doctor.checks]] 'cargo-llvm-cov');
#   * the target Wyrd checkout was not found;
#   * the toolchain has no llvm-tools component. Checked UP FRONT and deliberately: without
#     it `cargo llvm-cov` PROMPTS on stdin ("I will run `rustup component add
#     llvm-tools-preview ...`. Proceed? [Y/n]") and a gate that blocks on a prompt hangs until
#     the 7200s timeout. Every cargo SUBCOMMAND call below runs with stdin at /dev/null so no
#     future prompt can do the same. Installing a toolchain component is a networked side
#     effect and is NOT this gate's to perform — it reports and stops.
#   * no cargo test target maps to the patch's crates;
#   * any single run executed zero tests. `--cfg`-gated targets are the live cause: every
#     crates/dst test is `#![cfg(madsim)]` and compiles to an EMPTY binary without the flag
#     (#104). Measured during #197: cargo-llvm-cov's wrapper APPENDS its instrumentation to
#     inherited RUSTFLAGS, so `--cfg madsim` does compose — but the count is checked anyway,
#     because that is a property of the tool, not of this script. Checked PER RUN: a summed
#     count would let an empty target hide behind another crate's passing suite;
#   * a run did not PASS. Coverage of a failing fix measures nothing about the fix, and
#     whether the test should pass is C4-verify's verdict to give, not this row's;
#   * a changed file's package never entered the run, so nothing about that file was measured
#     and any percentage would describe only the part of the patch that was;
#   * NONE of the changed production lines carry a `DA:` record. The instrumentation never
#     reached those files, which is a broken measurement, not 0% coverage.
#
# What is deliberately NOT on that list: a large share of changed lines carrying no coverage
# region. That is reported (see the NOTE the run prints, and `unscored` in diff-cov.json) but
# never gated on — see the last bullet of the coverage boundary above for why no mechanical
# test separates a compiled-out region from a comment.
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
#   run-diff-cov.sh --score <lcov> <lines>     # MISS + NOFILE + `TOTAL <cov> <instr>` (hook)
#   run-diff-cov.sh --crate-measured <crate> <lcov>  # did the report measure that crate (hook)
#   run-diff-cov.sh --verdict <cov> <instr> [min]   # PASS|FAIL|UNVERIFIABLE (test hook)
#   run-diff-cov.sh --act-line <crate> <cov> <instr> # the recurring-signal line (test hook)
set -euo pipefail

case "${1:-}" in
  -h | --help) awk 'NR==1{next} /^#/{sub(/^# ?/,"");print;next} {exit}' "$0"; exit 0 ;;
esac

# The share of instrumentable changed lines that must be executed. 80 rather than a strict 100
# because a real patch carries error branches a single regression test does not walk, and a
# threshold no honest bundle can meet would just train the human to skip the row.
DIFFCOV_MIN="${WYRD_DIFFCOV_MIN:-80}"

# How many unscored spans the run prints before summarising the rest (#222). All of them land in
# coverage/diff-cov.json regardless; this only bounds what a human reads in the gate log, and 6
# is enough to show the block that matters without burying the verdict under a 700-line patch's
# interleaved doc comments.
UNSCORED_SPANS_SHOWN="${WYRD_DIFFCOV_SPANS:-6}"

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

# Production .rs files the patch changes that map to NO crate dir — the one shape that means
# "this gate cannot read the patch" rather than "this patch has nothing to score". Test files and
# non-.rs files are excluded first, so a test-only or docs-only patch never appears here; a
# deletion-only patch produces no records at all and likewise cannot. Reads the UNFILTERED
# records on purpose: `_changed_lines` has already dropped exactly what we are looking for.
_unmapped_production_rs() { # <patch> -> `<path>` per unmappable production Rust file
  local rec path prev=""
  while IFS= read -r rec; do
    [ -n "$rec" ] || continue
    path="${rec%:*}"
    [ "$path" = "$prev" ] && continue
    prev="$path"
    case "$path" in *.rs) ;; *) continue ;; esac
    [ "$("$RV" --is-test "$path")" = "no" ] || continue
    [ -n "$("$RV" --crate-dir "$path")" ] && continue
    printf '%s\n' "$path"
  done < <(_line_records "$1")
  return 0
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
#
# A file the report never mentions AT ALL is a third case, and it is reported separately as
# `NOFILE <path>` rather than folded into either count. It has two very different causes and
# `_score` cannot tell them apart — only the caller, which knows which packages it measured,
# can (see the UNMEASURED handling at the call site):
#   * the file has no executable region anywhere (a `pub mod x;` re-export, a consts file).
#     Measured: adding one `pub mod planted;` line to crates/core/src/lib.rs leaves lib.rs
#     out of the report entirely. Benign — nothing to score.
#   * the file's CRATE was never built or run, so nothing about it was measured. Silently
#     dropping that is a false green: a two-crate patch whose test lives in crate A scored
#     100% while crate B's uncovered function was never compiled (found by the #197 codex
#     review; reproduced before this was written).
_score() { # <lcov> <lines-file> -> `MISS path:line`… `NOFILE path`… `TOTAL <covered> <instr>`
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
      if (cur != "") present[cur] = 1
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
        if (k in seenk) continue                # the same line twice in one patch
        seenk[k] = 1
        if (!(k in instr)) {                    # no region here — out of the denominator…
          split(k, s, /\t/); print "UNSCORED " s[1] ":" s[2]   # …but never out of sight (#222)
          continue
        }
        n++
        if (k in hit) { c++ } else { split(k, s, /\t/); print "MISS " s[1] ":" s[2] }
      }
      for (i = 1; i <= nf; i++) if (!(files[i] in present)) print "NOFILE " files[i]
      print "TOTAL " c " " n
    }
  ' "$2" "$1"
}

# Collapse `path:line` records into contiguous `path:start-end<TAB>count` ranges, largest first.
#
# This is what makes the unscored set usable (#222). The gate cannot tell a line with no coverage
# region apart from real code the build compiled out behind a `#[cfg(...)]` — three mechanical
# discriminators were tried and all failed — so it does not guess. What it CAN do is stop
# reporting the gap as a bare number: `19 unscored` tells a human nothing, while
# `crates/metadata-tikv/src/lib.rs:740-755 (16 lines)` is recognised on sight as the body of the
# `feature = "tikv"` block. Ranges rather than lines because a real patch has hundreds — 716 has
# 555 unscored lines, which is noise printed one per line and a short list printed as spans.
# Sorted by size because the block you need to notice is the big one; the small runs are the
# interleaved doc comments and delimiters that are genuinely uninteresting.
_ranges() { # <path:line lines on stdin> -> `path:start-end\tcount`, largest run first
  sort -t: -k1,1 -k2,2n | awk -F: '
    function flush() { if (f != "") printf "%s:%d-%d\t%d\n", f, s, p, p - s + 1 }
    { if ($1 != f || $2 != p + 1) { flush(); f = $1; s = $2 } ; p = $2 }
    END { flush() }
  ' | sort -k2,2nr -t$'\t'
}

# Did the report measure ANY file of that crate? This is the discriminator the caller needs to
# read a `NOFILE` line: a crate with records but not this file means the file has no executable
# region (benign); a crate with no records at all means nothing about it ran, and any score
# would describe only the part of the patch that did. Matched on a whole path component, like
# the SF match above, so `crates/core` never answers for `vendor/notcore`.
_crate_measured() { # <crate-dir> <lcov> -> yes | no
  # BRE metacharacters in the crate dir are escaped: an unescaped `.` would match any character,
  # and this answer decides whether a file's absence is benign — over-permissive here means a
  # silent drop instead of the 77 (#197 review).
  local q; q="$(printf '%s' "$1" | sed 's/[][\.*^$\\]/\\&/g')"
  if grep -q -e "^SF:$q/" -e "^SF:.*/$q/" "$2" 2>/dev/null; then printf 'yes'; else printf 'no'; fi
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
#
# The percentage is the UNCOVERED share, because the sentence is about what did not execute.
# Quoting the covered share beside "not executed" read as a flat contradiction — the pinned
# 12-of-40 case said "28 of 40 not executed … (30.0%)" when 70% was not executed — and a frozen
# signal that understates the gap is worse than none, since sign-off and recurrence triage both
# read it as-is (PR #221 review).
_act_line() { # <crate> <covered> <instrumentable> -> the normalized signal line
  local crate="$1" c="$2" n="$3"
  printf 'C4-diff-cov: uncovered changed lines in crate %s — %s of %s not executed by the patch tests (%s%% uncovered)' \
    "$crate" "$((n - c))" "$n" "$(_pct "$((n - c))" "$n")"
}

# --changed-lines <patch>: the scored `path:line` set. Calls run-verify.sh's pure hooks only —
# no worktree, no cargo, no git. For engine/tests.
if [ "${1:-}" = "--changed-lines" ]; then
  _changed_lines "${2:?--changed-lines needs a patch path}"
  exit 0
fi

# --score <lcov> <lines-file>: MISS lines, NOFILE lines, and the TOTAL pair. No worktree, no
# cargo — for engine/tests.
if [ "${1:-}" = "--score" ]; then
  _score "${2:?--score needs an lcov path}" "${3:?--score needs a changed-lines file}"
  exit 0
fi

# --ranges: collapse `path:line` records on stdin into contiguous spans, largest first. No
# worktree, no cargo — for engine/tests (#222).
if [ "${1:-}" = "--ranges" ]; then
  _ranges
  exit 0
fi

# --unmapped-rs <patch>: changed production .rs files that map to no cargo package — the one
# shape that means "cannot read this patch" rather than "nothing to score". No worktree, no
# cargo — for engine/tests (PR #221 review).
if [ "${1:-}" = "--unmapped-rs" ]; then
  _unmapped_production_rs "${2:?--unmapped-rs needs a patch path}"
  exit 0
fi

# --crate-measured <crate-dir> <lcov>: did the report measure any file of that crate? No
# worktree, no cargo — for engine/tests (#197).
if [ "${1:-}" = "--crate-measured" ]; then
  _crate_measured "${2:?--crate-measured needs a crate dir}" "${3:?--crate-measured needs an lcov path}"
  echo
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
RANGES_FILE="$TMPD/unscored-ranges"

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
    # changed_lines - instrumentable: lines that scored on NEITHER side. Recorded because the
    # percentage alone cannot show it, and a large share is the one signal that a #[cfg]-gated
    # region left the measurement (see the NOTE the run prints).
    printf '  "unscored": %s,\n' "$(( ${CHANGED_TOTAL:-0} - n ))"
    printf '  "tests_ran": %s,\n' "$ran"
    printf '  "base_ref": "%s",\n' "$base"
    printf '  "test_args": "%s",\n' "$targs"
    printf '  "unscored_spans": ['
    if [ -s "${RANGES_FILE:-/dev/null}" ]; then
      printf '\n'
      cut -f1 "$RANGES_FILE" | awk '{ printf "%s    \"%s\"", (NR>1 ? ",\n" : ""), $0 } END { printf "\n  " }'
    fi
    printf '],\n'
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
  # "Scored nothing" has two causes and only one of them is a green, so the discriminator has to
  # be precise about WHICH. An earlier cut cross-checked `grep '^+++ b/.*\.rs$'` on the raw patch
  # — far too broad: a TEST-ONLY patch (the commonest verify-first bundle shape, shipping only a
  # regression test) and a DELETION-ONLY Rust patch both name .rs files while legitimately having
  # no production line to score, and both were answered with a spurious §6 item that also reset
  # the promotion streak (PR #221 review; reproduced both shapes before this was written).
  #
  # What actually means "could not read" is narrower, and neither case touches it:
  #   * a COMBINED/merge diff — `@@@ -1,3 -1,3 +1,4 @@@`, which the `/^@@ /` walker never matches,
  #     so every line reads as out-of-body and the patch silently scores nothing;
  #   * a production .rs file with changed lines that maps to NO crate dir — i.e. a workspace
  #     member outside `crates/*` / `xtask`. None today, which is exactly why it must be caught
  #     rather than assumed: `_crate_dir` now has two consumers and the first member added
  #     elsewhere would otherwise land as a silent green.
  if grep -q '^@@@ ' "$PATCH"; then
    _json "unverifiable" "the patch is a combined/merge diff, which this walker cannot read"
    echo "run-diff-cov.sh: UNVERIFIABLE — this is a combined (merge) diff; the hunk walker reads" >&2
    echo "                 only ordinary unified diffs, so it resolved no changed line at all." >&2
    echo "PDCA-UNVERIFIABLE: the patch is a combined/merge diff, so diff coverage was not measured" >&2
    exit 77
  fi
  mapfile -t _unmapped < <(_unmapped_production_rs "$PATCH")
  if [ "${#_unmapped[@]}" -gt 0 ] && [ -n "${_unmapped[0]:-}" ]; then
    _json "unverifiable" "${#_unmapped[@]} changed production .rs file(s) map to no cargo package"
    for f in "${_unmapped[@]}"; do echo "run-diff-cov.sh: UNMAPPED $f" >&2; done
    echo "run-diff-cov.sh: UNVERIFIABLE — the file(s) above are changed production Rust outside the" >&2
    echo "                 layout this gate knows (crates/*, xtask), so no package maps to them and" >&2
    echo "                 nothing could be measured. Teach run-verify.sh's --crate-dir the new" >&2
    echo "                 layout rather than reading this as a clean patch." >&2
    echo "PDCA-UNVERIFIABLE: changed production Rust maps to no cargo package, so diff coverage was not measured" >&2
    exit 77
  fi
  _json "n/a" "patch changes no production Rust line under a cargo package"
  echo "run-diff-cov.sh: nothing to measure — the patch changes no production Rust line under a" >&2
  echo "                 cargo package (docs / CI / test-only). C4-ci and C4-verify cover it." >&2
  echo "PDCA-EVIDENCE: diff coverage n/a — no production Rust lines changed" >&2
  exit 0
fi

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=../lib/ensure-cargo.sh
. "$here/../lib/ensure-cargo.sh"   # defines ensure_cargo; called below, before any cargo use
# A host with no cargo cannot MEASURE this patch; it has not found a defect in it. Propagating
# ensure_cargo's 127 would file a red row against the fix — and, because `gates.py` decides
# pass/fail on the exit code alone, no amount of output would soften it. It also resets the
# promote_after streak. Same reasoning as the missing-tool branch just below (#197 review).
if ! ensure_cargo; then
  _json "unverifiable" "cargo is not on PATH and no rustup env was found"
  echo "PDCA-UNVERIFIABLE: cargo is not available on this host, so diff coverage was not measured" >&2
  exit 77
fi

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
  # An environment fault, not a verdict — the same call as the two branches above. run-verify.sh
  # exits 2 here, which for IT is a red C4 row on a misconfigured host; this row does not repeat
  # that (#197 review).
  _json "unverifiable" "the target Wyrd checkout was not found"
  echo "run-diff-cov.sh: live Wyrd repo not found (set WYRD_REPO, or place this project beside ~/wyrd/wyrd)." >&2
  echo "PDCA-UNVERIFIABLE: the target Wyrd checkout was not found, so diff coverage was not measured" >&2
  exit 77
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

# One RUN SPEC per cargo invocation, as `<pkg>\t<test target>` (empty target = the package's
# whole suite). Deliberately NOT one combined arg list: `--test <name>` filters test targets
# across EVERY selected package, so `-p a --test t -p b` runs nothing at all for b and llvm-cov
# reports not one line of it. That is precisely how the false green survived the first fix
# attempt — the arg list looked right and measured one crate. cargo-llvm-cov's documented
# multi-run form is used instead: `--no-report` per run, then one `report` to merge.
declare -A SEEN_PKG=()
RUN_SPECS=(); PKGS=()
for t in "${ADDED_TESTS[@]+"${ADDED_TESTS[@]}"}"; do
  c="$("$RV" --crate-dir "$t")"; [ -n "$c" ] || continue
  pkg="$(_pkg_name "$c")"; [ -n "$pkg" ] || continue
  # Only `<crate>/tests/<name>.rs` is an auto-discovered integration-test TARGET. `_is_test_file`
  # deliberately matches deeper paths too (`tests/dserver/helper.rs` is still test code, and must
  # stay out of the coverage denominator), but naming one with `--test helper` asks cargo for a
  # target it does not have — and since every spec now shares one verdict, that error would take
  # the whole measurement UNVERIFIABLE rather than just its own run.
  case "$t" in
    "$c"/tests/*/*) continue ;;
  esac
  RUN_SPECS+=("$pkg"$'\t'"$(basename "$t" .rs)"$'\t'"$c"); SEEN_PKG["$pkg"]=1; PKGS+=("$pkg")
done
# EVERY changed crate joins the run, not only the one that ships a test. The denominator spans
# all of them, so the numerator has to as well — and a package that is never built produces no
# coverage record, which `_score` would otherwise drop silently. That is a false green, and it
# was real: a two-crate patch whose only added test lives in crate A reported 100% (5/5) while
# crate B's brand-new uncovered function was never compiled (#197 codex review, reproduced).
# A crate with no added test contributes `-p <pkg>` — its existing suite — which is the honest
# answer to "does anything that ships today reach these lines".
WHOLE_SUITE=0
for c in "${CRATES[@]+"${CRATES[@]}"}"; do
  pkg="$(_pkg_name "$c")"; [ -n "$pkg" ] || continue
  [ -n "${SEEN_PKG[$pkg]:-}" ] && continue
  RUN_SPECS+=("$pkg"$'\t'$'\t'"$c"); SEEN_PKG["$pkg"]=1; PKGS+=("$pkg")
  WHOLE_SUITE=1
done
if [ "${#RUN_SPECS[@]}" -eq 0 ]; then
  # Changed production lines exist but no test target maps to them — nothing to measure them
  # WITH. Not a 0% score: no test ran, so nothing was measured.
  _json "unverifiable" "no cargo test target maps to the patch's crates" 0 0 0 "$BASE_REF"
  echo "run-diff-cov.sh: UNVERIFIABLE — the patch changes production lines but no cargo test" >&2
  echo "                 target maps to their crates, so there is no test whose reach could be" >&2
  echo "                 measured." >&2
  echo "PDCA-UNVERIFIABLE: no test target maps to the patch's crates, so diff coverage was not measured" >&2
  exit 77
fi

# The cfg gate a spec's OWN test sources sit behind (#104), read AFTER the patch is applied —
# an added test does not exist in the worktree before that. RUSTFLAGS is APPENDED to, never
# clobbered; cargo-llvm-cov then appends its own instrumentation on top (measured in #197).
#
# Computed PER SPEC, never once for the whole selection. A union would apply one crate's cfg to
# every other crate's build: a patch touching crates/dst (whose tests are all `#![cfg(madsim)]`)
# and crates/core would build wyrd-core under `--cfg madsim` too, swapping tokio for madsim-tokio
# across the selection, invalidating the shared build cache every alternate cycle, and measuring
# a tree that is not the one C4-verify runs — breaking this file's own "one answer for both C4
# rows" premise. Caught by the #197 adversarial review after the multi-crate fix widened
# TEST_SRC_CRATES to every changed crate.
_spec_env() { # <test target|""> <crate dir> -> zero or more KEY=VALUE lines
  local tn="$1" c="$2" srcs=() f cfgs=() rf
  if [ -n "$tn" ]; then
    srcs+=("$COV/$c/tests/$tn.rs")
  else
    for f in "$COV/$c"/tests/*.rs; do [ -f "$f" ] && srcs+=("$f"); done
  fi
  [ "${#srcs[@]}" -gt 0 ] || return 0
  mapfile -t cfgs < <("$RV" --cfgs "${srcs[@]}")
  [ "${#cfgs[@]}" -gt 0 ] && [ -n "${cfgs[0]:-}" ] || return 0
  rf="${RUSTFLAGS:-}"
  for f in "${cfgs[@]}"; do rf="${rf:+$rf }--cfg $f"; done
  printf 'RUSTFLAGS=%s\n' "$rf"
  for f in "${cfgs[@]}"; do
    [ "$f" = "madsim" ] && printf 'MADSIM_TEST_NUM=%s\n' "${WYRD_VERIFY_MADSIM_SEEDS:-50}"
  done
  echo "run-diff-cov.sh: cfg-gated test target (${cfgs[*]}) for $c — passing the flag (#104)." >&2
}

# --- measure -------------------------------------------------------------------------------
# `--workspace`, not `--profraw-only`, and the difference is correctness rather than hygiene.
# The final `report` step merges the current profile data against EVERY instrumented object it
# finds in the target dir — including test binaries left by a PREVIOUS bundle's run. Those have
# no profile data any more, so every line they touch is merged in at zero hits, in files this
# patch did change. Measured: bundle 716 scored 99.0% (197/199) from a clean tree and 59.2%
# (135/228) with three earlier fixtures' binaries lying around — a false RED of 40 points, from
# stale state alone. `--profraw-only` cannot prevent it: the profraw is not what is stale, the
# objects are. So the workspace's own artifacts go every run (dependencies, the bulk of the
# cache, stay), which is also the multi-run form cargo-llvm-cov documents.
(cd "$COV" && cargo llvm-cov clean --workspace) </dev/null >/dev/null 2>&1 || true

if [ "$WHOLE_SUITE" = 1 ]; then
  echo "run-diff-cov.sh: at least one changed crate ships no test in this patch — it is scored" >&2
  echo "                 against its EXISTING suite, and any co-located #[cfg(test)] lines the" >&2
  echo "                 patch adds count as covered, so that part of the figure is the generous" >&2
  echo "                 one; see the coverage boundary in this file's header." >&2
fi

# One invocation per spec, each `--no-report` so the profile data accumulates, then a single
# `report` merges them. TEST_ARGS is kept only as the human-readable record of what ran.
COV_RC=0
TEST_ARGS=()
TESTS_RAN=0
: > "$TMPD/cargo-out"
rm -f "$LCOV"
for spec in "${RUN_SPECS[@]}"; do
  # Split by hand, NOT `IFS=$'\t' read`: tab is whitespace, and `read` collapses runs of
  # whitespace IFS into one delimiter — so a whole-suite spec's empty middle field disappeared
  # and its crate dir was handed to cargo as `--test crates/telemetry`.
  _pkg="${spec%%$'\t'*}"; _rest="${spec#*$'\t'}"
  _tn="${_rest%%$'\t'*}"; _crate="${_rest#*$'\t'}"
  _args=("-p" "$_pkg"); [ -n "$_tn" ] && _args+=("--test" "$_tn")
  TEST_ARGS+=("${_args[@]}")
  mapfile -t _env < <(_spec_env "$_tn" "$_crate")
  echo "run-diff-cov.sh: MEASURE — cargo llvm-cov test ${_args[*]} (fix applied)" >&2
  _rc=0
  _out="$( ( cd "$COV" && env "${_env[@]+"${_env[@]}"}" \
      cargo llvm-cov test --no-report "${_args[@]}" </dev/null ) 2>&1 )" || _rc=$?
  printf '%s\n' "$_out" >&2
  printf '%s\n' "$_out" >> "$TMPD/cargo-out"
  [ "$_rc" -ne 0 ] && COV_RC="$_rc"
  # Counted PER RUN, not only in aggregate: a summed total lets a run that executed nothing
  # disappear behind another crate's passing suite, and the human reading the log should see
  # WHICH crate contributed nothing.
  #
  # But a zero here is NOT escalated on its own, because it has two causes with opposite
  # correct answers, and this count cannot separate them — only the report can:
  #   * the crate genuinely has no tests. cargo still builds and instruments it, so its lines
  #     DO get records, all with zero hits. "Nothing executes these changed lines" is then a
  #     true and useful finding — the very thing this row exists to say — and refusing to give
  #     it would let a test-less crate escape the gate entirely. Measured: the multi-crate
  #     fixture's wyrd-telemetry runs 0 tests and correctly scores 6 real misses.
  #   * the target compiled to nothing (the #104 cfg shape). Then there is no profile data at
  #     all, the crate is absent from the report, and the UNMEASURED check below turns it into
  #     the 77 it deserves.
  # So the escalation lives with the evidence, and this stays a note.
  printf '%s\n' "$_out" > "$TMPD/spec-out"
  _ran="$("$RV" --tests-ran "$TMPD/spec-out")"
  TESTS_RAN=$((TESTS_RAN + _ran))
  # An `if`, not `[ … ] && echo … && echo …`: only the FIRST echo of such a chain is guarded,
  # so the rest print unconditionally — a shape that already shipped one wrong message here.
  if [ "$_ran" -eq 0 ] && [ -n "$_tn" ]; then
    # An EXPLICIT test target that executed nothing is the #104 shape, and it is missing
    # evidence rather than a finding: the patch named this test as its discriminator and the
    # test did not run. Cargo still instruments the package library, so the crate DOES appear
    # in the report and the crate-level backstop is satisfied — the changed lines would then
    # score as zero-hit MISSes and the row would report a coverage FAIL for a test that never
    # executed. A false red in the direction that blames the patch (PR #221 review).
    # `_spec_env` closes the known cause (a bare `#![cfg(NAME)]` crate root) but cannot close
    # `#![cfg(feature = "…")]`, which `_crate_cfgs` does not match by design.
    _json "unverifiable" "the ${_args[*]} target executed 0 tests" 0 0 "$TESTS_RAN" \
          "$BASE_REF" "${TEST_ARGS[*]}"
    echo "run-diff-cov.sh: UNVERIFIABLE — \`${_args[*]}\` is the test this patch ships, and it" >&2
    echo "                 executed 0 tests, so it measured nothing. Scoring on would report its" >&2
    echo "                 crate's changed lines as uncovered — a failure attributed to the patch" >&2
    echo "                 for a test that never ran. The target is compiled out: a cfg the gate" >&2
    echo "                 does not set (#104, incl. \`#![cfg(feature = \"…\")]\`), a feature it does" >&2
    echo "                 not enable, every test #[ignore]d, or a filter matching nothing." >&2
    echo "PDCA-UNVERIFIABLE: the ${_args[*]} target executed 0 tests, so diff coverage was not measured" >&2
    exit 77
  fi
  if [ "$_ran" -eq 0 ]; then
    # A WHOLE-SUITE spec is the opposite case and must not escalate: the crate simply has no
    # tests, it is still built and instrumented, and "nothing executes these changed lines" is a
    # true and useful finding — refusing to give it would let a test-less crate escape the row.
    echo "run-diff-cov.sh: NOTE — \`${_args[*]}\` executed 0 tests; that crate ships none, so its" >&2
    echo "                 changed lines can only score as uncovered. That is a finding, not a" >&2
    echo "                 measurement failure — the crate is still built and instrumented." >&2
  fi
done
( cd "$COV" && cargo llvm-cov report --lcov --output-path "$LCOV" </dev/null ) >/dev/null 2>&1 \
  || echo "run-diff-cov.sh: the coverage report step failed; see the verdict below." >&2

# Judge by BOTH facts, exactly as C4-verify's legs do: the runner's status AND how many tests
# actually executed. The tests-ran half is enforced per spec inside the loop above, where a
# zero can still be attributed to the run that produced it; what is left here is the status.
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
printf '%s\n' "$SCORED" | sed -n 's/^UNSCORED //p' | _ranges > "$RANGES_FILE" || true

# A changed file the report never mentions is benign ONLY if its package was actually measured
# — then it simply has no executable region (`pub mod x;`, a consts file). If its package was
# never in the run, nothing about that file was measured and dropping it would report a score
# for part of the patch as though it covered the whole. This is the backstop under the target
# assembly above: that loop should now put every changed crate in the run, and this catches
# whatever still slips through (a crate whose package name will not resolve, a package cargo
# skipped). Incomplete measurement is UNVERIFIABLE, never a percentage.
UNMEASURED=()
while IFS= read -r nf; do
  [ -n "$nf" ] || continue
  _c="$("$RV" --crate-dir "$nf")"
  # Ask the REPORT whether that crate was measured, never the request list. Asking what we
  # ASKED cargo for is what let the false green through a second time: the run specs named
  # wyrd-telemetry, so the file looked accounted for, while `--test` filtering meant cargo
  # had run none of it and the report held not one telemetry line.
  if [ -n "$_c" ] && [ "$(_crate_measured "$_c" "$LCOV")" = "yes" ]; then
    continue                                  # crate measured; this file just has no regions
  fi
  UNMEASURED+=("$nf")
done < <(printf '%s\n' "$SCORED" | sed -n 's/^NOFILE //p')
if [ "${#UNMEASURED[@]}" -gt 0 ]; then
  _json "unverifiable" "${#UNMEASURED[@]} changed file(s) were never measured" 0 0 "$TESTS_RAN" \
        "$BASE_REF" "${TEST_ARGS[*]}"
  for f in "${UNMEASURED[@]}"; do echo "run-diff-cov.sh: UNMEASURED $f" >&2; done
  echo "run-diff-cov.sh: UNVERIFIABLE — the file(s) above changed production code but their" >&2
  echo "                 package never entered the run, so nothing about them was measured." >&2
  echo "                 Scoring the rest would report part of the patch as though it were all" >&2
  echo "                 of it. Ran: ${TEST_ARGS[*]}" >&2
  echo "PDCA-UNVERIFIABLE: ${#UNMEASURED[@]} changed file(s) were never measured, so diff coverage is incomplete" >&2
  exit 77
fi

COVERED="$(printf '%s\n' "$SCORED" | awk '/^TOTAL /{print $2; exit}')"
INSTR="$(printf '%s\n' "$SCORED" | awk '/^TOTAL /{print $3; exit}')"
PCT="$(_pct "$COVERED" "$INSTR")"
UNSCORED=$((CHANGED_TOTAL - INSTR))
VERDICT="$(_verdict "$COVERED" "$INSTR" "$DIFFCOV_MIN")"

# One recurring-signal line PER CRATE that has misses, each carrying that crate's OWN counts.
# Two defects forced this. Naming `${PKGS[0]}` filed a telemetry-only miss under wyrd-core, and
# `act._norm` keys on exactly `… in crate <pkg> —`, so the signal pooled under the innocent
# package. Naming the first MISSED crate fixed the attribution but not the arithmetic: COVERED
# and INSTR are totals across the whole report, so a patch missing lines in two crates still
# reported one crate's name against both crates' numbers, overstating that crate's gap and
# hiding the other's entirely (PR #221 review). Per-crate counts come from re-scoring this
# crate's own changed lines — `_score` is already the tested intersection, so this reuses it
# rather than re-deriving the arithmetic.
_emit_act_lines() {
  local crate_dir pkg lines c n
  [ -s "$MISS_FILE" ] || return 0
  while IFS= read -r crate_dir; do
    [ -n "$crate_dir" ] || continue
    pkg="$(_pkg_name "$crate_dir")"; [ -n "$pkg" ] || pkg="$crate_dir"
    lines="$TMPD/lines-$(printf '%s' "$crate_dir" | tr '/' '_')"
    grep "^$crate_dir/" "$CHANGED" > "$lines" 2>/dev/null || true
    [ -s "$lines" ] || continue
    read -r c n < <(_score "$LCOV" "$lines" | awk '/^TOTAL /{print $2, $3; exit}')
    [ "${n:-0}" -gt 0 ] || continue
    [ "$c" -lt "$n" ] || continue          # this crate is fully covered — no signal to file
    echo "run-diff-cov.sh: $(_act_line "$pkg" "$c" "$n")" >&2
  done < <(sed 's/^MISS //; s/:[0-9]*$//' "$MISS_FILE" \
             | while IFS= read -r f; do "$RV" --crate-dir "$f"; done | sort -u)
}

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
echo "METRIC diff_cov=$PCT covered=$COVERED instrumentable=$INSTR unscored=$UNSCORED" >&2

# THE DENOMINATOR IS NOT THE PATCH. Say so on every run, in the numbers, because the difference
# is where this gate's last blind spot lives and the percentage alone hides it: a changed line
# that carries no coverage region is EITHER non-executable (a comment, a `use`, a bare `}` — the
# ordinary case) OR real code the build compiled out behind a `#[cfg(...)]`, and lcov cannot
# tell the caller which. Both simply vanish from the score. Measured during the #197
# adversarial review: 19 of 23 changed lines in a `#[cfg(feature = "tikv")]` block left the
# denominator and the gate reported "100.0% (4/4 changed lines)" — true of what it measured,
# deeply misleading about the patch. Three mechanical discriminators were tried and all failed
# (a ratio: a legitimate patch sat at 26% instrumentable against the gated one's 17%; a
# contiguous unscored run: 78 lines of ordinary doc comment beat the 16-line gated block; an
# added `#[cfg(` attribute: false on both sides, since lines added INSIDE an existing gated
# block carry no attribute of their own). So the gate does not guess — it reports the gap and
# leaves the judgement to the human reading §6, which is what an advisory row is for.
if [ "$UNSCORED" -gt 0 ]; then
  echo "run-diff-cov.sh: NOTE — $INSTR of $CHANGED_TOTAL changed production lines carried a coverage" >&2
  echo "                 region; the other $UNSCORED are not scored either way. Most are simply not" >&2
  echo "                 executable (comments, \`use\`, declarations, bare delimiters). But code" >&2
  echo "                 compiled out by a #[cfg(...)] your feature selection does not enable looks" >&2
  echo "                 identical here, and this gate cannot tell the two apart (#222)." >&2
  echo "                 The largest unscored spans, so you can tell at a glance which it is:" >&2
  _shown=0
  while IFS=$'\t' read -r span count; do
    [ -n "$span" ] || continue
    [ "$_shown" -ge "$UNSCORED_SPANS_SHOWN" ] && { _shown=$((_shown + 1)); continue; }
    printf 'run-diff-cov.sh:   UNSCORED %s (%s line%s)\n' "$span" "$count" \
      "$( [ "$count" = 1 ] || printf s)" >&2
    _shown=$((_shown + 1))
  done < "$RANGES_FILE"
  _total_spans="$(wc -l < "$RANGES_FILE" | tr -d ' ')"
  [ "$_total_spans" -gt "$UNSCORED_SPANS_SHOWN" ] && \
    echo "run-diff-cov.sh:   … and $((_total_spans - UNSCORED_SPANS_SHOWN)) smaller span(s); all of them are in coverage/diff-cov.json" >&2
fi

if [ "$VERDICT" = "FAIL" ]; then
  _emit_act_lines
  echo "run-diff-cov.sh: FAIL — the patch's tests execute $PCT% of its instrumentable changed" >&2
  echo "                 lines, below the ${DIFFCOV_MIN}% floor. Each MISS above is a REACH gap: the" >&2
  echo "                 line never ran. (A line that DID run may still be unasserted — that is" >&2
  echo "                 C5-mutants' question, not this row's.)" >&2
  echo "PDCA-EVIDENCE: diff coverage $PCT% — $COVERED of $INSTR instrumentable changed lines executed (below the ${DIFFCOV_MIN}% floor); $INSTR of $CHANGED_TOTAL changed lines were instrumentable" >&2
  exit 1
fi

echo "run-diff-cov.sh: PASS — the patch's tests execute $PCT% of its $INSTR instrumentable changed" >&2
echo "                 lines (floor ${DIFFCOV_MIN}%, $TESTS_RAN test(s) ran)." >&2
echo "PDCA-EVIDENCE: diff coverage $PCT% — $COVERED of $INSTR instrumentable changed lines executed (floor ${DIFFCOV_MIN}%); $INSTR of $CHANGED_TOTAL changed lines were instrumentable" >&2
exit 0
