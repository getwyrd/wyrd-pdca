# Build notes — #803 (662.1) staged protection class for GC and restore

Target: getwyrd/wyrd @ `main` = `78f9859`. All `path:line` below are on the patched worktree unless
marked "base". Patch: `patch.diff` (8 files, +2703/−80, 144 KB — see "Size"). Applies cleanly to a
fresh `git archive HEAD` copy (`git apply --check`).

## What changed, and why this shape

**One reader, two consumers, nothing shared with scrub or drain status.** The v4 sign-off rejected
putting the staged read inside `referenced_fragments` (shared by GC, scrub, restore and drain status).
So the staged class is a separate type and a separate reader in `gc.rs`, called only by GC and restore:

- `StagedSet` — `crates/custodian/src/gc.rs:672`. Three members mirroring `ReferenceSet`:
  `placed` (fragment pairs), `held` (chunk → untrusted records), `unresolvable` (key bytes → fault).
  `protection()` returns its own reasons: `staged`, `untrusted-staged-record`, `incomplete-staged-set`.
- `staged_fragments()` — `gc.rs:809`. Pages the `mpu:` listing; for each listed session walks
  `sidx:<id>:` (`read_owned_entry`, `gc.rs:711`) then `part:<id>:` (`read_part`, `gc.rs:739`).
  Every read goes through `staged_page` → `checked_page` (`gc.rs:858`, `gc.rs:1168`) with a page of
  `STAGED_PAGE` = 512 (`gc.rs:147`, compile-time tied to `U_REF` at `gc.rs:151`).
- Store faults are wrapped in `StagedReadFault` (`gc.rs:878`), whose text names the range and whose
  `source()` keeps the store's error reachable for `wyrd_traits::classify`.
- `checked_page` is the old `ledger_page` body generalized (prefix + walk name); `ledger_page`
  (`gc.rs:1148`) now calls it with `"orphan-ledger"`, so the orphan-walk refusal messages are
  byte-identical (existing `gc_ledger_walk.rs:1494-1497` asserts pin them; they pass).

**GC** (`gc.rs:258`): staged read first, attribution emitted immediately (`emit_unresolvable_staged`
`gc.rs:1338`, `emit_untrusted_staged` `gc.rs:1353`), then `referenced_fragments`. Gate:
`referenced.protection(..).or_else(|| staged.protection(..))` (`gc.rs:333`). `Blocked` if either set
is incomplete (`gc.rs:411`).

