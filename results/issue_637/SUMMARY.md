# Result — issue 637 / staged-byte-protection

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect: four distinct defects, one per child. They read one shared reference set, but each
  fails on its own and each child brief states its own.
  (1) **The `orphan:` ledger cannot be read at size, and some marks are never visited (#661).**
  GC and restore read the whole ledger with one `scan` (`crates/custodian/src/gc.rs:522-538`,
  `crates/custodian/src/restore.rs:308`), which fails outright past `SCAN_CAP`
  (`crates/traits/src/lib.rs:286`). A mark over a position with no fragment is never visited,
  because GC consumes marks only while iterating `list_fragments()` (`gc.rs:183-219`). This is a
  ledger defect, not a staged-byte one. It is in 637 because the tracker scopes it here ("GC,
  ledger walk") and because one maximum segmented-object retirement would install ~1.78 M marks
  (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:1392-1398`, X90 at `:2619`).
  (2) **Staged bytes are in no protected class, and GC destroys before it records (#662).**
  `ReferenceSet` holds committed placements only (`gc.rs:265-340`), so GC reclaims a marked staged
  fragment, and restore marks a live upload's fragments stranded (its gate is the same predicate,
  `restore.rs:383`). A pending byte retirement protects nothing. In the same reclaim decision sit
  two ledger-protocol defects: GC deletes the fragment (`gc.rs:214`) before it commits the key
  cleanup (`gc.rs:231`), and only the bare-decimal mark value decodes (`gc.rs:526-535`).
  (3) **Staged redundancy is never maintained (#663).** Scrub walks committed placements only
  (`crates/custodian/src/scrub.rs:88`), and reconstruction drops a repair obligation for a chunk
  it finds in no committed map (`crates/custodian/src/reconstruction.rs:613`).
  (4) **Drain status and restore claim more than is true (#664).** A drain reports `Satisfied` for
  a server holding only staged bytes (`crates/custodian/src/desired_state.rs:191-196`), and restore
  fences no resurrected session and records no fence generation (`0016:717-728`).
  Source: proposal 0016 decision 2 (`0016:765-893`) plus the ledger rules decision 2 depends on
  (`0016:1189-1404`).
- Success criterion: two checks. **(1) The split is complete — checked at this parent's
  sign-off.** Every row of the coverage table below names an owner: a child leg, an
  already-merged change, or a named deferral with its reason. Each child leg a row cites exists in
  that child's brief as a red→green assertion on the child's own NEW test file. A row with no
  owner, or a cited leg the child brief does not contain, fails the split.
  **(2) The outcome — checked once the last wave has landed on `main`.**
  `cargo test -p wyrd-custodian --test gc_ledger_walk --test staged_protection --test staged_repair --test staged_drain_restore`
  passes, and `cargo xtask ci` is green; `ci` also runs #663's seeded X29 case under
  `--cfg madsim` through `run_dst` (`xtask/src/main.rs:1567`). What those files pin, one line per
  child: **#661** GC walks the ledger in pages, exactly `B` entries per pass while `B` remain, and
  resumes where the last pass stopped even though each pass builds a fresh `GcContext`, so every
  mark is visited within `⌈P / B⌉ + 1` passes (legs B, C); restore never reads the ledger whole and
  never re-stamps a mark it did not read — an existing mark on any page keeps its bytes (leg F);
  a mark with no fragment is swept only past the late-write deadline, and only on this pass's own
  listing (leg D). **#662** GC and restore protect staged bytes through the shared reference set;
  a pending byte retirement protects its fragments; reclamation intent is durable before bytes
  are destroyed; all three mark value shapes decode. **#663** scrub checks committed-part fragments
  and queues repair; reconstruction repairs a staged chunk under the pre-mark and session-fence
  rules. **#664** drain status counts staged bytes as held; restore fences every resurrected
  session (a `Completing` one with its segment records' deleter in the same batch) and writes the
  fence-generation record. The parent ships no patch. **This is not all of tracker #637:** the
  seeded DST races, bar X29, stay with #665 (coverage table), so the tracker issue stays open
  until #665 lands.
- Repo + branch target: getwyrd/wyrd @ main
- Scope: proposal 0016 decision 2 across the custodian — the staged protection class and
  each consumer's stated answer (GC, restore, scrub, reconstruction, rebalance, drain status) —
  plus the `orphan:` ledger rules it depends on (paged walk, marks with no fragment,
  reclamation intent before destruction, the three value shapes, keyed protection while a byte
  retirement is pending), and restore's session fence. / out of scope: the seeded DST sweeps
  other than X29, and the full-plane observable (#665, after #659); the orphan-identity
  migration gate and its cleanup pass (X92, `0016:1249-1273` — it guards the retirement paths
  #659 turns on); the retire drain (#659); the reaper (#625); the S3 verbs (#508); upload-side
  placement and its drain fence (#657, X59); evacuation of committed segmented objects
  (#653/#722); any edit to 0016 or to an ADR.

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
- [x] The parent has no self-contained, resolvable success gate. Its criterion is “all four children reach COMPLETE,” while its falsifiability and repro defer to child briefs and `split-proposal.md` (`brief.md:23-34`, `brief.md:63-68`); none of those artifacts is in the supplied review inputs, and `dependency-state.json:1-31` resolves only 634/691/715/716/771/772, not #661–#664. A workflow status plus unavailable criteria cannot be checked as a red→green behavior; the parent needs either resolved child records with their exact commands/assertions or an explicit aggregate verification command.
- [x] “A page at a time under a fixed per-pass budget” does not define a safe or falsifiable ledger-walk result (`brief.md:23-25`). Restore currently loads the complete orphan map (`crates/custodian/src/restore.rs:308`) and treats absence from that map as authority to write a fresh mark timestamp (`crates/custodian/src/restore.rs:413-430`); a budgeted partial map would therefore make an existing mark outside the current page look absent and re-stamp it. The tracker itself says the budget “changes what one GC pass may conclude” (`notes.json`, “GC, ledger walk”), but the brief specifies neither cursor/progress/completion semantics nor a regression proving an off-page mark is not overwritten and is eventually visited.
- [x] The re-plan drops a tracker-level correctness commitment without a replacement. The tracker scopes “the seeded DST cases 0016 requires” and accepts “the seeded DST races are red before / green after” (`notes.json`, “Scope” and “Acceptance”), while the brief explicitly declines to materialise DST child #665 and only says X29 moved to #663 and X59 to #657 (`brief.md:48-50`). The target proposal calls Tier-0 DST the correctness authority and specifically names restore-fence X17/X57, GC/adoption X61, reclaim restart X86, fragment-less sweep X87/X88/X96, pagination X90, and pending-retirement protection X97 (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:2876-2906`). The four ordinary test filenames in `brief.md:65-68` do not account for those required interleavings.
- [x] The stated staged-byte defect also carries a separate ledger protocol and cleanup change. The brief itself introduces the orphan-ledger failure as “Separately” (`brief.md:18-22`), then puts paging and fragment-less mark deletion in #661 and reclamation ordering plus three value formats in #662 (`brief.md:24-28`). These are independently observable changes: current GC scans the whole ledger (`crates/custodian/src/gc.rs:522-536`) and deletes fragment bytes before committing ledger cleanup (`crates/custodian/src/gc.rs:213-231`). The split should identify this as a distinct defect/child contract rather than present every child as one staged-reference root cause.
- [x] Prerequisite declarations no longer match the tracker record. The tracker says “Depends on #634” and “#636 — the multipart record seam” (`notes.json`, “Depends on”), but the brief replaces #636 with #691/#715/#716/#771/#772 (`brief.md:41-43`) without stating that these supersede or discharge #636; correspondingly, `dependency-state.json:1-31` contains no resolution for #636. The target does contain the staged record seam (for example `crates/core/src/multipart.rs:1050-1059`), so this may be a split-ID migration, but that relationship must be made explicit or the tracker’s declared prerequisite remains unresolved.

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
- By / date: Eduard Ralph / 2026-09-12

## 10. Act candidates (hints for the next Act review)
- Plan advisory: 5 finding(s); brief revised: yes (plan-advisory-*.md)
- (empty is the common case)
