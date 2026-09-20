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
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass — run-verify.sh: PASS — red without the fix, green with it (26 test(s) ran red).
- C4 diff coverage: changed lines executed by the patch's tests: pass — diff coverage 98.9% — 258 of 261 instrumentable changed lines executed (floor 80%); 261 of 637 changed lines were instru
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): pass — 36 mutants tested in 84s: 9 caught, 27 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): pass — review-branch: 0 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_803/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 12.92s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Reviewing #803: add staged multipart protection so GC never reclaims, and post-restore never marks, fragments still named by `part:` or `sidx:` records; the implementation is technically sound, with prior-art and fitness sign-offs outstanding.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The brief fixes the protection classes, read order, fail-closed behavior, bounded scans, unchanged scrub/drain scope, operator output, and external dependencies with falsifiable acceptance criteria (`brief.md:17`). |
| C2 Reproduction (red pre-fix) | PASS | Independent pre-fix replay ran all 26 tests: 24 failed by assertion on the missing protection while the two intended guards stayed green (`crates/custodian/tests/staged_protection.rs:853`). |
| C3 Change | PASS | The change stays within scope: a separate staged class feeds only GC and post-restore, leaving the shared committed-reference consumers unchanged (`crates/custodian/src/gc.rs:641`, `crates/custodian/src/restore.rs:340`). |
| C4 Verification (red→green) | PASS | Independent fixed replay passed 26/26, both targeted madsim handoff properties passed, and TiKV feature clippy passed; local full CI reached cargo-deny before a read-only host lock stopped it, while frozen evidence shows cargo-deny, DST, and full CI green (`gate-logs/C4-ci.log:2969`). |
| C5 Causal adequacy | PASS | The patch restores the missing durable-reference class and source-before-destination ordering rather than adding a capability probe or symptom guard, with unreadable and untrusted records failing closed (`crates/custodian/src/gc.rs:783`). |
| T1 Structure | PASS | A disjoint `StagedSet` preserves the existing dependency direction and prevents scrub and drain status from inheriting upload-record reads or faults (`crates/custodian/src/gc.rs:647`). |
| T2 Shape | PASS | Per-session paged walks, exact placement validation, raw-key attribution, and source-preserving error wrapping make bounds and failure shapes explicit (`crates/custodian/src/gc.rs:709`, `crates/custodian/src/gc.rs:809`). |
| T3 Runtime | PASS | Both production entry points consume staged protection before destructive decisions, and seeded simulation covers moves both inside and outside the handoff windows (`crates/custodian/src/gc.rs:249`, `crates/custodian/src/restore.rs:312`, `crates/dst/tests/custodian.rs:2997`). |
| T4 Contribution | N/A | Contribution artifacts are intentionally absent during Check; the frozen deferred gate states that their substantive audit reruns at publish (`gate-logs/T4-contribution.log:10`). |
| T5 Judgment | NEEDS-HUMAN | Confirm affected-path prior art across merged and closed/rejected work before publish — this target contains one commit and no remotes, so overlap cannot be mechanically ruled out here and could duplicate or conflict with existing work (`brief.md:210`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Approve the staged-protection and operator-recovery behavior for production GC/restore use — automated evidence establishes mechanics, but operational fitness and availability impact remain the sign-off decision (`docs/design/architecture/06-runtime-view.md:78`). |

### Advisory — adversary

# Adversarial review — issue #803 (staged protection class for GC and restore)

Verdict: **could not refute the fix.** Two doc-accuracy defects found, both `[impl]`-shaped.
The protection logic itself survived every attack I could build.

## Findings

- NEEDS-HUMAN [impl] — `docs/design/architecture/m4-first-deployment-blueprint.md:610-612` tells the
  operator that for an upload session (`mpu:`) **or an in-flight staging entry (`sidx:`)** "the pass
  never decodes their values, so a damaged one is not what you are looking at". That is true of
  `mpu:` and **false of `sidx:`**: `StagedSet::read_owned_entry` (`crates/custodian/src/gc.rs:711-712`)
  decodes every owned value through `decode_owned_entry`, which also enforces byte-canonicality
  (`crates/core/src/multipart.rs:3735-3755`). Concrete case: an `sidx:` value corrupted in the store
  under a key that still parses. The runbook says there is nothing to look at; in fact both passes
  hold that chunk's fragments unmarked and emit `action=untrusted-staged-record`
  (`crates/custodian/src/gc.rs:1359`, `crates/custodian/src/restore.rs:1004`) — a signal this runbook
  entry never mentions, while the audit-action list two lines below names only the two
  `unresolvable-*` actions. An operator following this text concludes a damaged staging-entry value
  produces no signal and no held chunk. The CLI comment gets it right by scoping the same claim to
  "to build this class" (`crates/server/src/cli.rs:1339-1344`); the runbook dropped the scope. One
  sentence to reword; it does not touch any string leg I pins.

- NEEDS-HUMAN [impl] — `crates/custodian/src/gc.rs:906-909`: `object_name`'s contract still reads
  "How a blocker is named to an operator: the `inode:` key as the store spells it", but this patch
  routes three new key namespaces and one key *range* through it — `mpu:` / `part:` / `sidx:` keys at
  `gc.rs:262-268` and `restore.rs:814-824`, and `StagedReadFault.range` at `gc.rs:873`, which is a
  prefix, not a record key. The doc is now wrong about its own callers, in the one helper the report
  names, the CLI paragraph and the new fault text all depend on. Docs-currency nit, one line.

## What I attacked and could not break

- **Red→green, re-run independently.** `cargo test -p wyrd-custodian --test staged_protection` in a
  scratch copy: 26/26 green with the patch. The frozen `gate-logs/C4-verify.log` shows 24 of those
  26 failing on base by assertion, with the guards (D, F) green on base as the brief designed.
- **Leg I's red, which no gate can show** (a modified file earns no C4-verify red). I rebuilt
  `crates/server/src/cli.rs` as base production code + the new test only, and the test fails on base:
  `the verdict does not say "4 record(s) UNREADABLE" … 4 committed object(s) UNREADABLE`. The brief's
  claim that leg I is red-by-assertion on base holds.
- **Twelve hand-built mutants on the production path, all caught** (each compiled, each run against
  the new suite): allow an empty staged placement through `place` (`gc.rs:756`) → 2 fail; read
  `part:` before `sidx:` (`gc.rs:828-835`) → 4 fail; drop the `held` arm and drop the
  `incomplete-staged-set` arm of `StagedSet::protection` (`gc.rs:693`, `:695`) → 6 and 4 fail; skip
  `parse_part_key` in `read_part` (`gc.rs:740`) → 1 fails; stop `walk_staged_range` after one page
  (`gc.rs:856-859`) → 2 fail; drop `staged.unresolvable` from GC's `Blocked` answer (`gc.rs:411`) → 4
  fail; drop the staged arm from GC's reclaim gate (`gc.rs:331-334`) → 17 fail; drop it from restore's
  mark gate (`restore.rs:435-441`) → 9 fail; classify an undecodable owned value as unresolvable
  instead of held (`gc.rs:726-731`) → 1 fails; stop `attribute_staged` from naming the record in the
  report (`restore.rs:813-817`) → 4 fail. No tautology and no parallel re-implementation: every leg
  drives production `reconcile_step` / `reconcile_after_restore`.
- **The source-before-destination claim, all three handoffs.** `sidx:` → `part:` → `inode:` holds in
  both passes (`gc.rs:258` before `:273`; `restore.rs:340` before `:350`/`:360`), and the C(ii)
  fixture really lands the flip and the retirement drain as two batches
  (`staged_protection.rs:1274-1301` asserts the store state after each).
- **Scrub / drain-status isolation (leg F), including the path leg F cannot see.** `reconcile_step`
  runs GC first and short-circuits on `?` (`reconciliation.rs:136-148`), so a GC staged-read fault
  *would* suppress scrub in a combined call — but the deployed loop drives scrub, reconstruction and
  GC in three separate `reconcile_pass` calls (`crates/server/src/custodian.rs:519`, `:533`, `:610`)
  and swallows a GC `Store` error at `:619-623`. No regression reaches scrub or drain status.
- **`STAGED_PAGE` = 512 against real backends.** I expected a fleet-wide read failure on a backend
  whose scan cap is below 512; `page_limit` clamps from above (`crates/traits/src/lib.rs:414-424`),
  so a small cap only shortens the page. `checked_page` also rejects an empty page carrying a
  continuation token (`gc.rs:1190-1203`), closing the silent-truncation hole I looked for in
  `staged_fragments`' two loops.
- **Key-namespace cross-talk.** A `part:<id>:` / `sidx:<id>:` range cannot capture another upload's
  records — `UploadId` is 32 hex characters (`multipart.rs:846`), so no id can extend another. And
  `slot:` / `psum:` records name no chunk (`multipart.rs:2284-2289`), so no staged byte falls outside
  the two classes the patch reads.
- **The expired-pending interaction.** A staged-protected fragment now `continue`s before the
  `still_held` arm (`gc.rs:331-337` vs `:389-393`), so a chunk with both an expired `pending:` lease
  and a staged record could have its lease entry retired while protected fragments survive. It is
  unreachable today: owned staging entries are disjoint from `pending:` by construction
  (`multipart.rs:1139-1142`). Worth remembering if a later slice ever gives a staged chunk a
  `pending:` lease; not a defect in this diff.

## On the verdict

I found nothing unwarranted in `check-gates.json`. The one loose phrase — C4-verify's
"26 test(s) ran red", where 24 ran red and 2 are guards green on base — is the gate's own wording and
was already settled at the iteration-5 sign-off; I am noting it only so it is not re-litigated as a
finding. I did not re-run the madsim DST leg myself; `gate-logs/C4-ci.log:3506` and `:3517` show both
new properties green, and the leg is registered in the campaign and in `REGRESSION_SEEDS`
(`crates/dst/tests/custodian.rs:3138-3148`, `:3183`).

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [x] T5 Judgment — Confirm affected-path prior art across merged and closed/rejected work before publish — this target contains one commit and no remotes, so overlap cannot be mechanically ruled out here and could duplicate or conflict with existing work (`brief.md:210`).
- [x] Validation — fitness-to-purpose — Approve the staged-protection and operator-recovery behavior for production GC/restore use — automated evidence establishes mechanics, but operational fitness and availability impact remain the sign-off decision (`docs/design/architecture/06-runtime-view.md:78`).
- [x] `docs/design/architecture/m4-first-deployment-blueprint.md:610-612` tells the
- [x] `crates/custodian/src/gc.rs:906-909`: `object_name`'s contract still reads
- [x] The re-plan is not supported by the supplied tracker record. `notes.json` still titles #803 “staged protection class in the shared reference set” and its Scope says “the staged protection class in the shared reference set”; it has no comments recording the claimed 2026-09-16 re-plan or sign-off. The brief instead says “Do not re-attempt the shared-builder placement” (`brief.md:189-206`) and adds separate GC/restore reads plus CLI/runbook work (`brief.md:130-159`). Revise the brief to match the tracker-authorized change, or explicitly surface this authorization/scope conflict for adjudication rather than presenting the replacement design as recorded fact.
- [x] Success leg C(ii) tests the wrong publication handoff. It says one atomic batch writes the committed inode and removes the `part:` record (`brief.md:36-43`), but the target contract says the root flip moves protection to the inode and installs `retire:records:{parts}`, “whose drain then deletes those part records” (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:793-800`); the flip atomically commits the inode and `Completed` session (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:941-965`), while the record-mode obligation is explicitly the later deleter (`crates/core/src/multipart.rs:3144-3150`, `crates/core/src/multipart.rs:3424-3432`). A double that collapses flip and drain into a nonexistent transaction does not exercise the real three-event window; revise C and G to schedule publication and retirement-drain deletion separately.
- [x] The requested operator-verdict change has no red success check. Scope requires the NEEDS-HUMAN paragraph to name staged records beside committed objects (`brief.md:142-150`), but current code hard-codes “committed object(s),” “chunk maps,” and `action=unresolvable-chunk-map` for every `RestoreReport::unresolvable` entry (`crates/server/src/cli.rs:1318-1341`). The existing verdict test seeds only `inode:` names (`crates/server/src/cli.rs:2893-2899`), and legs A–G never assert CLI output, so `cargo xtask ci` can stay green if this semantic change is omitted or staged keys are mislabeled. Add a red assertion using `mpu:`/`part:`/`sidx:` report names and pin the required text and exit verdict.
- [x] The fail-closed criterion leaves a key-validation hole. Scope says a staged record that cannot be read makes GC/restore incomplete (`brief.md:130-143`), but E(i) covers a bad `part:` value and malformed `sidx:`/`mpu:` keys, not a malformed key inside a session's `part:<id>:` range (`brief.md:48-59`). On the target, `part:` key validation is a separate `parse_part_key` operation (`crates/core/src/multipart.rs:1267-1280`) from value validation by `decode_part_record` (`crates/core/src/multipart.rs:2575-2584`); the proposed tests can therefore pass even if the implementation consumes or silently skips a noncanonical `part:` key without the promised block/audit behavior. Define and test that case.
- [x] size backstop — this slice is behaving oversized: patch is 170 KB (threshold 100 KB); 4 round(s) already spent (threshold 2). Recommend answering `iterate-plan` at sign-off and authoring the split in the re-plan (`pdca split`), rather than `iterate-do`: a slice that is too big yields implementation-shaped findings every round, and splitting authors briefs, which is Plan's beat.

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
