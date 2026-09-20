# Build notes — #803 (662.1) staged protection class for GC and restore — iteration 6

Target: getwyrd/wyrd @ `main` = `78f9859`. All `path:line` below are on the patched worktree unless
marked "base". Patch: `patch.diff` (8 files, +3025/−80, 157 KB). It applies cleanly to base
(`git apply --cached --check` against a temporary index read from `78f9859`), and
`run-verify.sh --classify` sees `ADDED_TEST crates/custodian/tests/staged_protection.rs`.

## What this iteration did

The iteration-5 sign-off said the slice had converged and asked for three small fixes, with the
brief otherwise unchanged. I started from the iteration-5 patch (`iteration-v5/patch.diff`, which
applied cleanly to base) and made only those three changes, plus two small test-file tidy-ups
listed under fix 3. Production behaviour did not change: the only production edit is a comment.

### Fix 1 — the blocking review finding (budget-profile preflight) is deferred to #806

- Added the marker `// deferred: #806 — …` as the first lines of `staged_fragments`,
  `crates/custodian/src/gc.rs:810-815`. It names what is missing (`mpuctl` profile check,
  `0016:348`, X99 at `0016:2628`) and states the gap plainly: until #806, the reading's total size
  is bounded only by the profile the gateways admitted sessions under, which this pass does not
  check; each page stays bounded by `STAGED_PAGE`.
- The marker sits at `gc.rs:810`, the same line the finding cites (`review-batch.md`).
- Recorded the decision in the bundle's `review-rejected.md`:
  `crates/custodian/src/gc.rs:810 | BUG | checks its budget profile | Deferred — tracked in
  getwyrd/wyrd#806 …`. I checked it with the review gate's own parser
  (`scripts/review-branch`: `load_rejected` returns the tuple, and `is_rejected` returns `True`
  for the finding text in `review-batch.md`).
- I did not implement the check, as the sign-off asked.

### Fix 2 — paging test for one upload's own records (adversary finding 1)

New in `crates/custodian/tests/staged_protection.rs`:

- Fixture `seed_one_upload_across_pages` (`:865`): one `Open` upload, five `part:` records (parts
  1-5) and five `sidx:` entries (parts 6-10), fragments on servers 0-2, a control on server 3.
  Seeded into a store with scan cap 2, so each of the upload's two ranges is three pages. The
  fixture asserts that a single `scan` of either range fails (the cap bites), as leg D does for
  its own fixture.
- `gc_keeps_every_staged_fragment_of_an_upload_whose_records_span_pages` (`:923`): everything
  marked past grace; one GC pass keeps all ten fragments and their marks, reclaims the control,
  answers `Changed`.
- `restore_marks_no_staged_fragment_of_an_upload_whose_records_span_pages` (`:966`): unmarked;
  restore marks none of the ten, marks the control (`stranded_marked == 1`); a GC pass past grace
  then keeps all ten and reclaims the control.

The restore test runs over the **unmarked** store rather than "all marked past grace": restore
leaves an already-marked fragment alone, so over a marked store the mutant would change nothing
restore does and the test could not catch it. GC's test uses the marked store, as the sign-off
described.

The mutant the sign-off named — "read one page of `walk_staged_range` then return"
(`gc.rs:845-861`) — fails exactly these two tests (M11 below). Before this iteration nothing
caught it: leg A pages the session listing, but each of its sessions has one record per range.

### Fix 3 — restore's staged-before-committed read order is now tested (adversary finding 2)

I added the test rather than cutting the sentence at `restore.rs:239-241`, because the order is
what protects a publication during the pass, and the brief's Scope states the order ("`sidx:`
before `part:` before the `inode:` scan").

- Extracted the publication setup from `publication_during_gc` into a `publication` fixture
  (`:1092`), so GC and restore share it. The two GC C(ii) tests are unchanged in behaviour; I
  re-ran mutant M2 after the move and it still fails GC schedule 1.
- New harness `publication_during_restore` (`:1207`) and three schedules:
  `restore_leaves_unmarked_a_chunk_flipped_and_drained_after_read_2` (`:1281`),
  `…_flipped_and_drained_after_read_1` (`:1291`),
  `…_flipped_after_read_1_and_drained_after_read_2` (`:1301`).
- Each asserts: the published chunk's fragment is not marked; the control is marked and
  `stranded_marked == 1`; both batches landed during the pass; the store ends with the inode and
  without the part record; and the read log shows the last `sidx:<id>:` read before the first
  `part:<id>:` read, and the last `part:<id>:` read before the first `inode:` read.

