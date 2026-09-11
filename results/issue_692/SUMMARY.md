# Result — issue 692 / multipart-record-family

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect: **The multipart key space had no value half.** The key grammar landed with #691
  (merged, `d986069`), but nothing could decode the records those keys name, so any store round trip
  that later read one (#656–#659) would have had to trust an unvalidated blob — an internally
  inconsistent `mpuctl` admission record lets a gateway admit sessions past the memory bound the
  reconcile pass is sized for. That defect was real; it is now **almost entirely fixed**, by
  children rather than by this bundle. `Budget`/`AdmissionRecord` (#715) and the
  session/slot/part lifecycle records (#716) are merged on `origin/main`; the two remaining
  namespaces — `retire:` and `sidx:`/`pending:` — are carried by **#771** and **#772**. Nothing is
  fixed by THIS bundle.
- Success criterion: the decomposition is **complete and correct**: every element of #692's
  original scope is either carried by a filed, briefed child, or is a design element **explicitly
  withdrawn** on the target with its withdrawal recorded in-tree. Re-verified at this revision pass
  (2026-08-18) against `origin/main` @ **`a801997`**. It is checked **row by row** against the
  mapping table below — every row has a command, and no row is asserted from prose. (`$PDCA_TARGET`
  is the target checkout the driver exports, AGENTS.md; the `../wyrd` fallback is INTEGRATION §2's
  sibling checkout. The earlier draft hardcoded `../wyrd`, which does not resolve for a reader whose
  sandbox mounts the target elsewhere — hence the parameterised form throughout.)
  1. **The children exist — and so do THEIR children.**
     `gh api graphql -f query='{repository(owner:"getwyrd",name:"wyrd"){issue(number:692){subIssues(first:10){nodes{number title state}}}}}'`
     → exactly #715 CLOSED, #716 CLOSED, #717 OPEN. That query sees **direct** sub-issues only, so it
     is not sufficient alone; the same query with **`number:717`** → #771 OPEN
     (`multipart-retire-obligation`), #772 OPEN (`multipart-owned-staging-entry`), and
     `cat results/issue_717/split-lineage.json` → `{"children":["771","772"]}`. Both were run at this
     revision.
  2. **Every child has a bundle with a brief.** `ls results/issue_{715,716,717,771,772}/brief.md`
     → five paths, no error. This is the check for "a child's bundle is missing".
  3. **Namespace ownership — positively and negatively, in ONE command.**
     `git -C "${PDCA_TARGET:-../wyrd}" grep -n 'pub struct \(Budget\|AdmissionRecord\|SessionRecord\|SlotRecord\|PartRecord\|PartSummary\|RetirePayload\|OwnedEntry\|StagedPlacement\)' origin/main -- crates/core/src/multipart.rs`
     → exactly **six** hits: `:1158` `Budget`, `:1413` `AdmissionRecord` (#715); `:1711`
     `SessionRecord`, `:1862` `SlotRecord`, `:2073` `PartRecord`, `:2172` `PartSummary` (#716). No
     `RetirePayload`, no `OwnedEntry`, no `StagedPlacement`. That single output carries both halves of
     the overlap check: each merged child's symbols are present **and disjoint**, and the two in-flight
     children's symbols are genuinely still unbuilt — so #771/#772 cannot be re-deriving work that is
     already on `main`. (A file *log* — the previous draft's second command — shows only that four
     commits touched the file; it proves neither ownership nor disjointness, so it is demoted to a
     provenance note: `git -C "${PDCA_TARGET:-../wyrd}" log --oneline origin/main -- crates/core/src/multipart.rs`
     → `d986069`, `5eeca16`, `778f1cf`, `a3b2bbe`.)
  4. **Tests, not just touches.**
     `git -C "${PDCA_TARGET:-../wyrd}" ls-tree --name-only origin/main crates/core/tests/ | grep multipart`
     → `multipart_budget_admission.rs` (#715), `multipart_keys.rs` (#691), `multipart_session_records.rs`
     (#716) — one shipped regression file per merged child. The two remaining files named under
     `Test file` are absent from that listing, which is the expected state for children still in flight.
  5. **The one genuinely unbuilt element.**
     `git -C "${PDCA_TARGET:-../wyrd}" show origin/main:crates/core/src/metadata.rs | grep -n 'struct PendingEntry' -A6`
     → `:1556`, holding `lease_expiry_millis` alone. This command proves **absence only** — that is its
     entire job: it is the negative half of the `PendingEntry` row, whose positive half is #772's brief
     (`results/issue_772/brief.md:172`, the two optional ownership fields).
  6. **The withdrawn envelope.**
     `git -C "${PDCA_TARGET:-../wyrd}" show origin/main:crates/core/src/multipart.rs | sed -n '15,21p'`
     → the withdrawal, stated in-tree.
  7. **The module header the merges left stale** (last row of the table):
     `sed -n '5,25p' results/issue_771/patch.diff` → the correcting hunk, owned by #771.
- Repo + branch target: getwyrd/wyrd @ main   (INTEGRATION §2; base verified `a801997`.
  **No PR opens from this bundle** — publish exits 0 with *"nothing to contribute; close the tracker
  item by hand"* (`publish.py:97`, `:173`), so the tracker action is the human's, at sign-off.)
- Scope: record the decomposition of #692 — this brief's mapping table and its per-row checks.
  The two **operational** acts that follow are named separately here, because the earlier draft
  entangled them with the criterion and let it pass under mutually exclusive states:
  **(a) the control-flow marker** `results/issue_692/close-disposition` — **DONE**, restored
  2026-08-18 00:50 and reading `split` (`cat results/issue_692/close-disposition`); it is a
  file-state fact, not a judgement, and is no longer open (see "The mechanical item — settled").
  **(b) the tracker action for #692 itself** — the human's, at sign-off, and **deliberately NOT part
  of the success criterion**: the decomposition is complete-and-correct or it is not, and rows 1–7
  return the same answer whether #692 is closed today or kept open as an umbrella. Neither act can
  make a failing mapping pass, nor a passing one fail. / **out of scope:** any implementation of any
  record family (that is #715/#716,
  merged, and #771/#772, in flight — reopening this bundle to a fix path would rebuild the very
  11-file shape the split exists to abandon, and would duplicate two live bundles);
  **re-deriving the split**, which the stale carry-forward asks for and which events have overtaken;
  re-litigating the withdrawn `encode_record`/`decode_record` envelope, a design decision already
  settled and documented on the target.

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: likely-close
  — this bundle has three `iteration-v*` archives, so the hint alone does **not** fire the close
  fast path: `_close_class` returns `""` for a hinted brief once any archive exists
  (`driver.py:240`), and only an existing `close-disposition` marker wins outright
  (`driver.py:231-238`). The 2026-08-17 `iterate-plan` archived this bundle's marker into
  `iteration-v3/` (it is in `DOWNSTREAM_OF_BRIEF`); **it has since been restored** — 2026-08-18 00:50,
  reading `split`, so `_close_class` returns `"split"` on the marker branch and the builder/reviewer
  leaves are skipped. Verify with `cat results/issue_692/close-disposition`.
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
- [x] The no-code/close reframing is not supported by the supplied tracker record. `brief.md:3-18` says an accepted re-plan replaced #692 with #715/#716/#717 and later #771/#772, but `notes.json` still says “This child lands the record family and its **validating decoders**” and defines `crates/core/tests/multipart_records.rs` as success; its `comments` array is empty. With `split-proposal.md`, child briefs, and the alleged acceptance absent from the allowed inputs, the brief replaces the tracker problem rather than proving its disposition. Make that disposition observable from the review inputs or retain the tracker’s implementation scope.
- [x] “Complete and correct” is not established by the advertised three-command check (`brief.md:36-55`). Both Git commands hard-code absent `../wyrd` instead of `$PDCA_TARGET`; the GraphQL query sees only direct children #715–#717, not #771/#772 or their bundles/briefs; a file log proves touches, not namespace ownership or tests; and the `PendingEntry` grep proves only that one element is absent. Thus the stated failure cases—missing child bundle, missing lineage, overlap, or dropped scope—have no deterministic check. Specify commands/artifacts that resolve and test every mapping assertion.
- [x] The scope contains separate, undecided outcomes. `brief.md:84-90` says the bundle will “close the parent,” while `brief.md:171-192` separately asks the human to restore a harness control marker and then choose either closing #692 or keeping it open. The mapping criterion can pass under mutually exclusive tracker/control-flow states, so neither operational change has a falsifiable completion condition. Choose one disposition and criterion, or split the marker repair and tracker action out explicitly.
- [x] The target itself contradicts the claim that the shipped decomposition is fully correct and needs no source work. `crates/core/src/multipart.rs:5-13` says only the admission value is decoded and session/part records remain for later children, but those types now exist at `crates/core/src/multipart.rs:1711`, `:1862`, `:2073`, and `:2172`. Because `brief.md:84-90` excludes all implementation work, this false module-level description is left unowned; map its correction to a child/follow-up or narrow the completeness claim.
- [x] A load-bearing tracker conflict is dropped without resolution. The tracker says “#682 ... shares TWO files with this child” and “the two must never share a wave” (`notes.json` body lines 86-93), but `brief.md:68-80` declares no conflicts and lists #772’s external conflicts as only #721/#722. The allowed record contains no evidence that #682 merged or that the constraint ceased to apply to the child carrying `PendingEntry`; resolve #682’s state and preserve or transfer the edge if it is still live.
- [x] C4 per-fix red->green: this patch's test red pre-fix, green post-fix unverifiable —                why this slice has no isolable red (the cargo output is above).

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
- Plan advisory: 5 finding(s); brief revised: yes (plan-advisory-*.md)
- (empty is the common case)
