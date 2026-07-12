#!/usr/bin/env bash
# Tests for engine/scripts/run-verify.sh's patch classification (the `--classify`
# hook) and its lane-scoped isolation (the `--print-isolation` hook), so the
# red->green wiring and multi-lane safety don't rot. Pure: no worktree, no cargo, no git.
#
#   engine/tests/test_run_verify.sh   # exits 0 iff all cases pass
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
RV="$HERE/../scripts/run-verify.sh"
TMP="$(mktemp -d)"; trap 'rm -rf "$TMP"' EXIT
fail=0
check() { # <name> <expected-multiline> <actual-multiline>
  if [ "$2" = "$3" ]; then echo "ok   - $1"; else
    echo "FAIL - $1"; echo "  expected: [$2]"; echo "  actual:   [$3]"; fail=1
  fi
}

# 1. separate added test file + a modified production file in the same crate.
cat > "$TMP/separate.diff" <<'EOF'
diff --git a/crates/server/src/cli.rs b/crates/server/src/cli.rs
--- a/crates/server/src/cli.rs
+++ b/crates/server/src/cli.rs
@@ -1 +1 @@
-old
+new
diff --git a/crates/server/tests/foo.rs b/crates/server/tests/foo.rs
new file mode 100644
--- /dev/null
+++ b/crates/server/tests/foo.rs
@@ -0,0 +1 @@
+#[test] fn t() {}
EOF
check "separate test -> ADDED_TEST + one CRATE" \
  $'ADDED_TEST crates/server/tests/foo.rs\nCRATE crates/server' \
  "$("$RV" --classify "$TMP/separate.diff")"

# 2. co-located test (modified production file only; no separate */tests/*.rs).
cat > "$TMP/colocated.diff" <<'EOF'
diff --git a/xtask/src/main.rs b/xtask/src/main.rs
--- a/xtask/src/main.rs
+++ b/xtask/src/main.rs
@@ -1 +1,4 @@
 fn main() {}
