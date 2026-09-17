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
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass — run-verify.sh: PASS — red without the fix, green with it (14 test(s) ran red).
- C4 diff coverage: changed lines executed by the patch's tests: pass — diff coverage 100.0% — 136 of 136 instrumentable changed lines executed (floor 80%); 136 of 418 changed lines were instr
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): pass — 25 mutants tested in 72s: 6 caught, 19 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 2 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_803/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 12.83s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Review outcome: the staged multipart protection change prevents GC/restore data loss, but it must be rebuilt because its shared reference builder now makes scrub and drain-status fail on unrelated staged-metadata scan errors.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The brief is decision-complete about staged classes, handoff order, bounded reads, fail-closed behavior, DST, and consumer exclusions; the affected production seams are `crates/custodian/src/gc.rs:547` and `crates/custodian/src/restore.rs:309`. |
| C2 Reproduction (red pre-fix) | PASS | With production changes stashed but the new test retained, all 14 tests failed by assertion, including both destructive-pass legs and all three store-fault legs at `crates/custodian/tests/staged_protection.rs:821` and `crates/custodian/tests/staged_protection.rs:1417`. |
| C3 Change | FAIL | The change violates the explicit “scrub and drain status keep today's answers” boundary: the eager staged read at `crates/custodian/src/gc.rs:552` is now mandatory for callers at `crates/custodian/src/scrub.rs:88` and `crates/custodian/src/desired_state.rs:188`. |
| C4 Verification (red→green) | PASS | Independent red→green produced 14 assertion failures before the production fix and 14/14 passes after it; frozen evidence also shows full CI, TiKV feature builds, both DST properties, 100% instrumentable diff coverage, and all 25 mutants caught or unviable (`crates/dst/tests/custodian.rs:2942`). |
| C5 Causal adequacy | NEEDS-HUMAN [impl] | Rebuild must preserve source-before-destination staged reads for GC/restore without imposing them on committed-only consumers; locating the fallible read in their shared builder makes the causal boundary incomplete (`crates/custodian/src/gc.rs:547`, `crates/custodian/src/gc.rs:552`). |
| T1 Structure | FAIL | Consumer-specific staged acquisition is coupled into the shared committed-reference builder, so scrub and drain-status cannot opt out even though `ReferenceSet` documents that they should (`crates/custodian/src/gc.rs:446`, `crates/custodian/src/gc.rs:552`). |
| T2 Shape | PASS | The staged class is disjoint, public reconciliation signatures remain unchanged, and `RestoreReport` gains no field, preserving the required API/report shape (`crates/custodian/src/gc.rs:416`, `crates/custodian/src/gc.rs:451`, `crates/custodian/src/restore.rs:113`). |
| T3 Runtime | FAIL | A transient failure of `mpu:`, `sidx:<id>:`, or `part:<id>:` propagates before scrub can enqueue committed corruption or drain-status can answer, suppressing unrelated maintenance work (`crates/custodian/src/gc.rs:791`, `crates/custodian/src/gc.rs:799`, `crates/custodian/src/gc.rs:802`). |
| T4 Contribution | FAIL | The required batched review found this same blocking consumer-isolation defect; affected-path history found no open competing staged implementation and only unrelated closed #647, while the contribution-artifact subcheck is N/A because it is deliberately deferred to publish (`crates/custodian/src/gc.rs:552`). |
| T5 Judgment | NEEDS-HUMAN [impl] | Rebuild must add store-fault regressions proving scrub still verifies committed fragments and drain-status still answers when staged scans fail; current E(iii) asserts only GC and restore (`crates/custodian/tests/staged_protection.rs:1346`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Final sign-off must confirm the rebuilt behavior is operationally fit for C-1 maintenance: GC/restore may deliberately retain fleet-wide on unreadable staged state, but scrub and drain-status must remain available as the architecture promises (`docs/design/architecture/06-runtime-view.md:78`). |

### Advisory — adversary

# Adversarial review — issue 803 (staged protection class in the shared reference set)

Re-ran the asserted proof against the target source. `cargo test -p wyrd-custodian --test
staged_protection` is **green** (14/14), and the frozen `gate-logs/C4-verify.log:15-123` shows all
14 going **red by assertion** (not by compile error) with production reverted. The red→green
evidence is real, drives the production `reconcile_step` / `reconcile_after_restore`, and I could
not make it pass for the wrong reason. The findings below are about the fix, not the proof.

## Findings

- **NEEDS-HUMAN [impl]** — `crates/custodian/src/restore.rs:339` (and the gate at `:408`): the
  post-restore pass **still marks a live upload's staged fragment**, and a later GC pass deletes
  it — the exact chain `brief.md:14-17` says this slice exists to break. The pass reads the
  committed namespace **twice** (`referenced_fragments` at `:309`, then `committed_chunks` at
  `:337`) and diffs them through `appeared_since` precisely so "an object that committed between
  the two reads cannot have its live fragments marked on the strength of the older one"
  (`:339-342`). The patch adds the staged class to the **first** read only; nothing re-reads
  `mpu:` / `sidx:` / `part:`, so any upload whose intent record lands after
  `referenced_fragments` and whose fragment lands before the fleet walk at `:373-379` is
  unprotected at the gate. Concrete failing case, run against the target source: a session
  admitted with one owned `sidx:` entry the instant the reference build's `inode:` scan returns,
  with the fragment written right after it (the real client order: intent, then `put_fragment`) —
  `reconcile_after_restore` returns `RestoreReport { stranded_marked: 1, ... }` and
  `orphan:3:<chunk>:0` is written. The module doc added at `restore.rs:71-75` ("Nor is a fragment
  a multipart upload has **staged** but not yet published **ever** marked") and the brief's
  invariant ("protection overlaps across handoffs — no gaps, never a partition", `brief.md:76`)
  both claim more than the code delivers. Fix is either a staged half for `appeared_since` or a
  narrowed doc claim plus a `// deferred: #N` marker at `restore.rs:339`.

- **NEEDS-HUMAN [impl]** — `crates/custodian/src/gc.rs:791`, `:799`, `:802` (the three `?` in
  `staged_fragments`) reached from `crates/custodian/src/scrub.rs:88`: a store fault under a
  namespace scrub never reads **aborts the scrub pass**, so a committed fragment that is genuinely
  missing is never found and its repair obligation is never enqueued. This corroborates both T4
  blocking rows (`gate-logs/T4-batch-review.log:10`) with a demonstration rather than an argument:
  over one committed object whose only fragment is absent plus one `Open` session with an owned
  entry, `reconcile_step(.., ScrubContext, ..)` answers `Changed` with `queued_repairs() == [chunk]`
  when healthy, and `Err(Store(StoreFault("sidx:<id>:")))` with **zero** repairs queued once
  `scan("sidx:<id>:")` faults. The specific unwarranted claim is the doc the patch itself adds at
  `gc.rs:215-217`: *"it reads no staged record, so a staged one it cannot read is no hole in its
  reading"* — true for a **damaged record** (correctly kept out of `ReferenceSet::unresolvable`,
  pinned by `staged_protection.rs:1142`), false for an **unreachable read**, which the same commit
  makes fatal to scrub. The same fault also turns `reconciliation_status` into an `Err`
  (`crates/custodian/src/desired_state.rs:188`), against that function's own contract at `:178-180`
  ("One damaged object never turns this query into an `Err` ... blanking the fleet's drain status
  over one record is the outage the containment rule exists to prevent").
  **Test gap that let this through:** the E(iii) legs (`crates/custodian/tests/staged_protection.rs:1417`,
  `:1423`, `:1429`) assert only that GC and restore return `Err`; no leg asserts what scrub or
  drain status answer under a staged store fault, so the "scrub and drain status keep today's
  answers" claim (`brief.md:96-99`) is pinned for torn records and unpinned for unreachable ones.

- **NEEDS-HUMAN [human]** — `crates/custodian/src/gc.rs:790-806` read from
  `crates/custodian/src/desired_state.rs:188` and `crates/custodian/src/scrub.rs:88`: every
  consumer of the shared builder now pays `1 + 2N` serial metadata `scan`s for a staged set two of
  them immediately discard (`reconciliation_status` reads `placed` / `unresolvable` only; scrub the
  same). `N` is the live session count, capped at `MAX_SESSIONS = 46`
  (`crates/core/src/multipart.rs:4619-4624`), so a single drain-status query goes from 1 scan to up
  to 93, and a sweep across an `M`-server fleet — the surface is read per D server — from `M` to
  `93M`. This is the same root cause as the row above (the shared builder doing staged work for
  consumers that did not ask for it); it wants **one** decision, not two. Judgment call because the
  brief's whole framing is "the SHARED reference set every destructive pass reads" (`brief.md:74`),
  so scoping the staged read to its two consumers is a design change, not a bug fix.

## Attempted and could not refute

- **The red→green itself.** All 14 legs fail by assertion on reverted production and pass on it;
  the file names no symbol the slice adds; the doubles sit under the real `reconcile_step` fenced
  control point, not a re-implementation.
- **Bounded per-session reads (leg D).** `scan(MPU_PREFIX)` cannot return the admission singleton
  (`multipart.rs:1126-1132`, `mpuctl` has no `:`), `MAX_SESSIONS = 46` and
  `MAX_PARTS_PER_SESSION = 10_000` both sit far under `SCAN_CAP = 1 << 20`, so no range read here
  can blow the cap at the shipped profile.
- **A non-canonical `mpu:` key silently mis-deriving its ranges.** `UploadId::new` validates rather
  than normalises (`crates/core/src/multipart.rs:844-848`), so a key that is not the canonical spelling lands in
  `staged.unresolvable` and blocks, rather than sending `sidx_range` / `part_range` at a prefix
  that holds nothing.
- **Both build-window handoffs (leg C).** Source-before-destination holds for every interleaving I
  could construct: `sidx:` → `part:` per session, then the `inode:` scan; a publication landing
  before the `mpu:` listing is caught by the `inode:` scan, one landing after it by the part range
  or the inode scan.
- **`add_chunk`'s length rule.** `ChunkRef::fragment_count()` depends on `scheme` alone
  (`crates/core/src/metadata.rs:148`), so the synthetic `len: 0` at `gc.rs:711` cannot skew it;
  the empty-placement case is held rather than identity-filled and is covered at
  `staged_protection.rs:1282`.
- **Ordering inside `protection()`.** `placed` before `malformed` before `staged.placed` before
  `staged.held` before `is_incomplete` (`gc.rs:468-480`) — I could not construct a fragment that
  gets filed under a rule that did not actually hold.
- **Scrub / drain status answering differently over a *damaged* staged record.** They read
  `referenced.unresolvable` (`scrub.rs:205`, `desired_state.rs:225`), which the patch leaves
  committed-only; `staged_protection.rs:1142` pins it.
- **The drain answering `Satisfied` while `staged.placed` names the draining server.** Explicitly
  deferred to #664 in `brief.md:96-99` and at `staged_protection.rs:1167-1170` — settled per the
  rubric's deferral rule, not re-raised.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C5 Causal adequacy — Rebuild must preserve source-before-destination staged reads for GC/restore without imposing them on committed-only consumers; locating the fallible read in their shared builder makes the causal boundary incomplete (`crates/custodian/src/gc.rs:547`, `crates/custodian/src/gc.rs:552`).
- [ ] T5 Judgment — Rebuild must add store-fault regressions proving scrub still verifies committed fragments and drain-status still answers when staged scans fail; current E(iii) asserts only GC and restore (`crates/custodian/tests/staged_protection.rs:1346`).
- [ ] Validation — fitness-to-purpose — Final sign-off must confirm the rebuilt behavior is operationally fit for C-1 maintenance: GC/restore may deliberately retain fleet-wide on unreadable staged state, but scrub and drain-status must remain available as the architecture promises (`docs/design/architecture/06-runtime-view.md:78`).
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 2 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_803/review-b
- [ ] size backstop — this slice is behaving oversized: patch is 128 KB (threshold 100 KB); 3 round(s) already spent (threshold 2). Recommend answering `iterate-plan` at sign-off and authoring the split in the re-plan (`pdca split`), rather than `iterate-do`: a slice that is too big yields implementation-shaped findings every round, and splitting authors briefs, which is Plan's beat.

## 7. Proven / not proven
- Proven by which oracle: gates overall = fail (stub oracles).
- Unproven / needs manual run: anything flagged in §6.

## 8. Ready-to-ship attachments
- patch.diff
- tracker-comment.md     (ALWAYS, every tracker item)
- build-notes.md         (builder rationale — for the human, not the reviewer)

## 9. Check sign-off                     ← human completes Check here
- Disposition confirmed / overridden:
- Outcome: iterated-to-Plan
- Iteration delta (if iterating): Root cause is one design flaw, not implementation slips: staged-record reading (for the new GC/restore protection class) was put into code shared by consumers (scrub, drain-status) that never asked for it and discard the answer. That one placement decision produced three findings: (1) scrub/drain-status now fail on a transient staged-record read fault, though the brief required them to keep today's answers; (2) every consumer now pays for reading records most of them don't need (up to 93 scans/server); (3) restore's own fix is still incomplete for a narrow upload-timing window (restore.rs:339). Patch is 128 KB (100 KB threshold) and this is round 3 (2-round threshold) — re-plan and split so "protect staged fragments in GC/restore" is scoped separately from any shared-builder change that touches scrub/drain-status, and so restore's remaining gap gets its own slice with a regression test.
- By / date: Eduard Ralph / 2026-09-15

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