Why three schedules and a read-order check, not one schedule: restore reads the committed
namespace twice (`referenced_fragments` `restore.rs:350`, `committed_chunks` `restore.rs:360`),
and the mark gate also honours what the second reading saw (`appeared`, `restore.rs:437`). Work
through where a flip plus drain can land (reads in order R1, R2, R3 of {part range, `inode:`,
`inode:`}):

- The chunk is lost only if the part read comes after **both** `inode:` scans and the batches land
  between the last scan and the part read. Landing "after read 2" is that schedule for a
  staged-last pass, so it fails by the mark assertion (M12).
- If the staged read sits **between** the two committed reads, the chunk is still protected (the
  second scan sees the inode). No mark assertion can catch that order, but it contradicts the
  docs ("before either reading"). The read-order assertion catches it (M13).

Tidy-ups in the same file: `Read` now has a hand-written `Debug` (`:114`) that prints keys as text
(`scan_page(part:…:)`) instead of byte arrays, so every read-log failure message is readable; and
four doc lines over 100 columns were re-wrapped. The module header's leg list mentions the new
tests.

## Design (unchanged from iteration 5)

- `StagedSet` (`gc.rs:672`) is a separate type from `ReferenceSet`, with its own reasons
  (`staged`, `untrusted-staged-record`, `incomplete-staged-set`). `staged_fragments` (`gc.rs:809`)
  pages `mpu:`, then per session `sidx:<id>:` (`read_owned_entry` `:711`) and then `part:<id>:`
  (`read_part` `:739`), through `staged_page` (`:864`) → `checked_page` (`:1174`), pages of
  `STAGED_PAGE` = 512 (`:147`, tied to `U_REF` at compile time, `:151`). Store faults are wrapped in
  `StagedReadFault` (`:884`), which names the range and keeps the store error as `source()`.
- GC reads the staged class first (`gc.rs:258`), attributes at once, gates with
  `referenced.protection(..).or_else(|| staged.protection(..))` (`:333`), and answers `Blocked`
  when either set is incomplete (`:411`).
- Restore reads the staged class first (`restore.rs:340`, `// deferred: #805` at `:337`), puts
  unreadable staged keys into `RestoreReport::unresolvable` (`attribute_staged`, `:349`, `:813`;
  `// deferred: #664` at `:819`), and gates marks on `staged.protects` (`:438`).
- Scrub and drain status are untouched: they never call `staged_fragments`.
- CLI verdict text (`crates/server/src/cli.rs:1264`, `:1330-1347`, `:1367`, `:1377`), leg-I test
  `cli.rs:3032`, the moved phrase in the existing test `cli.rs:2990`. Docs:
  `docs/design/architecture/06-runtime-view.md:77-78`,
  `docs/design/architecture/m4-first-deployment-blueprint.md:609-622`, one doc line in
  `crates/custodian/src/reconciliation.rs:28`.
- DST (leg G): property and coverage property at `crates/dst/tests/custodian.rs:2997`, `:3010`,
  campaign registrations at `:3139`, `:3145`, regression-seed loop `:3183`.

Judgment calls carried from iteration 5 (the human saw these at the last sign-off): the session
value is never decoded; staged placements are held to exact length; staged reads are paged, not
`scan`; records of an unlisted session are not read; held records do not set `needs_human()`
(#664's); four new metric counters (`gc_unresolvable_staged_records`,
`gc_untrusted_staged_records`, `restore_unresolvable_staged_records`,
`restore_untrusted_staged_records`).

## Red → green evidence

Runner: the full gate ran through the project wrapper, `PDCA_WORKTREE=… engine/xtask.sh ci`
(`cargo xtask ci`). Steps it ran: typos, `lint_docs.py`, `render_site.py --check` (link audit
OK), gitlink guard, unsafe guard, `cargo fmt --check`, clippy, build, workspace tests, machete,
three `cargo deny` runs, statics gate, deploy guard, DST clippy and DST tests under
`--cfg madsim`. Result: **"xtask ci: all checks passed", exit 0**. The workspace-test step ran all
22 tests of `staged_protection.rs`, and the DST step ran
`gc_staged_handoffs_never_reclaim_the_chunk` and
`gc_staged_handoffs_reach_between_and_outside_the_reads`, all green. The DST base run below went
through `engine/xtask.sh dst`. For the quick red runs and mutations I used `cargo test -p …`
wrapped in `timeout 1500`, because xtask has no single-test subcommand.

### `staged_protection.rs` against base production (`gc.rs`, `restore.rs`, `reconciliation.rs` at `78f9859`)

