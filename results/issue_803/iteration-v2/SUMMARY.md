# Result — issue 803 / staged-reference-set

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect: staged bytes have no protection class. `ReferenceSet` holds committed placements
  only (`crates/custodian/src/gc.rs:383-413`), built from the `inode:` scan alone (`:478-573`).
  A committed part's fragments (`part:`) and an upload's in-flight owned fragments (`sidx:`,
  #772) are in no protected set, so GC reclaims one as soon as it carries an `orphan:` mark past
  grace (`:272`, `:277-326`). Restore gates on the same predicate
  (`crates/custodian/src/restore.rs:385`) and its pending skip (`:435-438`) no longer sees owned
  entries, so it marks a live upload's fragments stranded (`:440-443`) and the next GC pass
  deletes them.
- Success criterion: the NEW file `crates/custodian/tests/staged_protection.rs` passes over
  in-memory doubles. Records are seeded as raw JSON the base decoders accept (`SessionRecord`
  and `PartRecord` have no writer-side constructor, `crates/core/src/multipart.rs:2127`,
  `:2492`; shapes as `crates/core/tests/multipart_session_records.rs:81-141`), each
  round-tripped through `decode_session_record` / `decode_part_record` / `decode_owned_entry`
  first. Every protection leg also seeds an unprotected control the pass does reclaim or mark.
  Legs:
  **(A) GC protects both staged classes.** An `Open` session with a committed `part:` record
  (fragment `F1`) and an owned `sidx:` entry (`F2`) on D-server doubles, each with an `orphan:`
  mark past grace: after `reconcile_step`, both survive. (Unmarked, GC's conservative arm keeps
  any fragment, `gc.rs:307-310`, so the mark is what makes the leg bite.) Base: both reclaimed.
  **(B) Restore protects them through the same predicate.** `reconcile_after_restore` over the
  same store, unmarked, writes no `orphan:` key for `F1`/`F2` and `stranded_marked` excludes
  them; a GC pass past grace then keeps both. Base: marked, then deleted. (Staged counters are
  #664's.)
  **(C) Source before destination, both handoffs (`0016:782-800`, X67 `:2596`).** A double
  performs a handoff atomically after the first of the two reads involved completes — the
  source range or the destination range/scan, whichever the builder issues first: (i) a part
  commit (one batch deletes the chunk's `sidx:` entry and writes its `part:` record; source
  `sidx:<id>:`, destination `part:<id>:`); (ii) a publication (the committed inode naming the
  chunk is written and its `part:` record removed; source `part:<id>:`, destination the
  `inode:` scan). The fragment is marked past grace and is not reclaimed. Base: reclaimed.
  **(D) Bounded per-session reads (`0016:890`).** With the `scan` cap lowered, more
  sessions-with-parts than a global `scan("part:")` could return: `reconcile_step` succeeds and
  the double records no `scan`/`scan_page` of the bare `part:` or `sidx:` prefix. A guard.
  **(E) What it cannot read or trust fails closed (ADR-0045 decision 3,
  `docs/design/adr/0045-metadata-validation-boundaries.md:55-59`).** (i) A `part:` value that
  will not decode, an `sidx:` key naming no chunk, or an `mpu:` key naming no upload makes the
  set incomplete for GC and restore: GC reclaims nothing and answers `Reconciled::Blocked` (as
  `gc.rs:348-355`); restore marks nothing and names the record in `RestoreReport::unresolvable`.
  (ii) A staged placement of the wrong length, or an undecodable owned value under an `sidx:`
  key that names its chunk, holds that whole chunk in both passes and is named on each audit
  seam, while unrelated fragments are still judged. Base: reclaimed or marked.
  **(F) Seeded DST**, appended to the EXISTING `crates/dst/tests/custodian.rs`
  (`#![cfg(madsim)]`, `:53`; no new DST file): a concurrent part commit then publication at
  seed-chosen instants during GC's staged build never gets the chunk reclaimed; a coverage
  property proves landings between and outside the builder's reads are both reached, as `:2139`
  does.
  **(G) `cargo xtask ci` green.**
- Repo + branch target: getwyrd/wyrd @ main
- Scope: the staged protection class in the shared reference set: the committed `part:`
  records and owned `sidx:` entries of the sessions listed under `mpu:`, read through bounded
  per-session ranges, `sidx:` before `part:` before the `inode:` scan; its own member, disjoint
  from `placed`, honoured by the shared predicate (`gc.rs:424-451`) under its own audit reason.
  Never less than 0016's set; covering every listed session whatever its state is fine (it only
  keeps more). Scrub and drain status keep today's answers — they read `placed` and the
  committed `unresolvable` only — so an unreadable staged record makes the set incomplete for
  GC and restore alone. Restore names each staged record it holds or cannot read on its audit
  seam, and its report fields stay as they are (`restore.rs:105-170`): whether a held staged
  record sets `needs_human()` is #664's, marked `// deferred: #664` at the site. No change to
  `reconcile_step`'s or `reconcile_after_restore`'s signature; no new field on a context struct
  or `RestoreReport`. Docs: one paragraph in `docs/design/architecture/06-runtime-view.md` §6.7
  step 2 — GC never reclaims, and restore never marks, a staged fragment. / out of scope: mark
  shapes, reclaim intent, retirement protection (child-2); drain status, rebalance, restore's
  staged counters and fence (#664); scrub, reconstruction (#663); the fragment-less sweep
  (#800); `desired_state.rs`, `rebalance.rs`, `scrub.rs`, `reconstruction.rs`,
  `crates/core/src/metadata.rs`; 0016 and the ADRs.

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: likely-fix
- Confidence: medium
- Recommendation: (set by Do)

## 3. Correctness (Check — chain)
- C1 Spec: none — brief.md
- C2 Reproduction (red pre-fix): none — (no gate configured)
- C3 Change: none — patch.diff
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): pass — xtask ci: all checks passed
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass — run-verify.sh: PASS — red without the fix, green with it (11 test(s) ran red).
- C4 diff coverage: changed lines executed by the patch's tests: pass — diff coverage 100.0% — 136 of 136 instrumentable changed lines executed (floor 80%); 136 of 418 changed lines were instr
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): pass — 25 mutants tested in 72s: 6 caught, 19 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): pass — review-branch: 0 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_803/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 12.89s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Task under review: protect multipart `part:` and owned `sidx:` fragments from GC and post-restore marking through a shared, bounded, handoff-safe staged reference class.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The brief defines the data-loss defect, falsifiable A–G outcomes, scope boundaries, external tools, and affected-path prior art, leaving an unambiguous implementation contract (`brief.md:10`, `brief.md:18`, `brief.md:92`, `brief.md:112`, `brief.md:125`). |
| C2 Reproduction (red pre-fix) | PASS | An independent stash run kept the new test and compiled 11/11 assertion failures against pre-fix production, matching the frozen red evidence; the test drives existing production entry points (`crates/custodian/tests/staged_protection.rs:13`, `gate-logs/C4-verify.log:15`). |
| C3 Change | PASS | The required safety contract is implemented without public signature or report-field expansion: staged state is disjoint, and the shared predicate covers placed, held-whole, and incomplete staged records (`crates/custodian/src/gc.rs:446`, `crates/custodian/src/gc.rs:468`, `crates/custodian/src/gc.rs:494`, `crates/custodian/src/restore.rs:309`). |
| C4 Verification (red→green) | PASS | The independent rerun changed 11 failing assertions to 11 passes; frozen evidence also shows full CI green and 136/136 instrumentable changed lines covered (`gate-logs/C4-verify.log:102`, `gate-logs/C4-verify.log:14`, `gate-logs/C4-ci.log:3550`, `gate-logs/C4-diff-cov.log:748`). |
| C5 Causal adequacy | PASS | The causal gap is removed at the shared builder and predicate—`sidx:` precedes `part:`, both precede `inode:`, and both destructive consumers consult that protection—with no capability-probe or symptom-guard smell (`crates/custodian/src/gc.rs:548`, `crates/custodian/src/gc.rs:799`, `crates/custodian/src/restore.rs:408`). |
| T1 Structure | PASS | Coverage is placed in the required new custodian integration test and the existing madsim DST file, and the new test root forbids unsafe code (`crates/custodian/tests/staged_protection.rs:52`, `crates/dst/tests/custodian.rs:3085`). |
| T2 Shape | PASS | Tests stay black-box over production entry points, validate raw fixture shapes through production decoders, and include reclaim/mark controls that prevent vacuous protection passes (`crates/custodian/tests/staged_protection.rs:13`, `crates/custodian/tests/staged_protection.rs:524`, `crates/custodian/tests/staged_protection.rs:752`, `crates/custodian/tests/staged_protection.rs:776`). |
| T3 Runtime | PASS | The lowered-cap test proves per-session bounded reads, while an independent madsim rerun passed both concurrent-handoff and landing-coverage properties; frozen CI exercised the full 18-test DST target (`crates/custodian/tests/staged_protection.rs:925`, `crates/dst/tests/custodian.rs:2926`, `gate-logs/C4-ci.log:3504`). |
| T4 Contribution | N/A | Contribution artifacts are absent by design at Check and the publish gate owes their substantive audit; the affected-path merged/open/closed prior-art check is recorded (`gate-logs/T4-contribution.log:10`, `brief.md:125`). |
| T5 Judgment | PASS | The prior fidelity gap is closed: A/B cover `Open`, `Completing`, `Aborting`, and `Completed`, publication transitions through `Completing`, and DST proves a completing-session/part-only landing is reached (`crates/custodian/tests/staged_protection.rs:486`, `crates/custodian/tests/staged_protection.rs:877`, `crates/dst/tests/custodian.rs:2964`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether fleet-wide fail-closed retention for one unreadable staged record, plus protection of every listed session's residue, is operationally acceptable—it prevents data loss but can suspend reclamation until repair or drain (`crates/custodian/src/gc.rs:477`, `crates/custodian/src/gc.rs:789`). |

### Advisory — adversary

# Adversarial review — #803 staged protection class

Overall: I could not break the fix itself. The red→green evidence holds up. One test gap
survives: a one-line change the brief rules out passes every test.

- NEEDS-HUMAN [impl] — **Nothing pins "scrub and drain status keep today's answers" (brief.md:96-99; the patch's own doc at `crates/custodian/src/gc.rs:490-493`).** The code keeps it, but no test checks it. Concrete case: add `unresolvable.extend(staged.unresolvable.clone());` just before `Ok(ReferenceSet {` in `referenced_fragments` (`crates/custodian/src/gc.rs:641`). All 11 `staged_protection` tests still pass, and so does every other `wyrd-custodian` test target (re-run in scratch). Over the E(i) torn-`sidx:` world (one `Open` session plus `sidx:<u>:000001:not-a-chunk`), that mutant turns scrub (driven through `reconcile_step` with a `ScrubContext`) into `Blocked` (`crates/custodian/src/scrub.rs:205`). It also turns `reconciliation_status` for a draining server into `PendingUnresolvable { objects: ["sidx:…:not-a-chunk"] }` (`crates/custodian/src/desired_state.rs:225`), fleet-wide in both cases. As shipped, both answer `Satisfied`. Fix: in `e1_an_sidx_key_naming_no_chunk_blocks_gc_and_restore` (`crates/custodian/tests/staged_protection.rs:1043`), or in `assert_blocks_both_passes` (`:990`), also run scrub and assert it is not `Blocked`. That assertion is permanent: 0016:775-780 says scrub acts only on the committed-`part:` subset, so a torn `sidx:` key must never block it, and #663 won't flip it. The drain-status half would pin an interim answer that #664 may change, so either assert it under a `// deferred: #664` note or leave it out.

- **Evidence re-run, and it holds.** In a scratch copy: with the patch, 11/11 green. With base `gc.rs` + `restore.rs` and the new test kept, 11/11 fail by assertion (none by compile error), which matches `gate-logs/C4-verify.log`. The tests drive the production `reconcile_step` / `reconcile_after_restore`, not a copy of them. Every leg has a control that must be reclaimed or marked, plus an outcome check (`Changed` rather than `Blocked`). So "kept" can't pass on a pass that did nothing, and it can't pass on blanket containment either.

- **Attempted to refute, could not:**
  - Read order. With `inode:` read before the staged build, C2 fails (the base run shows the handoff firing after `"inode:"`). With `part:` read before `sidx:`, C1 fails. The `Handoff` double fires on the first read that covers either key (`staged_protection.rs`, `Meta::after_read`).
  - Iteration-1 carry-forward. A filter that drops any non-`Open` state fails leg A: the `Completing`/`Aborting`/`Completed` sessions each carry a part chunk and an owned chunk. C(ii) and DST property 13 move the session `Completing` → `Completed` inside the publication batch. The DST properties ran green in `gate-logs/C4-ci.log:3489`, `:3499`.
  - `mpu:` prefix collision. The admission counter is `mpuctl` (`crates/core/src/multipart.rs:1126-1132`), outside `mpu:`, so it can't show up as a torn session key and block GC fleet-wide.
  - Scan sizes per session. The `part:` range is ≤ 10,000 records (`multipart.rs:4470`), `sidx:` is ≤ 16 × `MAX_PART_CHUNKS`, and `mpu:` is ≤ `MAX_SESSIONS` = 46. All are far under `SCAN_CAP` = 2^20 (`crates/traits/src/lib.rs:286`).
  - `len: 0` in the planned `ChunkRef` that `add_owned` builds. `fragment_count()` ignores `len` (`crates/core/src/metadata.rs:148-153`), so an owned RS entry still expands to every fragment. A zero-fragment scheme can't slip through, because staged decode rejects unsupported geometry (`multipart.rs:3574-3581`).
  - Merging `staged.placed` into `placed` also passes all 11 tests. But scrub's answer doesn't change under it (probe: an owned fragment that is absent from disk still gives `Satisfied` with 0 repair obligations, either way). Only drain status moves, and drain status is #664's. Not raised.
  - A held staged record not setting `needs_human()` carries an in-code `deferred: #664` (`crates/custodian/src/restore.rs:326-327`). That is settled under the reviewer protocol.

- **On the verdict.** The C5 row's "pass" rests on 6 viable mutants out of 25 (19 unviable, `gate-logs/C5-mutants.log`). cargo-mutants never generates the "fold into the committed set" mutant above, and 100% diff coverage can't see a line that isn't there. So neither gate says anything about the scrub/drain scope boundary, and the T4 review's "0 blocking" missed it. This is the same gap as the first bullet, so it is not a separate item.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] Validation — fitness-to-purpose — Decide whether fleet-wide fail-closed retention for one unreadable staged record, plus protection of every listed session's residue, is operationally acceptable—it prevents data loss but can suspend reclamation until repair or drain (`crates/custodian/src/gc.rs:477`, `crates/custodian/src/gc.rs:789`).
- [ ] **Nothing pins "scrub and drain status keep today's answers" (brief.md:96-99; the patch's own doc at `crates/custodian/src/gc.rs:490-493`).** The code keeps it, but no test checks it. Concrete case: add `unresolvable.extend(staged.unresolvable.clone());` just before `Ok(ReferenceSet {` in `referenced_fragments` (`crates/custodian/src/gc.rs:641`). All 11 `staged_protection` tests still pass, and so does every other `wyrd-custodian` test target (re-run in scratch). Over the E(i) torn-`sidx:` world (one `Open` session plus `sidx:<u>:000001:not-a-chunk`), that mutant turns scrub (driven through `reconcile_step` with a `ScrubContext`) into `Blocked` (`crates/custodian/src/scrub.rs:205`). It also turns `reconciliation_status` for a draining server into `PendingUnresolvable { objects: ["sidx:…:not-a-chunk"] }` (`crates/custodian/src/desired_state.rs:225`), fleet-wide in both cases. As shipped, both answer `Satisfied`. Fix: in `e1_an_sidx_key_naming_no_chunk_blocks_gc_and_restore` (`crates/custodian/tests/staged_protection.rs:1043`), or in `assert_blocks_both_passes` (`:990`), also run scrub and assert it is not `Blocked`. That assertion is permanent: 0016:775-780 says scrub acts only on the committed-`part:` subset, so a torn `sidx:` key must never block it, and #663 won't flip it. The drain-status half would pin an interim answer that #664 may change, so either assert it under a `// deferred: #664` note or leave it out.
- [x] size backstop — this slice is behaving oversized: patch is 116 KB (threshold 100 KB). Recommend answering `iterate-plan` at sign-off and authoring the split in the re-plan (`pdca split`), rather than `iterate-do`: a slice that is too big yields implementation-shaped findings every round, and splitting authors briefs, which is Plan's beat. — Human OK: oversize is not a blocker here; proceeding with `iterate-do` instead.

## 7. Proven / not proven
- Proven by which oracle: gates overall = pass (stub oracles).
- Unproven / needs manual run: anything flagged in §6.

## 8. Ready-to-ship attachments
- patch.diff
- tracker-comment.md     (ALWAYS, every tracker item)
- build-notes.md         (builder rationale — for the human, not the reviewer)

## 9. Check sign-off                     ← human completes Check here
- Disposition confirmed / overridden:
- Outcome: iterated-to-Do
- Iteration delta (if iterating): Required fixes for the rebuild (both are the reason for this iteration, size overage is not a factor — waived by the human): 1. Fitness-to-purpose: fail-closed retention (one unreadable staged record can stall cleanup fleet-wide until fixed or drained) — keep this behavior for now, but the human wants the rebuild to close the second item so this tradeoff is fully covered rather than silently possible to break. 2. Test-fidelity gap (adversary review): no test pins "scrub and drain status keep today's answers" (brief.md:96-99). Add `unresolvable.extend(staged.unresolvable.clone())` at the exact site named (`crates/custodian/src/gc.rs:641`, just before `Ok(ReferenceSet {`) as the concrete mutant this must catch, and add the assertion in `e1_an_sidx_key_naming_no_chunk_blocks_gc_and_restore` (`crates/custodian/tests/staged_protection.rs:1043`) or `assert_blocks_both_passes` (`:990`) confirming scrub is not `Blocked` in the torn-`sidx:` (E(i)) case. Leave the drain-status half out or under `// deferred: #664` per the adversary's note — it pins an interim answer #664 may change.
- By / date: Eduard Ralph / 2026-09-15

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
