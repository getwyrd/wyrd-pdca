# Result — issue 711 / repoint-chunk-segmented-placement-moves

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect: **A chunk that lives in a `seg:` record can never be repaired or evacuated.**
  #695/#696/#697 stopped the maintenance passes aborting on a segmented object, but they write
  nothing: a repair obligation or a drain evacuation for a `seg:`-resident chunk is refused and
  stays queued, every pass, forever, because the only placement writers in the tree rebuild an
  **inode** record and can address a `seg:<nonce>:<epoch>:<index>` record not at all. That defect
  is real and unfixed — it is now carried by **#721** (repair) and **#722** (evacuation). Nothing
  is fixed by THIS bundle.
- Success criterion: the decomposition is **complete and correct**: every element of #711's
  original scope is either carried by a filed, briefed child or explicitly dispositioned in the
  mapping below; both children exist as tracker sub-issues of #711 and as `PLANNED` bundles; and
  no element is left unassigned. Verified at Plan (2026-08-10) and re-checkable at sign-off with
  two commands — `gh api graphql -f query='{repository(owner:"getwyrd",name:"wyrd"){issue(number:711){subIssues(first:10){nodes{number title state}}}}}'`
  and the scope mapping in §"What each child carries" read against each child's `Scope` / `Budget`
  file set.
- Repo + branch target: getwyrd/wyrd @ main   (inherited from the original slice and carried
  unchanged by both children. **No PR opens from this bundle** — publish exits 0 with *"nothing to
  contribute; close the tracker item by hand"* (`publish.py:161-166`), so the tracker action is the
  human's, at sign-off.)
- Scope: record the decomposition of #711 and close the parent — this brief, plus the human's
  sign-off decision. / **out of scope:** any implementation of the repoint primitive or its callers
  (that is #721 and #722, and reopening this bundle to a fix path would rebuild the very
  1595-line shape the split exists to abandon); re-litigating the split itself; re-opening the
  withdrawn dedup finding recorded below.

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: likely-close
  — the `close-disposition` marker already on disk reads `split` and outranks this hint outright
  (`driver.py:210-217`), so the hint is a record, not the control input. `split` is deliberately
  not in `[driver].close_dispositions` (verified: `close_class('split')` returns `''`, while
  `close_class('likely-close')` returns `likely-close`), which is why the configured token is
  written here and the marker carries the real disposition.
- Confidence: medium
- Recommendation: (set by Do)

## 3. Correctness (Check — chain)
- C1 Spec: none — brief.md
- C2 Reproduction (red pre-fix): none — N/A — close disposition (no patch to verify)
- C3 Change: none — patch.diff
- C4 Verification (red→green): none — N/A — close disposition (no patch to verify)
- C5 Causal adequacy: none — reviewer + human sign-off

## 4. Conformance (Check — stack)
- T1 Structure: none — N/A — close disposition (no patch to verify)
- T2 Shape: none — N/A — close disposition (no patch to verify)
- T3 Runtime: none — N/A — close disposition (no patch to verify)
- T4 Contribution: none — N/A — close disposition (no patch to verify)
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

# Advisory review — SKIPPED (close disposition)

The reviewer leaf was skipped: this bundle's Plan concluded a close / no-fix disposition (split), so there is no patch to review.

- NEEDS-HUMAN — Confirm the close disposition 'split' (no patch was built). Override to a fix path (iterate-to-Do) if the close is wrong.


## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [x] Confirm the close disposition 'split' (no patch was built). Override to a fix path (iterate-to-Do) if the close is wrong.

## 7. Proven / not proven
- Proven by which oracle: gates overall = pass (stub oracles).
- Unproven / needs manual run: anything flagged in §6.

## 8. Ready-to-ship attachments
- patch.diff
- tracker-comment.md     (ALWAYS, every tracker item)
- build-notes.md         (builder rationale — for the human, not the reviewer)

## 9. Check sign-off                     ← human completes Check here
- Disposition confirmed / overridden:
- Outcome: merged-wider
- Iteration delta (if iterating):
- By / date: Eduard Ralph / 2026-09-30

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
- Duplicate-ChunkId "one plan, not two" was withdrawn at Plan with no tracker item; decide whether to file it against rebalance.rs (flat + segmented) as a work-reduction cleanup.
- Tracker #711 kept OPEN as an umbrella until #721/#722 land; close it by hand then.
