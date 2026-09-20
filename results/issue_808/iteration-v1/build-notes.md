# Build notes — #808 staged-drain-status (child 1 of #664)

## Base

The cycle worktree (`$PDCA_WORKTREE` = `wyrd.pdca-wt-l1`) is at `origin/main` = `97fc2f9`, one
merge ahead of the brief's `f41e9c5`: PR #812 (#804, GC reclaim intent) landed in between. It
touched `crates/custodian/src/gc.rs` (+507/−123) and `docs/design/architecture/06-runtime-view.md`
(+2), so every brief citation into those two files has moved. **All `path:line` below are on
`97fc2f9`** ("base") or on the patched tree ("patched") where noted. The brief's "Conflicts with:
804" is therefore already settled: #804 merged first and this patch applies on top of it
(C4-verify applied it cleanly to `origin/main`).

Moved citations: `StagedSet` is `gc.rs:896` (brief `:672`), `StagedSet::protection` `:914`
(brief `:690`), `staged_fragments` `:1033` (brief `:809`), the `deferred:` marker `:893` (brief
`:669`), the drain sentence `06-runtime-view.md:80` (brief `:78`). `desired_state.rs`,
`rebalance.rs` and `staged_protection.rs` citations are unchanged from the brief.

## What changed, and why

1. **`crates/custodian/src/desired_state.rs` — the fix.** `reconciliation_status` (base `:181-247`,
   patched `:204-312`):
   - reads the staged class (`gc::staged_fragments`, base `gc.rs:1033`) **first** and the committed
     set second (patched `:222-223`), the order GC already uses (base `gc.rs:286`, `:301`). The
     order is required, not a matter of style: a publication moves a chunk's protection from its
     `part:` record to a committed inode, so reading inodes first can see the chunk in neither class
     and answer `Satisfied` over a live object's bytes (0016 `:793-800`, which names
     `reconciliation_status` for exactly this). Pinned by the read-order leg.
   - `genuinely_holds` (base `:191-194`) now also counts `staged.placed` (patched `:230-234`):
     committed-part placements and in-flight owned (`sidx:`) planned placements, as 0016 `:827`
     requires.
   - an unreadable staged record (`StagedSet::unresolvable`) blocks every drain inside the existing
     `PendingUnresolvable { objects }` answer (patched `:270-283`), committed objects first, then
     staged records, each named by `object_name`; it is also emitted on the drain audit seam by a
     new `emit_unresolvable_staged` (patched `:340`) under the action GC and restore already use
     for it (`unresolvable-staged-record`).
   - an untrusted staged record (`StagedSet::held`) blocks every drain inside the existing
     `PendingMalformed { chunks }` answer (patched `:298-311`), merged with committed malformed
     chunks, sorted, de-duplicated.
   - variant docs (`Pending` patched `:85-92`, `PendingMalformed` `:93-112`, `PendingUnresolvable`
     `:113-139`, `Satisfied` `:143-146`), the fn doc (`:189-196`) and the module doc (`:13-17`)
     updated to say what is now counted.
2. **`crates/custodian/src/rebalance.rs` — comments only, no behaviour change.** Module doc
   (patched `:60-65`), `reconcile` doc (`:139-140`), `plan_evacuations` doc (`:242-251`).
   `plan_evacuations` (base `:257-384`) reads `inode:` and nothing else (base `:264`), so it is
   disjoint from the staged class by construction; leg D proves it plans and writes nothing for a
   draining server holding only staged fragments. No real change to `plan_evacuations` was needed.
3. **`crates/custodian/src/gc.rs` — doc only.** Module doc (patched `:65-69`), `StagedSet` doc
   (`:872-883`: the drain query is now the third reader; rebalance reads it not), and the marker
   (base `:893-894` → patched `:899`): `deferred: #663, #664 — …` becomes `deferred: #663 — scrub
   and reconstruction acting on staged bytes.` #664's half is discharged; #663's half stays under
   the number the brief says to leave (#663, since split into #813/#814).
4. **`crates/custodian/tests/staged_protection.rs` — leg F narrowed to scrub.** Its drain-status
   half asserted that the query reads no upload record and answers the same with and without them
   (base `:2151-2204`). This slice makes both false on purpose (with a store fault armed under a
   staged read, the query now returns `Err`), so the drain half is removed and the leg becomes
   `scrub_does_not_read_upload_records` (patched `:2153`); marker (base `:2160`) → `deferred: #663
   — that slice adds upload records to scrub, and owns changing this leg.` (patched `:2151`).
   Knock-on: four unused imports removed (base `:64-68`), `DRAINING` const removed (base `:90-91`),
   the fixture's on-drain object renamed `F_INTACT` on server 3 and its `set_lifecycle` call
   removed, module doc lines `:11-12` and `:34` updated. The scrub comparison itself is unchanged.
5. **`docs/design/architecture/06-runtime-view.md:80`** — "Scrub and the drain-status query read
   committed references only." replaced by what the query now does (reads the staged class in the
   same order, counts it as held, rebalance plans nothing for staged-only servers while the drain
   stays pending, unreadable/untrusted records keep every drain pending, a store fault fails the
   query), ending "Scrub reads committed references only." Rubric: docs currency for an altered
   API operation.
