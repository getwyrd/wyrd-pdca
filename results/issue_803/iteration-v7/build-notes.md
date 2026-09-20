# Build notes — #803 (662.1) staged protection class for GC and restore — iteration 7

Target: getwyrd/wyrd @ `main` = `78f9859`. All `path:line` below are on the patched worktree unless
marked "base". Patch: `patch.diff` (8 files, +3098/−80). It applies cleanly to base
(`git apply --cached --check` against a temporary index read from `78f9859`), and
`run-verify.sh --classify` sees `ADDED_TEST crates/custodian/tests/staged_protection.rs`.

## What this iteration did

The iteration-6 sign-off asked for five fixes, with the brief otherwise unchanged. I started from
the iteration-6 patch (`iteration-v6/patch.diff`, which applied cleanly to base) and changed only
tests, one doc sentence and some comment wrapping. **No production code changed this iteration:**
`gc.rs`, `restore.rs`, `reconciliation.rs` and the production part of `cli.rs` are byte-identical to
iteration 6.

For each finding I first broke production the way the finding describes and ran the iteration-6
tests, to confirm the gap was real, then fixed the test and ran the same break again.

### Fix 1 — "hold the chunk whole" is now pinned (adversary finding 1)

- **Confirmed the gap.** Mutant M14 is the adversary's survivor: remove the `held` arm of
  `StagedSet::protection` (`crates/custodian/src/gc.rs:693-694`) and, in `place`, fill a
  wrong-length placement by identity before holding it (`gc.rs:766-772`). Against the iteration-6
  tests, 21 of 22 passed. Only `an_undecodable_owned_value_holds_its_chunk` failed, so the two
  wrong-length tests did not catch it.
- **Why:** the harness put the held chunk's fragments at `(0,0) (1,1) (2,2)`, which is exactly where
  `ChunkRef::placed_dserver`'s identity fallback puts fragment `i` (server `i`,
  `crates/core/src/metadata.rs:164-169`).
- **Fix:** `crates/custodian/tests/staged_protection.rs:1561`. Fragment 2 of the held chunk now sits
  on server 3, which neither the short placement nor the fallback names. The comment above it says
  why.
- **Result:** M14 now fails all three E(ii) tests (plus the new over-long test below), at the
  restore half's mark assertion (`staged_protection.rs:1570`), e.g. *"the post-restore pass marked
  FragmentId { chunk: 3866, index: 2 } on server 3 although the staged record sidx:…:000003:3866
  names its chunk"*.

### Same class, found while checking fix 1: an over-long placement was never tested

- Mutant M17 changes `==` to `>=` at `gc.rs:756`, so a placement LONGER than the scheme's fragment
  count is trusted instead of held. It passed every iteration-6 test, and every test after fixes 1
  and 2. The brief's E(ii) says "a staged placement of the wrong length", but only short placements
  were seeded.
- Added `a_part_with_an_over_long_placement_holds_its_chunk` (`staged_protection.rs:1639`): a part
  whose chunk's placement names four D servers (`[0, 1, 2, 2]`) for RS(2,1). Both decoders accept
  that length (`crates/core/src/multipart.rs:2542-2546`, `:3530-3538`).
- M17 now fails it; base fails it by assertion. 13 lines. This goes one step past the five listed
  fixes. I added it because it is the same defect as fix 1 (a wrong-length staged placement treated
  as trusted) in the other direction.

### Fix 2 — the audit name for an unreadable staged record is now asserted (adversary finding 2)

- **Confirmed the gap, in both passes.** M15 empties GC's `emit_unresolvable_staged`
  (`gc.rs:1344`, called at `:263`): iteration-6 tests 22 of 22 green. I checked restore's twin too:
  M16 empties `emit_unresolvable_staged` in `crates/custodian/src/restore.rs:989` (called from
  `attribute_staged`, `:349`): also 22 of 22 green.
- **Fix:** the E(i) harness now asserts `named_on_audit_seam(RESTORE_AUDIT, name)`
  (`staged_protection.rs:1451`) after the restore half and `named_on_audit_seam(GC_AUDIT, name)`
  (`:1475`) after the GC half, as E(ii) already did. The harness doc says why this matters for GC:
  the loop has no report, so the audit line is the only place an operator learns which record is
  holding every reclaim back.
