# Result — issue 626 / multipart-commit-protocol

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect / goal: The multipart **commit protocol** — what happens *underneath* a
- Success criterion: two legs, both evaluated at Check on the patched tree.
- Repo + branch target: getwyrd/wyrd @ main
- Scope (one logical fix) / out of scope: ONE logical change: REWORK draft proposal 0016 (starting from

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: new-feature
- Confidence: medium
- Recommendation: (set by Do)

## 3. Correctness (Check — chain)
- C1 Spec: none — brief.md
- C2 Reproduction (red pre-fix): none — (no gate configured)
- C3 Change: none — patch.diff
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): pass — xtask ci: all checks passed
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass — run-verify.sh: patch touches no Wyrd crate (docs/CI only) — nothing to verify per-fix; the C4-ci gate covers it.
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): pass — INFO Diff changes no Rust source files

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 6 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue_
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Review of issue #626’s draft multipart commit protocol, including bounded staging/reclamation, restore safety, reaping, and chunk-map segmentation above 10 GiB.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The acceptance decision has a falsifiable scope—seven protocol decisions, F1–F18 dispositions, computed bounds, and exactly two documentation paths—and the proposal identifies the same settlement surface at `docs/design/proposals/draft/0016-multipart-commit-protocol.md:17`. |
| C2 Reproduction (red pre-fix) | NEEDS-HUMAN | Whether iteration 1’s review failures are an adequate red oracle must be accepted—the sandbox’s read-only linked Git index prevented the requested stash/red-leg rerun, while the unpatched baseline’s absence of proposal 0016 alone does not reproduce the substantive protocol defects. |
| C3 Change | PASS | The scope decision is satisfied: the target contains only the new draft and its index edit, the required draft metadata is grounded at `docs/design/proposals/draft/0016-multipart-commit-protocol.md:1`, and the document explicitly makes #625 precede or accompany #508 at `docs/design/proposals/draft/0016-multipart-commit-protocol.md:126`. |
| C4 Verification (red→green) | NEEDS-HUMAN | Whether to accept partial independent verification is owed—the patched spell check, docs lint, and link audit passed, but the whole `cargo xtask ci` rerun ended at `cargo deny check` because its advisory-database lock path was read-only, and the red leg could not be stashed; this is a host caveat rather than a patch defect. |
| C5 Causal adequacy | NEEDS-HUMAN | The human must decide whether per-session part-boundary CAS serialization is the right root-level bound rather than an overly costly symptom guard—the proposal itself leaves that contested trade-off for sign-off at `docs/design/proposals/draft/0016-multipart-commit-protocol.md:1722`, and it determines whether F11a remains guaranteed without unacceptable contention. |
| T1 Structure | PASS | The implementation-planning decision is navigable as seven named decisions plus a complete F1–F18 register, whose settlement contract is stated at `docs/design/proposals/draft/0016-multipart-commit-protocol.md:102` and whose dispositions begin at `docs/design/proposals/draft/0016-multipart-commit-protocol.md:1535`. |
| T2 Shape | PASS | The record-shape decision is explicit enough for implementation review: segmentation is required rather than deferred and its computed flat-map constraint is grounded at `docs/design/proposals/draft/0016-multipart-commit-protocol.md:110`; no capability-probe/runtime-guard smell is introduced by this docs-only patch. |
| T3 Runtime | N/A | No runtime implementation changes in this design-only patch; runtime behavior remains an obligation for #508/#625, with the required seeded DST and classification sweep named at `docs/design/proposals/draft/0016-multipart-commit-protocol.md:1598`. |
| T4 Contribution | NEEDS-HUMAN | Whether the contribution can proceed requires triaging the six blocking findings reported by `T4-batch-review` and confirming affected-path prior art across merged plus closed/rejected work; neither the batch-review output nor a mechanical closed-work oracle is among the supplied artifacts, so the red result cannot be independently reproduced or safely affirmed. |
| T5 Judgment | NEEDS-HUMAN | The architecture board/founding maintainer must decide whether this draft is ready to govern #508/#625—the document expressly reserves ratification as a separate governance act at `docs/design/proposals/draft/0016-multipart-commit-protocol.md:39`, and the unresolved serialization-cost decision can materially affect implementation contention. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Maintainer sign-off must decide whether the proposed safety/capacity trade-offs are operationally fit—at the stated bound, maximum-size parts reduce concurrent-session capacity to about one, which directly affects service usability (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:1575`). |