+fn fix() {}
+#[cfg(test)]
+mod tests { #[test] fn t() { super::fix(); } }
EOF
check "co-located -> no ADDED_TEST, CRATE only" \
  "CRATE xtask" \
  "$("$RV" --classify "$TMP/colocated.diff")"

# 3. docs/CI-only change: no crate, nothing to verify per-fix.
cat > "$TMP/docs.diff" <<'EOF'
diff --git a/README.md b/README.md
--- a/README.md
+++ b/README.md
@@ -1 +1 @@
-a
+b
EOF
check "docs-only -> empty classification" "" "$("$RV" --classify "$TMP/docs.diff")"

# 3b. A crate the patch INTRODUCES: --pkg-name resolves its package name from the added
# Cargo.toml (a net-new crate has no pre-patch Cargo.toml to read). Regression for #88 —
# before the fix this path tripped `set -e` and killed the gate silently (exit 1).
cat > "$TMP/newcrate.diff" <<'EOF'
diff --git a/crates/metadata-tikv/Cargo.toml b/crates/metadata-tikv/Cargo.toml
new file mode 100644
--- /dev/null
+++ b/crates/metadata-tikv/Cargo.toml
@@ -0,0 +1,2 @@
+[package]
+name = "wyrd-metadata-tikv"
diff --git a/crates/metadata-tikv/tests/conformance.rs b/crates/metadata-tikv/tests/conformance.rs
new file mode 100644
--- /dev/null
+++ b/crates/metadata-tikv/tests/conformance.rs
@@ -0,0 +1 @@
+#[test] fn t() {}
EOF
check "net-new crate -> --pkg-name resolves it from the patch (#88)" \
  "wyrd-metadata-tikv" \
  "$("$RV" --pkg-name crates/metadata-tikv "$TMP/newcrate.diff")"
check "net-new crate -> --classify still emits ADDED_TEST + CRATE" \
  $'ADDED_TEST crates/metadata-tikv/tests/conformance.rs\nCRATE crates/metadata-tikv' \
  "$("$RV" --classify "$TMP/newcrate.diff")"

# 4. an added NON-test file (e.g. a Dockerfile) is not a discriminator.
cat > "$TMP/addnontest.diff" <<'EOF'
diff --git a/crates/chunkstore-grpc/tests/dserver/Dockerfile b/crates/chunkstore-grpc/tests/dserver/Dockerfile
new file mode 100644
--- /dev/null
+++ b/crates/chunkstore-grpc/tests/dserver/Dockerfile
@@ -0,0 +1 @@
+FROM rust
diff --git a/crates/chunkstore-grpc/tests/tier2.rs b/crates/chunkstore-grpc/tests/tier2.rs
new file mode 100644
--- /dev/null
+++ b/crates/chunkstore-grpc/tests/tier2.rs
@@ -0,0 +1 @@
+#[test] fn t() {}
EOF
check "added .rs test is the discriminator, Dockerfile is not" \
  $'ADDED_TEST crates/chunkstore-grpc/tests/tier2.rs\nCRATE crates/chunkstore-grpc' \
  "$("$RV" --classify "$TMP/addnontest.diff")"

# 5. lane isolation — a serial run (no $PDCA_LANE) keeps the historical names.
check "serial -> ../wyrd-verify on branch pdca-verify" \
  $'VERIFY wyrd-verify\nBRANCH pdca-verify' \
  "$(PDCA_LANE='' "$RV" --print-isolation)"

# 6. a concurrent lane scopes BOTH the worktree dir and the branch by the slot, so two
#    lanes never collide on the checkout or check out one branch in two worktrees.
check "lane 2 -> wyrd-verify-l2 on branch pdca-verify-l2" \
  $'VERIFY wyrd-verify-l2\nBRANCH pdca-verify-l2' \
  "$(PDCA_LANE=2 "$RV" --print-isolation)"

# 7. an explicit $WYRD_VERIFY wins for the dir, but the branch still scopes per lane
#    (the branch is the resource two lanes would actually fight over).
check "WYRD_VERIFY override + lane -> custom dir, lane-scoped branch" \
  $'VERIFY custom-verify\nBRANCH pdca-verify-l1' \
  "$(PDCA_LANE=1 WYRD_VERIFY=/tmp/custom-verify "$RV" --print-isolation)"

# 8. base resolution (#91): the verify base follows the brief's "Repo + branch target",
#    so a stacked milestone slice validates against ITS integration branch — not a
#    hardcoded origin/main. Precedence: $WYRD_VERIFY_BASE > brief base > origin/main.
mkbrief() { mkdir -p "$1"; printf '%s\n' "$2" > "$1/brief.md"; }

mkbrief "$TMP/b_main"  '- **Repo + branch target:** getwyrd/wyrd @ main'
check "brief base=main -> origin/main (historical default)" \
  "origin/main" \
  "$(PDCA_BUNDLE="$TMP/b_main" "$RV" --print-base)"

mkbrief "$TMP/b_m4" '- **Repo + branch target:** getwyrd/wyrd @ `feat/m4-production-metadata-backend`'
check "brief base=integration branch -> origin/<branch> (#91)" \
  "origin/feat/m4-production-metadata-backend" \
  "$(PDCA_BUNDLE="$TMP/b_m4" "$RV" --print-base)"

# the _clean_ref quirk publish uses: a backticked branch in trailing prose wins over the
# plain token — run-verify MUST match publish so both cut from the SAME base.
mkbrief "$TMP/b_paren" '- **Repo + branch target:** getwyrd/wyrd @ main (feature branch `feat/x-slice`)'
check "backtick span wins (matches publish._clean_ref) -> origin/feat/x-slice" \
  "origin/feat/x-slice" \
  "$(PDCA_BUNDLE="$TMP/b_paren" "$RV" --print-base)"

mkbrief "$TMP/b_none" $'- **Kind:** bug\n- **Slug:** something'
check "no branch target field -> origin/main default" \
  "origin/main" \
  "$(PDCA_BUNDLE="$TMP/b_none" "$RV" --print-base)"

check "WYRD_VERIFY_BASE override wins over the brief" \
  "origin/release-1.2" \
  "$(PDCA_BUNDLE="$TMP/b_m4" WYRD_VERIFY_BASE=origin/release-1.2 "$RV" --print-base)"

# 9. cfg-gated test targets (#104). A test whose crate root is `#![cfg(NAME)]` compiles to
#    an EMPTY binary without `--cfg NAME` — "running 0 tests", exit 0 — which an exit-status
#    check reads as a pass. The gate must read the cfg off the sources it compiles and pass
#    the flag, or every crates/dst (madsim) bundle is measured against a vacuum.
printf '%s\n' '#![cfg(madsim)]' '#[madsim::test] async fn t() {}' > "$TMP/dst_test.rs"
check "crate-root #![cfg(madsim)] -> madsim" \
  "madsim" \
  "$("$RV" --cfgs "$TMP/dst_test.rs")"

printf '%s\n' '#[test] fn plain() {}' > "$TMP/plain_test.rs"
check "ungated test -> no cfg (the flag must NOT be invented)" \
  "" \
  "$("$RV" --cfgs "$TMP/plain_test.rs")"

# An indented / attribute-adjacent form still counts; a `#[cfg(...)]` on an ITEM (not the
# crate root `#![...]`) does NOT — it gates one item, not the whole binary.
printf '%s\n' '  #![cfg(feature_x)]' > "$TMP/indented.rs"
check "indented crate-root cfg -> still detected" \
  "feature_x" \
  "$("$RV" --cfgs "$TMP/indented.rs")"

printf '%s\n' '#[cfg(madsim)] fn only_this_item() {}' > "$TMP/item_cfg.rs"
check "item-level #[cfg] is NOT a crate gate -> no cfg" \
  "" \
  "$("$RV" --cfgs "$TMP/item_cfg.rs")"

check "multiple sources -> deduped union, sorted" \
  $'feature_x\nmadsim' \
  "$("$RV" --cfgs "$TMP/dst_test.rs" "$TMP/indented.rs" "$TMP/plain_test.rs")"

# 10. tests actually EXECUTED (#114). `cargo test` exits 0 on a target that compiled to
#     nothing, so the gate must count what ran instead of trusting the exit status —
#     otherwise a 0-test run is a false GREEN, and in the RED leg a false accusation.
printf '%s\n' 'running 0 tests' \
  'test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s' \
  > "$TMP/out_zero.txt"
check "empty target (cargo still exits 0) -> 0 tests ran" \
  "0" \
  "$("$RV" --tests-ran "$TMP/out_zero.txt")"

printf '%s\n' 'test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s' \
  > "$TMP/out_three.txt"
check "3 passing tests -> 3 ran" \
  "3" \
  "$("$RV" --tests-ran "$TMP/out_three.txt")"

printf '%s\n' 'test result: FAILED. 1 passed; 2 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s' \
  > "$TMP/out_failed.txt"
check "failures count as RUN (a red is a measurement) -> 3 ran" \
  "3" \
  "$("$RV" --tests-ran "$TMP/out_failed.txt")"

# An #[ignore]d test asserted nothing, so it did NOT run — this is one of the ways a target
# reports zero after #104 removes the cfg cause.
printf '%s\n' 'test result: ok. 0 passed; 0 failed; 5 ignored; 0 measured; 0 filtered out; finished in 0.00s' \
  > "$TMP/out_ignored.txt"
check "all tests #[ignore]d -> 0 ran (ignored is not executed)" \
  "0" \
  "$("$RV" --tests-ran "$TMP/out_ignored.txt")"

# Multiple targets in one invocation: sum across every summary line.
printf '%s\n' 'test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s' \
  'test result: ok. 4 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s' \
  > "$TMP/out_multi.txt"
check "multiple targets -> summed across summaries" \
  "7" \
  "$("$RV" --tests-ran "$TMP/out_multi.txt")"

[ "$fail" -eq 0 ] && { echo "test_run_verify.sh: all passed"; exit 0; } || { echo "test_run_verify.sh: FAILURES"; exit 1; }
