#!/usr/bin/env bash
# Tests for engine/scripts/run-verify.sh's patch classification (the `--classify`
# hook), so the red->green wiring doesn't rot. Pure: no worktree, no cargo, no git.
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

[ "$fail" -eq 0 ] && { echo "test_run_verify.sh: all passed"; exit 0; } || { echo "test_run_verify.sh: FAILURES"; exit 1; }
