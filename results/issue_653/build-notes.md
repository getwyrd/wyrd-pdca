# Build notes — issue 653: staged segment publication committer (635.6), iteration 3

Target: `getwyrd/wyrd` @ `main` (`12dd082`). Citations are against the cycle worktree on that
base plus this patch.

## What this round is

The sign-off said: tests only. Keep the committer as it is and close five test gaps so each
guard has a red that only it can cause. So this round started from `iteration-v2/patch.diff`
(it applied cleanly to `12dd082`) and changed **only** `crates/core/tests/segmented_publication.rs`.

- `crates/core/src/staged_publication.rs` is byte-identical to iteration 2 (SHA-256
  `c134e28c…`, checked before and after every temporary edit below).
- `crates/core/src/lib.rs`, `crates/dst/tests/segmented_publication.rs` and
  `docs/design/architecture/08-crosscutting-concepts.md` are byte-identical to iteration 2 (the
  patch sections for those four files hash the same in both patches).
- The core test file grew from 1,199 to 1,382 lines: five new test functions, one helper, three
  new rows in the (C) table, and one new assertion block in the wide-ref test. Nothing was
  removed or rewritten.

The design rationale is unchanged and is in `iteration-v1/build-notes.md` and
`iteration-v2/build-notes.md`.

## The five gaps, one by one

| # | Sign-off item | Test added | Production guard it binds |
|---|---|---|---|
| 1a | A flip that deletes an unrelated key without moving the fence → zero-commit `FenceNotMoved` | (C) table row, `crates/core/tests/segmented_publication.rs:911-919` | `staged_publication.rs:847` (only a delete **of the fence** counts as a move) |
| 1b | A flip that deletes the fence succeeds | `c_a_flip_that_deletes_the_fence_record_moves_it`, `:1028` | same, `:847-849` |
| 1c | A caller delete where the byte envelope binds | (C) row, `:920-935`: 50 KB put + two 6 KB deletes at a 60,000-byte budget. Measured: 62,670 bytes with the deletes, 50,652 without | `batch_bytes`, `staged_publication.rs:695-696` |
| 1d | A caller delete where the ops envelope binds | (C) row, `:936-952`: `ops_budget = 10`, flip = 4 root ops + 3 `flip_move` ops + 4 deletes = 11 | `batch_ops`, `staged_publication.rs:702` |
| 2 | Inode pins on both flip branches | `a_the_flip_pins_the_inode_on_both_branches`, `:580`. Supersede after a concurrent PUT replaced generation 1 with generation 2 → `Conflict`, the PUT's record (version 2) kept. Fresh inode 9 at an unbound name while `inode:9` already holds a record → `Conflict`, that record kept, no dirent written. The dirent pin cannot catch either case (the name still names inode 8 in the first; the fresh name is unbound in the second) | `require(inode, encode(prior))` `staged_publication.rs:925`; `require_absent(inode)` `:919` |
| 3 | `read_group` paging over more than `SEGMENT_PAGE_LIMIT` (128) segments | `a_a_map_over_one_page_of_segments_publishes_and_resolves`, `:631`: 130 × 158 chunks → 130 segments, publishes, `resolve_chunk_map` returns the identical list | the paging loop, `staged_publication.rs:989-1011` |
| 4 | `StrandedRow` and `ForeignRow`, one test each | `d_a_durable_row_past_the_plan_refuses_the_publish` `:1060` (a durable `seg:<nonce>:4:00005` beside the 5-segment plan) and `d_a_foreign_key_in_the_groups_range_refuses_the_publish` `:1070` (`seg:<nonce>:4:junk`); shared helper `refused_over_planted_row` `:1047` asserts zero commits, an unchanged store and no root | `staged_publication.rs:1003-1005` and `:997-1002` |
| 5 | Wide-ref test: every `seg:` put within the byte budget | Assertion block in `a_planned_segments_is_the_count_publish_writes_for_wide_refs_too`, `:489-504`: every logged `seg:` value ≤ `MAX_ROOT_VALUE_BYTES` (the `V/2` budget the byte rule closes segments against), and one value per planned segment | `split_segments`, `staged_publication.rs:595` |

Why gap 5 needed this assertion: with `bytes + cost` changed to `bytes - cost`, the byte rule
closes a 408-byte-ref segment at 124 refs instead of 122. Both give 3 segments for 316 refs, so
the existing `planned > ⌈n / 158⌉` check stays green, and the 50,765-byte value is still under
the store's 100,000-byte ceiling, so the publish succeeds. Only a check against the `V/2`
budget itself can see it.

## Demonstrated reds (each a temporary edit to the committer, then restored)

