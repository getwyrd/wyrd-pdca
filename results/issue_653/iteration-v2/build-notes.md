# Build notes — issue 653: staged segment publication committer (635.6), iteration 2

Target: `getwyrd/wyrd` @ `main` (`12dd082`). Citations are against the cycle worktree on that
base plus this patch. Iteration 1 is preserved in `iteration-v1/`; this round started from its
patch (applied cleanly to `12dd082`) and changes what the carry-forward asked for. The rest of
the design (two phases, verify the whole range, one flip, owned keys, witnessed rows pinned
instead of rewritten) is unchanged and is described in `iteration-v1/build-notes.md`.

## What changed this round, finding by finding

| Finding (carry-forward) | What I did | Where |
|---|---|---|
| T4 / C5: resume rebuilt the prefix from one-segment batches and could raise a false `FenceCycled` (A→B batch then B→C flip refused on resume); the same rebuild could also miss a value | Removed the rebuild. The committer now reads the fence value at **every** cursor position `0..=n` off the hook's one-segment batches (`fence_track`), checks that every batch it commits requires the value at its first position and leaves the value at its last (`FenceOffCursor`, new variant), checks the flip requires the value at `n` (`FenceOffCursor { Flip }`), and runs the cycle check over the whole track plus the flip's put (`check_track`, `check_flip_fence`). The cycle verdict depends only on hook, plan and flip, never on `resume_from` or the budgets | `crates/core/src/staged_publication.rs:488-527` (`prepare`), `:770` (`segment_step`), `:790` (`fence_track`), `:816` (`check_track`), `:837` (`check_flip_fence`), variant `:239`; contract stated in the module doc `:26-59` |
| T5 / T4 TEST-GAP: (B) counted mutation bytes only; precondition accounting mutant survived | (B) now measures the **whole transaction** in the test (`transaction_bytes`: precondition keys + expected values, puts, deletes). New fixture with 4,000-byte fence values, so half of each contribution is a precondition's expected value, at a budget where dropping those bytes changes the split. Also asserts the split is tight (no batch left room for the next row), and measures a recovery whose first batch is pin-only | `crates/core/tests/segmented_publication.rs:368` (`transaction_bytes`), `:561-655` |
| (B) ops budget could never bind | New test with `ops_budget = Some(7)` and the default byte budget: two rows per batch, tight | `crates/core/tests/segmented_publication.rs:657` |
| Dirent pins untested on both branches | New test: a fresh inode over a bound name, and a supersede after a rename, each `Conflict` with the dirent and the inode unchanged | `crates/core/tests/segmented_publication.rs:513` |
| Pre-flip check: a `resume_from..count`-only check survived | (D2) now loses a record at index 3 (≥ cursor) **and** at index 0 (< cursor) right after the last segment commit | `crates/core/tests/segmented_publication.rs:929` |
| DST: "at most one root" could not fail | The recovering publisher mints a fresh inode id per attempt (`FIRST_ID + attempt`); the root is found by scanning `inode:` and must be the one the dirent binds. That alone was not enough (see "DST" below), so the strike count now varies per seed (1–7), even seeds write the phase in one batch, and every other block of 24 seeds holds a timed-out batch in flight until the run settles | `crates/dst/tests/segmented_publication.rs:117-174`, `:254-286`, `:288-323` |
| Segment count: the byte rule can plan more than `⌈n / MAX_SEG_CHUNKS⌉`; #658 had no way to get the count | New `pub fn planned_segments(&[ChunkRef]) -> Result<u32, PublicationError>` (reuses `split_segments`), docs corrected; tests use it instead of a hard-coded count, a wide-ref test (RS(10,4), 14 twenty-digit placements, 316 chunks) shows `planned > ⌈316/158⌉` and publishes; a flip that requires the wrong count is now a zero-commit refusal (`FenceOffCursor { Flip }`) instead of a `Conflict` after the rows are written | `crates/core/src/staged_publication.rs:409-421`, `:575-580`; test `:467` |
| Resume-then-cycle untested | (F) cycle test now has three legs: `A → B → A`; a resume from 3 whose flip returns to `cursor:1` (a value only the durable prefix held); a parity fence that cycles only on a one-row split while the default split is one batch | `crates/core/tests/segmented_publication.rs:1047` |
| Regression for recovery after a multi-segment commit | Reviewer's own probe shape: fence values `A C D E F B` at positions 0..=5. Flip `B → C` is refused on the **first** attempt with zero commits; flip `B → G` publishes on a resume from 5 after a first attempt committed the whole phase as one `A → B` batch | `crates/core/tests/segmented_publication.rs:1118` |
| Smaller C5 survivors (`FenceDeleted`, contribution `ValueOverCeiling`, `MAX_ROOT_SEGMENTS` boundary, Display, `rendered`) | Added (C) rows for `FenceDeleted`, `ValueOverCeiling`, `DuplicateMutation`, `ResumePastPlan`, segment-batch byte and ops refusals; (C) now compares the whole error value, so `OwnedKey.key` (the `rendered` output) is checked; two `to_string()` assertions cover both `Display` impls; the boundary (512 full segments accepted, one more chunk refused) is in the wide-ref test | `crates/core/tests/segmented_publication.rs:698-891`, `:1075`, `:1107` |
| `read_group` nonce/epoch guard mutants were equivalent | Removed the dead guard: a key under `seg:<nonce>:<epoch>:` that parses names that nonce and epoch (`seg_range_prefix` ends in `:`, `metadata.rs:1506-1508`; `parse_seg_key` is strict, `:1528-1556`) | `crates/core/src/staged_publication.rs:995-1003` |
| C4-verify UNVERIFIABLE (reverted base cannot compile the test) | Not a code change: the brief pre-declares it (net-new API). The behavioural reds are the temporary edits below. Still a sign-off item for the human | — |