**22 tests ran; 20 failed, all by assertion** (no compile error — the file names no symbol this
slice adds); 2 passed: D and F, the guards the brief says are green on base. With the fix: 22
passed.

| Test | Leg | Base failure (assertion message, abridged) |
|---|---|---|
| `gc_keeps_every_staged_fragment_in_every_session_state` | A | GC reclaimed the committed part fragment … of a Open session |
| `gc_keeps_every_staged_fragment_of_an_upload_whose_records_span_pages` | A (new) | GC reclaimed the committed part fragment (chunk 10769) … of an upload whose own records span several pages |
| `restore_marks_no_staged_fragment_and_gc_then_keeps_them` | B | the post-restore pass marked the committed part fragment … stranded |
| `restore_marks_no_staged_fragment_of_an_upload_whose_records_span_pages` | B (new) | the post-restore pass marked the committed part fragment (chunk 11025) … (base `stranded_marked: 11`) |
| `a_part_commit_between_its_two_reads_leaves_the_chunk_protected` | C(i) | … got the chunk reclaimed |
| `a_publication_flipped_and_drained_between_the_reads_leaves_the_chunk_protected` | C(ii) GC s1 | … got the published chunk reclaimed |
| `a_publication_flipped_between_the_reads_and_drained_after_leaves_the_chunk_protected` | C(ii) GC s2 | … got the published chunk reclaimed |
| `restore_leaves_unmarked_a_chunk_flipped_and_drained_after_read_2` | C(ii) restore (new) | … got the published chunk marked stranded (base `stranded_marked: 2`) |
| `restore_leaves_unmarked_a_chunk_flipped_and_drained_after_read_1` | C(ii) restore (new) | read-order assertion: base read log is `[scan(inode:), scan(pending:), scan(inode:), scan_page(orphan:)]` |
| `restore_leaves_unmarked_a_chunk_flipped_after_read_1_and_drained_after_read_2` | C(ii) restore (new) | same read-order assertion |
| `an_undecodable_part_record_withholds_both_passes` | E(i) | restore marked … while the staged record … could not be read |
| `a_part_key_the_parser_rejects_withholds_both_passes` | E(i) | same |
| `an_owned_key_naming_no_chunk_withholds_both_passes` | E(i) | same |
| `a_session_key_naming_no_upload_withholds_both_passes` | E(i) | same |
| `an_owned_entry_with_a_wrong_length_placement_holds_its_chunk` | E(ii) | restore marked … although the staged record … names its chunk |
| `a_part_with_a_wrong_length_placement_holds_its_chunk` | E(ii) | same |
| `an_undecodable_owned_value_holds_its_chunk` | E(ii) | same |
| `a_fault_reading_the_session_listing_fails_both_passes` | E(iii) | `expect_err`: base restore returned `Ok(RestoreReport { stranded_marked: 3, … })` |
| `a_fault_reading_a_session_owned_range_fails_both_passes` | E(iii) | same |
| `a_fault_reading_a_session_part_range_fails_both_passes` | E(iii) | same |

On base, two of the three new restore schedules fail at the read-order assertion, not the mark
assertion: base restore reads no upload record, and its second `inode:` scan sees an inode flipped
after the first. That is still a red by assertion, and the schedule the protection depends on
("after read 2") fails by the mark assertion.

### Leg I (`cli.rs` test module) against base `restore_verdict` — re-run this iteration

Reverted only the production hunks of `cli.rs` (kept the test module), ran
`cargo test -p wyrd-server --lib restore_verdict`: **2 ran, 2 failed by assertion.**

- `restore_verdict_names_unreadable_staged_records_as_staged` (patched `cli.rs:3032`; it panicked
  at line 3052 of the reverted copy): *the verdict does not say "4 record(s) UNREADABLE"*; base
  prints `NEEDS-HUMAN — 4 committed object(s) could not be READ: inode:7, mpu:0123…,
  part:0123…:000001, sidx:0123…:000001:9`.
- `restore_verdict_names_the_blocking_records_and_counts_the_ones_it_cannot_fit` (patched
  `cli.rs:2990`, phrase moved; panicked at line 3004 of the reverted copy): base prints
  `23 committed object(s) could not be READ`.

`cli.rs` restored afterwards (its diff checked byte-identical); with the fix both pass.

### Leg G (DST) against base production — re-run this iteration

