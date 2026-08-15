# build-notes — issue 695 (iteration 5)

Target: `getwyrd/wyrd @ main`, base `origin/main` = `339da46`. Worktree `$PDCA_WORKTREE` =
`/home/eddie/wyrd/wyrd.pdca-wt-l0`; every `path:line` below is against that tree with the patch
applied.

## What this round changed, and why

The production hunks are iteration v4's, **unchanged** — they passed C1–C5, C4-verify red→green
and mutation analysis (0 survivors), and the human's §9 did not fault them. Round 5 answers the two
items the sign-off named, and nothing else:

### 1. Seeded Tier-0 DST coverage for the generation-change race path (Rule A) — the sole gating T4 blocker

`review-batch.md` carried exactly one finding into this round:

> `crates/custodian/src/backfill.rs:190` **TEST-GAP** — the new generation-change race path is
> concurrent correctness logic but has only a Tokio test-double test, not the seeded Tier-0 DST
> coverage required by the repository rubric.

Iterations 3 and 4 tried to clear it by **recording a rejection** (`review-rejected.md`, on the
strength of `brief.md` § Verification posture). That failed twice, and it deserved to:

* mechanically, `scripts/review-branch:252` binds a rejection to the finding's exact `loc`, and the
  finding lands at whatever line that round's reviewer picks (`:190` in round 4, `:164`/`:203`/`:255`
  in the recorded file) — so a rejection keyed to a line number is a coin flip on every rebuild; and
* substantively, the rejection was **wrong**. Rule A exists *because* `resolve_chunk_map` restarts
  onto the live root when a writer moves the root under the pass (`crates/core/src/metadata.rs:2632`).
  That is a concurrent path by construction, and `AGENTS.md` § *Test fidelity* asks a concurrent path
  for seeded Tier-0 DST coverage. The brief's own § Verification posture argues "no new concurrent
  path" from the fact that the pass *writes* nothing on the restarted arm — but the defect Rule A
  prevents is not a bad write, it is a bad **claim** (see below), and a claim made under a race is
  still made under a race.

So this round **fixes** it. `crates/dst/tests/custodian.rs:2175` adds **property 12** to the M3.8
custodian campaign, in the shape #649/#650/#651 each used for their own slice (properties 9, 10, 11):

* the store is `RecordingMeta` over `SimTikvMetadataStore` — the DST tier's *second* `MetadataStore`
  implementation, whose every `get`/`scan`/`scan_page`/`commit` spans real madsim `network_hop`
  await boundaries, which is what lets a concurrent task land *inside* the pass;
* the nemesis is a genuinely spawned madsim task (`:2318`) that retires the raced object's
  **segmented** generation onto a **flat, fillable** successor, at a landing point the run **seed**
  draws (`:2424`) — never a seam a double hard-codes;
* the fixture puts two fillable flat objects *before* the raced key and one *after* it
  (`BACKFILL_FLAT`, `:2212`), so (a) the pass has real work on both sides of the object it contains
  and (b) the raced object's resolve sits far enough into the pass that a landing point can fall
  inside it;
* every assertion is conditioned on what the pass's **own store reads returned** (`Flip`, `:2223`,
  classified at `:2342` from the tap at `:1828`), never on the fixture's intended timing — a pass
  that never restarts satisfies the property too.

The invariants (`:2350`, `:2363`, `:2383`, `:2396`): progress on both sides of the race; the raced
row is byte-identical to the successor (never a record framed from the retired generation — the two
carry different `size`); the published population is `0` on **every** schedule; and where the
restart genuinely happened the object is named `changed-under-scan`, counted, and the pass answers
`Blocked`.

