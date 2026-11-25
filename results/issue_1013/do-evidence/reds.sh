#!/usr/bin/env bash
# Run every mutation of mutate.py against the new test file; restore production after each.
set -u
wt="$1"
work="$(cd "$(dirname "$0")" && pwd)"
out="$work/${OUT:-reds.log}"
: > "$out"
cd "$wt"
for m in ${MUTS:-$(python3 -I "$work/mutate.py" "$wt" --list)}; do
  if ! python3 -I "$work/mutate.py" "$wt" "$m"; then
    echo "== $m: MUTATION DID NOT APPLY" >> "$out"
    git checkout -q -- crates/custodian/src crates/core/src
    continue
  fi
  log="$work/red-$m.log"
  timeout 900 cargo test -p wyrd-custodian --test multipart_session_terminal > "$log" 2>&1
  rc=$?
  ran=$(grep -Eo 'test result: [a-zA-Z]+\. [0-9]+ passed; [0-9]+ failed' "$log" | tail -1)
  echo "== $m: rc=$rc ${ran:-NO TEST RESULT}" >> "$out"
  grep -E '^test .* FAILED$' "$log" | sed 's/^/   /' >> "$out"
  git checkout -q -- crates/custodian/src crates/core/src
done
git status --short >> "$out"
echo done >> "$out"
