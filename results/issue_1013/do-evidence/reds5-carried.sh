#!/usr/bin/env bash
# Re-run the brief's six reds and iterations 1-4's mutations on iteration 5's tree. Production
# files are restored from backups taken here (the patch is uncommitted in the worktree).
set -u
S="$(cd "$(dirname "$0")" && pwd)"; wt="${PDCA_WORKTREE:?}"; out="$S/reds5-carried.log"; : > "$out"
files="crates/custodian/src/retirement/marks.rs crates/custodian/src/retirement/sessions.rs crates/custodian/src/retirement.rs crates/core/src/multipart.rs crates/server/src/custodian.rs"
for f in $files; do mkdir -p "$S/backup/$(dirname "$f")"; cp "$wt/$f" "$S/backup/$f"; done
restore() { for f in $files; do cp "$S/backup/$f" "$wt/$f"; done; }
run() { # $1 label, $2 package, $3 test target
  local log="$S/red5-$1.log"
  timeout 900 cargo test -p "$2" --test "$3" > "$log" 2>&1; local rc=$?
  local ran; ran=$(grep -Eo 'test result: [a-zA-Z]+\. [0-9]+ passed; [0-9]+ failed' "$log" | tail -1)
  echo "== $1: rc=$rc ${ran:-NO TEST RESULT}" >> "$out"
  grep -E '^test .* FAILED$' "$log" | sed 's/^/   /' >> "$out"
  grep -E '^error(\[|:)' "$log" | head -3 | sed 's/^/   /' >> "$out"
}
cd "$wt"
for m in $(python3 -I "$S/mutate2.py" "$wt" --list); do
  restore
  if ! python3 -I "$S/mutate2.py" "$wt" "$m"; then echo "== $m: DID NOT APPLY" >> "$out"; continue; fi
  run "$m" wyrd-custodian multipart_session_terminal
done
for m in $(python3 -I "$S/mutate5-carried.py" x --list); do
  restore
  python3 -I "$S/mutate5-carried.py" "$wt/crates/custodian/src/retirement/sessions.rs" "$m" || { echo "== B-$m: DID NOT APPLY" >> "$out"; continue; }
  run "B-$m" wyrd-custodian multipart_session_terminal
done
for m in fleet tomb; do
  restore
  python3 -I - "$wt/crates/server/src/custodian.rs" "$m" <<'PY' || { echo "== S-$m: DID NOT APPLY" >> "$out"; continue; }
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
for f in $files; do cmp -s "$wt/$f" "$S/backup/$f" || echo "NOT RESTORED $f" >> "$out"; done
echo done >> "$out"