`gc.rs`, `restore.rs`, `reconciliation.rs` at base, `engine/xtask.sh dst` (50 seeds): **3 DST
tests failed by assertion** at `crates/dst/tests/custodian.rs:2953` — the campaign
(`gc_staged_handoffs_never_reclaim_the_chunk`), the coverage property, and
`committed_regression_seeds_stay_green` (which runs the property). Message: *"GC pass 1 reclaimed
the moving chunk's fragment, which an owned staging entry, a part record or a committed inode named
at every instant of the run — the pass's readings saw it in no class"*, base read log
`[Pass, Read("inode:"), Read("gc:orphan-cursor"), Read("orphan:")]`. The other 15 DST tests passed.

### Mutation checks (production broken on purpose, restored after each; `git diff` compared byte-for-byte with a saved snapshot after every restore)

New this iteration:

| Mutation | What it models | Result |
|---|---|---|
| M11 `walk_staged_range` returns after its first page | the sign-off's named mutant | fails the 2 new paging tests (GC: chunk 10771 reclaimed; restore: `stranded_marked: 7`); all 20 others pass |
| M12 restore's staged read moved after `committed_chunks` | staged class read after both committed readings | fails all 3 new restore tests: "after read 2" by the mark assertion (`stranded_marked: 2`), the other two by the read-order assertion |
| M13 restore's staged read moved between the two committed reads | order the docs rule out, still protection-safe | fails all 3 new restore tests, by the read-order assertion (the chunk stays unmarked, as predicted) |
| M2 (re-run) GC reads `inode:` before the staged class | destination-first publication | still fails `a_publication_flipped_and_drained_between_the_reads_leaves_the_chunk_protected` after the fixture move |

From iteration 5, not re-run (production code and their target tests are unchanged, apart from
the fixture move covered by the M2 re-run): M1 `part:` before `sidx:` (C(i), DST); M3 drop the
`StagedReadFault` wrapper (E(iii)); M4 undecodable owned value → unresolvable (E(ii)); M5 remove
restore's staged gate (B, E(ii)); M6 staged read inside `referenced_fragments` (F); M7 liberal
staged placement (E(ii)); M8 protect `Open` only (A, B, C(ii)); M9/M10 held record not audited
(E(ii)).

## Refuting my own tests

- **(a) Genuine red?** Yes. With `gc.rs`, `restore.rs` and `reconciliation.rs` reverted to base,
  `staged_protection.rs` fails 20 of 22 tests by assertion (table above); the 2 that pass are the
  brief's guards D and F. Leg I fails 2 of 2 by assertion against base's verdict. Leg G fails by
  assertion against base production. The sign-off's named mutant (M11) now fails both paging
  tests, and the two restore read-order mutants (M12, M13) fail the new restore tests.
- **(b) Production path?** Yes. Every test calls the production
  `wyrd_custodian::{reconcile_step, reconcile_after_restore, reconciliation_status}` and the real
  `restore_verdict`; only the stores are doubles. The metadata double pages with the seam's own
  `page_limit` / `page_start` / `page_cursor` and refuses an over-cap `scan` with
  `ScanCapExceeded`, so cap 2 limits pages the way a backend's `with_scan_cap` does. DST runs over
  the simulated store with network hops.
- **(c) Fixture includes the fault?** Yes. The paging fixture holds five records per range under
  cap 2 and checks that a single scan of either range fails. GC legs mark every fragment past
  grace (unmarked, GC keeps anything); restore legs leave staged fragments unmarked (a marked one
  is skipped anyway). Every protection test has a control that the pass is shown to reclaim or
  mark. The publication tests assert both batches landed during the pass and the store ended
  published. E(iii) asserts the faulted read was issued; D asserts its fixture really overflows a
  namespace scan.

## Size

157 KB total: test file 73 KB (was 60 KB; +13 KB for the five new tests, the shared publication
fixture and the `Debug` impl), `gc.rs` 28 KB, DST 23 KB, `restore.rs` 16 KB, `cli.rs` 9 KB, docs
7 KB, `reconciliation.rs` 1 KB. The brief records that the human accepted an oversize patch at the
re-plan (`brief.md:152-154`).

## Environment

No missing external dependency: `typos` and the docs renderer ran inside `cargo xtask ci`. Nothing
needs a display or a live service. No commit hooks are installed or documented in the target repo;
its commit-time checks are the `cargo xtask ci` steps above (fmt, clippy, typos, docs lint), all
green. No NEEDS-HUMAN item from the build.

One process note for the human: the review finding is recorded as deferred in
`review-rejected.md` at `gc.rs:810`. The review gate matches a rejection by exact `file:line`, so
if a later review cites the same finding at a different line, it will need its own entry. The
in-code `// deferred: #806` marker is what the repo's review rubric treats as settled.