6. **New test `crates/custodian/tests/staged_drain_status.rs`** (10 tests, legs below).

## Alternatives ruled out (with cost)

- **New `ReconciliationStatus` variants for staged blockers** (e.g. `PendingUnresolvableStaged`,
  `PendingUntrustedStaged`) instead of reusing `PendingUnresolvable` / `PendingMalformed`. Cost: 2
  more variants (about 25 lines of code and docs), and, decisively, the test could not name them:
  they do not exist on the base, so the discriminator would not compile on the RED leg and
  C4-verify would report UNVERIFIABLE (`engine/scripts/run-verify.sh`, `_red_verdict`). Leg E
  would shrink to `assert_ne!(status, Satisfied)`, which a wrong-variant bug also passes. Reuse
  keeps the enum's shape (no `match` anywhere in the tree breaks — nothing in `crates/server` or
  `crates/dst` matches it), lets E pin exact answers, and matches the brief's "the way a committed
  map with an untrustworthy placement does". Price paid: the field `objects` now also carries
  `mpu:` / `part:` / `sidx:` keys; its doc says so.
- **Containing a store fault under the staged read** (answer from what could be read) instead of
  propagating `Err`. Rejected: that is an answer from part of the class, the C-1 defect
  (`docs/principles.md` §5), and it differs from how the same function already treats a store
  fault under the committed read (`referenced_fragments(meta).await?`, base `:188`) and how GC
  treats this very read (base `gc.rs:286`). Pinned by E(iii).
- **Rebalance answering something other than `Satisfied`** when a draining server still holds
  staged bytes. Rejected (the brief expects no behaviour change). Cost, concretely: one more
  `staged_fragments` read per rebalance pass (the session listing plus two ranges per session, up
  to `MAX_SESSIONS × U_ref` chunk refs — the budget GC already spends per pass), a mapping of
  staged damage onto `Blocked` in `rebalance::reconcile` (roughly 15-25 lines), and there is no
  `Reconciled` variant that means "waiting on an upload's exit": `Partial` means "bounded window,
  the next pass resumes" (`reconciliation.rs:46-60`), `Blocked` means "incomplete set"
  (`:25-45`). A new variant changes the step API. See leg D for why the current answer does not
  reach an operator as a drain certification.
- **Emitting untrusted (held) staged records on the drain seam.** Not added: the committed twin
  (malformed placements) is not emitted by this function either (base `:234-246`); GC names both
  on its own seam every pass (`emit_malformed`, `emit_untrusted_staged`), and the answer carries
  the chunk ids.

## Leg D: which `Reconciled` the pass returns, and why that is not "the drain is done"

The pass returns **`Ok(Reconciled::Satisfied)`**, asserted in the test
(`staged_drain_status.rs:618`). `rebalance::reconcile` reads the draining set, `plan_evacuations`
scans `inode:` only and finds nothing, `withheld` stays false and no move was attempted, so it
falls to the final `Reconciled::Satisfied` (base `rebalance.rs:202`, patched `:210`).

Why an operator is not told the drain is done:

1. `Reconciled` is one loop's answer about its own pass. Rebalance's docs already said the per-server
   query is the authority (base `rebalance.rs:193-197`); the patch now states that the loop's
   `Satisfied` speaks for the committed chunks it moves, never for staged bytes (module doc
   patched `:60-65`, `reconcile` doc `:139-140`).
2. The operator's drain surface is `reconciliation_status`, the "policy satisfied" moment of
   `0005:351-352` (`desired_state.rs:13-17`). In the same leg, right after the pass, it answers
   `Pending` for that server.
3. Nothing in production shows the pass's value to an operator today: the server's custodian
   runtime never wires rebalance (`reconcile_pass(..., None, clock())` at
   `crates/server/src/custodian.rs:519`, `:533`, `:610`) and discards every pass outcome
   (`Ok(_) => {}` at `:522`, `:536`, `:613`).