### Advisory — adversary

# Adversarial review — issue 626 / multipart-commit-protocol (iteration 5)

Verdict context: the gating `T4-batch-review` is currently RED (6 blocking, 0 recorded-rejected
— `check-gates.json`), so the deterministic gate already blocks publish; `C4-ci` green was
re-verified plausible (typos clean on both changed files; template section set matches
`docs/design/templates/proposal.md`; patch touches exactly the two mandated docs paths). The
vacuity of `C4-verify`/`C5-mutants` is honestly recorded, as the brief requires. The findings
below are leg-B refutations under the brief's Refutation standard, grounded on the patched tree.

- NEEDS-HUMAN [impl] — **Segmented single-PUT publication has no evidence or reclamation
  machinery — a crashed one strands `seg:` records forever (outcome (a)), and the execution is
  absent from the register.** The doc mandates uniform segmentation for single PUTs
  (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:1164-1168` "one publication
  path for both"; `:1693-1694` "a large single PUT is published as a segmented object"), yet
  every piece of decision 7's machinery is anchored to a multipart session the single PUT does
  not have: the `seg:` key is `seg:<upload-id>:<epoch>:<i>` and its only stated writer is "the
  Completing session's segment-write phase" (§1 table, `:222`); segment batches are fenced by
  `require(mpu == Completing@E)` (`:393`); crash evidence is `publish_target` while
  `Completing@E` and the reaper's `W_completing` rollback installing
  `retire:records:{seg:<id>:<E>}` (`:1203-1219`); the reaper walks only `scan("mpu:")`
  (`:1080`); and a global `seg:` scan is forbidden (`:1252-1255`). Concrete failing case: a
  gateway publishes a 1 GiB single `PutObject` (> `MAX_MAP_CHUNKS` at default chunks ⇒ must be
  segmented) and dies between segment writes and the root flip — there is no `mpu:` record, no
  fence, no `publish_target`, no rollback arm, no `retire:records:` installer, no reaper
  coverage, and no scan allowed to find the residue; there is not even a defined `<upload-id>`
  to key the segments by. The dangling `seg:` records are stranded metadata with no bounded
  reclamation path — refutation outcome (a) — and X37/X40 (`:1387`, `:1388`) cover only the
  session-anchored variant. The open question at `:1743-1746` mislabels this as "code
  factoring"; the missing protocol half (group-token minting, fence analogue, crash evidence,
  reclaimer) gates #508's single-PUT-segmentation obligation and must be designed or explicitly
  carved out in the doc.

- NEEDS-HUMAN [impl] — **"Reader-transparent … a GET during a DELETE is untorn exactly as
  before" (`0016:1694-1696`) is unwarranted for segmented objects: `seg:` records get no reader
  grace.** Byte reclamation is grace-delayed at the drain's orphan mark (`:735-740`), but the
  drain deletes the *naming records* as soon as their fragments are marked
  (`:1102-1110` — "when all marked: delete the records the payload names"), and segmented
  resolution is non-atomic (root read, then `scan("seg:<id>:<E>:")`, `:1180-1182`). Concrete
  failing case: a GET resolves the root of a 50 GiB segmented object; a concurrent
  `DeleteObject` installs `retire:bytes:{generation}` (X45); the drain marks the fragments
  (bytes retained for hours of grace) and then immediately deletes `seg:<id>:<E>:*`; the GET's
  next segment-record read finds it absent and the stream fails mid-object. Today this tear is
  impossible — the flat map is one atomic value the reader already holds, and only bytes need
  grace. The same race transiently aborts any maintenance consumer caught between its root read
  and its segment-range read. Not an (a)–(d) outcome (availability, not durability), but the
  compatibility claim is falsified and neither decision 7's failure-mode table nor the
  execution register has the row. Doc-level fix: give `seg:`-record deletion the same
  reader-grace delay as byte reclamation (the drain already sequences; delaying the record
  delete is one line of protocol), or specify a resolve-retry rule — and add the register row.

- NEEDS-HUMAN [impl] — **X47's "no moved fragment is left unreferenced-and-unevidenced
  (outcome (a) closed)" (`0016:1397`, echoed `:1316`) is refuted by the production repair
  order.** Reconstruction writes the rebuilt fragment to the destination server *before* the
  binding CAS (`crates/custodian/src/reconstruction.rs:556` `put_fragment`, commit at
  `:593-598`); on `Conflict` the whole batch — including its orphan puts — rolls back, and for
  a superseded generation the re-queued obligation is then *drained*, not retried
  (`reconstruction.rs:188-191`, `Assessment::Drain`). Concrete failing case (X47's own
  scenario): re-place fragment *i* of `seg:<id>:<E>:<j>` onto server T (bytes durably landed on
  T), the supersede advances the inode before the CAS, the repoint conflicts and is dropped
  per X47; the retirement drain orphan-marks only the pre-repoint placement it read, so the
  fragment on T is unreferenced *and* unevidenced — GC's conservative arm keeps it forever
  (`crates/custodian/src/gc.rs:183-187`), the exact "fourth category" invariant (2) forbids.
  The leak class pre-exists on the flat repoint path (out of scope), but the *claim* that this
  diff's X47 row closes outcome (a) for the seg: repoint is what the register asserts and what
  a confirmatory pass would wave through. Doc-level fix: pre-evidence the destination position
  (an orphan pre-mark the winning CAS deletes), or dispose the residue honestly as a bounded
  cost with a named reclaimer and the register row corrected.

- NEEDS-HUMAN — **The proposal's own ⚑ sign-off question must be adjudicated, not waved
  through** (`0016:1711-1728`): the enforced F11a bound puts a `sinf:` CAS in *every* part
  commit batch (`:391`, `:400`) and a session read-precondition on every per-chunk intent
  (`:390`), which bends D-C's "part commits stay counter-free" from literal to
  in-spirit-only. The doc flags it correctly per the brief (a direction it cannot honour
  literally is a flagged question, never a silent alternative); the human must rule on the
  serialization/read cost — or bless the in-spirit reading — at sign-off. Routing, not a
  refutation.

## Attempted and could not refute

The fence/epoch machine and the O(1) session-precondition publication proof; the
empty-`sidx:`-gated, exact-bytes-preconditioned terminal delete (exactly-once `mpuctl:count`
decrement under gateway/reaper races, X42); per-attempt epoch-scoped `seg:` keys against the
rollback→re-Complete-while-obligation-pending trace (X40); the byte-budgeted batch inventory
(no row exceeds `E_tx/2`; the iteration-2 fixed-count defect is genuinely closed, `0016:377-401`);
the restore fence incl. the pre-fence serve window (X17/X17b); the late-fragment cover via
full-`staged`-placement orphan marks plus the renewal refusal (X49, verified against
`crates/core/src/write.rs:474-500`); the fenced-intent freeze of the `sidx:` range (X43); the
session-record-first clock guard (X26/X46); `PendingEntry` serialization identity for both new
optional fields (verified `crates/core/src/metadata.rs:344-350` and the re-encode CAS paths);
the derived `MAX_SESSIONS = ⌊W_ref/U_ref⌋` distribution-independence and the in-range
(`MAX_PART_CHUNKS ≤ 381`) arithmetic; and the record-prefix disjointness (`mpu:` is not a
prefix of `mpuctl:`; no `scan` returns a neighbour). Iteration-4's six carry-forward items are
each addressed in the reworked text (the stale ≈52×SCAN_CAP passages are gone; the `scan_page`
seam is named in "What the implementing slices change").

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C2 Reproduction (red pre-fix) — Whether iteration 1’s review failures are an adequate red oracle must be accepted—the sandbox’s read-only linked Git index prevented the requested stash/red-leg rerun, while the unpatched baseline’s absence of proposal 0016 alone does not reproduce the substantive protocol defects.
- [ ] C4 Verification (red→green) — Whether to accept partial independent verification is owed—the patched spell check, docs lint, and link audit passed, but the whole `cargo xtask ci` rerun ended at `cargo deny check` because its advisory-database lock path was read-only, and the red leg could not be stashed; this is a host caveat rather than a patch defect.
- [ ] C5 Causal adequacy — The human must decide whether per-session part-boundary CAS serialization is the right root-level bound rather than an overly costly symptom guard—the proposal itself leaves that contested trade-off for sign-off at `docs/design/proposals/draft/0016-multipart-commit-protocol.md:1722`, and it determines whether F11a remains guaranteed without unacceptable contention.
- [ ] T4 Contribution — Whether the contribution can proceed requires triaging the six blocking findings reported by `T4-batch-review` and confirming affected-path prior art across merged plus closed/rejected work; neither the batch-review output nor a mechanical closed-work oracle is among the supplied artifacts, so the red result cannot be independently reproduced or safely affirmed.
- [ ] T5 Judgment — The architecture board/founding maintainer must decide whether this draft is ready to govern #508/#625—the document expressly reserves ratification as a separate governance act at `docs/design/proposals/draft/0016-multipart-commit-protocol.md:39`, and the unresolved serialization-cost decision can materially affect implementation contention.
- [ ] Validation — fitness-to-purpose — Maintainer sign-off must decide whether the proposed safety/capacity trade-offs are operationally fit—at the stated bound, maximum-size parts reduce concurrent-session capacity to about one, which directly affects service usability (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:1575`).
- [ ] **Segmented single-PUT publication has no evidence or reclamation
- [ ] **"Reader-transparent … a GET during a DELETE is untorn exactly as
- [ ] **X47's "no moved fragment is left unreferenced-and-unevidenced
- [ ] **The proposal's own ⚑ sign-off question must be adjudicated, not waved
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 6 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue_

