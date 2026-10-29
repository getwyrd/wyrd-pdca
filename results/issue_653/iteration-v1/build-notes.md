# Build notes — issue 653: staged segment publication committer (635.6)

Target: `getwyrd/wyrd` @ `main` (`12dd082`). All citations below are against the cycle
worktree on that base plus this patch.

## What was built

| Change | Where |
|---|---|
| New module, registered in the core crate root | `crates/core/src/lib.rs:17` |
| The committer: `publish` and its types | `crates/core/src/staged_publication.rs` (new, 984 lines: 708 code, 236 doc/comment, 40 blank) |
| Named test (deterministic legs A–F) | `crates/core/tests/segmented_publication.rs` (new, 843 lines) |
| Seeded DST scenario (leg G) | `crates/dst/tests/segmented_publication.rs` (new, 343 lines) |
| Living-architecture paragraph (docs currency) | `docs/design/architecture/08-crosscutting-concepts.md:91` |

Nothing in `crates/core/src/metadata.rs` or `docs/design/architecture/05-building-block-view.md`
was touched (the brief keeps both for #657), nor `Cargo.lock`, `deny*.toml`, proposal 0016 or
any ADR. No production caller was added (#658 is the caller).

### The public API (`staged_publication.rs`)

- `RootTarget::{Fresh { id }, Supersede { id, prior }}` — `:87`.
- `SegmentBatchInfo { first_index, written }` — `:118`. Only cursor positions, no batch
  ordinal: the split is greedy, so the total batch count is unknown while it is measured, and
  the durable-prefix re-derivation needs a contribution keyed on the cursor anyway.
- `Publication` — `:127`: parent, name, root target, group, chunks, meta, fence key, segment
  contribution hook (`&(dyn Fn(SegmentBatchInfo) -> WriteBatch + Sync)`), `resume_from`,
  optional byte/ops budgets, flip contribution.
- `PublicationError` — `:187`, its own enum (so `ChunkMapError` in `metadata.rs` stays
  untouched). Variants down to `Unstamped` are decided from inputs alone; `ForeignRow`,
  `StrandedRow`, `SegmentNotDurable`, `SegmentMismatch` come from the range reads.
- `publish(store, &Publication) -> Result<CommitOutcome>` — `:400`. Store faults, `Conflict`
  and `CommitUnknownResult` pass through untouched (`?` on the store's `BoxError`, and the
  first `Conflict` returned as `Ok(Conflict)`).

### The protocol, step by step (`publish`, `:400-420`)

1. `prepare` (`:443`) decides every refusal that needs no store read, and assembles every
   batch it will commit:
   - `plan` (`:512`): flat (≤ `MAX_MAP_CHUNKS`, `multipart.rs:4959`) or segmented.
     Segmented spans come from `split_segments` (`:561`), which closes a segment on
     **whichever binds first**: `MAX_SEG_CHUNKS` (`multipart.rs:4963`) or the encoded-bytes
     budget `SEGMENT_VALUE_BUDGET = MAX_ROOT_VALUE_BYTES` (`:374`, V/2 per `0016:1467`). At
     the sizing scheme's widest ref both agree, so the plan is `⌈n / MAX_SEG_CHUNKS⌉`
     segments; a wider ref closes a segment sooner instead of overrunning the value. The
     records go through `SegmentRecord::new` (`metadata.rs:1358`), keys through `seg_key`
     (`metadata.rs:1481`), the table through `SegmentedMap::new` (`metadata.rs:1104`), and
     the root's size is `map.span()`, so the root meets its decode invariant by construction.
     More than `MAX_ROOT_SEGMENTS` (`metadata.rs:544`) → `TooManySegments`.
   - `split_batches` (`:608`): greedy, each batch grows one row at a time while
     **rows + that batch's own contribution** fit both the byte and the ops budget. Each row
     is charged its unwitnessed shape (`require_absent(key)` + `put(key, value)`, `:597`,
     `ROW_OPS = 2`), which is the larger of its two shapes, so what the store held never moves
     a boundary. One row plus its contribution not fitting is refused
     (`BatchOverBytes`/`BatchOverOps` for that batch).
   - `check_contribution` (`:703`) on every segment contribution and the flip contribution:
     no owned key in preconditions, puts or deletes (`Owned::owns`, `:693`: target inode,
     target dirent, any key under `seg_range_prefix(group)`), no key mutated twice, no put
     over `MAX_VALUE_BYTES`, and at least one value pin on the fence key. A segment
     contribution may not delete the fence (`FenceDeleted`). The flip must move it
     (`check_fence_moved`, `:747`).
   - `check_fence_never_cycles` (`:764`) over the durable prefix's states (re-derived one
     cursor position at a time for `0..resume_from`), every segment contribution and the flip
     contribution, in commit order.
   - `root_record` (`:803`): version 1 or `prior.version.checked_add(1)`
     (`VersionOverflow`), `modified` through `overwrite_modified` (`metadata.rs:2373`) for
     both branches (with `prior = None` for a fresh key, which returns the caller's stamp).
   - `flip_batch` (`:833`): mirrors the peers — `create_leased` (`metadata.rs:2024`) for a
     fresh key (`require_absent` inode and dirent, put both) and
     `commit_chunk_map_superseding_leased` (`metadata.rs:2434`) for a supersede (`require`
     the inode at `encode(prior)`), plus the dirent pinned on the supersede branch too
     (`0016:663`). Then the caller's whole flip contribution merged in, the root value checked
     against `MAX_VALUE_BYTES`, and the assembled flip checked against both envelopes.
2. `read_group` (`:915`): the group's whole range in `SEGMENT_PAGE_LIMIT` pages via
   `scan_page` (`traits/src/lib.rs:1438`), keyed by parsed index; a foreign key or a row at or
   past the plan is refused, so at most `segment_count` rows are kept. `check_durable`
   (`:954`) with `required = resume_from`: rows below the cursor must be durable and
   identical; any durable row that differs is refused at any index; the rest are witnessed.
3. Segment batches in order (`assemble_segments`, `:893`): unwitnessed rows as
   `require_absent + put`, witnessed rows as `require(key, value)` only — so a same-epoch
   recovery over durable rows never rewrites them (D3). First `Conflict` returns
   `Ok(Conflict)`.
4. `read_group` again and `check_durable` with `required = segment_count`: the whole plan,
   never only `0..resume_from` (D2).
5. Commit the flip. A flat publication is step 5 alone.

### Caller obligations (stated in the module doc, `staged_publication.rs:1-73`)

Both of the brief's obligations are spelled out there: (1) a rollback deletes a group's
`seg:` records only after committing its own fence move off every publisher value, never on
a value it merely read; (2) contributions never touch an owned key (also enforced). The doc
also states: contributions must be pure functions of `SegmentBatchInfo` (the hook is asked
more often than the phase commits), retiring a superseded generation is the caller's flip
contribution (`0016:663`), and the retry loop / re-read after an unknown result are the
caller's. Clock: the module reads none.

## Salvage from #647 — what was taken, what was not

Taken (design, rewritten for `main`'s shapes — `seg_key` returns `Result`, nonces are
`SegmentNonce` inside `SegmentGroup`): the two-phase shape; the fence-pin / fence-moved /
never-cycles rules and the "adjacent repeat is the same state" reading; the durable-prefix
re-derivation one cursor position at a time; the witness that pins durable rows instead of
rewriting them; charging the whole batch (`batch_bytes`, `batch_ops`); `rendered` keys;
`StrandedRow` (what #647 called `PublicationTailStranded`).

Not taken, and why:

- **#647's fixed-point "reserve" loop** for fitting the contribution (`MAX_SPLIT_ATTEMPTS`,
  `Measured`, ~110 lines in the salvage, patch lines 3627-3807). Replaced by the greedy
  split that asks the hook for the contribution of the exact candidate batch (`:608-664`,
  ~55 lines). Same guarantee (every assembled batch fits, measured on the real
  contribution), no convergence question, no "witness must not move the split" argument
  needed beyond the unwitnessed row charge.
- **Two public entry points (`write_segments` + `flip`)**. One `publish` that always
  verifies the whole range before the flip. A bare `flip` would have to re-verify anyway, and
  a recovery is just `publish` with `resume_from`.
- **#647's `publish` skipping the second range read** ("holds by construction", salvage
  lines 4299-4306). The brief's (D2) rejects that: a record at an index ≥ `resume_from` can
  vanish after the last segment commit, and only a second read sees it.
- **Allowing preconditions on owned keys** (#647's `merge_contribution` left preconditions
  alone). The brief's owned-keys rule forbids preconditions too, and that is what makes the
  fence key itself unable to be an owned key (a contribution must pin the fence, and a pin on
  an owned key is refused).
- **The `seggrp:` marker as an owned key.** The brief's owned list is inode, dirent and the
  group's `seg:` range; the marker is Create's and the terminal delete's.
- **Refusing a segmented prior** (`SegmentedRetirementUnsupported`). On `main` the brief
  gives retirement to the caller's flip contribution (`retire:bytes:{generation}`, #658/#659),
  so the committer supersedes any prior shape.
- **Write-side placement checks** (`WritePlacementMalformed`, `WritePlacementEmpty`). Neither
  peer committer (`create_leased`, `commit_chunk_map_superseding_leased`) checks placement,
  the brief does not ask for it, and the chunks #658 passes come from part records the
  multipart write path fills with full placements. Flagged below as a reviewer question.
- **A key-length ceiling.** `main` has no `MAX_KEY_BYTES` in core; the value ceiling is
  checked on every put.

## Alternatives ruled out (with cost)

- **Commit-time `require(seg, value)` for every segment inside the flip** (so the flip
  itself re-checks every record). Ruled out on size, not taste: at the worst case of 512
  segments × (64-byte key + up to 50,000-byte value) the flip would carry ~25.6 MB of
  preconditions against a 5 MB `MAX_BATCH_BYTES` (`multipart.rs:5067`). The fence plus the
  pre-flip range read is the design 0016 states; caller obligation (1) closes the window.
- **Skip committing pin-only batches when every row is witnessed** (D3 shape). Saves up to
  `segment_count` commits on a full-durable recovery, but the caller's cursor moves ride in
  those batches and the cycle check was run over them; skipping them would flip from a fence
  value the checks never saw. Kept them.
- **Putting the committer in `metadata.rs`** (as #647 did): the brief forbids it for
  scheduling with #657.

## Size discipline

Brief guide: ~900 production lines, ~1,500 test lines. Production: 984 lines in one file,
of which 708 are code (the rest docs and blanks; rustfmt spreads struct literals and error
constructors vertically). Tests: 843 + 343 = 1,186 lines. No split needed.

## Tests and runs

The project runner has no single-test entry (`cargo xtask` subcommands: `ci`, `dst`, …;
`xtask/src/main.rs:116-150`), so the red→green runs use the exact command C4-verify issues
(`engine/scripts/run-verify.sh`: `cargo test -p <crate> --test <name>`, with
`RUSTFLAGS=--cfg madsim MADSIM_TEST_NUM=50` for `crates/dst`, as `xtask::run_dst`,
`xtask/src/main.rs:1781-1817`, does), always under `timeout`.

- `cargo test -p wyrd-core --test segmented_publication` → `12 passed; 0 failed`.
- `RUSTFLAGS="--cfg madsim" MADSIM_TEST_NUM=50 cargo test -p wyrd-dst --test segmented_publication`
  → `3 passed; 0 failed` (the `#[madsim::test]` entry at 50 seeds, plus two 64-seed sweeps).
  Sweep spread: 31 seeds published (all 31 after at least one `CommitUnknownResult` and a
  same-epoch recovery), 33 rolled back, 0 gave up.
- `cargo fmt --all -- --check` clean; `cargo clippy -p wyrd-core --all-targets` and
  `RUSTFLAGS="--cfg madsim" cargo clippy -p wyrd-dst --all-targets` clean (workspace lints
  deny warnings).
- `typos` over every touched file: clean. `python3 docs/publishing/tools/lint_docs.py`:
  OK. `render_site.py --check` (the docs-renderer link audit): OK.
- Full gate `./engine/xtask.sh ci`: see "Full gate" at the end.

### Legs → tests (`crates/core/tests/segmented_publication.rs`)

The `Recorder` wrapper (`:75`, `MetadataStore` impl `:153`) logs every batch `commit`
receives, and one-shot rules can answer a matched commit `Conflict` without forwarding it,
or run a rival batch (built from the fence value read at that instant) just before, just
after, or read-before/commit-after the matched commit.

| Leg | Test |
|---|---|
| A — fresh segmented, flat ≤ 158 with no `seg:` write, supersede flat → segmented (version, group, `overwrite_modified`) | `a_segmented_and_flat_maps_round_trip_through_the_resolver` `:365` |
| B — ≥ 3 segment batches, every logged batch ≤ byte and ops budget in mutation bytes, root in exactly one commit, last | `b_every_batch_stays_inside_the_byte_and_operation_envelopes` `:427` |
| C — 9 refusals, each zero logged commits and an unchanged store snapshot | `c_every_deterministic_refusal_leaves_zero_commits` `:472` |
| D1 — record below the cursor deleted, or rewritten | `d1_…` `:633` |
| D2 — record at index ≥ `resume_from` deleted right after the last segment commit | `d2_…` `:661` |
| D3 — fully durable range, `resume_from = 0`, no `seg:` put | `d3_…` `:686` |
| E — flip carries the caller's put; flip `Conflict` leaves neither | `e_…` `:717` |
| F — epochs disjoint, drain of E after E′ publishes | `f_a_drained_…` `:744` |
| F — `A → B → A` refused, zero commits | `f_a_fence_…` `:774` |
| F-r — rollback's fence move lands before the flip | `f_r_…` `:797` |
| F-p — flip lands first, rollback's move `Conflict`s | `f_p_…` `:815` |

The (C) cases: more than `MAX_ROOT_SEGMENTS` segments; flip over the byte envelope; flip over
the ops envelope; segment contribution not pinning the fence; flip not moving the fence;
segment contribution deleting `seg:<nonce>:<E>:0`; flip contribution deleting
`seg:<nonce>:<E>:0`; flip contribution with a precondition on the target dirent; flip
contribution putting the target inode.

### Leg G (`crates/dst/tests/segmented_publication.rs`)

`SimFdbMetadataStore` at `FdbFidelity::CommitUnknownResult`, nemesis armed with
`arm_mixed_commit_ambiguity(2)` (1021/1031 on a seed coin, `support/mod.rs:715`) right after
the fence fixture, so it strikes the publisher's first segment batches. The publisher
(`:105`) re-reads its fence and calls `publish` again at the same epoch from the cursor it
finds. The rival (`:184`) has the target rollback's shape: wins a fence move off the value
it read, then drains (`:164`). `assert_atomic` (`:231`) after `quiesce()`: at most one
`inode:` key; a root that exists names the group and resolves through `resolve_chunk_map`
to the exact plan with `segment_count` rows and the fence at `published`; no root ⇒ the
fence is not `published`. Demonstrated red in the same file (`:339`): a rival that drains on
a merely-read value is caught by that invariant (`should_panic` on the resolver's failure
message, across 64 seeds).

## Demonstrated reds (required by the brief; each a temporary edit, then restored)

The mutation script and the saved original lived in
`$PDCA_SCRATCH/pdca-builder-653-reds/`. After every run the original file was copied back and
`cmp`-checked, and the full test re-run green (`12 passed`).

1. **Pre-flip check reads only `0..resume_from`** (`check_durable(.., count)` →
   `check_durable(.., resume)` at `staged_publication.rs:417`) → **D2 fails**:
   `expected a refusal, got Committed`.
2. **Segments batched by a fixed record count** (the fit test in `split_batches`' growth loop
   replaced by "at most 3 rows") → **B fails**: `batch 0 carries 25790 mutation bytes, over
   12000`.
3. **Root written in a batch separate from the caller's flip contribution** (no merge into
   the flip; the contribution committed as its own batch first) → **E fails**:
   `assertion failed: puts_key(&flip, &inode_key(8)) && puts_key(&flip, FLIP_MARK)`.
4. **One refusal check moved after the first segment commit** (`check_fence_moved` moved
   from `prepare` to just before the flip commit) → **C fails**: `a flip that does not move
   the fence: a refusal committed a batch`.
5. **Owned-key check removed** (`Owned::owns` → `false`) → **C fails** at its first owned
   case: `a segment contribution that deletes a segment of the group: wrong refusal
   SegmentNotDurable { index: 0 }` (the batch committed, and only the pre-flip read caught
   the self-deleted row).
   5b. **Owned-key check removed for the flip only** (the brief's example: a flip
   contribution deleting `seg:<nonce>:<E>:0`) → **C fails**: `expected a refusal, got
   Committed` — a root committed in the same batch that deleted a segment it names. This is
   the case no range read can catch.
6. **Fence-cycle check removed** (`FenceCycled` return replaced by a no-op) → **F `A → B → A`
   fails**: `expected a refusal, got Committed`.
7. **Fence precondition dropped from the assembled flip** (`flip.preconditions.retain(key !=
   fence)`) → **F-r fails**: `assertion `left == right` failed: a root names a drained range`
   (left: the published segmented root; right: `None`).

## Refuting my own test

- **(a) Genuine red?** Yes, at two levels. With the whole fix reverted (the module dropped
  from `crates/core/src/lib.rs`) the named test does not compile: `error[E0432]: unresolved
  import wyrd_core::staged_publication`. That is the brief's pre-declared posture (net-new
  API ⇒ C4-verify reports UNVERIFIABLE, exit 77, a sign-off item), not a pass. Because that
  revert proves nothing about the assertions, the eight targeted mutations above each turn a
  specific leg red on a compiling build.
