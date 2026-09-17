# Result — issue 803 / staged-protection-gc-restore

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
  `:2492`; shapes as the helpers in `crates/core/tests/multipart_session_records.rs:81-141`,
  with the `Completed` state spelled as at `:298-304`), each round-tripped through
  `decode_session_record` / `decode_part_record` / `decode_owned_entry` first. Every protection
  leg also seeds an unprotected control the pass does reclaim or mark. Legs:
  **(A) GC protects both staged classes, in every session state.** For each of an `Open`, a
  `Completing`, an `Aborting` and a `Completed` session: a committed `part:` record (fragment
  `F1`) and an owned `sidx:` entry (`F2`) on D-server doubles, each with an `orphan:` mark past
  grace. After `reconcile_step` with a `GcContext`, every one survives. (Unmarked, GC's
  conservative arm keeps any fragment, `gc.rs:307-310`, so the mark is what makes the leg bite.)
  Base: all reclaimed.
  **(B) Restore protects them through the same rule.** `reconcile_after_restore` over the same
  store, unmarked, writes no `orphan:` key for any staged fragment and `stranded_marked` counts
  the control alone; a GC pass past grace then keeps every staged fragment. Base: marked, then
  deleted. (Staged counters are #664's.)
  **(C) Source before destination, both handoffs (`0016:782-800`, X67 `:2596`).** A double
  acts right after the first of the two reads involved completes — the source range or the
  destination range/scan, whichever GC issues first: (i) a part commit, ONE atomic batch that
  deletes the chunk's `sidx:` entry and writes its `part:` record (`0016:782-784`; source
  `sidx:<id>:`, destination `part:<id>:`); (ii) a publication, which is TWO batches, not one
  (`0016:793-800`, `:941-944`, `:964-966`): first the root flip — the session goes to
  `Completing` before it, and the flip batch writes the committed inode naming the chunk and
  moves the session to `Completed`, leaving the `part:` record in place; then, as a separate later
  batch, the retirement drain deletes that `part:` record (source `part:<id>:`, destination the
  `inode:` scan). Leg C(ii) runs two schedules: flip AND drain both between the two reads (the
  only one a destination-first build sees in neither class), and flip between the reads with the
  drain after the second. The double omits the `retire:records:` obligation key itself: no pass
  in this slice reads `retire:` (#804's), and the drain batch is what deletes the record. In every
  schedule the fragment is marked past grace and is not reclaimed. Base: reclaimed.
  **(D) Bounded per-session reads (`0016:890`).** With the `scan` cap lowered, more
  sessions-with-parts than a global `scan("part:")` could return: `reconcile_step` (GC) and
  `reconcile_after_restore` both succeed, and the double records no `scan`/`scan_page` of the
  bare `part:` or `sidx:` prefix. A guard.
  **(E) What GC and restore cannot read or trust fails closed (ADR-0045 decision 3,
  `docs/design/adr/0045-metadata-validation-boundaries.md:55-59`).** (i) A `part:` value that
  will not decode; a key inside a listed session's `part:<id>:` range that `parse_part_key`
  rejects, seeded with a value `decode_part_record` accepts (e.g. an unpadded part number — key
  and value are validated separately, `crates/core/src/multipart.rs:1279`, `:2578`); an `sidx:`
  key naming no chunk; or an `mpu:` key naming no upload (one test each): GC reclaims
  nothing and answers `Reconciled::Blocked` (as `gc.rs:348-355`); restore marks nothing and names
  the record in `RestoreReport::unresolvable`. (ii) A staged placement of the wrong length, or an
  undecodable owned value under an `sidx:` key that names its chunk: that whole chunk is held in
  both passes and the record is named on each pass's audit seam, while unrelated fragments are
  still judged. (iii) A store fault: the metadata double fails the read of exactly one of `mpu:`,
  a session's `sidx:<id>:`, a session's `part:<id>:` (one test each). GC and restore both return
  `Err` whose text names the failed read, every staged fragment is still on disk and unmarked, and
  the double's read log shows the faulted read was issued. Healed, the same store runs a clean GC
  pass that still keeps them (the control). Base: reclaimed or marked; base restore returns `Ok`.
  **(F) Scrub and drain status do not see upload records.** One store holds a committed object
  whose only fragment is missing from its D server, a committed object with fragments on server
  `S` (draining, via `set_lifecycle(.., DServerLifecycle::Draining)`), and a listed session with a `part:` record and an owned
  `sidx:` entry whose fragments sit on servers other than `S`. Compared with the same store minus
  the upload records, `reconcile_step` with a `ScrubContext` gives the same `Reconciled` and
  leaves the same `repair:` key for the missing chunk (`wyrd_core::repair::repair_key`), and
  `reconciliation_status(S)` gives the same answer. That holds with the upload records healthy,
  with each E(i) damaged record in place (neither call answers `Blocked` or
  `PendingUnresolvable` on its account), and with each E(iii) store fault armed (neither call
  returns `Err`). The double logs no read under `mpu:`, `sidx:` or `part:` from either call. A
  guard: green on base, red against the rejected design where all four consumers share the
  staged read. Mark the leg `// deferred: #663, #664` — those slices add upload records to scrub
  and drain status and own changing it.
  **(G) Seeded DST**, appended to the EXISTING `crates/dst/tests/custodian.rs`
  (`#![cfg(madsim)]`, `:53`; no new DST file): a concurrent part commit, then the publication
  flip (session to `Completing` before, `Completed` within the batch that writes the inode), then
  the retirement drain's deletion of the `part:` record as its own batch — each at its own
  seed-chosen instant during GC's reads — never gets the chunk reclaimed; a coverage property
  proves landings between and outside those reads are both reached, as `prop_restore_two_readings_cover_the_divergence_window` (`:2139-2173`)
  does. Registered in the campaign as its neighbours are.
  **(H) `cargo xtask ci` green** (it runs the madsim DST suite and leg I).
  **(I) The operator verdict names staged records as staged.** A new `#[test]` in
  `crates/server/src/cli.rs`'s own test module, beside `:2978-3011` (`restore_verdict` is private
  to the server lib, so no integration test can reach it). A `RestoreReport` whose `unresolvable`
  is `["inode:7", "mpu:<id>", "part:<id>:000001", "sidx:<id>:000001:9"]` (keys as restore names
  them, `gc::object_name`, `gc.rs:588`): `restore_verdict(&report).needs_human` is `true`; the
  printed lines contain `INCOMPLETE`, `4 record(s) UNREADABLE`, `4 record(s) could not be READ`,
  each of the four names, `staged multipart record`, `action=unresolvable-chunk-map` and
  `action=unresolvable-staged-record` (the audit action restore's staged arm emits — use this
  exact string in `restore.rs` too); they contain neither `committed object(s) UNREADABLE` nor
  `committed object(s) could not be READ`. A second report holding only `part:<id>:000001`: the
  same, with `1 record(s)`. The existing test's total-count phrase (`:3004-3010`) moves to
  `record(s) could not be READ`; its other asserts stay. Base: red by assertion — base prints
  `4 committed object(s) could not be READ` for all four names (`cli.rs:1331-1341`).