**Restore** (`restore.rs:340`): staged read first (`// deferred: #805` at `restore.rs:337`),
`attribute_staged` (`restore.rs:349`, `:813`) puts unreadable staged keys into the same
`unreadable` BTreeSet as `inode:` keys, so they land in `RestoreReport::unresolvable` in key order
(`inode:` < `mpu:` < `part:` < `sidx:`, which is also leg I's example order) and flip `incomplete`.
Held records are only audited (`// deferred: #664` at `restore.rs:819`). Mark gate adds
`|| staged.protects(..)` (`restore.rs:438`). Audit action for unreadable staged records is exactly
`unresolvable-staged-record` in both `restore.rs:989` and `gc.rs:1338`, as leg I requires.

**CLI** (`crates/server/src/cli.rs:1263`, `:1338`): "record(s) UNREADABLE" / "record(s) could not be
READ", names both audit actions, says "staged multipart record". Existing test phrase moved
(`cli.rs:3017`); new leg-I test `cli.rs:3032`.

**Docs**: one paragraph in `docs/design/architecture/06-runtime-view.md:78` (§6.7 step 2); runbook
UNREADABLE entry `docs/design/architecture/m4-first-deployment-blueprint.md:609`. One doc line in
`crates/custodian/src/reconciliation.rs:27` (`Reconciled::Blocked` said only committed objects could
block; GC now also blocks on a staged record — left stale it would be a docs-currency finding).

**DST** (`crates/dst/tests/custodian.rs:2562`): property 13 — `staged_handoffs_under_gc` (`:2799`),
campaign leg (`:2997`), coverage leg (`:3010`), registered at `:3139`, `:3145` and in the
regression-seed loop (`:3183`).

## Judgment calls the human should know about

1. **Session value is never decoded.** Protection does not depend on state (the brief: "whatever its
   state"), so only the `mpu:` key is parsed. Decoding the value would add a failure class the brief
   does not list (a damaged session value blocking GC fleet-wide) while protecting the same bytes.
2. **Staged placements are held to exact length, empty included.** The committed classifier treats
   an empty placement as valid (identity fallback for pre-M3 records). No staged record is written
   with an empty placement (`0016:828`; 0016 X65 `:2594` says wrong length → malformed), so holding
   the chunk whole only keeps more. I apply this to both `sidx:` staged placements and `part:` chunk
   placements. Leg E(ii) tests the `part:` case too (an extra test the brief did not list).
3. **Paged reads, not `scan`.** Leg D (more sessions than the cap) makes `scan("mpu:")` fail, and a
   backend whose configured cap is below a range size would fail GC and restore on every pass.
   Cost, stated: for ranges under 512 records a page read is one round trip, same as `scan`. At the
   shipped maximum (46 sessions × (5 owned pages + 20 part pages) + 1) it is ~1,151 round trips per
   pass versus 93 scans; a range that is an exact multiple of 512 costs one extra empty page. Only
   GC and restore pay it.
4. **Records of an unlisted session are not read** (no per-session range reaches them without a
   namespace scan). Documented at `gc.rs:809` docs.
5. **Restore held (untrusted) records don't set `needs_human()`** — #664's, marked.
6. **New metric counters** (no doc lists metric names, so no docs-currency obligation):
   `gc_unresolvable_staged_records`, `gc_untrusted_staged_records`,
   `restore_unresolvable_staged_records`, `restore_untrusted_staged_records`.
7. `RestoreReport::needs_human` doc said "three findings" but the code checks four; I fixed that
   sentence while adding staged records to it (`restore.rs`, docs only).
8. Existing DST properties 11/12 now see one extra `mpu:` page read per pass (one more simulated hop).
   Both still pass their coverage legs over the gate's 50 seeds.

### Rejected alternatives (with cost)

- **Shared-builder placement (v4)** — rejected by the brief; mutation M6 below shows leg F catches it.
- **A `staged` member on `ReferenceSet`, filled only by GC/restore** — same code volume (≈0 lines
  saved), but scrub and drain status would then hold an always-empty staged member that a later
  consumer could read as "nothing staged". A separate type makes that impossible to do by accident.
- **Callback sink so the builder emits per record before the next read** — would add a closure
  parameter to a shared reader; the only extra coverage is a store fault in a later session's range
  of the same build, where the pass fails anyway. `referenced_fragments` already collects-then-emits
  within one build, so I matched it.

## Red → green evidence

Runner: targeted checks used time-bounded `cargo test` (each wrapped in `timeout`) in the worktree;
the full verification ran through the project's runner, `PDCA_WORKTREE=… ./engine/xtask.sh ci`
(`cargo xtask ci`: typos, docs lint + render, guards, fmt, clippy, build, workspace tests,
machete, deny, conformance, statics, deploy guard, DST clippy, DST tests at 50 seeds).
**Final `cargo xtask ci`: "all checks passed", exit 0**, with all new tests present in its log.
`typos` (1.48.0) and the docs renderer deps were installed, so the prose gates really ran.

### `crates/custodian/tests/staged_protection.rs` against base production (fix reverted, final test)

17 tests ran. **15 failed, all by assertion** (no compile failure — the file names no symbol the fix
adds). 2 passed: D and F, the guards the brief says are green on base.

| Test | Leg | Base failure (assertion message, abridged) |
|---|---|---|
| `gc_keeps_every_staged_fragment_in_every_session_state` | A | GC reclaimed the committed part fragment of a Open session |
| `restore_marks_no_staged_fragment_and_gc_then_keeps_them` | B | the post-restore pass marked the committed part fragment … stranded |
| `a_part_commit_between_its_two_reads_leaves_the_chunk_protected` | C(i) | … got the chunk reclaimed |
| `a_publication_flipped_and_drained_between_the_reads_leaves_the_chunk_protected` | C(ii) s1 | … got the published chunk reclaimed |
| `a_publication_flipped_between_the_reads_and_drained_after_leaves_the_chunk_protected` | C(ii) s2 | … got the published chunk reclaimed |
| `an_undecodable_part_record_withholds_both_passes` | E(i) | restore marked … while the staged record … could not be read (stranded_marked: 3) |
| `a_part_key_the_parser_rejects_withholds_both_passes` | E(i) | same |
| `an_owned_key_naming_no_chunk_withholds_both_passes` | E(i) | same |
| `a_session_key_naming_no_upload_withholds_both_passes` | E(i) | same |
| `an_owned_entry_with_a_wrong_length_placement_holds_its_chunk` | E(ii) | restore marked … although the staged record … names its chunk (stranded_marked: 6) |
| `a_part_with_a_wrong_length_placement_holds_its_chunk` | E(ii) | same |
| `an_undecodable_owned_value_holds_its_chunk` | E(ii) | same |
| `a_fault_reading_the_session_listing_fails_both_passes` | E(iii) | `expect_err`: restore returned `Ok(RestoreReport { stranded_marked: 3, … })` |
| `a_fault_reading_a_session_owned_range_fails_both_passes` | E(iii) | same |
| `a_fault_reading_a_session_part_range_fails_both_passes` | E(iii) | same |

With the fix: 17 passed.

### Leg I (`cli.rs` test module) against base `restore_verdict`

Built a copy of `cli.rs` = base production + the patched test module, ran
`cargo test -p wyrd-server --lib restore_verdict`. Both tests failed by assertion:

- `restore_verdict_names_unreadable_staged_records_as_staged`: *the verdict does not say
  "4 record(s) UNREADABLE"*; the printed text reads `4 committed object(s) UNREADABLE …` and
  `NEEDS-HUMAN — 4 committed object(s) could not be READ: inode:7, mpu:0123…, part:0123…:000001,
  sidx:0123…:000001:9`.
- `restore_verdict_names_the_blocking_records_and_counts_the_ones_it_cannot_fit` (phrase moved):
  base prints `23 committed object(s) could not be READ`.

With the fix restored: all 4 `restore_` tests in the server lib pass.

### Leg G (DST) against base production

`gc.rs`/`restore.rs` reverted to base, `RUSTFLAGS=--cfg madsim MADSIM_TEST_NUM=10`: both new
properties failed by assertion — "GC pass 1 reclaimed the moving chunk's fragment" (the base pass
reads only `inode:`, `gc:orphan-cursor`, `orphan:`). With the fix: both pass (10 seeds locally, 50 in
the gate).

### Mutation checks (production broken on purpose, restored after each; worktree verified identical to `patch.diff` afterwards)

| Mutation | What it models | Caught by |
|---|---|---|
| M1 read `part:` before `sidx:` | destination-first part commit | C(i); DST campaign + coverage (reclaim assertion) |
| M2 read `inode:` before staged in GC | destination-first publication | C(ii) schedule 1; DST campaign + coverage (reclaim assertion) |
| M3 drop the `StagedReadFault` wrapper | fault not named | all 3 E(iii) |
| M4 undecodable owned value → unresolvable | hold turned into fleet-wide block | E(ii) undecodable-owned |
| M5 remove restore's staged gate | "a filter inside GC alone" (brief SELF-TEST) | B + all 3 E(ii) |
| M6 staged read inside `referenced_fragments` | v4 shared-builder design | F |
| M7 liberal staged placement | identity-fill a truncated placement | both wrong-length E(ii) |
| M8 protect `Open` sessions only | 0016's narrower count | A, B, both C(ii) |
| M9 / M10 held record not audited (restore / GC) | silent hold | all 3 E(ii), at the audit assertion |

The DST first version (1 ms reply hop) caught M1 but, for M2, only failed its coverage check: the
flip and drain could not both land inside one read window. I widened the tap's reply hop to 3 ms
(`HANDOFF_REPLY_MILLIS`) and added a `[d, d, 0]` sweep plus a "flip and drain in one window"
coverage assertion; after that M2 fails the reclaim assertion itself, in both DST tests.

## Refuting my own tests

- **(a) Genuine red?** Yes. With production reverted to base the new file fails 15 of 17 tests by
  assertion (table above; D and F are the brief's guards). Leg I fails by assertion against base's
  verdict text. Leg G fails by assertion against base production. Nine targeted mutations each fail
  the leg aimed at them.
- **(b) Production path?** Yes. Every leg calls `wyrd_custodian::{reconcile_step,
  reconcile_after_restore, reconciliation_status}` and the real `restore_verdict`; only the stores
  are doubles. The metadata double pages with the seam's own `page_limit`/`page_start`/`page_cursor`
  and refuses an over-cap `scan` with `ScanCapExceeded`; the DST runs over `SimTikvMetadataStore`.
- **(c) Fixture includes the fault?** Yes. Staged fragments carry `orphan:` marks past grace for GC
  legs (unmarked GC keeps anything) and are unreferenced for restore legs; every protection leg has a
  control that the pass is shown to reclaim or mark; C's hooks and G's writer assert their batches
  actually landed during the pass; E(iii) asserts the faulted read was issued; E(ii) seeds a fragment
  index the truncated placement no longer names; D asserts its fixture really overflows a global scan.

## Size

144 KB total: test file 60 KB, `gc.rs` 28 KB, DST 23 KB, `restore.rs` 16 KB, `cli.rs` 9 KB, docs
7 KB, `reconciliation.rs` 1 KB. Most production bytes are doc comments in the repo's existing style.
The brief records that the human accepted an oversize patch at this re-plan.

## Environment

No missing external dependency: `typos` and the docs renderer (`markdown_it`, `yaml`) are present
and ran inside `cargo xtask ci`. Nothing here needs a display or live service. No NEEDS-HUMAN item
from the build.

`cargo doc` is not part of the gate and already fails on base (private-item links in `backfill.rs`,
`gc.rs:23-28`, redundant links in `rebalance.rs`/`reconstruction.rs`, unresolved links at
`cli.rs:92`, `:397`). With those lints allowed, the custodian crate's docs have no broken intra-doc
links, and no new ones appear in `cli.rs`.