- The check matches the record key, not the new action string: the brief says the new test file
  names no string this slice adds (`brief.md:114-116`).
- **Result:** M15 fails all four E(i) tests (*"GC withheld every reclaim over the unreadable staged
  record mpu:not-an-upload-id without naming it on its audit seam"*); M16 fails all four at the
  restore check.

### Fix 3 — the doc sentence no longer claims the committed read is paged

- `docs/design/architecture/06-runtime-view.md:78`: removed "a page at a time and never one listing
  of a whole namespace" from the sentence that also covers the committed objects, and added a
  separate sentence limited to upload records: *"The upload records are read a page at a time — the
  list of uploads, then each upload's own records — never in one listing of a whole namespace."*
  The committed read is still one `meta.scan(b"inode:")` (`gc.rs:549`), and restore reads the
  committed namespace twice.
- I searched the rest of the patch for the same claim (`page at a time`, `whole namespace`,
  `namespace scan`, `paged`). The other hits (the `gc.rs` module doc, `STAGED_PAGE`, the
  `staged_fragments` doc) talk about the staged class only, so nothing else changed.

### Finding 4 — verified: the handoff was already two batches; a test now proves it

The sign-off said "verify, and revise if the finding holds". It does not hold against the build.
The finding came from the plan-advisory review of the brief as it stood before the re-plan, when
C(ii) was one batch.

- **Per-pass tests.** `publication()` (`staged_protection.rs:1107`) builds the root flip
  (`:1129-1138`) as one batch. It requires the `Completing` session and an absent inode and dirent,
  and writes the inode, the dirent and the `Completed` session. It deletes nothing. The drain
  (`:1139`) is a separate batch that deletes only the part record. They are armed as two hooks
  (`:1198-1199` for GC, `:1268-1269` for restore), and each hook applies its batch with its own
  `Meta::apply` call.
- **DST (leg G).** `crates/dst/tests/custodian.rs`: the fence (`:2898`, `Open` → `Completing`), the
  flip (`:2904-2919`: inode, dirent, `Completed`, no delete), `pause(gaps[2])` (`:2921`), then the
  drain as its own commit deleting the part record (`:2922-2926`). The coverage property requires
  one run where the flip and the drain both land between one pass's part read and its `inode:` scan
  (`published_within_one_window`, `:3037`).
- **Both match 0016.** `0016:793-800`: the root flip moves protection to the inode and installs
  `retire:records:{parts}`, "whose drain then deletes those part records". `0016:941-944`: the flip
  moves the session to `Completed` "in the same batch as the inode commit".
- **But no test checked the shape.** I collapsed the fixture (the flip also deletes the part record,
  the drain is empty): the iteration-6 harness stayed 22 of 22 green. So I added a check.
  `Hook::keys_after` (`staged_protection.rs:134`, recorded at `:275`) records the store's keys the
  instant each hook's batch applies. `assert_published_in_two_batches` (`:1154`) asserts that right
  after the flip the store held the inode AND the part record, and right after the drain the inode
  without it. Both publication harnesses call it (`:1222`, `:1299`), in place of the old end-state
  check it covers. With the collapsed fixture, all 5 publication tests now fail at it.
- **Why the DST did not get the same check:** `SimTikvMetadataStore` can only be read through a
  simulated network hop (`crates/dst/tests/support/mod.rs:139-178`, `:190-192`). A read between the
  flip and the drain would move the drain's landing by a hop and could change which windows the
  coverage property reaches. The DST's two commits are plain in the code cited above.

### Finding 5 — verified: the leg I test was already there, is red on base, and pins text and exit status

- `crates/server/src/cli.rs:3032`, `restore_verdict_names_unreadable_staged_records_as_staged`,
  covers both reports the brief names: the mixed one (`inode:7`, `mpu:<id>`, `part:<id>:000001`,
  `sidx:<id>:000001:9`) and the one holding only `part:<id>:000001`. For each it asserts
  `needs_human`, which the command turns into its exit status (`cli.rs:1200-1203` returns
  `ExitCode::FAILURE`). It also asserts `INCOMPLETE`, `N record(s) UNREADABLE`,
  `N record(s) could not be READ`, every name, `staged multipart record`, both audit actions, and
  that neither `committed object(s)` phrase appears. The existing test's count phrase moved at
  `cli.rs:3017`.
- Re-run this iteration against base `restore_verdict` (results below). No change was needed.

### Comment wrapping

The base DST file has no comment line over 100 characters. Iteration 6 added three doc lines at 101
and one assertion-message line at 102; I re-wrapped them (the message string is unchanged, since the
`\` continuation joins to the same text). I added "as a batch of its own" to the DST run's doc, and
reflowed one ragged doc comment in `staged_protection.rs`. The new test file has no line over 100.

## Design (unchanged from iterations 5 and 6)

- `StagedSet` (`gc.rs:672`) is a separate type from `ReferenceSet`, with its own reasons (`staged`,
  `untrusted-staged-record`, `incomplete-staged-set`). `staged_fragments` (`gc.rs:809`, its
  `// deferred: #806` marker at `:810`) pages `mpu:`, then for each session `sidx:<id>:` and then
  `part:<id>:`, through `walk_staged_range` (`:845`) and `checked_page`, in pages of
  `STAGED_PAGE` = 512. Store faults are wrapped in `StagedReadFault`, which names the range and keeps
  the store error as `source()`.
- GC reads the staged class first (`gc.rs:258`), names what it could not read or trust at once
  (`:262-270`), gates with `referenced.protection(..).or_else(|| staged.protection(..))` (`:333`),
  and answers `Blocked` when either set is incomplete (`:411`).
- Restore reads the staged class first (`restore.rs:340`, `// deferred: #805` above it), puts
  unreadable staged keys into `RestoreReport::unresolvable` (`attribute_staged`, `:349`;
  `// deferred: #664` inside it), and gates marks on `staged.protects` (`:438`).
- Scrub and drain status are untouched: they never call `staged_fragments`.
- CLI verdict text: `cli.rs:1263-1266`, `:1338-1347`. Docs: `06-runtime-view.md:78`,
  `m4-first-deployment-blueprint.md:609-622`, one doc line in `reconciliation.rs:27-29`.
- DST leg G: property and coverage property at `crates/dst/tests/custodian.rs:2997`, `:3010`,
  campaign registrations after them, and the regression-seed loop entry.

## Red → green evidence

**Runner.** Full gate through the project wrapper: `PDCA_WORKTREE=/home/eddie/wyrd/wyrd.pdca-wt
engine/xtask.sh ci` (`cargo xtask ci`: typos, `lint_docs.py`, `render_site.py --check` with link
audit, gitlink and unsafe guards, `cargo fmt --check`, clippy, build, workspace tests, machete, three
`cargo deny` runs, statics gate, deploy guard, DST clippy and DST tests under `--cfg madsim`). xtask
has no single-test subcommand, so the quick red runs and the mutations used
`timeout 1500 cargo test -p wyrd-custodian --test staged_protection` and
`timeout 1500 cargo test -p wyrd-server --lib restore_` inside the worktree.

**Gate run 1** (after the fixes, before the comment re-wraps): `xtask ci: all checks passed`,
exit 0. `staged_protection.rs` ran 23 tests, all passed; the DST step ran 18 tests including
`gc_staged_handoffs_never_reclaim_the_chunk` and
`gc_staged_handoffs_reach_between_and_outside_the_reads`, all passed; the three
`restore_verdict` / `restore_needs_human` CLI tests passed.

**Gate run 2** (final tree, after the re-wraps): fmt, clippy and build passed; then the workspace
test step hung in `crates/server/tests/custodian_gc.rs`. Six of its ten tests
(`deployed_role_*` and `deployed_run_loop_refuses_duplicate_endpoints`) sat past 60 seconds with
every thread in a lock wait (`futex_do_wait`) and the machine idle. After more than two minutes I
stopped that test process, so run 2 ended with exit 1 on that step. I could not get a thread dump:
`gdb` cannot attach on this host (ptrace is restricted). This patch does not touch that test or the
server run loop it drives (`crates/server/src/custodian.rs`). Between run 1 and run 2 only comments
changed, and in run 1 the same binary finished all ten tests in 0.17 s.

**Gate run 3** (final tree, unchanged since run 2): `xtask ci: all checks passed`, exit 0.
`custodian_gc.rs` passed again in 0.17 s. `staged_protection.rs` ran 23 tests, all passed; the DST
step ran 18 tests including both `gc_staged_handoffs_*` tests, all passed; the three
`restore_verdict` / `restore_needs_human` CLI tests passed.

So the hang is intermittent and outside this patch. If Check's C4-ci row meets it, the confirm-once
re-run should clear it, but it may be worth a tracker issue against that test binary.

### `staged_protection.rs` against base production (`gc.rs`, `restore.rs`, `reconciliation.rs` at `78f9859`)

**23 tests ran; 21 failed, all by assertion** (no compile error: the file names no symbol this slice
adds); **2 passed: D and F**, the guards the brief says are green on base. With the fix: 23 passed.

| Test | Leg | Base failure (location, message abridged) |
|---|---|---|
| `gc_keeps_every_staged_fragment_in_every_session_state` | A | `:783` GC reclaimed the committed part fragment … of a Open session |
| `gc_keeps_every_staged_fragment_of_an_upload_whose_records_span_pages` | A | `:952` GC reclaimed the committed part fragment (chunk 10769) … |
| `restore_marks_no_staged_fragment_and_gc_then_keeps_them` | B | `:830` the post-restore pass marked the committed part fragment … stranded |
| `restore_marks_no_staged_fragment_of_an_upload_whose_records_span_pages` | B | `:992` the post-restore pass marked the committed part fragment (chunk 11025) … |
| `a_part_commit_between_its_two_reads_leaves_the_chunk_protected` | C(i) | `:1069` … got the chunk reclaimed |
| `a_publication_flipped_and_drained_between_the_reads_leaves_the_chunk_protected` | C(ii) GC | `:1203` … got the published chunk reclaimed |
| `a_publication_flipped_between_the_reads_and_drained_after_leaves_the_chunk_protected` | C(ii) GC | `:1203` same |
| `restore_leaves_unmarked_a_chunk_flipped_and_drained_after_read_2` | C(ii) restore | `:1275` … got the published chunk marked stranded |
| `restore_leaves_unmarked_a_chunk_flipped_and_drained_after_read_1` | C(ii) restore | `:1310` read-order assertion: base log `[scan(inode:), scan(pending:), scan(inode:), scan_page(orphan:)]` |
| `restore_leaves_unmarked_a_chunk_flipped_after_read_1_and_drained_after_read_2` | C(ii) restore | `:1310` same |
| `an_undecodable_part_record_withholds_both_passes` | E(i) | `:1438` restore marked … while the staged record … could not be read |
| `a_part_key_the_parser_rejects_withholds_both_passes` | E(i) | `:1438` same |
| `an_owned_key_naming_no_chunk_withholds_both_passes` | E(i) | `:1438` same |
| `a_session_key_naming_no_upload_withholds_both_passes` | E(i) | `:1438` same |
| `an_owned_entry_with_a_wrong_length_placement_holds_its_chunk` | E(ii) | `:1570` restore marked … although the staged record … names its chunk |
| `a_part_with_a_wrong_length_placement_holds_its_chunk` | E(ii) | `:1570` same |
| `a_part_with_an_over_long_placement_holds_its_chunk` | E(ii) (new) | `:1570` same (`stranded_marked: 6`) |
| `an_undecodable_owned_value_holds_its_chunk` | E(ii) | `:1570` same |
| `a_fault_reading_the_session_listing_fails_both_passes` | E(iii) | `:1684` `expect_err`: base restore returned `Ok(RestoreReport { stranded_marked: 3, … })` |
| `a_fault_reading_a_session_owned_range_fails_both_passes` | E(iii) | `:1684` same |
| `a_fault_reading_a_session_part_range_fails_both_passes` | E(iii) | `:1684` same |

On base, two restore C(ii) schedules fail at the read-order assertion rather than the mark
assertion: base restore reads no upload record, and its second `inode:` scan sees an inode flipped
after the first. That is still a red by assertion. The schedule the protection depends on ("after
read 2") fails at the mark assertion.

### Leg I (`cli.rs` test module) against base `restore_verdict` — re-run this iteration

I rebuilt `cli.rs` as base plus only the test-module hunk of the patch (the three production hunks
left out), and ran `cargo test -p wyrd-server --lib restore_verdict`: **2 ran, 2 failed by
assertion.**

- `restore_verdict_names_unreadable_staged_records_as_staged` (panicked at `cli.rs:3052` of that
  file): *the verdict does not say "4 record(s) UNREADABLE"*. Base prints `4 committed object(s)
  UNREADABLE` and `NEEDS-HUMAN — 4 committed object(s) could not be READ: inode:7, mpu:0123…,
  part:0123…:000001, sidx:0123…:000001:9`.
- `restore_verdict_names_the_blocking_records_and_counts_the_ones_it_cannot_fit` (panicked at
  `:3004`): base prints `23 committed object(s) could not be READ`.

I then restored `cli.rs` from the patched tree; with the fix, all four `restore_` tests pass.

### Leg G (DST) against base production

Not re-run this iteration. Production is unchanged, and the DST property changed only in comments
and one re-wrapped assertion message that joins to the same string. Iteration 6's run against base
(`engine/xtask.sh dst`, 50 seeds): 3 DST tests failed by assertion at the reclaim check — the
campaign, the coverage property, and `committed_regression_seeds_stay_green`. On the fix, gate runs
1 and 3 this iteration ran the DST leg green (run 2 stopped before it).

### Mutation checks this iteration (production or fixture broken on purpose, restored after each; `git status` clean after every restore)

| Break | What it models | Iteration-6 tests | Final tests |
|---|---|---|---|
| M14: no `held` arm (`gc.rs:693-694`) + identity-fill in `place` (`:766-772`) | adversary finding 1 | 21/22 pass (only the undecodable-value test fails) | 4 fail: all three E(ii) tests + the over-long test |
| M15: GC's `emit_unresolvable_staged` emptied (`gc.rs:1344`) | adversary finding 2 | 22/22 pass | 4 fail: all E(i) tests, at the GC audit check |
| M16: restore's `emit_unresolvable_staged` emptied (`restore.rs:989`) | same gap in restore | 22/22 pass | 4 fail: all E(i) tests, at the restore audit check |
| M17: `==` → `>=` at `gc.rs:756` | over-long placement trusted | 22/22 pass | 1 fails: the over-long test |
| Fixture: flip also deletes the part record, drain empty | finding 4, collapsed publication | 22/22 pass | 5 fail: every publication test, at `assert_published_in_two_batches` |

Earlier iterations' mutants (M1-M13: read order, paging, fault wrapping, restore gate, shared
builder placement, held-record audit) target production code that did not change; I did not re-run
them.

## Refuting my own tests

- **(a) Genuine red?** Yes. With `gc.rs`, `restore.rs` and `reconciliation.rs` reverted to base,
  `staged_protection.rs` fails 21 of 23 tests by assertion (table above). The 2 that pass are the
  brief's guards D and F. Leg I fails 2 of 2 by assertion against base's verdict. Each assertion
  added this iteration also goes red on a targeted break that the iteration-6 tests let through
  (M14-M17 and the collapsed fixture).
- **(b) Production path?** Yes. Every test calls the production
  `wyrd_custodian::{reconcile_step, reconcile_after_restore, reconciliation_status}` and the real
  `restore_verdict`; only the stores are doubles. The new `keys_after` record is the metadata
  double observing its own state after a hook's batch; it stands in for no production code.
- **(c) Fixture includes the fault?** Yes, and fix 1 is exactly a case where it did not before: the
  held chunk's fragments sat only where the identity fallback already put them. Now one sits on a
  server only a whole-chunk hold covers, and the over-long test seeds the other wrong length. E(i)
  seeds the damaged record and marks every fragment past grace before GC. The publication fixture
  lands both batches during the pass (hook outcomes asserted) and checks the store after each one.

## Size

160.9 KB total (160,878 bytes; iteration 6: 157,120). The added 3.8 KB is the test changes above,
the doc sentence and the comment re-wraps. The brief records that the human accepted an oversize patch at the re-plan
(`brief.md:152-154`).

## Environment

No missing external dependency: `typos` and the docs renderer both ran inside `cargo xtask ci`
(typos clean, `lint_docs: OK`, `render_site: link audit OK`). Nothing needs a display or a live
service. The target repo has no commit hooks installed or configured (no hooks path, no hook files,
none documented); its commit-time checks are the `cargo xtask ci` steps above, all green in run 3.
No NEEDS-HUMAN item from the build. The one environment event was the intermittent test hang
described under gate run 2.

Process note for the human: `review-rejected.md` records the #806 deferral at `gc.rs:810`. `gc.rs`
did not change this iteration, so the marker `// deferred: #806` is still on that line and the entry
still matches.
