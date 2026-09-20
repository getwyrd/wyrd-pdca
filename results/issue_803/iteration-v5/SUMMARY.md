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
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass — run-verify.sh: PASS — red without the fix, green with it (17 test(s) ran red).
- C4 diff coverage: changed lines executed by the patch's tests: unverifiable — gate exceeded its 7200s timeout
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): pass — 36 mutants tested in 83s: 9 caught, 27 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 1 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_803/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 12.93s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Task under review: protect staged multipart `part:` and owned `sidx:` fragments from GC and post-restore marking without changing scrub/drain semantics; verdict: rebuild required because the staged-reference build omits its budget-profile fail-closed preflight.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The governing contract states the protection sources, source-before-destination order, aggregate `W_ref` bound, and fail-closed behavior needed to judge the change (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:765`, `docs/design/proposals/draft/0016-multipart-commit-protocol.md:801`). |
| C2 Reproduction (red pre-fix) | PASS | An independent tracked-hunk stash left the new test in place and reproduced 15 assertion failures plus 2 guard passes, then the identical restored patch passed all 17; the primary destructive-path assertions are grounded at `crates/custodian/tests/staged_protection.rs:736` and `crates/custodian/tests/staged_protection.rs:786`. |
| C3 Change | PASS | The scoped change reads staged records before committed references in GC and restore while leaving scrub/drain on the committed builder, which is the consumer boundary the task requires (`crates/custodian/src/gc.rs:250`, `crates/custodian/src/restore.rs:331`, `crates/custodian/tests/staged_protection.rs:1509`). |
| C4 Verification (red→green) | NEEDS-HUMAN | Decide whether full green CI plus the independently reproduced 15-red/17-green result is sufficient without a changed-line percentage—the coverage run completed relevant test phases but timed out at 7,200 seconds before reporting coverage (`gate-logs/C4-verify.log:10`, `gate-logs/C4-diff-cov.log:7`). |
| C5 Causal adequacy | NEEDS-HUMAN [impl] | Rebuild must compare the local budget profile with `mpuctl` and fail closed before retaining staged references; the current builder starts directly with `mpu:` pages, so rolling to a smaller `W_ref` can OOM instead of producing a controlled refusal (`crates/custodian/src/gc.rs:809`, `docs/design/proposals/draft/0016-multipart-commit-protocol.md:2628`). |
| T1 Structure | PASS | The regression is a new custodian integration test and the concurrency property is appended to the existing madsim campaign, preserving the required crate-root safety and test placement (`crates/custodian/tests/staged_protection.rs:1`, `crates/custodian/tests/staged_protection.rs:32`, `crates/dst/tests/custodian.rs:3138`). |
| T2 Shape | PASS | The tests use destructive controls, malformed/untrusted/store-fault cases, bounded per-session reads, and explicit between/outside handoff coverage, so the exercised cases cannot pass through no-op behavior (`crates/custodian/tests/staged_protection.rs:736`, `crates/custodian/tests/staged_protection.rs:999`, `crates/dst/tests/custodian.rs:3002`). |
| T3 Runtime | PASS | The applied integration suite and CLI verdict test pass locally, while frozen CI also ran the madsim handoff properties and the TiKV feature build successfully (`crates/server/src/cli.rs:3031`, `gate-logs/C4-ci.log:3497`, `gate-logs/host-tikv.log:207`). |
| T4 Contribution | FAIL | The multi-pass review gate found the grounded budget-profile blocker above; the separate contribution-artifact check is N/A because `pr-description.md` is intentionally drafted and audited at publish (`gate-logs/T4-batch-review.log:10`, `gate-logs/T4-contribution.log:10`). |
| T5 Judgment | NEEDS-HUMAN [impl] | Add a stored-profile mismatch regression before treating the suite as causally complete: its fixtures seed `mpu:` records without `mpuctl`, so they cannot fail when the required preflight is omitted (`crates/custodian/tests/staged_protection.rs:13`, `crates/core/src/multipart.rs:1533`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Confirm production fitness and the affected-path prior-art result—the disposable target has one commit and no remotes, so merged plus closed/rejected work could not be mechanically rechecked; that matters before accepting this fleet-wide GC/restore safety behavior. |

### Advisory — adversary

# Adversarial review — #803 staged protection class for GC and restore

I re-ran the proof on a scratch copy of the patched tree: `cargo test -p wyrd-custodian --test staged_protection` passes 17 of 17. The base run in `gate-logs/C4-verify.log:130` fails by assertion, not by a compile error. I then tried four mutations and one new input. The fix held up against every input I built. What I found are two holes in the tests, one gate claim that overstates, and the blocking T4 finding, which needs a scope call.

## Findings

- NEEDS-HUMAN [impl] — **Reading past the first page of an upload's own records is never tested.** `crates/custodian/src/gc.rs:851` moves the cursor inside `walk_staged_range` (`:839`). In every test, each upload holds one `part:` record and one `sidx:` entry. The only store with a lowered cap (`crates/custodian/tests/staged_protection.rs:738`, `:788`, `:1001`) spreads the *list of uploads* over several pages, never one upload's own records. Concrete case: I replaced the loop in `walk_staged_range` with "read one page, then return". All 17 tests stayed green, and so did the DST leg, which also stages one part. In production that mutant loses protection for everything past record 512 of an upload (`STAGED_PAGE`, `gc.rs:147`), or sooner on a backend with a lower `with_scan_cap`. An upload with 10,000 parts would lose parts 513 and up. The fix itself handles this correctly. I added a test: one `Open` upload, a store cap of 2, five `part:` records and five `sidx:` entries, all marked past grace. It passes on the patch and fails on the mutant with "GC reclaimed ... a record past the first page". C5 did not generate this mutant (9 caught, 27 unviable), and C4-diff-cov timed out, so no gate covers this line. Add a test like it for both GC and restore.
- NEEDS-HUMAN [impl] — **Restore's read order is never tested; only GC's is.** Restore reads the staged records at `crates/custodian/src/restore.rs:340`, before the two committed reads (`:350`, `:360`). The doc at `restore.rs:239-241` claims this order means "a publication that moves a chunk from its part record to a committed inode while the pass runs leaves it protected by one reading or the other." I moved the staged read after `committed_chunks` and after `appeared_since`. All 17 tests stayed green. GC's order is tested: moving GC's staged read after the committed reference build fails `a_publication_flipped_and_drained_between_the_reads_leaves_the_chunk_protected`, and swapping the `sidx:` and `part:` reads fails `a_part_commit_between_its_two_reads_leaves_the_chunk_protected`. The brief's leg C covers GC only, and restore's window for new writes belongs to #805. But this claim is about records that already existed, which this slice does own. So either add a restore version of C(ii) (hook the upload's `part:` range read and the `inode:` scan), or cut the sentence so the doc claims only what a test checks. Low severity: the runbook stops writers during restore.
- NEEDS-HUMAN [human] — **The blocking T4 finding is unanswered: no profile check before building the staged set, and no deferral marker.** `check-gates.json:104-112` (`gate-logs/T4-batch-review.log`) blocks on `gc.rs:810`: `staged_fragments` never reads `mpuctl`. Proposal 0016 says the custodian "compares `profile` against its own configuration and fails closed before building the reference set" (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:348`, X99 `:2628`). This patch is the first code that builds that reference set. Nothing in the tree implements the check (no match for X99 or a profile comparison under `crates/`), and neither the brief's scope nor its out-of-scope list mentions it. It can't cause a failure today: no client creates uploads before #508, and the profile is a compile-time constant (`crates/core/src/multipart.rs:4617-4624`). The risk is future: OOM during a rolling config change instead of a clean refusal. Per the rubric, the choice is to fold the check in here (`decode_admission_record` exists at `multipart.rs:1899`) or to decline with an issue reference and add `// deferred: #N` at `gc.rs:809`. Leaving it as it is keeps T4 red.
- **Overstated gate claim, not a patch defect.** `check-gates.json:48` says "17 test(s) ran red". The log shows `2 passed; 15 failed` on base (`gate-logs/C4-verify.log:130`). The two that pass on base are D (`gc_and_restore_never_scan_a_whole_staged_namespace`) and F (`scrub_and_drain_status_do_not_read_upload_records`), which the brief designs to pass on base. The red→green proof holds for the 15 that should fail. If `build-notes.md` copied "17", it should say 15.