For the human: rebalance's older invariant bullets (base `rebalance.rs:43-58`) still say "an
operator reading `Satisfied` is being told the server is safe to decommission". I left them: they
are about committed moves that did not persist, where they stay true. The new paragraph directly
after them limits that reading to committed chunks. If you want rebalance itself to stop saying
`Satisfied` while a drain waits on staged bytes, that is a behaviour change outside this brief and
would be a follow-up.

## The test

`crates/custodian/tests/staged_drain_status.rs`, over in-memory doubles (a `MetadataStore` over a
`BTreeMap` with one armable read fault and one read-triggered writer; a `ChunkStore` per D server):

| Test | Leg | Base (red?) |
|---|---|---|
| `an_in_flight_part_holds_the_drain` (`:541`) | A | red: `Satisfied` ≠ `Pending` |
| `a_committed_part_holds_the_drain` (`:564`) | B | red: `Satisfied` ≠ `Pending` |
| `a_drain_finishes_when_the_uploads_live_elsewhere` (`:587`) | C | green (guard, by design) |
| `rebalance_moves_no_staged_fragment_while_the_drain_is_pending` (`:618`) | D | red on the `Pending` half |
| `an_unreadable_staged_record_blocks_every_drain` (`:719`) | E(i), 4 record shapes | red |
| `unreadable_records_of_both_classes_are_all_named` (`:758`) | E(i), order | red |
| `an_untrusted_staged_record_blocks_every_drain` (`:827`) | E(ii), 3 record shapes | red |
| `untrusted_placements_of_both_classes_are_each_named_once` (`:858`) | E(ii), sort + dedup | red |
| `a_store_fault_under_the_staged_reading_fails_the_query` (`:891`) | E(iii) | red: `Ok(Satisfied)` |
| `a_publication_during_the_query_cannot_hide_a_chunk_from_both_classes` (`:916`) | read order | red |

A and B each put exactly one fragment of an `RS(2, 1)` chunk on the drained server, at index 1 of
`[1, 3, 0]` — not index 0, and not where the identity fallback would put it — named by only one
staged class and no committed map. C seeds both classes and a committed object on servers 0-2
only; it is the leg that kills the `*server != dserver` mutant iteration 1 left alive (confirmed,
below). D puts real, intact v1 fragments of both staged chunks on the draining server's disk, so a
pass that did plan over them would copy them rather than abort on bad bytes. E's untrusted records
name no drained server, so the block is shown to be fleet-wide, not scoped to what a damaged record
names.

**Symbol set.** The brief says the test "may name only base-visible symbols" and lists some. The
list cannot be exhaustive (`mpu_key` takes an `UploadId`), so I read it as "only symbols that
exist on the base", which is what keeps the RED leg compiling (it did: 10 tests ran there).
Beyond the list the test uses, all present on `97fc2f9`: `UploadId`, `PartNumber`, `part_range`,
`MPU_PREFIX`, `decode_session_record` / `decode_part_record` / `decode_owned_entry` (fixture
checks only), `wyrd_core::metadata::{encode, inode_key, ChunkRef, EcScheme, InodeId, InodeRecord,
InodeState}`, `wyrd_custodian::{Custodian, FencedZone, Topology}`,
`wyrd_chunk_format::{encode, FragmentHeader}`, `wyrd_coordination_mem::MemCoordination`,
`wyrd_testkit::test_double_scan_page`, and `tracing` / `tracing-subscriber` for the audit capture
(the pattern of `segmented_map_restore.rs:224-276`). No `Cargo.toml` change.

**Records.** Every staged record is raw JSON: session and part values in the shapes of
`crates/core/tests/multipart_session_records.rs:81-145`; the owned `sidx:` value in
`OwnedEntry::to_pending`'s shape (`crates/core/src/multipart.rs:3670-3676`), which that test file
does not contain. Each seeded value is decoded by the base decoder before a leg relies on it; those
decoders also refuse non-canonical spellings (`require_canonical`, `multipart.rs:3755`).

## Red → green

