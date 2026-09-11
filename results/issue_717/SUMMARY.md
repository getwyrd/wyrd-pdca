# Result — issue 717 / multipart-staging-retire-pending

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect: **Two disjoint record namespaces have no value half.** The base carries the *key*
  half of both — `RetireToken` (`multipart.rs:1022`), `retire_key` (`:1071`), `parse_retire_key`
  (`:1092`), `sidx_key` (`:907`), `parse_sidx_key` (`:925`) — and nothing that can read the
  values those keys name. Nothing decodes a retirement obligation, nothing decodes an owned
  staging entry, and the `pending:` ledger cannot tell an owned entry from an ordinary lease.
  That defect is real and unfixed — it is now carried by **#771** (the `retire:` obligation
  value) and **#772** (`sidx:` + the `PendingEntry` extension). Nothing is fixed by THIS bundle.
- Success criterion: the decomposition is **complete and correct**: every element of #717's
  original scope is carried by a filed, briefed child; both children exist as tracker sub-issues
  of #717 and as `PLANNED` bundles; and the two children's namespaces do not overlap. Verified at
  the re-plan (2026-08-14) and re-checkable at sign-off with two commands —
  `gh api graphql -f query='{repository(owner:"getwyrd",name:"wyrd"){issue(number:717){subIssues(first:10){nodes{number title state}}}}}'`
  (returns #771 and #772 OPEN) and `cat results/issue_717/split-lineage.json` (`children:
  ["771","772"]`) — read against the scope mapping below.
- Repo + branch target: getwyrd/wyrd @ main   (inherited from the original slice and carried
  unchanged by both children. **No PR opens from this bundle** — publish exits 0 with *"nothing to
  contribute; close the tracker item by hand"* (`publish.py:161-166`), so the tracker action is the
  human's, at sign-off.)
- Scope: record the decomposition of #717 and close the parent — this brief, plus the human's
  sign-off decision. / **out of scope:** any implementation of either record family (that is #771
  and #772, and reopening this bundle to a fix path would rebuild the very 12-file shape the split
  exists to abandon); re-litigating the split itself; the three record-format decisions, each of
  which now belongs to a named child.

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: likely-close
  — the `close-disposition` marker already on disk reads `split` and outranks this hint outright
  (`driver.py:210-217`). `split` is deliberately **not** in `[driver].close_dispositions`, which is
  exactly why the parent never freezes on the marker alone; the configured token is written here so
  the close fast path fires and the bundle can reach sign-off.
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
- [x] C3 Change — Decide whether to re-enter Plan for the size overrun — `patch.diff` adds 1,780 raw lines and at least 1,131 nonblank/noncomment lines against the brief's ≤960-semantic-line budget, materially increasing review and rebase surface. (moot — superseded by the split; no `patch.diff` in this bundle)
- [x] The “ONE `metadata.rs` hunk — nothing else in that file changes” scope (`brief.md:79-82`) omits an existing in-file `PendingEntry` constructor at `crates/core/src/metadata.rs:3369-3377`; adding two required fields makes that constructor fail to compile unless a second, distant hunk is changed. The scope/hunk and line budgets must explicitly allow this ninth constructor site (within the already-counted substantive file). (moot — superseded by the split; see §10 for follow-up)

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
- By / date: Eduard Ralph / 2026-08-17

## 10. Act candidates (hints for the next Act review)
- The pre-split iteration on #717 found an existing `PendingEntry` constructor at `crates/core/src/metadata.rs:3369-3377` that a "ONE metadata.rs hunk" scope missed — whichever of #771/#772 owns the `PendingEntry` extension should account for this second constructor site in its brief/hunk budget.