## Attempted, could not refute

- **GC read order.** Both order mutations (GC's staged read after the `inode:` read; `part:` before `sidx:` within an upload) fail a leg-C test by assertion. The C doubles apply the concurrent batch only after the read's result is taken (`crates/custodian/tests/staged_protection.rs:233-249`, `:310-324`), so they test the real schedule, not a tie.
- **DST timing is honest.** The simulated TiKV store applies a commit and returns with no await in between (`crates/dst/tests/support/mod.rs:348-357`). A read takes its snapshot after its network hop (`:389-391`). So the tap's `Landed` and `Read` events are logged in the same order the changes took effect, and the between/outside coverage leg measures real windows. The DST leg ran green in C4-ci (`gate-logs/C4-ci.log:3497`, `:3508`).
- **Damaged records, faults, scope.** An unreadable record blocks the whole pass: GC answers `Blocked` (`gc.rs:410-420`) and restore names the record. An untrusted record holds only its own chunk, since `held` is keyed by chunk id, and the control is still marked or reclaimed. A store fault is wrapped with its `source` kept, and it fires before any delete or mark. Scrub and drain status issue no read under `mpu:`, `sidx:` or `part:`. Paging uses the shared `checked_page` bound and cursor check. `mpuctl` can't show up in the `mpu:` listing (`multipart.rs:1126-1132`). Upload ids are fixed-width, so `sidx:<id>:` and `part:<id>:` can't catch another upload's keys. I found no input that makes the patched GC reclaim, or restore mark, a fragment that an upload record already in the store names.
- **Leg I.** No unreadable-record wording in `crates/server/src/cli.rs` still says "committed object(s)". The runbook entry (`m4-first-deployment-blueprint.md:609-622`) matches the new text.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C4 Verification (red→green) — Decide whether full green CI plus the independently reproduced 15-red/17-green result is sufficient without a changed-line percentage—the coverage run completed relevant test phases but timed out at 7,200 seconds before reporting coverage (`gate-logs/C4-verify.log:10`, `gate-logs/C4-diff-cov.log:7`).
- [ ] C5 Causal adequacy — Rebuild must compare the local budget profile with `mpuctl` and fail closed before retaining staged references; the current builder starts directly with `mpu:` pages, so rolling to a smaller `W_ref` can OOM instead of producing a controlled refusal (`crates/custodian/src/gc.rs:809`, `docs/design/proposals/draft/0016-multipart-commit-protocol.md:2628`).
- [ ] T5 Judgment — Add a stored-profile mismatch regression before treating the suite as causally complete: its fixtures seed `mpu:` records without `mpuctl`, so they cannot fail when the required preflight is omitted (`crates/custodian/tests/staged_protection.rs:13`, `crates/core/src/multipart.rs:1533`).
- [ ] Validation — fitness-to-purpose — Confirm production fitness and the affected-path prior-art result—the disposable target has one commit and no remotes, so merged plus closed/rejected work could not be mechanically rechecked; that matters before accepting this fleet-wide GC/restore safety behavior.
- [ ] **Reading past the first page of an upload's own records is never tested.** `crates/custodian/src/gc.rs:851` moves the cursor inside `walk_staged_range` (`:839`). In every test, each upload holds one `part:` record and one `sidx:` entry. The only store with a lowered cap (`crates/custodian/tests/staged_protection.rs:738`, `:788`, `:1001`) spreads the *list of uploads* over several pages, never one upload's own records. Concrete case: I replaced the loop in `walk_staged_range` with "read one page, then return". All 17 tests stayed green, and so did the DST leg, which also stages one part. In production that mutant loses protection for everything past record 512 of an upload (`STAGED_PAGE`, `gc.rs:147`), or sooner on a backend with a lower `with_scan_cap`. An upload with 10,000 parts would lose parts 513 and up. The fix itself handles this correctly. I added a test: one `Open` upload, a store cap of 2, five `part:` records and five `sidx:` entries, all marked past grace. It passes on the patch and fails on the mutant with "GC reclaimed ... a record past the first page". C5 did not generate this mutant (9 caught, 27 unviable), and C4-diff-cov timed out, so no gate covers this line. Add a test like it for both GC and restore.
- [ ] **Restore's read order is never tested; only GC's is.** Restore reads the staged records at `crates/custodian/src/restore.rs:340`, before the two committed reads (`:350`, `:360`). The doc at `restore.rs:239-241` claims this order means "a publication that moves a chunk from its part record to a committed inode while the pass runs leaves it protected by one reading or the other." I moved the staged read after `committed_chunks` and after `appeared_since`. All 17 tests stayed green. GC's order is tested: moving GC's staged read after the committed reference build fails `a_publication_flipped_and_drained_between_the_reads_leaves_the_chunk_protected`, and swapping the `sidx:` and `part:` reads fails `a_part_commit_between_its_two_reads_leaves_the_chunk_protected`. The brief's leg C covers GC only, and restore's window for new writes belongs to #805. But this claim is about records that already existed, which this slice does own. So either add a restore version of C(ii) (hook the upload's `part:` range read and the `inode:` scan), or cut the sentence so the doc claims only what a test checks. Low severity: the runbook stops writers during restore.
- [ ] **The blocking T4 finding is unanswered: no profile check before building the staged set, and no deferral marker.** `check-gates.json:104-112` (`gate-logs/T4-batch-review.log`) blocks on `gc.rs:810`: `staged_fragments` never reads `mpuctl`. Proposal 0016 says the custodian "compares `profile` against its own configuration and fails closed before building the reference set" (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:348`, X99 `:2628`). This patch is the first code that builds that reference set. Nothing in the tree implements the check (no match for X99 or a profile comparison under `crates/`), and neither the brief's scope nor its out-of-scope list mentions it. It can't cause a failure today: no client creates uploads before #508, and the profile is a compile-time constant (`crates/core/src/multipart.rs:4617-4624`). The risk is future: OOM during a rolling config change instead of a clean refusal. Per the rubric, the choice is to fold the check in here (`decode_admission_record` exists at `multipart.rs:1899`) or to decline with an issue reference and add `// deferred: #N` at `gc.rs:809`. Leaving it as it is keeps T4 red.
- [ ] C4 diff coverage: changed lines executed by the patch's tests unverifiable — gate exceeded its 7200s timeout
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 1 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_803/review-b
- [ ] The re-plan is not supported by the supplied tracker record. `notes.json` still titles #803 “staged protection class in the shared reference set” and its Scope says “the staged protection class in the shared reference set”; it has no comments recording the claimed 2026-09-16 re-plan or sign-off. The brief instead says “Do not re-attempt the shared-builder placement” (`brief.md:189-206`) and adds separate GC/restore reads plus CLI/runbook work (`brief.md:130-159`). Revise the brief to match the tracker-authorized change, or explicitly surface this authorization/scope conflict for adjudication rather than presenting the replacement design as recorded fact.
- [ ] Success leg C(ii) tests the wrong publication handoff. It says one atomic batch writes the committed inode and removes the `part:` record (`brief.md:36-43`), but the target contract says the root flip moves protection to the inode and installs `retire:records:{parts}`, “whose drain then deletes those part records” (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:793-800`); the flip atomically commits the inode and `Completed` session (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:941-965`), while the record-mode obligation is explicitly the later deleter (`crates/core/src/multipart.rs:3144-3150`, `crates/core/src/multipart.rs:3424-3432`). A double that collapses flip and drain into a nonexistent transaction does not exercise the real three-event window; revise C and G to schedule publication and retirement-drain deletion separately.
- [ ] The requested operator-verdict change has no red success check. Scope requires the NEEDS-HUMAN paragraph to name staged records beside committed objects (`brief.md:142-150`), but current code hard-codes “committed object(s),” “chunk maps,” and `action=unresolvable-chunk-map` for every `RestoreReport::unresolvable` entry (`crates/server/src/cli.rs:1318-1341`). The existing verdict test seeds only `inode:` names (`crates/server/src/cli.rs:2893-2899`), and legs A–G never assert CLI output, so `cargo xtask ci` can stay green if this semantic change is omitted or staged keys are mislabeled. Add a red assertion using `mpu:`/`part:`/`sidx:` report names and pin the required text and exit verdict.
- [ ] The fail-closed criterion leaves a key-validation hole. Scope says a staged record that cannot be read makes GC/restore incomplete (`brief.md:130-143`), but E(i) covers a bad `part:` value and malformed `sidx:`/`mpu:` keys, not a malformed key inside a session's `part:<id>:` range (`brief.md:48-59`). On the target, `part:` key validation is a separate `parse_part_key` operation (`crates/core/src/multipart.rs:1267-1280`) from value validation by `decode_part_record` (`crates/core/src/multipart.rs:2575-2584`); the proposed tests can therefore pass even if the implementation consumes or silently skips a noncanonical `part:` key without the promised block/audit behavior. Define and test that case.
- [ ] size backstop — this slice is behaving oversized: patch is 141 KB (threshold 100 KB). Recommend answering `iterate-plan` at sign-off and authoring the split in the re-plan (`pdca split`), rather than `iterate-do`: a slice that is too big yields implementation-shaped findings every round, and splitting authors briefs, which is Plan's beat.

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
- Iteration delta (if iterating): The slice converged; do NOT split (a split was vetoed at the 2026-09-16 re-plan, and the size backstop is overridden — brief.md:152-154). Three implementation-sized fixes, then the same brief applies unchanged: 1. Clear the blocking T4 finding (review-batch.md, gc.rs:810): the mpuctl budget-profile preflight (0016:348, X99 :2628) is deferred to getwyrd/wyrd#806, filed 2026-09-16. Add `// deferred: #806` at the staged_fragments site (gc.rs:809) as the patch already does for #663/#664/#805, and record the finding in review-rejected.md with that reference. Do not implement the check in this slice. 2. Adversary finding 1: add a paging test for one upload's OWN records — one Open upload, store scan cap 2, five part: records and five sidx: entries all marked past grace — for both GC (reconcile_step) and restore (reconcile_after_restore); the mutant "read one page of walk_staged_range then return" (gc.rs:839-851) must fail it. 3. Adversary finding 2: restore's staged-before-committed read order (restore.rs:340 before :350/:360) is claimed at restore.rs:239-241 but untested. Either add a restore version of C(ii) (hook the upload's part: range read and the inode: scan; flip+drain between the reads) or cut the sentence so the doc claims only what a test checks. Ignore: the four plan-advisory lines in §6 (already revised into the brief) and the size-backstop line. build-notes.md correctly says 15 of 17 ran red (D and F are guards, green on base); the gate's "17" is the gate's wording.
- By / date: Eduard Ralph / 2026-09-15

## 10. Act candidates (hints for the next Act review)
- Plan advisory: 4 finding(s); brief revised: yes (plan-advisory-*.md)
- (empty is the common case)
- Driver: §6 re-surfaces plan-advisory findings the brief already revised (4 stale lines here) and prints the size backstop despite the human's override recorded in brief.md — both are harness behaviour, candidate for eduralph/pdca-harness.