`crates/dst/tests/custodian.rs:2435` is the **coverage leg**, mirroring
`prop_restore_two_readings_cover_the_divergence_window`: it walks the whole landing span and fails
unless a landing point actually fell inside the raced object's resolve. It earned its keep
immediately — my first timing model was wrong (the writer's commit costs two hops and `sleep(0)`
still costs one, so with the raced object first in the walk the flip could never land before the
resolver's settling re-read), and the coverage leg redded rather than letting property 12 pass on a
window it never reached.

Both are registered in the 50-seed sweep (`:2536`, `:2542`) and the campaign leg in the
`REGRESSION_SEEDS` replay (`:2581`).

**Why this is a third file, against the brief's `exactly 2 files` budget.** A seeded Tier-0 DST case
cannot live anywhere else: `crates/dst` is the only crate that compiles under `--cfg madsim`, and
`crates/dst/tests/custodian.rs` is the custodian campaign every sibling slice extended. The human's
§9 for round 4 directed this explicitly ("Next round must: 1. Add seeded Tier-0 DST coverage …"),
which is why I did not stop and hand back on the budget. `src/backfill.rs` is **111** added semantic
lines (cap 130) and the per-slice test is **315** semantic / **492** raw (caps 320/520), so both
in-budget files are inside their bounds.

**Wave note for the human.** The third file is shared campaign ground. If either sibling child
(rebalance / reconstruction) also appends a property to `crates/dst/tests/custodian.rs` in this
wave, the two will conflict textually at merge. My edits are: one import line (`:81`), a paragraph
in the module docs (`:45`), two fields/one method on `RecordingMeta` (`:1789`, `:1801`, `:1828`),
and an append-only block (`:2175`–`:2453`) plus three registrations — all easy to resolve, but worth
knowing before the fold.

### 2. Back under the 320-semantic-line cap

Round 4 shipped 336 semantic / 517 raw (T2 Shape FAIL, 16 over). Legs 5 and 9 were merged into one
leg rather than trimmed by shaving comments, because they were the *same rule*: leg 5 (Rule C — read,
write and name under the store's own key) and leg 9 (Rule C's other half — a row under a key that
names no object) both turn on `inode:007` vs `inode:7`. The merged leg
(`crates/custodian/tests/segmented_map_backfill.rs:390`) seeds `inode:-1`, `inode:007`, `inode:7`
(canonical, nothing owed) and `inode:8` (healthy, fillable) in one store and asserts the conjunction:
both unaccountable rows byte-identical, named and counted; **`inode:7` byte-unchanged** — the row a
pass re-deriving its CAS key from `inode:007`'s parse would have written over; `inode:8` filled;
`Blocked`; `("0", "2")` published. Nothing was dropped: the base-red character of both old legs is
preserved (on the base this store answers `Changed` with `remaining = 2` and names nothing).

Result: **315 semantic / 492 raw**, no waiver needed — and *measured after `cargo fmt`*, which is
where round 4's overage partly came from: rustfmt explodes a tuple literal past its 60-column
`struct_lit_width` into one line per element, so the leg's before/after byte-comparison is written as
a 2-tuple plus a scalar rather than a 3-tuple. Re-running `cargo fmt` now changes nothing.

I rejected the alternative of trimming doc comments to fit: the count is non-blank/non-comment, so
comment trimming buys **0** semantic lines — it could only have been bought by deleting assertions,
which is the wrong direction.

## The three forced questions

**(a) Genuine red?** Yes, both legs, actually run:

* per-slice test, production reverted (`git stash push crates/custodian/src/backfill.rs`), test kept:
  **7 of 8 fail** — `a_lost_cas_…`, `a_pass_never_writes_to_a_generation_it_did_not_scan`,
  `a_record_is_read_written_and_named_…`, `a_segmented_object_no_longer_ends_the_pass_…`,
  `a_segmented_record_is_declined_…`, `an_unreadable_committed_object_is_named_…`,
  `one_resolving_reading_of_the_namespace_per_pass`. The 8th
  (`a_fault_that_is_not_one_objects_map_still_ends_the_pass`) is the brief's declared non-red
  over-containment guard. Restored → 8/8 green.
* DST property 12, production reverted: both legs fail with
  `SegmentedMapUnsupported { operation: "backfill::reconcile" }`.
* DST property 12 against a **targeted** mutation — the fix present but only the Rule A comparison
  at `crates/custodian/src/backfill.rs:208` deleted: fails with *"the published population counts a
  placement this pass never read (UnderTheResolve, landing at 0 ms) left: [1] right: [0]"*. So the
  DST leg binds **Rule A specifically**, not merely "segmented no longer aborts".

**(b) Production path?** Yes. Both drive `wyrd_custodian::backfill::reconcile` — the shipped
`pub async fn` — over the real `MetadataStore` trait seam. No stand-in, no re-implementation. The DST
leg additionally drives the real `wyrd_core::metadata::resolve_chunk_map` restart
(`crates/core/src/metadata.rs:2632`) through the campaign's second, non-toy store implementation.

**(c) Fixture includes the fault?** Yes, and it is *proved* rather than assumed. Property 12's
coverage leg (`crates/dst/tests/custodian.rs:2435`) fails unless a landing point in the swept span
actually put the flip inside the raced object's resolve — it caught exactly that hole once during
development. The per-slice legs likewise assert their fault is real before asserting the behaviour
(leg 3 asserts `resolve_chunk_map` genuinely errors on the seeded root,
`crates/custodian/tests/segmented_map_backfill.rs:306`; leg 4 asserts the resolve genuinely did not
answer the scanned generation, `:356`).

## What I ruled out

* **Re-recording the rejection with better line coverage** (a rejection line for every line the
  reviewer might pick). Cheap — 5 lines — but it suppresses a finding that is *correct*, and it
  would have to be re-guessed on every rebuild. Withdrawn instead; `review-rejected.md` now records
  **no** machine-readable rejections at all.
* **A new `crates/dst/tests/backfill.rs`** instead of extending the campaign file. Rejected on a
  concrete cost, not a feeling: an added `*/tests/*.rs` under `crates/dst` becomes a second
  `ADDED_TEST` in C4-verify's classification (`engine/scripts/run-verify.sh:364`), which makes the
  gate compile the per-slice custodian test under `RUSTFLAGS=--cfg madsim` with
  `MADSIM_TEST_NUM=50` (`:129`, `:422`) — flags that alias tokio/tonic for the whole build. That
  turns a 2-second discriminator into a madsim sweep and risks an unverifiable leg. As a *modified*
  file, `crates/dst/tests/custodian.rs` is reverted with production in the red leg and never
  compiled by C4-verify at all — verified: `run-verify.sh --classify` on the shipped patch prints
  `ADDED_TEST crates/custodian/tests/segmented_map_backfill.rs`, `CRATE crates/custodian`,
  `CRATE crates/dst`, and only the added test drives `TEST_ARGS`.
* **Asserting the DST invariant with a count only** (`remaining == 0`). Kept, but not alone: a
  count-based assertion that can pass while the property fails is a named defect class in the repo
  rubric, so the byte-level assertion (2) at `crates/dst/tests/custodian.rs:2363` carries the "never
  wrote a generation it did not read" half, and (4) carries the naming half.
* **Widening the swept span instead of adding work before the race.** The window is bounded by the
  pass's own hop count, not by the span: with the raced object first, *no* delay ≥ 0 can land before
  the resolver's settling read, so a wider span adds only `PastTheRead` samples. Two fillable objects
  ahead of the race (4 hops) is what actually opens it — delays 0–3 land inside, 4 ties, 5–6 land
  past.

## Gate evidence run locally, through the project's own runner

* `./engine/xtask.sh ci` (the whole Wyrd gate: fmt + clippy `-D warnings` + build + test + DST +
  cargo-deny + conformance vectors) → **`xtask ci: all checks passed`**.
* `./engine/xtask.sh dst` → the custodian campaign runs **16** properties over the 50-seed sweep,
  all green, including `backfill_never_acts_on_a_generation_it_did_not_read` and
  `backfill_covers_the_generation_flip_window`.
* `cargo fmt` run over both touched crates; the target's commit hooks (fmt/clippy) are what
  `xtask ci` runs, so the patch is commit-ready.
* `crates/custodian/tests/backfill.rs` is **unmodified** and green, as the brief requires.

## Budget

| File | Bound | Measured |
|---|---|---|
| `crates/custodian/src/backfill.rs` | ≤ 130 added semantic | **111** |
| `crates/custodian/tests/segmented_map_backfill.rs` | ≤ 320 semantic / 520 raw | **315 / 492** |
| `crates/dst/tests/custodian.rs` | *(third file — human-directed, see above)* | +279 raw |

## Not done, deliberately

No `Cargo.toml`, no docs, no ADR/spec, no conformance vector, no sibling file (`rebalance.rs`,
`reconstruction.rs`), no `gc.rs`/`scrub.rs`/`restore.rs`/`desired_state.rs`. The backfill metrics
this patch adds join a set that is documented nowhere in `docs/` or `deploy/` (grepped:
`backfill_placement_remaining` appears only in `crates/`), so there is no metric surface to update —
consistent with the brief's Plan-time docs check. The segmented **write** path stays #682's, marked
`// deferred: #682` at `crates/custodian/src/backfill.rs:245`.