### Why the fence rule is shaped this way

The rollback the fence protects against is keyed on a fence value. The committer cannot know
how an earlier attempt split its batches, so any check that replays "the history" from this
call's view either invents values (false refusal, the T4 bug) or misses some (the open
direction the adversary found). Two things make the check sound without knowing the history:

1. The hook must be a function of the cursor: what a batch requires depends on `first_index`
   only, what it leaves on `written` only. That gives one value per cursor position, read off
   the one-segment batches. Every batch any attempt commits is checked against those values
   before it is committed, so every committed batch moves the fence from `track[i]` to
   `track[j]` for `i < j`. Across attempts the fence only walks forward along `track`.
2. The cycle check runs over the whole `track` and the flip's put. Any return to a left
   value on any split is a return in `track` (the one-row split visits every position), so
   it is refused on every attempt, including the first, before anything is written.

Cost: `n ≤ 512` extra hook calls per `publish` (`fence_track`, `:790-812`), each building a
`WriteBatch` of two small values for #658's hook. The alternative that would have kept the
old API semantics without a contract — probing every pair `(i, j)` — is `n(n+1)/2 = 131,328`
hook calls at `n = 512`; rejected. The other alternative, changing the hook to a
`Fn(u32) -> Bytes` fence-value function, makes the shape unrepresentable-wrong but removes
the per-batch caller contribution the brief's Design names ("per batch, the fence
contribution") and makes (C)'s segment-batch owned-key and unfenced refusals vacuous; #658
builds against the brief's interface, so I kept the hook and enforced the contract instead.

The reviewer's `A C D E F B` hook with a `B → C` flip is now refused on the first attempt.
On that one split (`A → B` in one batch) it would have been safe; it is refused because a
resume or another budget can split at position 1, hold `C`, and then return to it. Refusing
before the first write is what makes "a publication whose first attempt passed is never
refused on recovery" hold. #658's hook (`segments_written` cursor) has distinct values at
every position and a `Completed` flip, so it passes.

## Demonstrated reds (each a temporary edit, then restored)

