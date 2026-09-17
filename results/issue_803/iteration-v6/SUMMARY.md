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
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass — run-verify.sh: PASS — red without the fix, green with it (22 test(s) ran red).
- C4 diff coverage: changed lines executed by the patch's tests: pass — diff coverage 98.8% — 249 of 252 instrumentable changed lines executed (floor 80%); 252 of 628 changed lines were instru
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): pass — 36 mutants tested in 84s: 9 caught, 27 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): pass — review-branch: 0 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_803/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 12.84s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Issue #803 is a verified staged-byte protection fix for GC and post-restore marking, with only the required human fitness-to-purpose sign-off outstanding.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The decision is sufficiently constrained around destructive consumers, handoff order, failure containment, bounded reads, consumer isolation, and tracked deferrals, so implementation can be judged without inventing scope (`brief.md:160`). |
| C2 Reproduction (red pre-fix) | PASS | An independent stash run on the supplied pre-fix base produced 20 assertion failures out of 22 tests while the two intended guards stayed green, corroborating the frozen red evidence (`gate-logs/C4-verify.log:15`). |
| C3 Change | PASS | The patch addresses the data-loss path at both destructive decision points by reading a separate staged set before committed references, without moving staged reads into the shared scrub/drain builder (`crates/custodian/src/gc.rs:249`, `crates/custodian/src/restore.rs:340`). |
| C4 Verification (red→green) | PASS | Restoring the patch made the same 22 tests pass independently; focused fmt, typos, docs-lint, CLI, and madsim reruns were green, while frozen full CI and 98.8% diff coverage also passed (`gate-logs/C4-verify.log:10`, `gate-logs/C4-diff-cov.log:760`). |
| C5 Causal adequacy | PASS | The visibility gap is removed through source-before-destination reads and fail-closed record classification rather than a capability probe or symptom guard, covering both handoffs and corrupt/unknown staged metadata (`crates/custodian/src/gc.rs:786`, `crates/custodian/src/gc.rs:809`). |
| T1 Structure | PASS | The reusable staged-set reader and per-consumer gates are separated from the new integration suite, existing DST/CLI tests, and current architecture docs, preserving the declared module and scope boundaries (`crates/custodian/src/gc.rs:641`, `crates/custodian/tests/staged_protection.rs:1`). |
| T2 Shape | PASS | Public reconciliation signatures and `RestoreReport` remain unchanged, while staged state stays disjoint from committed references and is read only through paged per-session ranges (`crates/custodian/src/gc.rs:647`, `crates/custodian/src/gc.rs:794`). |
| T3 Runtime | PASS | Production entry points are exercised with positive controls, multi-page uploads, both handoffs, malformed records, store faults, consumer-isolation guards, and seeded madsim schedules; the targeted runtime reruns passed (`crates/custodian/tests/staged_protection.rs:751`, `crates/dst/tests/custodian.rs:2997`). |
| T4 Contribution | N/A | Contribution artifacts are intentionally absent during Check and their substantive audit must rerun at publish (`gate-logs/T4-contribution.log:10`); the affected-path prior-art check across merged, open, closed, and rejected work is recorded (`brief.md:210`). |
| T5 Judgment | PASS | No unresolved implementation defect remained after deep review; mutation testing had no surviving viable mutant and the independent batched review reported zero blocking findings (`gate-logs/C5-mutants.log:12`, `gate-logs/T4-batch-review.log:10`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | The human must confirm the availability-versus-data-loss tradeoff is fit for deployment: one unreadable staged record intentionally stalls GC and restore fleet-wide until repair (`brief.md:167`, `crates/custodian/src/gc.rs:411`). |

### Advisory — adversary

# Adversarial review — #803 staged protection for GC and restore

**Verdict: I tried to refute the production fix and could not.** The staged read order, paging,
fail-closed handling and the restore gate all held up against targeted breakage. What I did find
is two test gaps (a wrong build passes) and one doc sentence that claims more than the code does.

## What I re-ran (scratch copy of `$PDCA_TARGET`, since removed)

- `cargo test -p wyrd-custodian --test staged_protection`: 22/22 green. The C4-verify log shows 20
  red on base, all by assertion; the 2 green are the D and F guards, as the brief expects.
- Leg I against base `restore_verdict` (base `cli.rs`, new test added): red by assertion —
  `the verdict does not say "4 record(s) UNREADABLE"`; base prints `4 committed object(s) UNREADABLE`.
- 14 hand mutations of `gc.rs` / `restore.rs`; **12 caught**: one page per staged range (the
  carry-forward #2 mutant → both paging legs red); one page of the `mpu:` listing; GC reading
  `inode:` before the staged class; `part:` before `sidx:`; restore reading the staged class after
  both committed reads, or between them (the three restore C(ii) tests red); `Blocked` ignoring
  staged unresolvable; an `sidx:` key naming no chunk skipped silently; `part:` key not parsed;
  the read fault not wrapped with its range; restore not naming staged records in the report;
  restore not auditing untrusted records.
- DST leg G (`--cfg madsim`, `MADSIM_TEST_NUM=50`): green on the patch, and **red** on both
  order-breaking builds (inode-first, part-before-sidx), so the property actually bites.

## Attacks that did not break it

- Non-session keys under `mpu:` — `mpuctl` is outside the prefix (`crates/core/src/multipart.rs:1126-1132`).
- A 512-record page overrunning a backend — `page_limit` clamps (`crates/traits/src/lib.rs:414`)
  and TiKV fetches in chunks (`crates/metadata-tikv/src/lib.rs:1275`).
- Another GC delete path skipping the gate — the expired-lease arm sits behind it (`crates/custodian/src/gc.rs:331-337`).
- A staged fault in GC silencing scrub — true inside one combined `reconcile_step`
  (`crates/custodian/src/reconciliation.rs:136-149`), but the production runtime runs each loop in
  its own call with GC last (`crates/server/src/custodian.rs:519`, `:533`, `:610`). Not a break.
- A second restore marking site — there is one, and it is gated (`crates/custodian/src/restore.rs:435-441`).
- Carry-forwards: `// deferred: #806` is present (`gc.rs:810`); items 2 and 3 are covered (mutants above went red).

## Findings

- NEEDS-HUMAN [impl] — **"hold the chunk whole" for a wrong-length staged placement is not pinned.**
  `crates/custodian/tests/staged_protection.rs:1501` places the held chunk's fragments at
  `(0,0) (1,1) (2,2)`, exactly where `ChunkRef::fragments()`'s identity fallback puts them
  (`crates/core/src/metadata.rs:164-169`, `:187-189`). So a build that identity-fills the short
  placement (the alternative `StagedSet`'s doc rejects: "held to the exact length") passes both
  wrong-length legs. Concrete survivor: remove the `held` arm at `crates/custodian/src/gc.rs:693`
  and, in `place` (`gc.rs:754-756`), insert `chunk.fragments()` into `placed` before `hold` →
  `an_owned_entry_with_a_wrong_length_placement_holds_its_chunk` and
  `a_part_with_a_wrong_length_placement_holds_its_chunk` stay green (only the undecodable-value leg
  goes red). Fix: put one held fragment on a server the fallback does not name, e.g.
  `(3, frag(held, 2))`. I checked: that turns all three legs red on the mutant and stays green on the patch.

- NEEDS-HUMAN [impl] — **GC's audit name for an unreadable staged record is never checked.**
  Emptying `emit_unresolvable_staged` (`crates/custodian/src/gc.rs:1344`, called at `:263`) leaves
  all 22 tests green: the E(i) harness's GC half (`staged_protection.rs:1378-1420`) asserts only
  `Blocked` and survival. That line is the only place the unattended GC loop names the record
  behind a fleet-wide stall — the stall the human accepted because it can be found and repaired —
  and the committed twin (`action=unresolvable-chunk-map`) is pinned elsewhere
  (`crates/custodian/tests/segmented_map_consumers.rs:369`). Fix: assert
  `named_on_audit_seam(GC_AUDIT, <damaged key>)` in the E(i) harness, as E(ii) already does.

- NEEDS-HUMAN [impl] — **doc sentence overclaims bounded reads.**
  `docs/design/architecture/06-runtime-view.md:78` says both passes read staging entries, committed
  parts "and only then the committed objects, a page at a time and never one listing of a whole
  namespace". The committed objects are one `meta.scan(b"inode:")` (`crates/custodian/src/gc.rs:549`),
  and restore does it twice. Limit the "page at a time" clause to the upload records.

- Gate claim not to lean on (no patch action): the C5 row in `check-gates.json` ("36 mutants: 9
  caught, 27 unviable") reads as mutation evidence, but the workspace sets `warnings = "deny"`
  (`Cargo.toml:227`), so most body-replacement mutants leave an unused parameter and fail to
  compile — 27 of 36 never ran. The hand mutations above cover that gap. Likewise C4-verify's
  "22 test(s) ran red" is 20 red plus 2 green guards (its own log shows this).

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [x] Validation — fitness-to-purpose — The human must confirm the availability-versus-data-loss tradeoff is fit for deployment: one unreadable staged record intentionally stalls GC and restore fleet-wide until repair (`brief.md:167`, `crates/custodian/src/gc.rs:411`). Cleared: already approved at the 2026-09-16 re-plan sign-off (`brief.md:168`, iteration-v4 §9); a tracked bug covers the operational follow-up.
- [ ] **"hold the chunk whole" for a wrong-length staged placement is not pinned.**
- [ ] **GC's audit name for an unreadable staged record is never checked.**
- [ ] **doc sentence overclaims bounded reads.**
- [x] The re-plan is not supported by the supplied tracker record. `notes.json` still titles #803 “staged protection class in the shared reference set” and its Scope says “the staged protection class in the shared reference set”; it has no comments recording the claimed 2026-09-16 re-plan or sign-off. The brief instead says “Do not re-attempt the shared-builder placement” (`brief.md:189-206`) and adds separate GC/restore reads plus CLI/runbook work (`brief.md:130-159`). Revise the brief to match the tracker-authorized change, or explicitly surface this authorization/scope conflict for adjudication rather than presenting the replacement design as recorded fact. Cleared: already flagged as the human's call at hand-off (`brief.md`, Plan review 2026-09-16, response (1)); not blocking.
- [ ] Success leg C(ii) tests the wrong publication handoff. It says one atomic batch writes the committed inode and removes the `part:` record (`brief.md:36-43`), but the target contract says the root flip moves protection to the inode and installs `retire:records:{parts}`, “whose drain then deletes those part records” (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:793-800`); the flip atomically commits the inode and `Completed` session (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:941-965`), while the record-mode obligation is explicitly the later deleter (`crates/core/src/multipart.rs:3144-3150`, `crates/core/src/multipart.rs:3424-3432`). A double that collapses flip and drain into a nonexistent transaction does not exercise the real three-event window; revise C and G to schedule publication and retirement-drain deletion separately.
- [ ] The requested operator-verdict change has no red success check. Scope requires the NEEDS-HUMAN paragraph to name staged records beside committed objects (`brief.md:142-150`), but current code hard-codes “committed object(s),” “chunk maps,” and `action=unresolvable-chunk-map` for every `RestoreReport::unresolvable` entry (`crates/server/src/cli.rs:1318-1341`). The existing verdict test seeds only `inode:` names (`crates/server/src/cli.rs:2893-2899`), and legs A–G never assert CLI output, so `cargo xtask ci` can stay green if this semantic change is omitted or staged keys are mislabeled. Add a red assertion using `mpu:`/`part:`/`sidx:` report names and pin the required text and exit verdict.
- [ ] The fail-closed criterion leaves a key-validation hole. Scope says a staged record that cannot be read makes GC/restore incomplete (`brief.md:130-143`), but E(i) covers a bad `part:` value and malformed `sidx:`/`mpu:` keys, not a malformed key inside a session's `part:<id>:` range (`brief.md:48-59`). On the target, `part:` key validation is a separate `parse_part_key` operation (`crates/core/src/multipart.rs:1267-1280`) from value validation by `decode_part_record` (`crates/core/src/multipart.rs:2575-2584`); the proposed tests can therefore pass even if the implementation consumes or silently skips a noncanonical `part:` key without the promised block/audit behavior. Define and test that case.
- [x] size backstop — this slice is behaving oversized: patch is 153 KB (threshold 100 KB). Recommend answering `iterate-plan` at sign-off and authoring the split in the re-plan (`pdca split`), rather than `iterate-do`: a slice that is too big yields implementation-shaped findings every round, and splitting authors briefs, which is Plan's beat. Cleared: size already accepted at the 2026-09-16 re-plan sign-off (`brief.md:152-154`, "the human accepted an oversize patch at this re-plan... so size alone is not a reason to split again"); human confirms iterate-do for the remaining implementation items, not a re-split.

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
- Iteration delta (if iterating): Fix the five implementation-shaped findings from the adversary review, keeping the rest of the brief unchanged: 1. "Hold the chunk whole" test gap — the wrong-length placement tests at staged_protection.rs:1501 coincide with ChunkRef::fragments()'s identity fallback, so a build missing the `held` arm still passes. Move the held fragment to a server the fallback doesn't name (e.g. `(3, frag(held, 2))`) so all three wrong-length legs actually catch the mutant at gc.rs:693/754-756. 2. GC's audit line for an unreadable staged record (emit_unresolvable_staged, gc.rs:1344, called at :263) is never asserted. Add `named_on_audit_seam(GC_AUDIT, <damaged key>)` to the E(i) harness, as E(ii) already does. 3. docs/design/architecture/06-runtime-view.md:78 overclaims "page at a time... never one listing of a whole namespace" — the committed-inode scan is still one meta.scan(b"inode:") (gc.rs:549) and restore does it twice. Narrow the "page at a time" clause to the upload records only. 4. Leg C(ii)/G may test the wrong publication handoff shape — the double may be collapsing the root-flip batch and the retirement-drain batch into one, when 0016:793-800/941-965 and multipart.rs:3144-3150/3424-3432 describe them as two separate batches. Verify and, if the finding holds, revise C(ii) and G to schedule the flip and the drain-deletion of the `part:` record as separate batches. 5. No red test proves the operator-facing verdict actually says "staged record" vs "committed object" — cli.rs:1318-1341 hard-codes committed-object wording and the existing verdict test only seeds inode: names. Add a red assertion using mpu:/part:/sidx: report names, pinning the exact text and exit verdict per brief.md leg I. Explicitly NOT blocking, cleared this round — do not re-raise unchanged: - Validation fitness-to-purpose tradeoff (one unreadable staged record stalls GC/restore fleet-wide) — already accepted at the 2026-09-16 re-plan sign-off; a tracked bug covers the operational follow-up. - Tracker-record vs brief mismatch — already flagged as the human's call at hand-off (Plan review 2026-09-16, response (1)); not a build-blocking item. - Size backstop (153 KB vs 100 KB) — already accepted as an oversize patch at the 2026-09-16 re-plan; do not re-split. Stay iterate-do, not iterate-plan.
- By / date: Eduard Ralph / 2026-09-15

## 10. Act candidates (hints for the next Act review)
- Plan advisory: 4 finding(s); brief revised: yes (plan-advisory-*.md)
- (empty is the common case)
