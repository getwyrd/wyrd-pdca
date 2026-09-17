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
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 12.93s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

The staged-reference-set patch prevents custodian GC and post-restore reconciliation from treating live multipart fragments as reclaimable, with only the mandated final fitness sign-off outstanding.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The brief identifies a concrete C-1 data-loss path and gives falsifiable A–G outcomes for GC, restore, handoffs, bounds, corruption, seeded DST, and CI. |
| C2 Reproduction (red pre-fix) | PASS | An independent stash run kept the unchanged new test and produced 0/11 red by assertion; restoring the patch produced 11/11 green (`crates/custodian/tests/staged_protection.rs:621`). |
| C3 Change | PASS | The shared predicate now distinguishes placed, whole-chunk-held, and incomplete staged protection, and restore consults that same gate before marking (`crates/custodian/src/gc.rs:468`, `crates/custodian/src/restore.rs:408`). |
| C4 Verification (red→green) | PASS | Independent runs passed 11/11 focused cases, both 50-seed madsim handoff tests, the CLI verdict test, `typos`, docs lint/render, and fmt; the frozen full gate also ends `xtask ci: all checks passed` (`gate-logs/C4-ci.log:3550`) with 100% instrumentable diff coverage (`gate-logs/C4-diff-cov.log:746`). |
| C5 Causal adequacy | PASS | The change removes the missing-reference cause by reading protection source-before-destination (`sidx:` → `part:` → `inode:`), not by adding a capability probe or symptom guard (`crates/custodian/src/gc.rs:542`, `crates/custodian/src/gc.rs:773`). |
| T1 Structure | PASS | Staged state is a disjoint member of the shared reference set, leaving committed-only consumers structurally able to keep their existing answers (`crates/custodian/src/gc.rs:416`, `crates/custodian/src/gc.rs:446`). |
| T2 Shape | PASS | The focused suite drives production entry points without naming patch-added symbols and pairs each protection assertion with a reclaim/mark control (`crates/custodian/tests/staged_protection.rs:13`, `crates/custodian/tests/staged_protection.rs:45`). |
| T3 Runtime | PASS | Runtime work is bounded by one capped `mpu:` scan plus two capped per-session scans, avoiding an unbounded global `part:` or `sidx:` read (`crates/custodian/src/gc.rs:768`, `crates/custodian/src/gc.rs:789`). |
| T4 Contribution | N/A | Contribution artifacts are absent by design and their substantive audit reruns at publish (`gate-logs/T4-contribution.log:10`); the frozen brief records the affected-path prior-art check across merged, open, closed, and rejected work. |
| T5 Judgment | PASS | The evidence exercises every claimed safety leg, mutation testing reports no survivors, and the frozen deep multi-pass review records zero findings (`gate-logs/C5-mutants.log:13`, `gate-logs/T4-batch-review.log:10`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether fleet-wide fail-closed retention on unreadable staged records and the added per-session metadata reads are acceptable operational tradeoffs—they prevent data loss but can defer reclamation and increase every shared reference-set build (`crates/custodian/src/gc.rs:477`, `crates/custodian/src/gc.rs:789`). |

### Advisory — adversary

# Adversarial review — #803 staged protection class

Verdict: I could not break the fix. One test gap is real: the rule that protects a session
"whatever its state" is not pinned by any test.

## Findings

- NEEDS-HUMAN [impl] — **Nothing tests a session that is not `Open`, so dropping every non-`Open` session goes unnoticed.** `crates/custodian/src/gc.rs:791-805` reads every session listed under `mpu:`, in any state, and that is correct. But every test fixture is `Open`: `crates/custodian/tests/staged_protection.rs:465` (`session_value`) and `crates/dst/tests/custodian.rs:2699` (`STAGED_SESSION`). Concrete case: I added a filter at the top of the `mpu:` loop (`gc.rs:791`) that skips any session whose value lacks `"kind":"Open"`. All 11 tests in `staged_protection.rs` passed, and so did both new DST properties (`gc_staged_build_under_concurrent_handoffs`, `gc_staged_build_reaches_every_landing`, 50 seeds). That filter drops exactly the parts that matter most. The root flip requires `mpu == Completing@E` (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:662`), and a `Completed` session's `part:` records stay until its `retire:records:` drain runs (`0016:573`). So in the real protocol every publication handoff happens under a non-`Open` session. Leg C(ii) (`staged_protection.rs:762`) and the DST writer (`crates/dst/tests/custodian.rs:2785`) both publish from an `Open` session, which the protocol never does. That breaks the rubric's test-fidelity rule: a test model should match production behaviour. Fix: seed `Completing`, `Aborting` and `Completed` sessions in legs A and B (valid shapes are in `crates/core/tests/multipart_session_records.rs:81-141`). In c2 and property 13, move the session to `Completing` before the publication and to `Completed` in the publication batch.

## What I tried that did not break it

- **Red→green, re-run at `$PDCA_TARGET`.** 11 of 11 pass on the patched tree. `gate-logs/C4-verify.log` shows all 11 red on the base, each failing an assertion, none failing to compile. The tests drive the production `reconcile_step` and `reconcile_after_restore` (`staged_protection.rs` `gc_pass` / `restore_pass`), not a copy of their logic.
- **Mutations, run in a scratch copy.** Each was caught:
  - Reading `part:` before `sidx:` (`gc.rs:799-804`): c1 goes red, and so does the DST at `crates/dst/tests/custodian.rs:2819` (the fragment is reclaimed).
  - Building the staged set after the `inode:` scan (`gc.rs:552`): c2 goes red, and so does the DST at `:2819`.
  - Filling an empty staged placement by identity (`gc.rs:742`): e2 (owned placement of the wrong length) goes red.
  - Protecting the whole chunk instead of each (server, fragment) pair (`gc.rs:473`): A and B go red.
  
  The C5 gate's "pass" is thin: only 6 of its 25 mutants compiled. These manual mutations are the stronger evidence.
- **Scan-cap overflow.** Not reachable. `MAX_SESSIONS` is 46 (`crates/core/src/multipart.rs:4624`). Each session's `part:` range holds at most 10,000 records (`:4470`) and its `sidx:` range at most 16 × 158 (`:4479`). The scan cap is 1,048,576 (`crates/traits/src/lib.rs:286`).
- **`mpuctl` matching the `mpu:` prefix.** It doesn't: the fourth byte differs (`multipart.rs:1126-1132`), and core tests pin this.
- **A scheme with zero fragments, which would place nothing and hold nothing.** Rejected at decode: `checked_chunk_scheme` for `part:` (`multipart.rs:2556`) and `checked_staged_scheme` for `sidx:` (`:3574`).
- **Deadlock with teardown, where GC keeps bytes that teardown waits on.** None. Teardown orphan-marks the bytes and deletes the records in the same step (`0016:355`, `:671`), so the records never wait on GC.
- **A handoff between restore's two readings.** Only the first reading (`restore.rs:309`) reads staged records. But it reads source before destination, so a chunk that moves afterwards was already seen in the class it left.
- **Not raised:**
  - A held staged record does not set `needs_human()`, so the CLI still says "complete". This is marked `// deferred: #664` at `restore.rs:326`, and the rubric treats that as settled.
  - Restore's displaced-copy check (`restore.rs:385-436`) only looks at committed placements. So a staged fragment moved off its recorded server would be marked. Nothing moves staged fragments until #663, and the restore fence is #664's.
  - Scrub (`scrub.rs:88`) and drain status (`desired_state.rs:188`) now pay for the staged build and ignore the result. The brief accepted the shared builder.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] Validation — fitness-to-purpose — Decide whether fleet-wide fail-closed retention on unreadable staged records and the added per-session metadata reads are acceptable operational tradeoffs—they prevent data loss but can defer reclamation and increase every shared reference-set build (`crates/custodian/src/gc.rs:477`, `crates/custodian/src/gc.rs:789`).
- [ ] **Nothing tests a session that is not `Open`, so dropping every non-`Open` session goes unnoticed.** `crates/custodian/src/gc.rs:791-805` reads every session listed under `mpu:`, in any state, and that is correct. But every test fixture is `Open`: `crates/custodian/tests/staged_protection.rs:465` (`session_value`) and `crates/dst/tests/custodian.rs:2699` (`STAGED_SESSION`). Concrete case: I added a filter at the top of the `mpu:` loop (`gc.rs:791`) that skips any session whose value lacks `"kind":"Open"`. All 11 tests in `staged_protection.rs` passed, and so did both new DST properties (`gc_staged_build_under_concurrent_handoffs`, `gc_staged_build_reaches_every_landing`, 50 seeds). That filter drops exactly the parts that matter most. The root flip requires `mpu == Completing@E` (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:662`), and a `Completed` session's `part:` records stay until its `retire:records:` drain runs (`0016:573`). So in the real protocol every publication handoff happens under a non-`Open` session. Leg C(ii) (`staged_protection.rs:762`) and the DST writer (`crates/dst/tests/custodian.rs:2785`) both publish from an `Open` session, which the protocol never does. That breaks the rubric's test-fidelity rule: a test model should match production behaviour. Fix: seed `Completing`, `Aborting` and `Completed` sessions in legs A and B (valid shapes are in `crates/core/tests/multipart_session_records.rs:81-141`). In c2 and property 13, move the session to `Completing` before the publication and to `Completed` in the publication batch.
- [ ] size backstop — this slice is behaving oversized: patch is 106 KB (threshold 100 KB). Recommend answering `iterate-plan` at sign-off and authoring the split in the re-plan (`pdca split`), rather than `iterate-do`: a slice that is too big yields implementation-shaped findings every round, and splitting authors briefs, which is Plan's beat.

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
- Iteration delta (if iterating): Size backstop (106 KB vs 100 KB threshold) waived by the human: the overage is minor and does not warrant a re-plan/split. Required fix for the rebuild: close the test-fidelity gap the adversarial reviewer demonstrated — every fixture seeds sessions in the `Open` state, but real publication handoffs never happen from `Open` (they require `Completing`/`Completed`); a one-line filter dropping non-`Open` sessions passed all 11 tests plus both DST properties, proving the gap is real. Seed `Completing`, `Aborting`, and `Completed` sessions in legs A and B (valid shapes in `crates/core/tests/multipart_session_records.rs:81-141`); in leg C(ii) and DST property 13, move the session to `Completing` before the publication batch and to `Completed` within it.
- By / date: Eduard Ralph / 2026-09-15

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