Driven by `$PDCA_SCRATCH/pdca-builder-653-reds2/mutate.py`: apply one named edit to
`staged_publication.rs`, run the target (`cargo test -p wyrd-core --test segmented_publication`,
or the DST command below), restore the file and check its SHA-256 (`dc6023ca…` before and after
every run). One doc-comment edit to the module (`staged_publication.rs:40-50`, wording only)
came after the mutation runs; the final file is `c134e28c…`, and every test was re-run on it
(see "Final runs"). Logs per
mutation are in that directory.

Brief's required list:

| Edit | Leg that goes red | Failure |
|---|---|---|
| Pre-flip check reads only `0..resume_from` (`check_durable(.., resume)` at `:462`) | D2 | `expected a refusal, got Committed` |
| Segments batched by a fixed record count (growth loop breaks at 3 rows, `:670`) | B (bytes) | `batch 0 carries 33927 bytes, over 23000` (also B-ops: `8 operations, over 7`) |
| Root written in a batch separate from the caller's flip contribution (no merge in `flip_batch`, contribution committed first) | E | `assertion failed: puts_key(&flip, &inode_key(8)) && puts_key(&flip, FLIP_MARK)` |
| One refusal check moved after the first segment commit (`check_flip_fence` result applied after the first segment commit) | C | `the root flip: …: a refusal committed a batch` |
| Owned-key check removed (`Owned::owns` → `false`) | C | `segment 0 is not durable: a refusal committed a batch` |
| Fence-cycle check removed (`check_track` → `Ok`, flip cycle arm disabled) | F `A → B → A` (and the verdict test) | `expected a refusal, got Committed` |
| Fence precondition dropped from the assembled flip | F-r | `a root names a drained range` (left: the published segmented root, right: `None`) |

This round's additions:

| Edit | Leg that goes red | Failure |
|---|---|---|
| Pre-flip check over `resume_from..count` only | D2 (lost index 0) | `expected a refusal, got Committed` |
| Byte count drops precondition expected values (`batch_bytes`, `:688-697`) | B (bytes) | `batch 0 carries 25253 bytes, over 23000` |
| Segment-phase ops check removed from the growth loop | B (ops) | `batch 0 carries 12 operations, over 7` |
| Fresh flip's `require_absent(dirent)` removed | A dirent pins | `left: Committed, right: Conflict` |
| Supersede flip's `require(dirent)` removed | A dirent pins | `left: Committed, right: Conflict` |
| `planned_segments` returns `⌈n / MAX_SEG_CHUNKS⌉` | A wide refs | `wide refs close segments early: 2` |
| Per-batch cursor check removed (`left != track[end]` disabled) | C (`wide_batches_disagree`) | `expected a refusal, got Conflict` |
| The old design: cycle check over this call's split points only | F cycle test + F verdict test | `expected a refusal, got Committed` |

### DST (`RUSTFLAGS="--cfg madsim" MADSIM_TEST_NUM=50 cargo test -p wyrd-dst --test segmented_publication`)

| Edit | Result |
|---|---|
| Flip's fence pin dropped | red: `a published root names a missing or different seg: record: SegmentAbsent { … index: 0 }` |
| Flip's dirent pins dropped (both branches) | **green**. The flip's fence pin alone already stops a second root here: two flips of one publication both require `cursor:n`, and only one can land. The dirent pins are bound by the core test (`a_the_flip_pins_the_dirent_on_both_branches`), one red per branch above. I say this plainly because the adversary expected the fresh-id change to make the DST bind the dirent pin alone; it binds the conjunction. |
| Fence pin and dirent pins dropped | red (drained-range assertion first). With the rival switched off as well, `more than one root: 2` fires — the two-roots property can now fail, which it could not in iteration 1 |

