# Build notes — #803 (662.1) staged protection class for GC and restore — iteration 8

Target: getwyrd/wyrd @ `main` = `78f9859`. All `path:line` below are on the patched worktree
(`$PDCA_WORKTREE` = `/home/eddie/wyrd/wyrd.pdca-wt`) unless marked "base". Patch: `patch.diff`
(8 files, +3204/−80). It applies cleanly to base (`git apply --cached --check` against a temporary
index read from `78f9859`). Its one **added** file is
`crates/custodian/tests/staged_protection.rs` — I ran `run-verify.sh`'s own `_added_files` rule
(`engine/scripts/run-verify.sh:144`) over the patch — which is what C4-verify needs to earn its
red (`run-verify.sh:141-144`).

## What this iteration did

The iteration-7 sign-off asked for two fixes and named the rest of its list stale or plan-level.
I started from the iteration-7 patch (`iteration-v7/patch.diff`, which applies cleanly to base)
and changed **only the new test file**. Nothing else moved: `gc.rs`, `restore.rs`,
`reconciliation.rs`, `cli.rs`, the DST file and both docs are byte-identical to iteration 7
(checked by blob hash: `git hash-object crates/custodian/src/gc.rs` = `9c2db3b`, the v7 patch's
post-image; `cli.rs` = `ced153b`, likewise).

For each item I broke production the way the finding describes and ran it **both** ways — against
the iteration-7 test file, to see whether the gap was real, and against this iteration's, to see
that the new assertion closes it. All four runs are recorded below.

### Item 2 — a healthy erasure-coded upload now pins how a staged placement expands

**The gap was real, and I confirmed it first-hand.** I restored the iteration-7 test file from
`iteration-v7/patch.diff` and ran the adversary's two mutants of `StagedSet::place`
(`crates/custodian/src/gc.rs:754-773`) against it: **23 passed, 0 failed, for each of them.**

- **M1** — `index` replaced by `0` in the `FragmentId` at `gc.rs:760-763`. Every staged fragment
  is then recorded as index 0, so for a real Reed-Solomon chunk fragments `1..k+m-1` are in no
  protected set and GC deletes them once they carry a mark past grace. That is the data loss this
  slice exists to stop.
- **M2** — `if false && …` at `gc.rs:756`, so every healthy staged chunk is held whole instead of
  placed. GC and restore would then log a false `untrusted-staged-record` signal for every healthy
  upload on every pass, and a stray copy on a server outside the placement would never be
  reclaimed while the record lives.

**Why they passed:** every positive fixture staged single-fragment `EcScheme::None` chunks, whose
whole placement is one D server at index 0. Index 0 is the only index, and the placement holds the
only server, so zeroing the index or holding the chunk whole changes nothing any assertion could
see. The only RS(2,1) fixtures were E(ii)'s wrong-length ones, which take the `held` path by
design.

**The fix** (`crates/custodian/tests/staged_protection.rs:674-744`, the `ErasureCoded` fixture,
wired into leg A at `:849-850`, `:887-893` and leg B at `:914-915`, `:938-949`, `:967-970`):