Run through the configured per-fix gate, `engine/scripts/run-verify.sh` (`PDCA_BUNDLE=results/issue_808
PDCA_LANE=1 WYRD_VERIFY_BASE=origin/main`, lane 1's own `../wyrd-verify-l1` worktree), on the
final `patch.diff`:

- GREEN (fix applied): 10 tests ran, 10 passed.
- RED (every modified production file reverted, the new test kept): 10 tests ran, **9 failed, all
  by assertion** (no compile failure), 1 passed — leg C, the guard the brief says is green on the
  base. Gate verdict: `PASS — red without the fix, green with it (10 test(s) ran red)` (its count
  is tests executed on the RED leg; 9 of them failed).

An earlier run had only 8 red: `untrusted_placements_of_both_classes_are_each_named_once` passed
on the base because its only staged chunk was also a committed one. I added a staged-only untrusted
chunk so it goes red on the base; it still pins the sort and the de-duplication.

## Mutation probes (manual, each reverted; production diff re-checked byte-identical after)

1. `*server == dserver` → `*server != dserver` (iteration 1's survivor): 7 tests fail, leg C among
   them.
2. Read order swapped (committed first): only the read-order leg fails.
3. `chunks.dedup()` removed: `untrusted_placements_of_both_classes_are_each_named_once` fails.
4. `emit_unresolvable_staged` call removed: both unreadable-record tests fail on the audit-seam
   assertion (the captured log still shows the committed emission, so the capture itself works).

## Refuting my own test

- **(a) Genuine red?** Yes. `run-verify.sh` reverted every modified non-test file (`desired_state.rs`,
  `gc.rs`, `rebalance.rs`, `staged_protection.rs`, `06-runtime-view.md`) and kept the new test:
  9 of 10 failed by assertion, e.g. `left: Satisfied, right: Pending` for A, B, D and the
  read-order leg; `left: Ok(Satisfied)` for E(iii).
- **(b) Production path?** Yes. Every leg calls the production `wyrd_custodian::reconciliation_status`;
  leg D calls the production `reconcile_step` with a `RebalanceContext` — the code the server
  runtime calls. Only the storage seams are doubles, as in every custodian test.
- **(c) Fixture includes the fault?** Yes. A/B: the drained server is named only by the staged
  record, which is present; D: the draining server's disk really holds both staged fragments,
  intact; E: the damaged records are present (and the untrusted ones do not name the drained
  server); read order: the publication batch really lands mid-query, asserted
  (`holds(inode)` and `!holds(part)` after the query).

## Costs and scope notes

- Every `reconciliation_status` call now reads the whole staged class (paged: the session listing,
  then two ranges per session). It is a per-poll cost, bounded by the same budget GC's per-pass read
  is (`gc.rs:1018-1025`, `0016:890`).
- The staged class counts sessions in every state, not only `Open` ones (`StagedSet` doc, base
  `gc.rs:865-870`: "covering more only keeps more"). For the drain this errs toward `Pending`.
- The drain's read order is pinned by a deterministic hook test, not DST: `crates/dst/tests/custodian.rs`
  is out of scope per the brief.

## Gates run here

- `cargo fmt --all -- --check`: clean (the new test needed one `cargo fmt` pass, applied).
- `cargo clippy -p wyrd-custodian --all-targets` (workspace lints, warnings are errors): clean.
- `typos` on every changed file: clean. `typos-cli 1.48.0` and the docs renderer (`markdown_it`,
  `yaml`) are installed, so `cargo xtask ci`'s prose gates really ran (the brief's two external
  dependencies are present).
- `cargo xtask ci` via `./engine/xtask.sh ci` on the cycle worktree (success criterion F):
  **exit 0, "xtask ci: all checks passed"**, no failed test anywhere in the log. That run
  included: `typos`, `lint_docs: OK`, `render_site … link audit OK` (99 pages), gitlink and
  unsafe guards, `cargo fmt --check`, workspace clippy and build (all targets), `cargo test
  --workspace` (custodian: `staged_drain_status` 10 passed, `staged_protection` 26 passed,
  `rebalance` 10, `segmented_map_consumers` 8, `segmented_map_restore` 5, `placement_ceiling` 5 —
  every existing caller of `reconciliation_status` still green), cargo-machete, cargo-deny,
  conformance vectors, the statics and deploy guards, and the madsim DST suite (including
  `crates/dst/tests/custodian.rs`, untouched here).
- No commit hooks are installed in the target (`.git/hooks` has only samples, no pre-commit
  config); its formatter and linters are the ones `cargo xtask ci` runs.

## Where the artifacts are

- `patch.diff` — against `origin/main` @ `97fc2f9`, 6 files, +1118 / −83. It matches the cycle
  worktree byte for byte (checked with `git diff | diff -q`). The one change made after the
  C4-verify and `cargo xtask ci` runs above is a single doc-comment line in the test
  (`drain_seam_names`'s doc now mentions its `field` argument); rustfmt, clippy, `typos` and the
  test itself (10 passed) were re-run on the final file.
- The test ships inside `patch.diff` as an added file at the path the brief names,
  `crates/custodian/tests/staged_drain_status.rs`, and is in the cycle worktree there; the
  per-fix gate takes it from the patch. No copy is kept elsewhere in the bundle.
- Nothing was pushed, no PR opened.
