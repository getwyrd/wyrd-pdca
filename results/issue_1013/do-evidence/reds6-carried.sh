#!/usr/bin/env bash
# Re-run the brief's six reds and iterations 1-5's mutations on iteration 6's tree. Production
# files are restored from the backups in $1 (a copy of the five files, laid out as in the tree),
# since the patch is uncommitted in the worktree. Results go to $2.
set -u
E="$(cd "$(dirname "$0")" && pwd)"; wt="${PDCA_WORKTREE:?}"
backup="${1:?backup dir}"; out="${2:?result log}"; logs="$(dirname "$out")/red6"; mkdir -p "$logs"; : > "$out"
files="crates/custodian/src/retirement/marks.rs crates/custodian/src/retirement/sessions.rs crates/custodian/src/retirement.rs crates/core/src/multipart.rs crates/server/src/custodian.rs"
sessions="$wt/crates/custodian/src/retirement/sessions.rs"
restore() { for f in $files; do cp "$backup/$f" "$wt/$f"; done; }
run() { # $1 label, $2 package, $3 test target
  local log="$logs/$1.log"
  timeout 900 cargo test -p "$2" --test "$3" > "$log" 2>&1; local rc=$?
  local ran; ran=$(grep -Eo 'test result: [a-zA-Z]+\. [0-9]+ passed; [0-9]+ failed' "$log" | tail -1)
  echo "== $1: rc=$rc ${ran:-NO TEST RESULT}" >> "$out"
  grep -E '^test .* FAILED$' "$log" | sed 's/^/   /' >> "$out"
  grep -E '^error(\[|:)' "$log" | head -3 | sed 's/^/   /' >> "$out"
}
cd "$wt"
# mutate.py: the brief's six (M1-M6), M0 and R1-R12. R5 is re-spelled in mutate6-carried.py.
for m in $(python3 -I "$E/mutate.py" "$wt" --list); do
  [ "$m" = R5-walk-step-spends-no-step ] && continue
  restore
  python3 -I "$E/mutate.py" "$wt" "$m" || { echo "== $m: DID NOT APPLY" >> "$out"; continue; }
  run "$m" wyrd-custodian multipart_session_terminal
done
# Iteration 4's bookmark mutations, as iteration 5 spelled them.
for m in $(python3 -I "$E/mutate5-carried.py" x --list); do
  case "$m" in iter3|keptfirst) continue ;; esac
  restore
  python3 -I "$E/mutate5-carried.py" "$sessions" "$m" || { echo "== B-$m: DID NOT APPLY" >> "$out"; continue; }
  run "B-$m" wyrd-custodian multipart_session_terminal
done
# Iteration 5's.
for m in $(python3 -I "$E/mutate5.py" --list); do
  case "$m" in P-notidle|P-nopass|P-passed-ge|B-kept-over-elsewhere) continue ;; esac
  restore
  python3 -I "$E/mutate5.py" "$m" "$sessions" || { echo "== 5-$m: DID NOT APPLY" >> "$out"; continue; }
  run "5-$m" wyrd-custodian multipart_session_terminal
done
# The seven whose text iteration 6 changed, re-spelled.
for m in $(python3 -I "$E/mutate6-carried.py" --list); do
  restore
  python3 -I "$E/mutate6-carried.py" "$m" "$sessions" || { echo "== 6c-$m: DID NOT APPLY" >> "$out"; continue; }
  run "6c-$m" wyrd-custodian multipart_session_terminal
done
# The deployed caller's two inputs, against the server test.
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
for f in $files; do cmp -s "$wt/$f" "$backup/$f" || echo "NOT RESTORED $f" >> "$out"; done
echo done >> "$out"
