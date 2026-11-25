#!/usr/bin/env bash
# Apply each iteration-6 mutation in turn, run the session test file under a timeout, restore.
# FINAL is a copy of the unmutated sessions.rs, kept under the harness's scratch root.
set -u
S="$(cd "$(dirname "$0")" && pwd)"
WT="${PDCA_WORKTREE:?}"
FINAL="${1:?path to the unmutated sessions.rs}"
F="$WT/crates/custodian/src/retirement/sessions.rs"
cd "$WT"
for m in $(python3 "$S/mutate6.py" --list); do
  cp "$FINAL" "$F"
  python3 "$S/mutate6.py" "$m" "$F" || { echo "== $m: MUTATION DID NOT APPLY"; continue; }
  out="$(timeout 900 cargo test -p wyrd-custodian --test multipart_session_terminal 2>&1)"
  rc=$?
  echo "== $m: rc=$rc $(echo "$out" | grep -E '^test result|could not compile' | head -1)"
  echo "$out" | grep -E '^test .* FAILED$' | sed 's/^/     /'
done
cp "$FINAL" "$F"
cmp "$F" "$FINAL" && echo "RESTORED byte-identical"
