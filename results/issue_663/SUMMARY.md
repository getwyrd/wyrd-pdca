# Result — issue 663 / staged-scrub-and-repair

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect: staged redundancy decays untended. **Scrub** walks only committed placements
  (`crates/custodian/src/scrub.rs:88`, `:130-203`), so a committed part's fragment can rot or
  vanish for the hours a session stays open and nothing notices. **Reconstruction** resolves an
  obligation only against committed inodes (`read_committed`,
  `crates/custodian/src/reconstruction.rs:468`). A staged chunk finds no committed site, is assessed
  `Drain` (`:613`), joins `drain_only` (`:218`) and its obligation is deleted (`:333-339`), so the
  part stays a fragment short until it is published, or forever if the client never completes.
- Success criterion: on `main` after both children, four separate outcomes hold, each
  asserted by a named test, and `cargo xtask ci` is green.
  **(1) Found.** One scrub pass queues a repair for a corrupt or missing fragment of a committed
  `part:` record (663.1, NEW `crates/custodian/tests/staged_scrub.rs`).
  **(2) Kept while it cannot be repaired.** When reconstruction cannot rebuild a staged chunk —
  it is named only by an `sidx:` entry, its session is not `Open`, or the re-place loses to a
  session fence, a rewritten `part:` record, an expired write deadline or a drain — the obligation
  is still queued after the pass and nothing is adopted. It is never drained (663.1 legs D–F in
  `crates/custodian/tests/staged_protection.rs`; 663.2 legs B–E).
  **(3) Rebuilt when it can be.** For a committed part in an `Open` session, one reconstruction
  pass rebuilds the fragment on a new D server, repoints the `part:` record's
  `ChunkRef.placement`, and deletes the obligation in that same commit (663.2 leg A, NEW
  `crates/custodian/tests/staged_repair.rs`). This is the tracker's "the queued repair updates
  the part placement" and 0016's "resolved, not drained" (`0016:889`).
  **(4) The race, seeded.** A seeded DST (deterministic simulation test) case appended to
  `crates/dst/tests/custodian.rs` (663.2 leg F) sweeps the session fence across every point of
  the re-place (0016 X29, `:888`): in every interleaving no fragment ends unreferenced and
  unevidenced, and a losing re-place leaves its pre-mark and the obligation queued. It runs under
  `cargo xtask ci` → `run_dst` (`xtask/src/main.rs:1578`, `--cfg madsim`), as `AGENTS.md:189-190`
  requires of a new destructive or concurrent path.
  **Handover between the children:** 663.1's leg D asserts "kept" for a committed part's chunk as
  well as an `sidx:`-only one. For a committed part in an `Open` session that is the interim state
  on 663.1's own base; 663.2 turns it into outcome (3) and must retarget or remove that case of
  leg D. The `sidx:`-only case keeps. A committed part's chunk in an `Open` session found at full
  redundancy is not in outcome (2): after 663.2 its obligation drains as a duplicate finding, as a
  committed chunk's does (`reconstruction.rs:216-218`), so 663.1's leg E must seed a degraded
  chunk or it goes red after 663.2.
- Repo + branch target: getwyrd/wyrd @ main
- Scope: scrub over committed staged fragments; reconstruction's resolution of a staged
  chunk's obligation (keep it while it cannot be repaired, rebuild it when it can) under 0016's
  re-place rules; the time source and write window that re-place needs. The committed repair path
  (`reconstruction.rs:829-955`) keeps its behaviour. / out of scope: the committed path's
  lost-CAS leak (#723); `seg:`-resident repair (#777, #682); drain status, rebalance and restore
  (#808, #809, #810); the upload-side drain fence (#657); the GC sweep of fragment-less marks
  (#800); edits to 0016 or any ADR.

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: likely-fix
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
- [x] The success criterion cannot literally have “both children’s criteria hold on `main`”: 663.1 says a staged obligation “survives a reconstruction pass,” while 663.2 says one reconstruction pass rebuilds it (`brief.md:21-27`). The target’s established successful-repair transaction deletes `repair:<chunk>` while a losing CAS keeps it (`crates/custodian/src/reconstruction.rs:937-953`), and the tracker requires that “the queued repair updates the part placement.” `brief.md:51-53` hints at the missing distinction (“keep it while it cannot be repaired, rebuild it when it can”), but that condition is absent from the criterion. State separate falsifiable outcomes: blocked/conflicting repair remains queued; successful repair repoints the part and drains the obligation.
- [x] The claimed invariant smuggles in a change that the scope explicitly excludes. `brief.md:31-34` promises that **every** fragment a re-place writes is always referenced or pre-marked, while `brief.md:53-54` says the committed repair path keeps its behavior. That path currently writes destination fragments before the metadata commit (`crates/custodian/src/reconstruction.rs:931-935`), creates marks only for displaced old positions (`crates/custodian/src/reconstruction.rs:937-947`), and on CAS conflict returns without marking the newly written destination (`crates/custodian/src/reconstruction.rs:949-953`). Either narrow the invariant to the new staged re-place path or include the committed path and its regression coverage in scope.
- [x] The verification surface omits the mandatory deterministic concurrency proof. The brief names only `crates/custodian/tests/staged_scrub.rs` and `staged_repair.rs` (`brief.md:62-63`), but repository policy requires a new destructive/concurrent path to land with seeded Tier-0 DST coverage (`AGENTS.md:188-190`). The governing design even specifies the relevant seeded interleavings: pre-mark expiry versus GC and write authorization, and destination-write versus Complete/Abort/reap fencing (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:887-888`). Add the seeded DST test file/cases and command to the success criterion; `cargo xtask ci` plus the two named integration files does not identify that proof.
- [x] One load-bearing source for the invariant is unverifiable on the declared target: `brief.md:35` cites `docs/principles.md` §5/§6, but `docs/principles.md` does not exist at target `origin/main` commit `97fc2f9` (it is absent from the target tree). Replace it with a resolvable target-source citation or remove the unsupported C-1 attribution.

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
- By / date: Eduard Ralph / 2026-09-15

## 10. Act candidates (hints for the next Act review)
- Plan advisory: 4 finding(s); brief revised: yes (plan-advisory-*.md)
- (empty is the common case)