Getting the two-roots property to fire took three steps, each checked with the "both pins
dropped, rival off" probe. Fresh ids alone: never fired (the 2-strike nemesis always spent its
strikes on the first segment batches, so the flip was never ambiguous). Strikes varied 1–7 and
deferred in-flight landings on half the seeds: still never fired (strikes were spent re-trying
the first one-row batches). Even seeds writing the phase as one batch: fires. Sweep spread with
the final scenario, 64 seeds: 25 published (all after at least one unknown result), 39 rolled
back, 7 published after a mid-phase recovery (`0 < resume_from < n`), 10 seeds saw a stale
in-flight batch rejected when it finally landed, 0 gave up. The sweep now asserts a published
seed with a mid-phase resume (`crates/dst/tests/segmented_publication.rs:346-368`).

## Refuting my own test

- **(a) Genuine red?** Yes, at two levels. With the fix reverted (module removed) the named
  test does not compile (`unresolved import wyrd_core::staged_publication`) — the brief's
  pre-declared posture, C4-verify UNVERIFIABLE (exit 77), a sign-off item, not a pass. Because
  that proves nothing about the assertions, each of the 15 targeted edits above turns a named
  leg red on a compiling build, and each restore was hash-checked.
- **(b) Production path?** Yes. Every test calls `wyrd_core::staged_publication::publish` (or
  `planned_segments`) over a real `RedbMetadataStore::in_memory()` (core) or the
  simulated-FoundationDB store (DST), and reads results back through the production resolver
  `metadata::resolve_chunk_map` (`metadata.rs:3218`). The `Recorder` wrapper only forwards,
  logs, answers one batch `Conflict`, or runs a rival batch around one; it never stands in for
  the committer. The test's `transaction_bytes` is an independent measure of the logged
  batches, not a call into the committer's `batch_bytes`.
- **(c) Fixture includes the fault?** Yes. D1 deletes or rewrites a real durable row; D2's
  wrapper deletes a real row (above and below the cursor) after the real last segment commit;
  the dirent test really rebinds and really renames; F-r commits a real fence move before the
  flip and really drains; F-p's rival really loses; E's wrapper really refuses the flip; (B)
  carries real 4 KB preconditions and real durable pins; G arms the real nemesis (every seed
  asserts `ambiguous_commits >= 1`), defers real in-flight landings, and its red twins show the
  invariant catches a rollback that skips the fence move and a second root.

## Size discipline

Brief guide: ~900 production lines, ~1,500 test lines. Production: `staged_publication.rs`
1,048 lines, of which 732 are code (the rest docs and blanks). Tests: 1,199 (core) + 380
(DST) = 1,579, about 5% over the test guide; the growth is the cases this round's findings
asked for (ops-bound split, dirent pins, D2 twin, wide refs, verdict-on-resume, extra (C)
rows). I did not split the slice.

## Self-review against the repo rubric (`AGENTS.md` "Review rubric & protocol")

- One clock per lifecycle: the module reads no clock; `modified` comes from the caller and
  `overwrite_modified` (module doc `staged_publication.rs:83-85`, `root_record` `:870-896`).
- Narrow seams: only `MetadataStore`, and of it only `scan_page` and `commit`.
- No shared mutable global state; no new crate root (test files carry
  `#![forbid(unsafe_code)]`).
- Docs currency: the new API (`publish`, `planned_segments`) and its rules are in
  `08-crosscutting-concepts.md:91`.
- Absent entries: a missing or different segment is a typed error before the flip, never a
  skip (`check_durable`, `:1018-1043`).
- Await discipline: see question 4 below.
- Test fidelity: the DST runs on the simulated-FDB store with the mixed 1021/1031 nemesis and
  deferred landings; the new concurrent path has seeded coverage.
- The three T4 findings (`review-batch.md`) are fixed, not rejected, so no
  `review-rejected.md` entry is needed.

## Questions for the human

1. **C4-verify exception.** Accept or decline the pre-declared UNVERIFIABLE result (new API;
   the reverted base cannot compile the test). The behavioural evidence is the reds above.
2. **Hook contract.** The committer now refuses a segment hook whose fence values are not a
   function of the cursor (`FenceOffCursor`). #658's hook meets it. A hook that meets it on
   one split and not another would be refused on the split that breaks it, possibly on a
   resume after a budget change; that is a caller bug and it fails before any write of that
   attempt. Documented at `staged_publication.rs:31-59`.
