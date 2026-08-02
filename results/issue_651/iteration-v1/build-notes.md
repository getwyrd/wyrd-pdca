# Build notes — issue 651 (withheld from the reviewer)

## What I built, and why (rationale, not a patch summary)

The brief's four numbered criteria all reduce to one shape: **the five maintenance passes
that repair/move/rewrite a chunk map must go through the same shared resolver GC/scrub
already adopted in #650, per object, once per pass — and a maintenance repoint must be
able to target either shape of record (flat root or `seg:` segment) without ever
persisting past the value ceiling.** Everything below follows from that.

### 1. `crates/core/src/metadata.rs` — the homed resolver + `repoint_chunk`

The existing `resolve_chunk_map`/`resolve_current_chunk_map` (#649) answer "what chunks
does this object own", flattening segments into one ordered `Vec<ChunkRef>` and throwing
away *which record* held each one. A maintenance repoint needs the opposite: not just the
chunk, but the record to CAS against. I added a parallel pair —
`resolve_snapshot_homes`/`resolve_chunk_homes`/`resolve_current_chunk_homes`
(`metadata.rs:2691-2782`) — that reuse the *same* private `read_segments`/`root_dropped`
machinery (so the two resolvers can never disagree about which generation answered) and
additionally carry a `ChunkHome` (`metadata.rs:2657-2677`): `Root { position }` for a flat
map, `Segment { key, record, position }` for a segmented one. `repoint_chunk`
(`metadata.rs:2805-2870`) is the CAS: for `Root` it's the same
`require(inode)/put(inode)` shape every repair CAS already used; for `Segment` it adds a
**second** precondition — `require(inode_key, encode(prior_root))` beside
`require(seg_key, encode(prior_segment))` — which is what makes a repoint racing a
concurrent supersede/retirement lose the CAS instead of writing into (or resurrecting) a
generation nothing live names any more (0016 decision 7(f)). Both arms run
`check_record_ceilings`/`check_value_ceiling` (`metadata.rs:2882-2903`) before returning
the batch, refusing (typed `ValueOverCeiling`) rather than persisting a record that would
land over `MAX_VALUE_BYTES`.

**What I did NOT carry over from `sources/salvage.diff`:** the salvage's `metadata.rs` is
a *different, larger* design for the whole 0016 chunk-segmentation feature (its own
`ChunkMapError` variant set, its own `read_group_range`/`retired_or`, a `SegmentedPublication`
committer, `check_fenced`, etc.) — most of that is **already landed** in this tree by #648
under different names/shapes. Porting the salvage's `repoint_chunk`/`ChunkHome` verbatim
would have meant reconciling two incompatible resolver designs. Instead I re-derived
`repoint_chunk` and the ceiling helpers against *this tree's actual* `resolve_chunk_map`/
`read_segments`/`SegmentRecord::new` primitives, keeping the same three ideas the brief
calls out (record-identity CAS, the second-precondition supersede guard, the ceiling
refusal) without importing the salvage's committer, fence checks, or its speculative
`ChunkMapError` variants (`SegmentGroupMismatch`, `Unfenced`, etc.) that this slice's scope
explicitly excludes ("carve out only the record-ceiling helpers `repoint_chunk` needs, not
the committer around them").

**Cost of the alternative I ruled out** (reusing salvage's `crate::resolve` module
wholesale, cited in `sources/salvage.diff:3200-4300` for `ChunkHome`/`repoint_chunk`, and
its own `read_group_range`/`retired_or` reimplementation, ~1,100 lines): it duplicates
logic this tree's `read_segments`/`root_dropped`/`decode_segment_record` already provide,
and — because it never resolves generation identity the same way `resolve_chunk_map`
does — a maintenance pass's containment and the read path's containment could disagree
about which generation is live. Reusing the existing private helpers (~230 lines added:
`ChunkHome`/`HomedChunk` ~25, the two homed-resolve functions ~90, `repoint_chunk` ~65,
ceiling helpers ~25, three new `ChunkMapError` variants ~30) is the smaller, single-source
change.

### 2. `crates/custodian/src/reconstruction.rs` — criteria (2) and (4)

Replaced the per-queued-chunk `find_chunk` (a full `meta.scan(b"inode:")` **per chunk in
the queue**, and a hard `SegmentedMapUnsupported` error the moment it met any committed
segmented record — reconstruction.rs:322-338, :620-646 on `origin/main`) with
`build_chunk_index` (`reconstruction.rs:668-731`): one scan, resolving every committed
object's homed chunk list **once**, building a `HashMap<ChunkId, ChunkLocation>` plus an
`unresolvable: bool` flag. `assess` (`reconstruction.rs:359-390`) now looks the chunk up in
that index — O(1) — instead of re-scanning. This is exactly what criterion (4)'s
instrumented-store test (`segmented_map_repair.rs`,
`reconstruction_resolves_each_objects_root_once_per_pass_not_once_per_queued_chunk`)
counts: Q=6 obligations over N=3 segmented objects cost **3** `seg:`-range reads, not 18.
`repair_chunk`'s commit (`reconstruction.rs:571-650`) now builds its batch through
`metadata::repoint_chunk`, catching a refusal (`Err`) as `RepairOutcome::Aborted` +
`emit_repoint_refused` (`reconstruction.rs:632`, `:798-810`) rather than propagating it —
one chunk's ceiling refusal must not fail the whole pass.

**Alternative ruled out:** memoizing per-object resolves in a `HashMap` built lazily
*inside* `assess` (only resolving an object the first time one of its chunks is queued),
which is closer to what a naive "close but wrong" fix would do (the brief's own warning:
"the perf finding … the deployed custodian's repair loop turns Q namespace scans into
Q × N point reads" — a lazy per-first-touch cache over Q chunks *sharing* N objects still
costs O(min(Q,N) × object-count-per-lookup) in the worst distribution, and more subtly it
still requires **one scan of every committed key per queued chunk** to find which object
(if any) owns it, because a lazy cache only helps once you already know which object to
look in.  Building the full index up front is the only way to make the store scan itself
happen once per pass, which is what criterion (4) actually measures.

### 3. `crates/custodian/src/rebalance.rs` — criterion (2), evacuation + ceiling

`plan_evacuations` (`rebalance.rs:158-236`) now resolves each committed object's homed
chunks once via `metadata::resolve_chunk_homes`, containing (not propagating) an
unresolvable object — `emit_unresolvable` (`rebalance.rs:410-422`) — so one damaged record
doesn't blank the whole drain. `evacuate_chunk` (`rebalance.rs:260-345`) commits through
`metadata::repoint_chunk`, same refusal handling as reconstruction
(`rebalance.rs:322`, `emit_repoint_refused` at `:425-437`). The `EvacPlan.scheme` field
replaces the old code's re-read of `prior_chunk_map[chunk_index].scheme` for the
`fragment_intact` check, since a segmented chunk's `ChunkRef` is no longer indexed off a
flat `prior.chunk_map`.

### 4. `crates/custodian/src/restore.rs` — criterion (1)

The mark half (pass 1/2) already routed through `gc::referenced_fragments` as of #650
(`restore.rs:183-199`'s own comment already said so) — the acute defect left was pass 3's
`committed_chunks`, which built its own `Expected{k, frags}` table via a **second** scan
that called `record.chunk_map.as_flat().ok_or(SegmentedMapUnsupported)?` per committed
record (origin/main `restore.rs:400-406`), erroring the *whole* `reconcile_after_restore`
call the moment it met a segmented record. Rather than teach that second scan to resolve
segments too (a second, independently-fallible resolution of the same objects GC's
`referenced` set already resolved), I deleted `committed_chunks` entirely and derive the
per-chunk `Expected` directly from the **already-built** `referenced` set
(`restore.rs:339-378`): group `referenced.placed` by `frag.chunk`, read `k` off
`referenced.schemes`. This is smaller (no second scan, no second resolve) and structurally
cannot disagree with the mark half about which chunks are referenced, since both read the
same `ReferenceSet`. `RestoreReport::unresolvable` (`restore.rs:143-152`) surfaces objects
`referenced_fragments` could not read, named via `crate::gc::object_name` — every verdict
in the report (dangling/misplaced/under-replicated) is drawn only over what the pass could
resolve, so the field says how much of the store the "clean" verdict actually covers.

### 5. `crates/custodian/src/backfill.rs` — criterion (3)

Per the brief's own "do-not-re-earn" item (iv): skipping a segmented record with a stated
reason is the accepted disposition, not a gap to close by making backfill *resolve* one.
So the change is deliberately small: `if record.chunk_map.is_segmented() { emit_skipped_segmented(inode_id); continue; }`
(`backfill.rs:100-102`) before the flat-only classify/fill logic runs at all — the
commit path this pass takes is structurally unreachable for a segmented record, so
"never rewrite a record it did not read" holds by construction, not by a runtime check
inside the rewrite path. `emit_remaining` (`backfill.rs:171-195`) skips a segmented record
the same way rather than erroring the whole gauge scan.

### 6. `crates/custodian/src/desired_state.rs` — attribution (secondary, not one of the
four numbered criteria, but explicitly named in the file's own `deferred: #651` comment)

Added `ReconciliationStatus::PendingUnresolvable { objects }` and had
`reconciliation_status` return it (named, sorted) instead of the plain unattributed
`Pending` for an incomplete reference set (`desired_state.rs:188-198`). This forced
updating **two** existing #650 assertions in `segmented_map_consumers.rs` (lines ~721 and
~1094 on the pre-patch tree) that literally asserted `Pending` for this case and said in
their own comments "the attributed answer is #651's" — I updated them to assert
`PendingUnresolvable` (counting distinct names, not spelling, matching that file's own
stated philosophy for the analogous `attributed_objects` helper). This is the one place I
touched a file outside the five named passes + `metadata.rs`; I judged it in-scope because
(a) the brief explicitly lists `desired_state.rs` in Scope, (b) the existing code already
names #651 as the slice that closes it, and (c) leaving it unattributed while every other
container in this slice attributes its blocker would be an inconsistent containment shape
across the same feature set.

### 7. The DST property (`crates/dst/tests/custodian.rs`)

Added `prop_repoint_loses_to_a_concurrent_supersede` (new "property 10" section) plus a
`SupersedeOnFetch` `ChunkStore` wrapper that commits a scripted root-flip the first time
*any* fragment is fetched from the draining server — the exact window between
`plan_evacuations` resolving a clean home over the OLD generation and `evacuate_chunk`'s
own commit. This exercises `repoint_chunk`'s second precondition
(`require(inode_key, encode(prior_root))`) directly: the repoint must lose the CAS, the
live (new) generation must read back intact, and the retired generation's `seg:` records
must be byte-identical afterward. Wired into the campaign via `dst_campaign_test!`
(not into `REGRESSION_SEEDS` — no bug-finding seed exists yet for it, so there is nothing
to pin there). Per the brief's "Verification posture", this is **not** part of the
`C4-verify` discriminator — it needs `--cfg madsim` — but it does ship in this patch and I
verified it red→green under madsim myself (below), not left for Check to discover it
doesn't exist.

## What I ruled out entirely

- **Reusing `sources/salvage.diff`'s `crate::resolve` module verbatim** — see §1 above.
  Concrete cost: that module (plus its own `ChunkHome`/committer helpers) is ~1,100 lines
  spanning `metadata.rs:3150-5350` of the salvage diff and duplicates resolver machinery
  this tree already has under different names; the actual change is ~230 lines reusing
  the existing private helpers.
- **Teaching `restore::committed_chunks` to resolve segments itself** (a second,
  independent resolve of the same objects) — ruled out for the reason in §4: it can
  disagree with the mark half's `ReferenceSet` about which chunks are referenced, which is
  exactly the kind of divergence issue #508-attempt-4 was rejected over (cited in the
  salvage's own comments, `sources/salvage.diff:108-110`). Deriving `Expected` from the
  already-built set removes the second resolve and the divergence risk together, and is
  smaller (restore.rs's net diff is -19/+56 vs. the salvage's own +85 for the equivalent
  section, `sources/salvage.diff:58-140`).
- **Making backfill resolve and identity-fill a segmented record** — explicitly rejected
  by the brief's own do-not-re-earn item (iv); recorded in `review-rejected.md`.
- **A per-chunk lazy memoization cache in `reconstruction::assess`** instead of an
  up-front index — ruled out in §2: it still costs a scan per queued chunk to find which
  object (if any) owns it, so it does not actually achieve O(N).
- **Running the full `cargo xtask ci`** (fmt, clippy, workspace test, `cargo-machete`,
  `cargo-deny`, conformance, statics, orchestrator guard, 50-seed DST sweep) as my own
  red→green check — this is Check's gate (`C4-ci`), not a bounded command I should
  hand-roll inside this Do beat's time budget (a 50-seed DST sweep alone can run several
  minutes). I ran the project's own `cargo test`/`cargo check`/`cargo clippy`/`cargo fmt`
  (see below) scoped to the crates this patch touches, plus a smaller (8–30 seed) madsim
  sweep of just the new DST property and a full (non-sweep) run of the whole
  `crates/dst/tests/custodian.rs` file at `MADSIM_TEST_NUM=6`, all green. Check's
  `C4-ci`/`C4-verify` gates re-run the real suite at full scale.

## Budget

Brief: ≤ ~1,500 added semantic lines (non-blank/non-comment/non-mechanical), ≤ 15 files;
salvage's own estimate for equivalent production code was 279 + ~31 = 310 lines. This
patch touches **10** files. I did not run a mechanical semantic-line counter (no such
tool was named in `External dependencies`), but by inspection the production-code diffs
(`metadata.rs`, `backfill.rs`, `desired_state.rs`, `rebalance.rs`, `reconstruction.rs`,
`restore.rs`) are in the same order of magnitude as that estimate (each function replaced
is comparable in size to what it replaced, plus the new homed-resolution machinery
`metadata.rs` needed beyond the ~31-line ceiling-helpers estimate — see §1's "cost of the
alternative" for why that machinery is necessary and not just the two ceiling functions).
The bulk of the raw diff size is `segmented_map_repair.rs` (762 lines) and the DST
addition (~220 lines), both mostly doc comments and fixture code, which the brief's own
budget note anticipates ("the whole budget risk is tests"). I did not hit the "STOP and
hand back a proposed split" threshold at any point.

## The three refutation questions

**(a) Genuine red?** Yes, verified by actually reverting the fix. I ran
`git stash push -- <the six changed pass/metadata files + segmented_map_consumers.rs>`,
keeping only the new `segmented_map_repair.rs`, and ran
`cargo test -p wyrd-custodian --test segmented_map_repair`: all 6 tests **FAILED**, each
via a runtime panic on `SegmentedMapUnsupported { operation: "…" }` propagating out of
`reconcile_step`/`reconcile_after_restore`/`backfill::reconcile` — an assertion-style
failure (the test harness reports it as `FAILED`), never a compile error (I confirmed the
file itself still compiles pre-fix — one iteration was needed to remove a
`report.unresolvable` field access that IS patch-added and would otherwise have made the
RED leg a *compile* red, which the falsifiability note explicitly forbids). I did the same
for the new DST property (`git stash` the five pass files + `metadata.rs`, keep
`crates/dst/tests/custodian.rs`): `repoint_loses_to_a_concurrent_supersede` failed the
same way (`Store(SegmentedMapUnsupported { operation: "rebalance::plan_evacuations" })`).
Then `git stash pop` and re-ran everything green (recorded below).

**(b) Production path?** Yes. Every assertion drives the real entry points named in the
brief: `wyrd_custodian::reconcile_step` (dispatching the real `reconstruction::reconcile` /
`rebalance::reconcile` / `gc::reconcile`), `wyrd_custodian::reconcile_after_restore`, and
`wyrd_custodian::backfill::reconcile` — never a copy or a parallel re-implementation. The
DST property drives the same `reconcile_step` under the `madsim` scheduler.

**(c) Fixture includes the fault?** Yes. Every fixture in `segmented_map_repair.rs` seeds
a genuinely **segmented** object (`ChunkMap::Segmented`, raw `seg:` records, no committer)
— never a flat map standing in for one — and the criterion-2 tests seed a genuinely
**missing** fragment (server excluded from the fleet, matching the existing repo-wide
"kill a D server" convention) or a genuinely **draining** server
(`desired_state::set_lifecycle`), not a healthy fleet with the fault filtered out. The
ceiling test seeds a segment record that is verifiably (`assert!`) already over
`MAX_VALUE_BYTES` before the pass runs. The DST property's supersede fires for real (via a
committed `WriteBatch`), not a flag the property merely checks.

## Commands run (all green post-fix; the custodian/core targeted subset is what a
`cargo test -p wyrd-custodian -p wyrd-core` run drives; `cargo xtask ci` is Check's to run
at full scale)

```
cargo fmt --all -- --check                                  # clean
cargo check --workspace --all-targets                       # clean
cargo clippy -p wyrd-core -p wyrd-custodian --all-targets    # clean
cargo test -p wyrd-core --lib                                # 42 passed
cargo test -p wyrd-custodian                                 # all pre-existing + new tests pass
  (gc.rs, gc_delete_backstop.rs, gc_telemetry.rs, rebalance.rs, reconstruction.rs,
   restore_reconcile.rs, scrub.rs, segmented_map_consumers.rs, segmented_map_repair.rs,
   backfill.rs, backfill_telemetry.rs, skeleton.rs — 0 failures)
RUSTFLAGS="--cfg madsim" cargo check -p wyrd-dst --test custodian     # clean
RUSTFLAGS="--cfg madsim" cargo clippy -p wyrd-dst --all-targets       # clean
RUSTFLAGS="--cfg madsim" MADSIM_TEST_NUM=6 cargo test -p wyrd-dst     # all 13 custodian.rs
  properties pass (incl. the new one), plus network.rs / no_fdb_linkage.rs /
  tikv_await_commit_interleaving.rs unaffected
RUSTFLAGS="--cfg madsim" MADSIM_TEST_NUM=30 cargo test -p wyrd-dst --test custodian \
  repoint_loses_to_a_concurrent_supersede                            # green over 30 seeds
python3 docs/publishing/tools/lint_docs.py                           # OK
typos docs/design/architecture/06-runtime-view.md                    # clean (typos IS
  installed locally, so this ran for real rather than warn-and-skip)
```

I did **not** run the full `cargo xtask ci` (fmt+clippy+build+test+machete+deny+
conformance+statics+orchestrator-guard+50-seed-dst) — see "What I ruled out" above. This
is not a NEEDS-HUMAN gap in the sense of a missing external dependency (both `typos` and
`docs-renderer` deps were present and I ran them); it is a scope choice given the size of
this slice and the Do beat's own guidance against hand-rolling a long-running gate run.
Check's `C4-ci` runs the real thing at full scale.

## Citations (path:line on this target branch, i.e. this worktree, post-patch)

- `crates/core/src/metadata.rs:2657-2677` — `ChunkHome`/`HomedChunk`.
- `crates/core/src/metadata.rs:2691-2782` — `resolve_snapshot_homes`/`resolve_chunk_homes`/
  `resolve_current_chunk_homes`.
- `crates/core/src/metadata.rs:2805-2870` — `repoint_chunk`.
- `crates/core/src/metadata.rs:2882-2903` — `check_record_ceilings`/`check_value_ceiling`.
- `crates/custodian/src/reconstruction.rs:359-390` — `assess`, index lookup replacing
  per-chunk `find_chunk`.
- `crates/custodian/src/reconstruction.rs:571-650` — `repair_chunk`'s `repoint_chunk`-based
  commit + refusal handling.
- `crates/custodian/src/reconstruction.rs:668-731` — `build_chunk_index`.
- `crates/custodian/src/rebalance.rs:158-236` — `plan_evacuations` over homed chunks.
- `crates/custodian/src/rebalance.rs:260-345` — `evacuate_chunk`'s `repoint_chunk`-based
  commit.
- `crates/custodian/src/restore.rs:143-152` — `RestoreReport::unresolvable`.
- `crates/custodian/src/restore.rs:339-378` — `Expected` derived from `referenced`, no
  second scan.
- `crates/custodian/src/backfill.rs:100-102` — the segmented-record skip.
- `crates/custodian/src/desired_state.rs:96-104`, `:188-198` — `PendingUnresolvable`.
- `crates/custodian/tests/segmented_map_consumers.rs` (updated assertions, ~lines
  717-728, 1088-1102) — the two `Pending` → `PendingUnresolvable` updates #650 flagged as
  deferred to this slice.
- `crates/custodian/tests/segmented_map_repair.rs` — the new discriminator (all six
  criteria).
- `crates/dst/tests/custodian.rs` — `SupersedeOnFetch` + `prop_repoint_loses_to_a_concurrent_supersede`,
  wired into `dst_campaign_test!` (new "property 10" section, immediately before "the seed
  sweep").
- `docs/design/architecture/06-runtime-view.md` §6.2 step 2 — the containment paragraph
  extended with the repair/evacuation-walk sentences.

## NEEDS-HUMAN

None. No external dependency named in the brief was missing (`typos`, `docs-renderer`
deps both present and run for real). The one thing I deliberately did **not** do — a full
`cargo xtask ci` run, including the 50-seed DST sweep — is a scope/time choice for this Do
beat, not a missing capability; I validated the new DST property directly under madsim
(8 and 30 seed sweeps, plus the whole `custodian.rs` file at a smaller sweep) instead of
leaving it unverified for Check to discover a compile or logic problem.