- One healthy `Open` session with a committed `part:` record and an owned `sidx:` entry, each
  naming an `RS(2, 1)` chunk — three fragments — at a **full, non-identity** placement
  (`[2, 0, 1]` for the part's chunk, `[1, 2, 0]` for the owned one, `:678-679`). Neither names
  fragment `i` on server `i`, so neither coincides with the identity fallback
  (`crates/core/src/metadata.rs:164-169`) nor with M1's all-index-0 expansion: under M1, four of
  these six fragments are unprotected.
- Every fragment sits on its placed server (`:715-738`) and is marked past grace with the rest of
  the leg, so leg A's existing survival loop and leg B's existing "no mark" loop cover them — the
  two legs keep one assertion shape, and the fixture's `class` string names the record in the
  failure message.
- Beside them a **stray** copy of the part chunk's fragment 0 on server 3, which no staged
  placement names (`:740-742`). Leg A asserts GC reclaims it (`:887-893`), leg B that the
  post-restore pass marks it (`:938-945`), that `stranded_marked` is exactly 2 — the control and
  the stray (`:946-949`) — and that the following GC pass deletes it (`:967-970`). That is the M2
  detector: a chunk held whole protects every fragment carrying its id, the stray included.

**Result.** M1 fails both legs at the survival/mark loop (*"GC reclaimed the erasure-coded
committed part (`part:`) fragment FragmentId { chunk: 2625, index: 1 } of a Open session on server
0"*). M2 fails both at the stray assertion (*"GC kept the stray copy FragmentId { chunk: 2625,
index: 0 } on server 3"*; restore's twin reports `stranded_marked: 1`). Both legs stay red on base
for their original reason (table below).

### Item 6 — the key-validation hole: already closed in iteration 7, and it binds

The sign-off asked for "a test case for a malformed key (not just a malformed value) inside a
session's `part:<id>:` range … assert the pass fails closed the same way E(i) requires". That test
exists — `a_part_key_the_parser_rejects_withholds_both_passes`
(`crates/custodian/tests/staged_protection.rs:1601-1614`), added in **iteration 5** — the first
attempt after the 2026-09-16 re-plan put the case into the brief's E(i) — and carried unchanged
since (it is in `iteration-v5/patch.diff`, `iteration-v6/` and `iteration-v7/`, and in none
before). It seeds `part:<id>:2` — inside
`part_range(id)`, rejected by `parse_part_key`, and asserted to be both — under a value
`decode_part_record` **accepts**, and runs the shared E(i) harness: restore marks nothing and names
the key in `RestoreReport::unresolvable`, GC reclaims nothing and answers `Blocked`, and each pass
names the record on its own audit seam.

I did not take that on trust. **M18**: drop the key check from `StagedSet::read_part`
(`gc.rs:740`, `parse_part_key(key).and_then(|_| decode_part_record(value))` → `decode_part_record(value)`,
plus the now-unused import). Result: **exactly that test fails**, 22 of 23 pass —

> the post-restore pass marked FragmentId { chunk: 3619, index: 0 } on server 3 while the staged
> record part:e2e2…e2:2 could not be read — the fragments it owns cannot be told from strays

So key and value validation are separately covered, and no new test was needed. Nothing in the
patch changed for this item.

### Items I did not act on, and why

The same sign-off marked them stale or plan-level, and the brief is unchanged on all of them:

- **Item 1** (fitness-to-purpose: one unreadable upload record stalls GC and restore fleet-wide) —
  accepted at the 2026-09-16 re-plan sign-off and recorded in the brief's Scope ("The human
  accepted at sign-off … keep that", `brief.md:167-169`); left open for the human at this sign-off.
- **Item 3** (tracker record vs brief) — the brief's own Plan-review response (1) records it as the
  human's call at hand-off.
- **Item 4** (leg C(ii) handoff mechanics) — verified in iteration 7 against `0016:793-800`,
  `:941-944` and `multipart.rs:3144-3150`, `:3424-3432`: the fixture already commits the root flip
  and the retirement drain as **two** batches, and `assert_published_in_two_batches`
  (`staged_protection.rs:1260`) now pins that shape — a collapsed fixture fails all five
  publication tests.
- **Item 5** (CLI verdict red check) — `restore_verdict_names_unreadable_staged_records_as_staged`
  (`crates/server/src/cli.rs:3032`) covers it; I re-ran its red against base this iteration
  (below), so the record is first-hand for this attempt rather than carried over.

## Design (unchanged since iteration 5; production untouched for two iterations)

- `StagedSet` (`gc.rs:672`) is a separate type from `ReferenceSet`, with its own reasons
  (`staged`, `untrusted-staged-record`, `incomplete-staged-set`). `staged_fragments` (`gc.rs:809`,
  its `// deferred: #806` marker at `:810`) pages `mpu:`, then per session `sidx:<id>:` and then
  `part:<id>:`, through `walk_staged_range` (`:845`) and `checked_page`, in pages of
  `STAGED_PAGE` = 512. Store faults are wrapped in `StagedReadFault`, which names the range and
  keeps the store error as `source()`.
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

**Runner.** The full gate through the project wrapper: `PDCA_WORKTREE=/home/eddie/wyrd/wyrd.pdca-wt
engine/xtask.sh ci` (`cargo xtask ci`: typos, `lint_docs.py`, `render_site.py --check` with link
audit, gitlink and unsafe guards, `cargo fmt --check`, clippy, build, workspace tests, machete,
three `cargo deny` runs, statics gate, deploy guard, DST clippy and the DST suite under
`--cfg madsim`). xtask has no single-test subcommand, so the quick red runs and the mutations used
`timeout 1500 cargo test -p wyrd-custodian --test staged_protection` and
`timeout 1500 cargo test -p wyrd-server --lib restore_` inside the worktree.

**Gate run 1** (final code, before the two doc-comment word fixes below): `xtask ci: all checks
passed`, exit 0. The DST step ran 18 tests including `gc_staged_handoffs_never_reclaim_the_chunk`
and `gc_staged_handoffs_reach_between_and_outside_the_reads`, all green.

**Gate run 2** (the tree this patch ships): `xtask ci: all checks passed`, exit 0 — see the
"Gate runs" note at the end. Between the runs only two words changed ("a ninth session" → "a fifth
session"; the fixture is the fifth, after `seed_every_state`'s four) and one doc line's wording.

**`staged_protection.rs` against base production** (`gc.rs`, `restore.rs`, `reconciliation.rs`
restored to `78f9859`, the new test file kept): **23 tests ran; 21 failed, all by assertion** — no
compile error, since the file names no symbol this slice adds — and **2 passed**: `D` and `F`, the
guards the brief says are green on base. With the fix: 23 passed.

| Test | Leg | Base failure (location, message abridged) |
|---|---|---|
| `gc_keeps_every_staged_fragment_in_every_session_state` | A | `:865` GC reclaimed the committed part fragment … of a Open session |
| `gc_keeps_every_staged_fragment_of_an_upload_whose_records_span_pages` | A | `:1058` GC reclaimed the committed part fragment (chunk 10769) … |
| `restore_marks_no_staged_fragment_and_gc_then_keeps_them` | B | `:924` the post-restore pass marked the committed part fragment … stranded |
| `restore_marks_no_staged_fragment_of_an_upload_whose_records_span_pages` | B | `:1098` the post-restore pass marked the committed part fragment (chunk 11025) … |
| `a_part_commit_between_its_two_reads_leaves_the_chunk_protected` | C(i) | `:1175` … got the chunk reclaimed |
| `a_publication_flipped_and_drained_between_the_reads_leaves_the_chunk_protected` | C(ii) GC | `:1309` … got the published chunk reclaimed |
| `a_publication_flipped_between_the_reads_and_drained_after_leaves_the_chunk_protected` | C(ii) GC | `:1309` same |
| `restore_leaves_unmarked_a_chunk_flipped_and_drained_after_read_2` | C(ii) restore | `:1381` … got the published chunk marked stranded |
| `restore_leaves_unmarked_a_chunk_flipped_and_drained_after_read_1` | C(ii) restore | `:1416` read-order assertion: base log `[scan(inode:), scan(pending:), scan(inode:), scan_page(orphan:)]` |
| `restore_leaves_unmarked_a_chunk_flipped_after_read_1_and_drained_after_read_2` | C(ii) restore | `:1416` same |
| `an_undecodable_part_record_withholds_both_passes` | E(i) | `:1544` restore marked … while the staged record … could not be read |
| `a_part_key_the_parser_rejects_withholds_both_passes` | E(i) | `:1544` same |
| `an_owned_key_naming_no_chunk_withholds_both_passes` | E(i) | `:1544` same |
| `a_session_key_naming_no_upload_withholds_both_passes` | E(i) | `:1544` same |
| `an_owned_entry_with_a_wrong_length_placement_holds_its_chunk` | E(ii) | `:1676` restore marked … although the staged record … names its chunk |
| `a_part_with_a_wrong_length_placement_holds_its_chunk` | E(ii) | `:1676` same |
| `a_part_with_an_over_long_placement_holds_its_chunk` | E(ii) | `:1676` same |
| `an_undecodable_owned_value_holds_its_chunk` | E(ii) | `:1676` same |
| `a_fault_reading_the_session_listing_fails_both_passes` | E(iii) | `:1790` `expect_err`: base restore returned `Ok(RestoreReport { stranded_marked: 3, … })` |
| `a_fault_reading_a_session_owned_range_fails_both_passes` | E(iii) | `:1790` same |
| `a_fault_reading_a_session_part_range_fails_both_passes` | E(iii) | `:1790` same |

As in earlier iterations, two of the restore C(ii) schedules fail on base at the read-order
assertion rather than the mark assertion (base restore reads no upload record at all, and its
second `inode:` scan sees an inode flipped after the first). Both are still reds by assertion; the
schedule the protection depends on ("after read 2") fails at the mark assertion.

The two legs this iteration touched still fail on base first at their **original** assertion — the
single-fragment part record of an `Open` session — not at the new erasure-coded one, because the
loop reaches the four-state fixture's fragments first. Both fixtures are reclaimed/marked on base;
the message quoted above is simply the first.

### Leg I (`cli.rs` test module) against base `restore_verdict` — re-run this iteration

I rebuilt `cli.rs` as base plus **only** the test-module hunk of the patch (the three production
hunks left out) and ran `cargo test -p wyrd-server --lib restore_`: 4 tests matched, and **both
leg-I tests failed by assertion**. The other two passed, as they should: neither pins the staged
wording (`restore_needs_human_agrees_with_every_paragraph_it_prints` checks the verdict's internal
agreement, and the third is flag parsing).

- `restore_verdict_names_unreadable_staged_records_as_staged` — *the verdict does not say
  "4 record(s) UNREADABLE"*. Base prints `4 committed object(s) UNREADABLE` and `NEEDS-HUMAN — 4
  committed object(s) could not be READ: inode:7, mpu:0123…, part:0123…:000001,
  sidx:0123…:000001:9`, and names only `action=unresolvable-chunk-map`.
- `restore_verdict_names_the_blocking_records_and_counts_the_ones_it_cannot_fit` — *…and the total
  is the report's own, not the number that fitted*: base prints `23 committed object(s) could not
  be READ`.

I then restored the patched `cli.rs` (blob `ced153b`, the v7 post-image) and all four `restore_`
tests pass.

### Leg G (DST) against base production

Not re-run this iteration: production and the DST file are byte-identical to iteration 7, and this
iteration's diff does not reach them. Iteration 6's run against base (`engine/xtask.sh dst`, 50
seeds) had 3 DST tests failing by assertion at the reclaim check — the campaign, the coverage
property and `committed_regression_seeds_stay_green`. On the fix, both gate runs this iteration ran
the DST leg green.

### Mutation checks this iteration (production broken on purpose, restored after each; `git hash-object` verified identical after every restore)

| Break | What it models | Iteration-7 tests (run here) | This iteration's tests |
|---|---|---|---|
| M1: `index` → `0` in `place`'s `FragmentId` (`gc.rs:760-763`) | staged placement expanded without the fragment index | 23 pass, 0 fail | 2 fail: legs A and B, at the survival / no-mark loop |
| M2: `if false && …` (`gc.rs:756`) | every healthy staged chunk held whole instead of placed | 23 pass, 0 fail | 2 fail: legs A and B, at the stray-copy assertion |
| M18: `parse_part_key` dropped from `read_part` (`gc.rs:740`) | key validation skipped, value validation kept | — (same test, unchanged) | 1 fails: `a_part_key_the_parser_rejects_withholds_both_passes` |

Every mutant was reverted by restoring the file from a copy taken before it was applied, and
`git hash-object crates/custodian/src/gc.rs` is `9c2db3b` — the v7 post-image — on the tree this
patch ships from. The regenerated `patch.diff` is byte-identical to the one in the bundle.

Earlier iterations' mutants (M3–M17 and the collapsed-publication fixture: read order, paging,
fault wrapping, the restore gate, the `held` arm, the over-long placement, the audit seam) target
code that has not changed since iteration 7 and tests this iteration did not touch; I did not
re-run them. Their results are in `iteration-v6/build-notes.md` and `iteration-v7/build-notes.md`.

## Refuting my own tests

- **(a) Genuine red?** Yes. With `gc.rs`, `restore.rs` and `reconciliation.rs` reverted to base,
  `staged_protection.rs` fails 21 of 23 tests by assertion (table above); the 2 that pass are the
  brief's guards D and F, which the brief says are green on base. Leg I fails 2 of 2 by assertion
  against base's `restore_verdict`. And each assertion **added this iteration** goes red on a
  targeted break the iteration-7 tests let through: M1 on the survival loop, M2 on the stray.
- **(b) Production path?** Yes. Every leg drives the production entry points —
  `wyrd_custodian::{reconcile_step, reconcile_after_restore, reconciliation_status}` and the real
  `restore_verdict` — through the same `GcContext` / `ScrubContext` the server builds. Only the
  stores are doubles (an in-memory `MetadataStore` and four `ChunkStore`s), and the records are
  raw JSON round-tripped through the **production** decoders before any pass reads them. The
  erasure-coded fixture adds no new seam: it uses the same `part()` / `owned()` helpers, which
  assert the seeded bytes are the decoder's own spelling.
- **(c) Fixture includes the fault?** Yes, and this iteration is precisely a case where it did not
  before. The old positive fixtures could not exhibit a mis-expanded placement, because a
  single-fragment `EcScheme::None` chunk has nothing to mis-expand: the new fixture puts three
  fragments per chunk on servers the identity fallback does not name, marks every one past grace
  (so GC's conservative arm cannot be what saves them), and keeps a stray copy on an unplaced
  server so a blanket hold is visible as a survivor. The control fragment each leg already seeded
  is still reclaimed/marked, so a pass that did nothing still cannot pass for one that protected
  the staged bytes.

## Size

166.0 KB (166,016 bytes; iteration 7: 160,878). The added 5.0 KB is the `ErasureCoded` fixture and
the leg A/B assertions — test code only. The brief records that the human accepted an oversize
patch at the 2026-09-16 re-plan (`brief.md:152-154`), and the iteration-6 and iteration-7 sign-offs
both re-confirmed it ("do not re-split").

## Environment

No missing external dependency: `typos` and the docs renderer both ran inside `cargo xtask ci`
(typos clean, `lint_docs: OK`, `render_site: link audit OK`). Nothing here needs a display or a
live service — the whole test file runs on in-memory doubles under `tokio`, and the DST leg runs
under madsim inside the gate. The target repo has no commit hooks installed or configured (no
`core.hooksPath`, no files in `.git/hooks` beyond the samples, none documented); its commit-time
checks are the `cargo xtask ci` steps above, and `cargo fmt --all -- --check` is clean on the final
tree. No NEEDS-HUMAN item arises from this build.

Process note for the human: `review-rejected.md` records the #806 deferral at `gc.rs:810`. `gc.rs`
has not changed since that entry was written, so the marker `// deferred: #806` is still on that
line and the entry still matches.

### Gate runs

Both runs this iteration finished `xtask ci: all checks passed` with exit 0. Iteration 7 saw one
intermittent hang in `crates/server/tests/custodian_gc.rs` (six `deployed_role_*` tests in a lock
wait, on a tree whose only change since a green run was comments); it did not recur — both gates
ran to the end, and run 2's log shows that binary's 10 tests green in 0.17 s. If Check's C4-ci row meets it, the
confirm-once re-run should clear it; it may still be worth a tracker issue against that test
binary, since nothing in this patch touches it or the server run loop it drives
(`crates/server/src/custodian.rs`).