3. **Per-deployment chunk caps (adversary's separate note, for #658).** The committer reads the
   compile-time `MAX_MAP_CHUNKS` / `MAX_SEG_CHUNKS`; `KnobSet` (`multipart.rs:5430`) makes them
   per-deployment knobs. Out of scope here; #658 should decide.
4. **Await discipline.** Each await is one store call, bounded by the plan (≤ 512 rows, two
   range reads of ≤ 5 pages, ≤ one batch per row, one flip); per-call deadlines are the
   backends' (the 5 s transaction envelope), as for the peer committers in `metadata.rs`. No
   timeout wrapper added.

## Scope check

Touched: `crates/core/src/lib.rs:17` (module registration), the new module, the two new test
files, and the staged-publication paragraph at
`docs/design/architecture/08-crosscutting-concepts.md:91` (updated for `planned_segments` and
the cursor rule). Not touched: `crates/core/src/metadata.rs`,
`docs/design/architecture/05-building-block-view.md`, proposal 0016, ADRs, `Cargo.lock`,
`deny*.toml`. No production caller added.

## Final runs

All on the final tree, in the cycle worktree:

- `./engine/xtask.sh ci` (the project runner; `cargo xtask ci`, `TMPDIR` pointed at the
  scratch root): **`xtask ci: all checks passed`, exit 0** — typos, docs lint and render/link
  audit (99 pages), fmt, clippy, build, workspace tests (the new core test: `16 passed`),
  machete, the cargo-deny legs, conformance, the statics/orchestrator guards,
  `clippy -p wyrd-dst (--cfg madsim)` and the DST sweep (the new scenario: `3 passed`). Log:
  `$PDCA_SCRATCH/pdca-builder-653-ci2.log`.
- After that run I changed one comment block (the DST file's header,
  `crates/dst/tests/segmented_publication.rs:7-25`) and added one assertion to (B)'s byte test
  (every logged batch also within `MAX_BATCH_OPS`, the brief's literal (B) wording,
  `crates/core/tests/segmented_publication.rs:592`), then re-ran: `cargo fmt --all -- --check` clean; `typos` on every touched
  file clean; `RUSTFLAGS="--cfg madsim" cargo clippy -p wyrd-dst --all-targets` clean;
  `cargo clippy -p wyrd-core --all-targets` clean;
  `cargo test -p wyrd-core --test segmented_publication` → `16 passed`;
  `RUSTFLAGS="--cfg madsim" MADSIM_TEST_NUM=50 cargo test -p wyrd-dst --test segmented_publication`
  → `3 passed`. The project runner has no single-test entry, so these use the command
  C4-verify itself issues (`engine/scripts/run-verify.sh`), always under `timeout`.
- Reverted fix (module unregistered in `lib.rs`): `error[E0432]: unresolved import
  wyrd_core::staged_publication` — compile-only, as pre-declared.
- `patch.diff` is the worktree against `12dd082` (5 files, 2,635 added lines, no deletions);
  `git apply -R --check` in the worktree and `git apply --check` on the base's two touched
  files are both clean. The named test is also copied to the bundle root as
  `segmented_publication.rs` (byte-identical; the DST scenario shares the basename and is in
  `patch.diff` only).

## Housekeeping

Scratch used: `$PDCA_SCRATCH/pdca-builder-653-reds2/` (mutation script, per-mutation logs,
saved originals for the DST probes, sweep log), `$PDCA_SCRATCH/pdca-builder-653-applycheck/`
(the base's two touched files, for the forward apply check), `$PDCA_SCRATCH/pdca-builder-653-ci2.log`,
and the docs render output CI wrote under the scratch `TMPDIR` (`wyrd-docs-build-*`).
Left for the harness to reclaim. No files created outside the worktree, the bundle and the
scratch root. No external dependency outside the brief's list was needed (`typos` and the docs
renderer ran), so there is no missing-dependency item to declare.