Driven by `$PDCA_SCRATCH/pdca-builder-653-reds3/mutate.py` (last round's script plus nine
new edits). Each run: apply the named edit to `staged_publication.rs`, run
`timeout 900 cargo test -p wyrd-core --test segmented_publication` (the command C4-verify
issues), restore the file, check its SHA-256. Per-edit logs are in that directory.

### New this round (one per gap)

| Edit | Test that goes red | Failure |
|---|---|---|
| `:847` `k == fence_key` → `k != fence_key` (the C5 survivor) | `c_a_flip_that_deletes_the_fence_record_moves_it` and the (C) table | `Err(FenceNotMoved)` on the fence-deleting flip; `expected a refusal, got Committed` on the unrelated delete |
| `:696` `puts + deletes` → `puts - deletes` (the C5 survivor) | (C) table, byte row | `expected a refusal, got Committed` |
| `:702` `puts.len() + deletes.len()` → `-` (the C5 survivor) | (C) table, ops row | `expected a refusal, got Committed` |
| Fresh flip's `require_absent(inode)` (`:919`) removed | `a_the_flip_pins_the_inode_on_both_branches` (`:619`) | `left: Committed, right: Conflict` |
| Supersede flip's `require(inode, encode(prior))` (`:925`) removed | `a_the_flip_pins_the_inode_on_both_branches` (`:608`) | `left: Committed, right: Conflict` |
| `read_group` returns after the first page (`:1008-1010`) | `a_a_map_over_one_page_of_segments_publishes_and_resolves` | `Err(SegmentNotDurable { index: 128 })` |
| Stranded-row check disabled (`:1003`) | `d_a_durable_row_past_the_plan_refuses_the_publish` | `expected a refusal, got Committed` |
| Foreign key skipped instead of refused (`:998-1001` → `continue`) | `d_a_foreign_key_in_the_groups_range_refuses_the_publish` | `expected a refusal, got Committed` |
| `:595` `bytes + cost` → `bytes - cost` (the C5 survivor) | `a_planned_segments_is_the_count_publish_writes_for_wide_refs_too` | `a segment value of 50765 bytes, over 50000` |

Two of these edits first failed to compile (Wyrd denies warnings: an unused `prior`, an
unneeded `mut`). I adjusted the edit to drop the binding / the `mut`, so the reds above are
test failures, not build failures.

### Re-run on the final test file (all still red)

The brief's required list: pre-flip check over `0..resume_from` only → D2; segments batched by
a fixed count → B (both byte and ops tests); root in a batch separate from the caller's flip
contribution → E; a refusal check moved after the first segment commit → C; owned-key check
removed → C; fence-cycle check removed → F `A → B → A`; fence precondition dropped from the
flip → F-r (`a root names a drained range`). Last round's additions also stay red: suffix-only
pre-flip check → D2; precondition bytes dropped → B; segment-phase ops check removed → B-ops;
each dirent pin removed → the dirent test; `planned_segments` by count → wide refs; per-batch
cursor check removed → C; split-local cycle check → F. Logs in the same directory.

### C5's own tool

`cargo mutants --in-diff patch.diff --no-shuffle --cap-lints=true -j 4` (the gate's command,
`scripts/mutants-in-diff`, plus `-j 4`): **177 mutants, 151 caught, 18 missed, 7 unviable,
1 timeout** (iteration 2: 147 caught, 22 missed). The four survivors the sign-off named
(`:595:66`, `:696:27`, `:702:51`, `:847:38`) are now caught, and no new survivor appeared.

The 18 left are the same ones as last round, and none was on the sign-off's list:

- `>` → `>=` boundary flips at `:588`, `:595` (×2), `:602`, `:652`, `:659`, `:670` (×2),
  `:750`, `:910` (×2, one is `==`), `:930`;
- the size estimate's slack: the extra comma per chunk at `:587` (`+ 1` → `- 1` / `* 1`), `:588`
  `+` → `*`, and `row_bytes`' `2 * key` at `:617` (×2), each moving the estimate by 1–45 bytes.

Each needs a fixture tuned to land exactly on one byte or one operation boundary. That is the
case-matrix the sign-off asked me not to add ("no matrices"), so I left them. They are
off-by-one or off-by-a-few-bytes on budgets that are themselves conservative (the estimate is
an upper bound; `V/2` leaves the other half of the value as margin).

## Refuting my own test