## 7. Proven / not proven
- Proven by which oracle: gates overall = fail (stub oracles).
- Unproven / needs manual run: anything flagged in §6.

## 8. Ready-to-ship attachments
- patch.diff
- tracker-comment.md     (ALWAYS, every tracker item)
- build-notes.md         (builder rationale — for the human, not the reviewer)

## 9. Check sign-off                     ← human completes Check here
- Disposition confirmed / overridden:
- Outcome: iterated-to-Do
- Iteration delta (if iterating): Rejected at sign-off: gating T4-batch-review red (6 blocking, 0 recorded-rejected) plus three adversary refutations. Rework the document, do not restart — the fence/epoch machine, O(1) publication proof, exactly-once decrement, byte-budgeted batches, and restore fence all survived refutation and must be preserved. Fix (or record-reject in review-rejected.md) the six T4 findings: - 0016:393 — shared `sinf:` CAS makes concurrent part commits conflict; compensation path discards a valid upload on a counter-only collision. - 0016:522 — segment repoint vs supersede race: repoint doesn't touch the inode, supersede doesn't precondition the segment; moved fragment stranded. - 0016:844 — renewal refusal doesn't cancel an already-authorized fragment write; can land after orphan grace and stay unevidenced. - 0016:1164 — single-PUT segmentation underspecified: no upload id / session / Completing epoch to anchor decision 7's machinery. - 0016:925 (x2) — named regression test requires `sinf`→0, contradicting the protocol's own rule (crashed slots stay counted; `sinf` deleted outright). Address the three adversary refutations (two corroborate T4 findings): - Segmented single-PUT has no crash-evidence/reclamation half — design it (group token, fence analogue, crash evidence, reclaimer) or explicitly carve it out of the doc; add register coverage. - Reader-transparency claim falsified for segmented objects: `seg:` records deleted without reader grace tears a concurrent GET — grace-delay the record delete or specify a resolve-retry rule; add the register row. - X47 over-claims "outcome (a) closed" for the seg repoint: pre-evidence the destination position (orphan pre-mark deleted by the winning CAS) or honestly dispose the residue as bounded cost with a named reclaimer; correct the register row. The ⚑ serialization-cost question (0016:1722) remains unruled — keep it flagged as a sign-off question; it is the human's to adjudicate at the next sign-off, not the builder's to resolve silently.
- By / date: Eduard Ralph / 2026-07-23

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
