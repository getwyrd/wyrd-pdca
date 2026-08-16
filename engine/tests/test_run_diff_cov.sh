#!/usr/bin/env bash
# Tests for engine/scripts/run-diff-cov.sh's pure logic — the diff line arithmetic
# (`--changed-lines`), the lcov intersection (`--score`), the verdict truth table
# (`--verdict`), the lane-scoped isolation (`--print-isolation`) and the recurring-signal
# line's normalization shape (`--act-line`), so the C4-diff-cov gate doesn't rot.
# Pure: no worktree, no cargo, no git, no cargo-llvm-cov.
#
#   engine/tests/test_run_diff_cov.sh   # exits 0 iff all cases pass
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DC="$HERE/../scripts/run-diff-cov.sh"
TMP="$(mktemp -d)"; trap 'rm -rf "$TMP"' EXIT
fail=0
check() { # <name> <expected-multiline> <actual-multiline>
  if [ "$2" = "$3" ]; then echo "ok   - $1"; else
    echo "FAIL - $1"; echo "  expected: [$2]"; echo "  actual:   [$3]"; fail=1
  fi
}

# 1. the ordinary case: a modified production file. A modified line arrives as a `+` line, so
#    "added or modified" needs no second rule — what is reported is the NEW-file line number,
#    which is the only numbering an lcov report can be intersected with.
cat > "$TMP/modified.diff" <<'EOF'
diff --git a/crates/core/src/multipart.rs b/crates/core/src/multipart.rs
--- a/crates/core/src/multipart.rs
+++ b/crates/core/src/multipart.rs
@@ -10,3 +10,5 @@ impl Session {
 fn keep() {}
-fn old() {}
+fn new() {}
+fn extra() {}
+fn more() {}
 fn tail() {}
EOF
check "modified production file -> the new-side line numbers of its + lines" \
  $'crates/core/src/multipart.rs:11\ncrates/core/src/multipart.rs:12\ncrates/core/src/multipart.rs:13' \
  "$("$DC" --changed-lines "$TMP/modified.diff")"

# 2. TWO hunks in one file. The second hunk's numbering comes from its own header, never from
#    a counter carried over the first — get this wrong and every reported line after the first
#    hunk names innocent code, which reads as a coverage miss in a line nobody touched.
cat > "$TMP/twohunks.diff" <<'EOF'
diff --git a/crates/core/src/a.rs b/crates/core/src/a.rs
--- a/crates/core/src/a.rs
+++ b/crates/core/src/a.rs
@@ -1,2 +1,3 @@
 fn one() {}
+fn two() {}
 fn three() {}
@@ -40,2 +41,4 @@
 fn forty() {}
+fn forty_one() {}
+fn forty_two() {}
 fn tail() {}
EOF
check "two hunks -> each numbered from its own header" \
  $'crates/core/src/a.rs:2\ncrates/core/src/a.rs:42\ncrates/core/src/a.rs:43' \
  "$("$DC" --changed-lines "$TMP/twohunks.diff")"

# 3. A hunk body is consumed by COUNT, not by pattern. An added line whose CONTENT begins
#    `++ ` or `@@ ` renders as `+++ ` / `+@@ ` in the diff — a parser keying on the pattern
#    would re-anchor onto that phantom file and misnumber everything after it. This is not
#    hypothetical for this repo: patches here routinely add fixture diffs (see
#    engine/tests/*.sh, scripts/review-branch) whose bodies are literal diff text.
cat > "$TMP/patchinpatch.diff" <<'EOF'
diff --git a/crates/core/src/a.rs b/crates/core/src/a.rs
--- a/crates/core/src/a.rs
+++ b/crates/core/src/a.rs
@@ -1,1 +1,4 @@
 fn a() {}
+++ b/crates/evil/src/lib.rs
+@@ -1 +999 @@
+fn b() {}
EOF
check "diff text INSIDE a hunk is content, not structure" \
  $'crates/core/src/a.rs:2\ncrates/core/src/a.rs:3\ncrates/core/src/a.rs:4' \
  "$("$DC" --changed-lines "$TMP/patchinpatch.diff")"

# 4. Test files leave BOTH numerator and denominator — added ones and modified ones alike.
#    `run-verify.sh --classify` flags only the tests a patch ADDS, which is the right question
#    for red->green and the wrong one here: a test file the patch merely edits is still not
#    production code whose reach is being scored.
cat > "$TMP/withtests.diff" <<'EOF'
diff --git a/crates/core/src/lib.rs b/crates/core/src/lib.rs
--- a/crates/core/src/lib.rs
+++ b/crates/core/src/lib.rs
@@ -1 +1,2 @@
 fn prod() {}
+fn added_prod() {}
diff --git a/crates/core/tests/added.rs b/crates/core/tests/added.rs
new file mode 100644
--- /dev/null
+++ b/crates/core/tests/added.rs
@@ -0,0 +1,2 @@
+#[test]
+fn t() {}
diff --git a/crates/core/tests/existing.rs b/crates/core/tests/existing.rs
--- a/crates/core/tests/existing.rs
+++ b/crates/core/tests/existing.rs
@@ -5 +5,2 @@
 fn old_t() {}
+fn new_t() {}
EOF
check "test files are excluded, ADDED and MODIFIED alike" \
  "crates/core/src/lib.rs:2" \
  "$("$DC" --changed-lines "$TMP/withtests.diff")"

# 5. Docs / CI-only: nothing to score. The gate treats this as a genuine N/A (exit 0), not as
#    a missing measurement — there is no production line whose reach could be in question.
cat > "$TMP/docs.diff" <<'EOF'
diff --git a/README.md b/README.md
--- a/README.md
+++ b/README.md
@@ -1 +1,2 @@
 a
+b
EOF
check "docs-only -> nothing scored" "" "$("$DC" --changed-lines "$TMP/docs.diff")"

# 6. A non-.rs file inside a crate is outside the boundary too: lcov reports Rust regions, so
#    a changed .toml/.json/Dockerfile line has no coverage record to intersect with and must
#    never enter the denominator.
cat > "$TMP/nonrust.diff" <<'EOF'
diff --git a/crates/core/Cargo.toml b/crates/core/Cargo.toml
--- a/crates/core/Cargo.toml
+++ b/crates/core/Cargo.toml
@@ -3 +3,2 @@
 [dependencies]
+serde = "1"
diff --git a/crates/core/src/lib.rs b/crates/core/src/lib.rs
--- a/crates/core/src/lib.rs
+++ b/crates/core/src/lib.rs
@@ -1 +1,2 @@
 fn a() {}
+fn b() {}
EOF
check "a non-.rs file under a crate is outside the boundary" \
  "crates/core/src/lib.rs:2" \
  "$("$DC" --changed-lines "$TMP/nonrust.diff")"

# 7. A DELETED file has no new side, so it contributes no scored line — and, critically, the
#    `+++ /dev/null` header must not leave the previous file's path latched.
cat > "$TMP/deleted.diff" <<'EOF'
diff --git a/crates/core/src/gone.rs b/crates/core/src/gone.rs
deleted file mode 100644
--- a/crates/core/src/gone.rs
+++ /dev/null
@@ -1,2 +0,0 @@
-fn a() {}
-fn b() {}
diff --git a/crates/core/src/kept.rs b/crates/core/src/kept.rs
--- a/crates/core/src/kept.rs
+++ b/crates/core/src/kept.rs
@@ -1 +1,2 @@
 fn k() {}
+fn added() {}
EOF
check "a deleted file scores nothing and does not latch its path" \
  "crates/core/src/kept.rs:2" \
  "$("$DC" --changed-lines "$TMP/deleted.diff")"

# 8. `\ No newline at end of file` is a marker, not a line — it consumes no counter.
cat > "$TMP/nonewline.diff" <<'EOF'
diff --git a/crates/core/src/n.rs b/crates/core/src/n.rs
--- a/crates/core/src/n.rs
+++ b/crates/core/src/n.rs
@@ -1,2 +1,2 @@
 fn a() {}
-fn old() {}
\ No newline at end of file
+fn new() {}
\ No newline at end of file
EOF
check "the no-newline marker consumes no line number" \
  "crates/core/src/n.rs:2" \
  "$("$DC" --changed-lines "$TMP/nonewline.diff")"

# ---------------------------------------------------------------------------------------
# --score: intersect the changed lines with the lcov report.
# ---------------------------------------------------------------------------------------
# 9. The load-bearing exclusion. lcov emits `DA:<line>,<hits>` only for lines the compiler
#    generated a coverage REGION for. A comment, a `use`, a blank, a bare `}` get none — and
#    scoring those as uncovered would red a patch over lines no test could ever execute (the
#    `false-red` class, process/act-log.md 2026-07-22). Verified against a real report during
#    #197: in a 14-line fixture crate, lines 1 (comment) and 2 (`use`) carried no DA record
#    while the unreached `else` arm carried `DA:8,0`.
cat > "$TMP/basic.lcov" <<'EOF'
SF:/home/build/wyrd-cov/crates/core/src/lib.rs
DA:4,1
DA:5,1
DA:8,0
DA:10,3
LF:4
LH:3
end_of_record
EOF
printf '%s\n' \
  'crates/core/src/lib.rs:1' \
  'crates/core/src/lib.rs:4' \
  'crates/core/src/lib.rs:5' \
  'crates/core/src/lib.rs:8' \
  'crates/core/src/lib.rs:10' > "$TMP/basic.lines"
check "a changed line with no DA record leaves the denominator entirely" \
  $'MISS crates/core/src/lib.rs:8\nTOTAL 3 4' \
  "$("$DC" --score "$TMP/basic.lcov" "$TMP/basic.lines")"

# 10. The report's SF paths are ABSOLUTE (they point into whichever worktree built it) while
#     the patch's paths are repo-relative, so matching is by whole path COMPONENT — never a
#     bare substring, or `core/src/lib.rs` would also claim `vendor/notcore/src/lib.rs` and
#     silently score one file's coverage against another file's changed lines.
cat > "$TMP/suffix.lcov" <<'EOF'
SF:/home/build/wyrd-cov/vendor/notcore/src/lib.rs
DA:1,0
DA:2,0
end_of_record
SF:/home/build/wyrd-cov/core/src/lib.rs
DA:1,7
DA:2,7
end_of_record
EOF
printf '%s\n' 'core/src/lib.rs:1' 'core/src/lib.rs:2' > "$TMP/suffix.lines"
check "SF matching is by path component — notcore/ does not claim core/" \
  "TOTAL 2 2" \
  "$("$DC" --score "$TMP/suffix.lcov" "$TMP/suffix.lines")"

# A workspace-relative SF (no leading directory at all) still matches exactly.
cat > "$TMP/relative.lcov" <<'EOF'
SF:crates/core/src/lib.rs
DA:4,0
DA:5,2
end_of_record
EOF
printf '%s\n' 'crates/core/src/lib.rs:4' 'crates/core/src/lib.rs:5' > "$TMP/relative.lines"
check "a workspace-relative SF matches by exact equality" \
  $'MISS crates/core/src/lib.rs:4\nTOTAL 1 2' \
  "$("$DC" --score "$TMP/relative.lcov" "$TMP/relative.lines")"

# 11. A changed file the report never mentions contributes nothing to either side. It is not
#     0% coverage of that file — it is no measurement of it, and the run-level verdict handles
#     "nothing was measured at all" separately (case 13).
cat > "$TMP/partial.lines" <<'EOF'
crates/core/src/lib.rs:4
crates/core/src/lib.rs:8
crates/other/src/lib.rs:1
crates/other/src/lib.rs:2
EOF
check "a changed file absent from the report scores on neither side" \
  $'MISS crates/core/src/lib.rs:8\nTOTAL 1 2' \
  "$("$DC" --score "$TMP/basic.lcov" "$TMP/partial.lines")"

# ---------------------------------------------------------------------------------------
# --verdict: the truth table.
# ---------------------------------------------------------------------------------------
# 12. Integer arithmetic on covered*100 >= min*instrumentable, so the boundary is exact and no
#     float rounding decides a pass. 80/100 at a floor of 80 must PASS, not fail by an epsilon.
check "verdict: comfortably above the floor -> PASS" \
  "PASS" "$("$DC" --verdict 9 10 80)"
check "verdict: exactly AT the floor -> PASS (the boundary is inclusive)" \
  "PASS" "$("$DC" --verdict 80 100 80)"
check "verdict: one line below the floor -> FAIL" \
  "FAIL" "$("$DC" --verdict 79 100 80)"
check "verdict: nothing covered, with lines to cover -> FAIL" \
  "FAIL" "$("$DC" --verdict 0 40 80)"
check "verdict: a floor of 100 admits no miss" \
  "FAIL" "$("$DC" --verdict 99 100 100)"

# 13. The cell the issue's own spec does not name. Zero INSTRUMENTABLE lines is reached only
#     after the run has already found changed production lines, so it means the report covered
#     none of the changed FILES — the instrumentation never reached them. That is a broken
#     measurement, not 0% coverage, and a gate never turns "no evidence" into a verdict
#     (engine/README.md). Reporting it as a fail would accuse a patch of a defect the evidence
#     cannot support; reporting it as a pass would be a false green in a gate about reach.
check "verdict: zero instrumentable -> UNVERIFIABLE, never PASS and never FAIL" \
  "UNVERIFIABLE" "$("$DC" --verdict 0 0 80)"

# The default floor is 80 when none is passed, so the gate's own default is pinned here rather
# than only in the header prose.
check "verdict: the default floor is 80" \
  "FAIL" "$("$DC" --verdict 79 100)"
check "verdict: the default floor admits 80%" \
  "PASS" "$("$DC" --verdict 80 100)"
check "verdict: \$WYRD_DIFFCOV_MIN overrides the default floor" \
  "PASS" "$(WYRD_DIFFCOV_MIN=50 "$DC" --verdict 60 100)"

# ---------------------------------------------------------------------------------------
# --print-isolation: lane safety.
# ---------------------------------------------------------------------------------------
# 14. pdca.toml: "Any FUTURE gate that adds a container / port / scratch dir must likewise
#     $PDCA_LANE-qualify it." Both the worktree AND the branch are scoped, because two lanes
#     collide on either one. A serial run keeps the unsuffixed names.
check "serial -> ../wyrd-cov on branch pdca-cov" \
  $'COV wyrd-cov\nBRANCH pdca-cov' \
  "$(PDCA_LANE='' "$DC" --print-isolation)"
check "lane 2 -> wyrd-cov-l2 on branch pdca-cov-l2" \
  $'COV wyrd-cov-l2\nBRANCH pdca-cov-l2' \
  "$(PDCA_LANE=2 "$DC" --print-isolation)"
# $WYRD_COV moves the DIR only — the branch stays lane-scoped, since the branch is the resource
# two lanes would actually fight over (the same asymmetry test_run_verify.sh pins for $WYRD_VERIFY).
check "WYRD_COV override + lane -> custom dir, lane-scoped branch" \
  $'COV custom-cov\nBRANCH pdca-cov-l1' \
  "$(PDCA_LANE=1 WYRD_COV=/tmp/custom-cov "$DC" --print-isolation)"
# And it must never be C4-verify's checkout: sharing one worktree would have each gate
# invalidating the other's build cache every cycle (llvm-cov builds under -C instrument-coverage).
check "the cov worktree is NOT the verify worktree" \
  "distinct" \
  "$([ "$("$DC" --print-isolation | head -1)" != "COV wyrd-verify" ] && echo distinct)"

# ---------------------------------------------------------------------------------------
# --act-line: the recurring-signal shape.
# ---------------------------------------------------------------------------------------
# 15. Act pools §10 candidates and §6 items and calls two of them the same when their first
#     EIGHT WORDS match, lowercased (act._norm). A line carrying its numbers early can never
#     match itself across cycles, so the recurrence this row exists to expose — one crate
#     repeatedly shipping fixes its tests do not reach — would never register. This fails
#     SILENTLY (nothing errors; the signal simply never recurs), which is why it is pinned
#     mechanically rather than left to the header prose.
_first8() { printf '%s' "$1" | tr 'A-Z' 'a-z' | tr -s '[:space:]' ' ' | cut -d' ' -f1-8; }
_line_a="$("$DC" --act-line wyrd-core 12 40)"
_line_b="$("$DC" --act-line wyrd-core 31 33)"
check "act-line: two DIFFERENT scores for one crate normalize to the same first 8 words" \
  "$(_first8 "$_line_a")" "$(_first8 "$_line_b")"
# A blanket "no digits" assertion would be wrong: `C4-diff-cov` carries a digit and the crate
# name may too (wyrd-s3), and both are INVARIANT across cycles. What must not appear in the
# matched prefix is a VARYING quantity — the counts and the percentage.
check "act-line: no varying quantity appears in the first 8 words" \
  "clean" \
  "$(_first8 "$_line_a" | grep -qE '(^| )(28|40|30\.0)($| )' && echo "a count leaked into the key" || echo clean)"
check "act-line: a different crate is a DIFFERENT signal" \
  "differs" \
  "$([ "$(_first8 "$_line_a")" != "$(_first8 "$("$DC" --act-line wyrd-chunkstore 12 40)")" ] && echo differs)"
# The numbers still have to be there — the line is the human's evidence at sign-off, not just
# a matcher key. 40 instrumentable with 12 covered is 28 lines never executed.
check "act-line: still carries the counts and the percentage" \
  "C4-diff-cov: uncovered changed lines in crate wyrd-core — 28 of 40 not executed by the shipped test (30.0%)" \
  "$_line_a"

[ "$fail" -eq 0 ] && { echo "test_run_diff_cov.sh: all passed"; exit 0; } || { echo "test_run_diff_cov.sh: FAILURES"; exit 1; }