- **(a) Genuine red?** Yes, in the only way available. With the fix reverted (module removed)
  the named test does not compile (`unresolved import wyrd_core::staged_publication`): the
  brief's pre-declared posture, C4-verify UNVERIFIABLE (exit 77), a sign-off item, not a pass.
  Because that proves nothing about the assertions, every guard has its own temporary edit that
  turns a named test red on a build that compiles: 9 new edits this round, plus the 15 from
  earlier rounds re-run on the final test file. Each restore was hash-checked.
- **(b) Production path?** Yes. Every new test calls `wyrd_core::staged_publication::publish`
  over a real `RedbMetadataStore::in_memory()` and reads back through the production resolver
  `resolve_chunk_map` (`metadata.rs:3218`) or the store itself. The `Recorder` wrapper only
  forwards and logs here; none of the new tests uses its `Conflict` or rival hooks.
- **(c) Fixture includes the fault?** Yes. The concurrent PUT is a real committed CAS on
  `inode:8`; the pre-existing `inode:9` is a real record; the stranded and foreign rows are real
  durable keys under the group's range; the paging map really has 130 segments (2 more than one
  page); the flip's deletes are real deletes, and the measured flip really is 62,670 bytes /
  11 operations against 60,000 / 10.

## Size

Production: unchanged, `staged_publication.rs` 1,048 lines (the human accepted this).
Tests: 1,382 (core) + 380 (DST) = 1,762, up 183 from iteration 2, all of it the five gaps.

## Self-review against the repo rubric (`AGENTS.md` "Review rubric & protocol")

- One clock per lifecycle: the new tests read no clock; stamps are fixed values.
- Narrow seams: the tests use only `MetadataStore` through the `Recorder`.
- `#![forbid(unsafe_code)]` stays at the top of the test file.
- Docs currency: no port, API, RPC, CLI flag or persisted field changed this round, so no docs
  edit. The staged-publication paragraph from iteration 2 is unchanged.
- Absent or unsupported entries: the new tests check that a stranded or foreign row is an
  explicit refusal, not a skip, and that a missing paged row is `SegmentNotDurable`.
- Test fidelity: real redb store; DST scenario unchanged and green in the CI sweep.

## Questions for the human

1. **C4-verify exception** (unchanged from earlier rounds). The committer is new API, so the
   reverted base cannot compile the test and C4-verify reports UNVERIFIABLE (exit 77). The
   behavioural evidence is the 24 temporary-edit reds above. Accept or decline.
2. **The 18 remaining mutants.** Left on purpose (see above). If you want any of them closed,
   name which; each is one boundary fixture.

## Scope check

Touched this round: `crates/core/tests/segmented_publication.rs` only. The patch as a whole
still touches `crates/core/src/lib.rs:17` (module registration), the new module, the two new
test files and `docs/design/architecture/08-crosscutting-concepts.md:91`. Not touched:
`crates/core/src/metadata.rs`, `docs/design/architecture/05-building-block-view.md`, proposal
0016, ADRs, `Cargo.lock`, `deny*.toml`. No production caller added.

## Final runs

All on the final tree, in the cycle worktree:

- `./engine/xtask.sh ci` (the project runner, `cargo xtask ci`, `TMPDIR` in scratch):
  **`xtask ci: all checks passed`, exit 0**. Includes fmt, clippy, typos, docs lint/render,
  the workspace tests (core `segmented_publication`: `21 passed`), machete, cargo-deny,
  conformance, the statics guards, and the madsim DST sweep (the DST `segmented_publication`
  scenario: `3 passed`). Log: `$PDCA_SCRATCH/pdca-builder-653-ci3.log`.
- `cargo fmt --all -- --check` clean; `typos` on the touched files clean;
  `cargo clippy -p wyrd-core --all-targets` clean.
- After CI, one throwaway run added an `eprintln!` to the (C) loop to measure the byte figures
  quoted in the test comment (62,670 / 50,652), then restored the file; `cmp` confirms it is
  identical to the copy in the bundle and to the file in `patch.diff`.
- `patch.diff` equals `git diff 12dd082` of the worktree (`cmp` clean); `git apply -R --check`
  is clean. 5 files, 2,813 added lines, no deletions. The named test is also copied to the
  bundle root as `segmented_publication.rs` (byte-identical).

## Housekeeping

Scratch used: `$PDCA_SCRATCH/pdca-builder-653-reds3/` (mutation script and logs,
`mutants-out/`, `mutants-tmp/`, `ci-tmp/`, a saved copy of the test file) and
`$PDCA_SCRATCH/pdca-builder-653-ci3.log`. Left for the harness to reclaim. No files created
outside the worktree, the bundle and the scratch root. No external dependency outside the
brief's list was needed (`typos` and the docs renderer both ran), so there is no
missing-dependency item to declare.