- Repo + branch target: getwyrd/wyrd @ main
- Scope: the staged protection class, for the two passes that delete or mark: GC's reclaim
  and the post-restore mark gate. It covers the committed `part:` records and owned `sidx:`
  entries of every session listed under `mpu:`, whatever its state (0016 counts fewer; covering
  more only keeps more), read through each session's own bounded ranges and never a global
  `part:` or `sidx:` scan; within each pass's reading, `sidx:` before `part:` before the `inode:`
  scan. The class is disjoint from committed placements and has its own audit reasons. A staged
  record that cannot be read makes the set incomplete for GC and restore (E(i)); one that reads
  but cannot be trusted holds its chunk (E(ii)); a store fault fails the pass (E(iii)). The human
  accepted at sign-off that one unreadable upload record stalls GC and restore fleet-wide until it
  is repaired — keep that. **Scrub and drain status stay as on `main`:** they read no upload
  record, so an upload record's damage, a fault reading one, or the cost of reading them cannot
  reach their answers (F); the committed reference build they share (`gc.rs:478-573`) keeps its
  behaviour. **Restore** names each staged record it cannot read in `RestoreReport::unresolvable`
  (the existing field, so the report is not clean), and each record it holds as untrusted on its
  audit seam; whether a held record sets `needs_human()` is #664's, marked `// deferred: #664` at
  the site. Restore's docs claim only what the code does: its protection covers upload records
  already durable when the pass read them, and a write or upload that starts mid-pass is #805's,
  marked `// deferred: #805` where the pass reads them. The post-restore command's summary line
  and NEEDS-HUMAN paragraph (`crates/server/src/cli.rs:1263`, `:1331`) name staged records beside
  committed objects, with the text leg I pins; the runbook's UNREADABLE entry
  (`docs/design/architecture/m4-first-deployment-blueprint.md:609`) says the same. No change to the signatures of `reconcile_step`,
  `reconcile_after_restore` or `reconciliation_status`; no new field on a context struct or
  `RestoreReport`. Docs: one paragraph in `docs/design/architecture/06-runtime-view.md` §6.7 step
  2 (`:74`) — GC never reclaims, and restore never marks, a staged fragment; scrub and drain status
  read committed references only. / out of scope: mark shapes, reclaim intent, retirement
  protection (#804); drain status and rebalance reading upload records, restore's staged counters
  and fence (#664); scrub and reconstruction reading them (#663); restore's mid-pass window for
  writes and uploads (#805); the fragment-less sweep (#800); any edit to `desired_state.rs`,
  `rebalance.rs`, `scrub.rs`, `reconstruction.rs`, `crates/core/src/metadata.rs`,
  `crates/core/src/multipart.rs`; 0016 and the ADRs.

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: likely-fix
- Confidence: medium
- Recommendation: (set by Do)

## 3. Correctness (Check — chain)
- C1 Spec: none — brief.md
- C2 Reproduction (red pre-fix): none — (no gate configured)
- C3 Change: none — patch.diff
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): pass — xtask ci: all checks passed
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass — run-verify.sh: PASS — red without the fix, green with it (23 test(s) ran red).
- C4 diff coverage: changed lines executed by the patch's tests: pass — diff coverage 98.8% — 249 of 252 instrumentable changed lines executed (floor 80%); 252 of 628 changed lines were instru
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): pass — 36 mutants tested in 84s: 9 caught, 27 unviable

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

Task under review: add a staged multipart protection class so GC never reclaims, and post-restore never marks, live `part:`/owned `sidx:` fragments while scrub and drain status retain their existing boundary; the technical judgment cells pass, with production fitness reserved for human sign-off.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The brief makes the data-loss invariant, two-pass scope, failure semantics, controls, handoff schedules, exclusions, dependencies, and falsifiable outcomes explicit; the decision boundary is reviewable without builder rationale (`brief.md:22`, `brief.md:126`, `brief.md:160`). |
| C2 Reproduction (red pre-fix) | PASS | An independent production-hunk stash run compiled the unchanged test and failed 21/23 cases by assertion while the two intended guards stayed green, matching the frozen red leg (`gate-logs/C4-verify.log:15`, `gate-logs/C4-verify.log:174`). |
| C3 Change | PASS | The change addresses the scoped destructive consumers at their authorization points—GC reads staged protection before committed references and restore does so before either committed reading—while the scrub/drain isolation guard confirms the rejected shared-builder design did not return (`crates/custodian/src/gc.rs:249`, `crates/custodian/src/restore.rs:331`, `crates/custodian/tests/staged_protection.rs:1898`). |
| C4 Verification (red→green) | PASS | After restoring the patch, the independent focused suite passed 23/23; frozen evidence also shows full CI green, 98.8% instrumentable diff coverage, TiKV feature compilation, and the required `typos`/docs-renderer dependencies exercised (`gate-logs/C4-verify.log:10`, `gate-logs/C4-ci.log:11`, `gate-logs/C4-ci.log:16`, `gate-logs/C4-diff-cov.log:761`, `gate-logs/host-tikv.log:207`). |
| C5 Causal adequacy | PASS | The patch removes the missing-protection cause rather than probing around it: source ranges precede destinations, staged protection gates deletion/marking, and the coverage property proves both in-window and out-of-window handoffs are reached (`crates/custodian/src/gc.rs:783`, `crates/custodian/src/gc.rs:331`, `crates/custodian/src/restore.rs:435`, `crates/dst/tests/custodian.rs:3002`). |
| T1 Structure | PASS | Test ownership matches the required architecture: a new custodian integration suite, additions to the existing madsim campaign, and a co-located test for the private CLI verdict, with the new test crate forbidding unsafe code (`crates/custodian/tests/staged_protection.rs:1`, `crates/custodian/tests/staged_protection.rs:36`, `crates/dst/tests/custodian.rs:2562`, `crates/server/src/cli.rs:3031`). |
| T2 Shape | PASS | Fixtures round-trip raw records through production decoders and every protection assertion has a reclaim/mark control, while pagination, malformed-input, store-fault, and boundary guards test properties rather than counts alone (`crates/custodian/tests/staged_protection.rs:11`, `crates/custodian/tests/staged_protection.rs:520`, `crates/custodian/tests/staged_protection.rs:1351`, `crates/custodian/tests/staged_protection.rs:1539`). |
| T3 Runtime | PASS | Tests drive the production `reconcile_step`, `reconcile_after_restore`, and operator-verdict paths, and the independently rerun madsim properties exercised concurrent two-batch publication handoffs (`crates/custodian/tests/staged_protection.rs:11`, `crates/custodian/tests/staged_protection.rs:1183`, `crates/dst/tests/custodian.rs:2995`, `crates/server/src/cli.rs:3032`). |
| T4 Contribution | N/A | Contribution artifacts are absent by design and their substantive audit reruns at publish; meanwhile the affected-path prior-art check covers merged/open/closed and rejected work, and the frozen multi-pass review found no blockers (`gate-logs/T4-contribution.log:10`, `brief.md:210`, `gate-logs/T4-batch-review.log:10`). |
| T5 Judgment | PASS | The suite distinguishes safety from no-op behavior with positive controls, exercises the previously missed pagination/read-order/audit cases, and the mutation gate reports no surviving viable mutant (`crates/custodian/tests/staged_protection.rs:17`, `crates/custodian/tests/staged_protection.rs:930`, `crates/custodian/tests/staged_protection.rs:1245`, `gate-logs/C5-mutants.log:13`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Human must decide whether the in-memory, simulated-TiKV, operator-path, and compile evidence is sufficient to accept this destructive GC/restore lifecycle change for production—automated gates establish implementation behavior but cannot authorize its operational fitness (`crates/custodian/src/gc.rs:322`, `crates/custodian/src/restore.rs:393`, `gate-logs/C4-ci.log:3563`). |

### Advisory — adversary

# Adversarial review — issue #803 (staged protection class for GC and restore)

Verdict: I could not break the production fix. I found one test gap the builder can close.

## What I re-ran (in a scratch copy of `$PDCA_TARGET`)

- **Green:** `cargo test -p wyrd-custodian --test staged_protection` → 23 passed.
- **Red evidence:** `gate-logs/C4-verify.log:10-44` shows 21 tests failing on base, each at an assertion or `expect_err` line in `crates/custodian/tests/staged_protection.rs`. D and F are guards and pass on base, as the brief says. The gate's "23 ran red" is the gate's own wording, which the iteration-5 sign-off already cleared.
- **Leg G (DST, the madsim deterministic simulation tests) has real teeth.** No gate shows G red, so I built `wyrd-dst` under `--cfg madsim` with `MADSIM_TEST_NUM=50` against two hand-made mutants of `crates/custodian/src/gc.rs`:
  - GC reads the committed `inode:` set before the staged class (swapping `gc.rs:258` and `:273`): both `gc_staged_handoffs_*` properties fail. The trace is `Read("inode:"), Landed(PartCommit), Read("mpu:"), Landed(Flip), Landed(Drain), … Read("part:…")`, and the chunk is reclaimed.
  - The `part:` range is read before the `sidx:` range (swapping the two `walk_staged_range` calls at `gc.rs:828-835`): both properties fail. The part commit lands between the two reads.

## Refutation attempts that failed (no finding)

- **Read order across all three handoffs** (`gc.rs:258`/`:273`, `restore.rs:340`/`:350`/`:360`). I traced the part commit, the flip and the drain, landing together or one at a time within one pass. Every schedule leaves the chunk in at least one reading. For restore, the second `inode:` read (the `appeared` set, `restore.rs:366`) adds a second safety margin.
- **Paging** (`gc.rs:809-861`). `checked_page` (`gc.rs:1174-1205`) refuses an empty page that still has a cursor, and a page that does not move forward. So a page cannot silently cut the walk short. With a scan cap of 2, the listing and per-upload paging legs fail on base.
- **Other keys under `mpu:`.** `MPUCTL_KEY` is disjoint (`crates/core/src/multipart.rs:1126-1132`), and nothing else in the tree writes under `mpu:`. So a healthy store cannot be marked `Blocked` by a key that is not a session.
- **Degenerate schemes** such as RS k=0 are refused at decode (`crates/core/src/multipart.rs:2359-2370` called at `:2556`, and `:3574-3581`), so `place()` never sees zero fragments.
- **A GC fault stopping scrub in a combined step** (`reconciliation.rs:136-149`). Production runs GC in its own `reconcile_pass` (`crates/server/src/custodian.rs:609-624`), so a staged-read fault cannot suppress scrub or reconstruction there.
- **Response size of a 512-record page of large `part:` values.** The TiKV `scan_page` fetches in round-trip-sized pieces (`crates/metadata-tikv/src/lib.rs:1313-1320`). No metadata backend in the tree has a per-response limit this would cross.

## Findings

- NEEDS-HUMAN [impl] — **No positive test pins how a healthy staged placement expands, and two concrete mutants of `StagedSet::place` (`crates/custodian/src/gc.rs:754-773`) pass all 23 tests.** I ran both:
  - **M1:** `index` replaced by `0` in the `FragmentId` at `gc.rs:760-763`. Every staged fragment is then recorded as index 0. For a real Reed-Solomon staged chunk (the shape the gateway writes), fragments 1..k+m-1 are unprotected, and GC deletes them once they carry a mark past grace. That is the data loss this slice exists to stop. Result: 23 passed.
  - **M2:** `if false && …` at `gc.rs:756`. Every healthy staged chunk is then held whole instead of placed. GC and restore would log a false `untrusted-staged-record` operator signal for every healthy upload on every pass. A stray copy on a server outside the placement would never be reclaimed while the record lives. Result: 23 passed.

  Cause: every positive fixture uses a one-fragment `EcScheme::None` placement (`staged_protection.rs:610`, `:625`, `:653`, `:656`, `:894`, `:909`, `:1051`, `:1063`, `:1113`, and the DST's `crates/dst/tests/custodian.rs:2721-2725`). The only RS(2,1) fixtures (`:1617`, `:1630`, `:1643`) have the wrong length, so they take the `held` path. C5-mutants (`gate-logs/C5-mutants.log:13`: 9 caught, 27 unviable, 0 missed) generates neither mutant.

  Fix, without naming any new symbol (the brief allows this): in legs A and B, add one healthy upload whose `part:` and `sidx:` chunks are `ReedSolomon { k: 2, m: 1 }` with a full, non-identity placement such as `[2, 0, 1]`. Put every fragment on its placed server, marked past grace. Assert all three survive GC and none is marked by restore; that catches M1. Also put one extra copy of fragment 0 on server 3, which the placement does not name, and assert GC reclaims it and restore marks it; that catches M2.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] Validation — fitness-to-purpose — Human must decide whether the in-memory, simulated-TiKV, operator-path, and compile evidence is sufficient to accept this destructive GC/restore lifecycle change for production—automated gates establish implementation behavior but cannot authorize its operational fitness (`crates/custodian/src/gc.rs:322`, `crates/custodian/src/restore.rs:393`, `gate-logs/C4-ci.log:3563`).
- [ ] **No positive test pins how a healthy staged placement expands, and two concrete mutants of `StagedSet::place` (`crates/custodian/src/gc.rs:754-773`) pass all 23 tests.** I ran both:
- [ ] The re-plan is not supported by the supplied tracker record. `notes.json` still titles #803 “staged protection class in the shared reference set” and its Scope says “the staged protection class in the shared reference set”; it has no comments recording the claimed 2026-09-16 re-plan or sign-off. The brief instead says “Do not re-attempt the shared-builder placement” (`brief.md:189-206`) and adds separate GC/restore reads plus CLI/runbook work (`brief.md:130-159`). Revise the brief to match the tracker-authorized change, or explicitly surface this authorization/scope conflict for adjudication rather than presenting the replacement design as recorded fact.
- [ ] Success leg C(ii) tests the wrong publication handoff. It says one atomic batch writes the committed inode and removes the `part:` record (`brief.md:36-43`), but the target contract says the root flip moves protection to the inode and installs `retire:records:{parts}`, “whose drain then deletes those part records” (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:793-800`); the flip atomically commits the inode and `Completed` session (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:941-965`), while the record-mode obligation is explicitly the later deleter (`crates/core/src/multipart.rs:3144-3150`, `crates/core/src/multipart.rs:3424-3432`). A double that collapses flip and drain into a nonexistent transaction does not exercise the real three-event window; revise C and G to schedule publication and retirement-drain deletion separately.
- [ ] The requested operator-verdict change has no red success check. Scope requires the NEEDS-HUMAN paragraph to name staged records beside committed objects (`brief.md:142-150`), but current code hard-codes “committed object(s),” “chunk maps,” and `action=unresolvable-chunk-map` for every `RestoreReport::unresolvable` entry (`crates/server/src/cli.rs:1318-1341`). The existing verdict test seeds only `inode:` names (`crates/server/src/cli.rs:2893-2899`), and legs A–G never assert CLI output, so `cargo xtask ci` can stay green if this semantic change is omitted or staged keys are mislabeled. Add a red assertion using `mpu:`/`part:`/`sidx:` report names and pin the required text and exit verdict.
- [ ] The fail-closed criterion leaves a key-validation hole. Scope says a staged record that cannot be read makes GC/restore incomplete (`brief.md:130-143`), but E(i) covers a bad `part:` value and malformed `sidx:`/`mpu:` keys, not a malformed key inside a session's `part:<id>:` range (`brief.md:48-59`). On the target, `part:` key validation is a separate `parse_part_key` operation (`crates/core/src/multipart.rs:1267-1280`) from value validation by `decode_part_record` (`crates/core/src/multipart.rs:2575-2584`); the proposed tests can therefore pass even if the implementation consumes or silently skips a noncanonical `part:` key without the promised block/audit behavior. Define and test that case.
- [ ] size backstop — this slice is behaving oversized: patch is 157 KB (threshold 100 KB); 2 round(s) already spent (threshold 2). Recommend answering `iterate-plan` at sign-off and authoring the split in the re-plan (`pdca split`), rather than `iterate-do`: a slice that is too big yields implementation-shaped findings every round, and splitting authors briefs, which is Plan's beat.

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
- Iteration delta (if iterating): Rebuild to close two real implementation gaps found by adversarial review, not the size/plan concerns: - Item 2 (StagedSet::place mutants M1/M2 undetected): add a healthy multi-fragment ReedSolomon{k:2,m:1} upload with a real, non-identity placement (e.g. [2,0,1]) to legs A and B, fully placed and marked past grace, asserting all fragments survive GC/are unmarked by restore. Add an extra untracked copy on an unplaced server and assert GC reclaims it / restore marks it. This catches both M1 (index-zeroing bug) and M2 (place() short-circuit bug). - Item 6 (fail-closed key-validation hole): add a test case for a malformed key (not just a malformed value) inside a session's `part:<id>:` range, since key validation (`parse_part_key`) and value validation (`decode_part_record`) are separate code paths on the target branch; assert the pass fails closed the same way E(i) requires for value-level corruption. Human explicitly overrode the size backstop's `iterate-plan` recommendation (patch 157-160KB vs 100KB threshold, 2/2 rounds already spent) — proceeding with iterate-do on the basis that items 2 and 6 are genuine implementation gaps, not slicing/plan problems. Items 3 (re-plan/tracker mismatch) and 4 (leg C(ii) handoff mechanics) are brief/plan-level concerns, not addressed by this iteration; item 5 (CLI verdict red check) appears already covered by the current patch (`restore_verdict_names_unreadable_staged_records_as_staged` in the diff) and looks stale. Item 1 (fitness-to-purpose) remains open for the next sign-off.
- By / date: Eduard Ralph / 2026-09-15

## 10. Act candidates (hints for the next Act review)
- Plan advisory: 4 finding(s); brief revised: yes (plan-advisory-*.md)
- (empty is the common case)
