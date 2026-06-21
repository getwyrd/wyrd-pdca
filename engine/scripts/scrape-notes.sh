#!/usr/bin/env bash
# Seed $PDCA_BUNDLE/notes.json from the Wyrd GitHub issue for the Plan leaf (issue #65).
#
# Wyrd's tracker is GitHub Issues ([tracker] system = "github", url getwyrd/wyrd;
# INTEGRATION §1), so the planner's source of truth is the live ticket. The harness
# template ships [tracker].notes_cmd COMMENTED because its worked example has no GitHub
# tracker — this script is the Wyrd concretization of that seam.
#
# Contract ([tracker].notes_cmd): a `.format(id=)` shell template; $PDCA_BUNDLE is the
# bundle dir; this command MUST write notes.json there itself. The driver runs it only
# when notes.json is absent, and a non-zero exit is non-fatal (Plan falls back to
# --from-csv or asking the human).
#
#   scrape-notes.sh <issue-id>      # writes $PDCA_BUNDLE/notes.json
set -euo pipefail

id="${1:?usage: scrape-notes.sh <issue-id>}"
: "${PDCA_BUNDLE:?\$PDCA_BUNDLE is unset (the driver sets it to the bundle dir)}"
repo="${WYRD_TRACKER_REPO:-getwyrd/wyrd}"

# The full ticket the planner briefs from: title, body, state, labels, milestone, and the
# comment thread. `gh` emits JSON; land it as notes.json in the bundle dir.
gh issue view "$id" --repo "$repo" \
  --json number,title,state,body,labels,milestone,comments,url \
  > "$PDCA_BUNDLE/notes.json"
