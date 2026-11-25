#!/usr/bin/env bash
# Run every named mutation against the custodian test file (and the server ones against the
# server test), restoring production from backups taken here — never from git, since the patch
# is uncommitted in the worktree.
set -u
wt="$1"; work="$2"; out="$work/reds4.log"; : > "$out"
files="crates/custodian/src/retirement/marks.rs crates/custodian/src/retirement/sessions.rs crates/custodian/src/retirement.rs crates/core/src/multipart.rs crates/server/src/custodian.rs"
for f in $files; do mkdir -p "$work/backup/$(dirname "$f")"; cp "$wt/$f" "$work/backup/$f"; done
restore() { for f in $files; do cp "$work/backup/$f" "$wt/$f"; done; }
run() { # $1 label, $2 package, $3 test target
  local log="$work/red4-$1.log"
  timeout 900 cargo test -p "$2" --test "$3" > "$log" 2>&1; local rc=$?
  local ran; ran=$(grep -Eo 'test result: [a-zA-Z]+\. [0-9]+ passed; [0-9]+ failed' "$log" | tail -1)
  echo "== $1: rc=$rc ${ran:-NO TEST RESULT}" >> "$out"
  grep -E '^test .* FAILED$' "$log" | sed 's/^/   /' >> "$out"
  grep -E '^error(\[|:)' "$log" | head -3 | sed 's/^/   /' >> "$out"
}
cd "$wt"
for m in $(python3 -I "$work/mutate2.py" "$wt" --list); do
  restore
  if ! python3 -I "$work/mutate2.py" "$wt" "$m"; then echo "== $m: DID NOT APPLY" >> "$out"; continue; fi
  run "$m" wyrd-custodian multipart_session_terminal
done
for m in iter3 noreorder notmoved nofilter keptlt keptge coveredge heldge keptfirst; do
  restore
  python3 -I "$work/mutate.py" "$wt/crates/custodian/src/retirement/sessions.rs" "$m" >/dev/null || { echo "== $m: DID NOT APPLY" >> "$out"; continue; }
  run "B-$m" wyrd-custodian multipart_session_terminal
done
for m in fleet tomb; do
  restore
  python3 -I - "$wt/crates/server/src/custodian.rs" "$m" <<'PY' || { echo "== $m: DID NOT APPLY" >> "$out"; continue; }
import sys, pathlib
f = pathlib.Path(sys.argv[1]); m = sys.argv[2]; s = f.read_text()
old, new = {
 "fleet": ("                operator_fleet_size,\n                // Nothing measures", "                operator_fleet_size: fleet.len(),\n                // Nothing measures"),
 "tomb": ("                tombstone: Tombstone::Retain,", "                tombstone: Tombstone::Elapsed,"),
}[m]
assert s.count(old) == 1
f.write_text(s.replace(old, new))
PY
  run "S-$m" wyrd-server custodian_gc
done
restore
for f in $files; do cmp -s "$wt/$f" "$work/backup/$f" || echo "NOT RESTORED $f" >> "$out"; done
git -C "$wt" status --short >> "$out"
echo done >> "$out"