- **(b) Production path?** Yes. Every test calls `wyrd_core::staged_publication::publish`
  over a real `RedbMetadataStore::in_memory()` (core) or the simulated FoundationDB store
  (DST), and reads the result back through the production resolver
  (`metadata::resolve_chunk_map`, `metadata.rs:3218`). The wrapper only forwards, records,
  or interleaves a rival's batch; it never stands in for the committer.
- **(c) Fixture includes the fault?** Yes. D1 deletes or rewrites a real durable row; D2's
  wrapper deletes a real row after the real last segment commit; F-r commits a real fence
  move just before the flip, then really drains the range; F-p's rival really loses; E's
  wrapper really refuses the flip; G arms the real nemesis (every seed asserts
  `ambiguous_commits >= 1`, and the sweep asserts both winners and at least one recovery
  after an unknown result), and the DST red shows the invariant catches a rollback that
  skips the fence move.

## Questions for the reviewer / human

1. **Placement validation on write.** The committer does not refuse a chunk whose placement
   is empty or malformed (#647 did). Peers do not either; #658's chunks come from part
   records. If the reviewer wants the strict-write boundary (ADR-0040 decisions 3–4) here,
   it is ~15 lines in `split_segments` and one variant.
2. **Byte charge of preconditions.** `batch_bytes` (`:669`) charges every precondition's key
   *and expected value*, which is conservative (FDB charges conflict ranges, not read
   values). It never makes a batch bigger than the backend counts; it can make batches
   smaller. Pinned witnessed rows therefore cost as much as a write, which is what keeps the
   split independent of the store.
3. **Await discipline.** Each await is one store call; the loop is bounded by the plan
   (≤ `MAX_ROOT_SEGMENTS` rows, two range reads of ≤ 5 pages each, ≤ one batch per row, one
   flip). Per-call deadlines are the backends' (the 5 s transaction envelope), as for every
   other committer in `metadata.rs`. No timeout wrapper was added.
4. **Contribution purity.** The hook is called for candidate batch sizes that never commit
   and for every durable-prefix position. Documented as a requirement; #658's cursor CAS is
   pure by construction.

## Housekeeping

- I created one stray empty file outside the harness roots by mistake: `/tmp/x` (0 bytes,
  created 2026-10-20 01:19 by a mistyped `cat > /tmp/x`; it did not exist before, so nothing
  was overwritten). Not deleted, per the builder's no-`rm` rule; safe to remove.
- Scratch used: `$PDCA_SCRATCH/pdca-builder-653-reds/` (mutation script, saved originals,
  red logs), `$PDCA_SCRATCH/pdca-builder-653-docs/` (render_site output),
  `$PDCA_SCRATCH/pdca-builder-653-probe/` (empty), `$PDCA_SCRATCH/pdca-builder-653-ci.log`.
  Left for the harness to reclaim.
- No external dependency outside the brief's list was needed: `typos` and the docs renderer
  (`markdown_it`, `yaml`) are both installed and ran. No NEEDS-HUMAN external dependency.

## Full gate

`./engine/xtask.sh ci` (the project runner, `pdca.toml` `[gates] runner`; `cargo xtask ci` in
the cycle worktree, `TMPDIR` pointed at the scratch dir) on the final tree:
**`xtask ci: all checks passed`, exit 0** — typos, docs lint and render audit, fmt, clippy,
build, the workspace tests (the new core test among them), cargo-machete, the three
cargo-deny legs, conformance, the statics and orchestrator guards, `clippy -p wyrd-dst
(--cfg madsim)` and the DST sweep (the new scenario among them: `3 passed`). Log:
`$PDCA_SCRATCH/pdca-builder-653-ci.log`.

`patch.diff` is `git diff` of the cycle worktree against `12dd082` (5 files, +2173, no
deletions); `git apply --check -R` of it in that worktree is clean. The named test is also
copied to the bundle root as `segmented_publication.rs` (the DST scenario shares that
basename and is in `patch.diff` only).
