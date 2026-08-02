# Build notes — issue #635 (segmented-chunk-map), **iteration 2**

Withheld from the reviewer; written for the human at sign-off.

Target branch: `getwyrd/wyrd @ main`. All edits made in `$PDCA_WORKTREE`
(`/home/eddie/development/wyrd/wyrd.pdca-wt-l0`, base `b0cd199` = `origin/main`).
45 files, +5713 / −330. `patch.diff` is the working-tree diff against `b0cd199`.

---

## 0. The three forced questions (answer them first)

**(a) Genuine red? YES — measured through the project's own runner, twice.**
`engine/scripts/run-verify.sh` (the C4-verify gate) resets `../wyrd-verify` to `origin/main`,
applies `patch.diff`, reverts every production file, keeps the added test:

```
run-verify.sh: GREEN — cargo test -p wyrd-custodian --test segmented_map_consumers (fix applied)
running 8 tests … test result: ok. 8 passed; 0 failed
run-verify.sh: RED — … (production reverted, test kept)
running 8 tests … test result: FAILED. 0 passed; 8 failed
run-verify.sh: PASS — red without the fix, green with it.
```

**8 tests ran on the RED leg and 8 failed** — the brief demands this number be recorded. The
red is **assertions / propagated `Err`s, not a build error**: every failure message is
`invalid type: map, expected a sequence, line 1 column 23` (the strict
`metadata::decode(&value)?` at `crates/custodian/src/gc.rs:265` refusing the segmented value)
surfacing through `reconcile_step`, `reconcile_after_restore`, `reconciliation_status`,
`read_object` and `backfill::reconcile`. The file compiles on the pre-fix tree because it names
**only base symbols** and seeds the segmented object by **raw record bytes**.

Two further targeted counterfactuals (both reverted afterwards):

* neutralising backfill's segmented decision (`if record.chunk_map.is_segmented()` → `if false`,
  `crates/custodian/src/backfill.rs:134`) → `backfill_never_rewrites_a_segmented_map_even_with_an_empty_placement`
  FAILS while the other 7 stay green. That is the adversary's #1 finding, now discriminated;
* disabling the resolve restart (an early `return Ok(None)` in `resolve_live_chunk_map`,
  `crates/core/src/metadata.rs:1876`) → `a_get_racing_an_overwrite_serves_the_new_generation_not_a_404`
  (`crates/server/src/lib.rs:909`) and
  `a_retired_generation_resolves_against_the_root_that_replaced_it` FAIL.

**(b) Production path? YES.** Leg A drives the real `reconcile_step`
(`crates/custodian/src/reconciliation.rs:65-114`), the real `reconcile_after_restore`, the real
`desired_state::reconciliation_status`, the real `backfill::reconcile`, the real
`core::read::read_object` — no doubles of any of them. The only stand-ins are the *stores*
(`MemMeta`/`MemDServer`, the shape `crates/custodian/tests/gc.rs:26-120` uses) and, in leg B,
`RedbMetadataStore::in_memory()` — a real backend. The gateway legs drive `Gateway::get_object_streaming`
/ `get_object_range` over real redb + real `FsChunkStore`.

**(c) Fixture includes the fault? YES.** The segmented object is *in* every fixture, on servers
0–2, with **real erasure-coded fragments** written through `write_fragments`; the flat object is
on the disjoint half so leg A(ii)'s `Pending` can only come from resolving the `seg:` range. The
rebalance leg drains a server the **segmented** object sits on. The backfill discriminator seeds a
segmented chunk with `"placement":[]` — the one shape backfill's rewrite would otherwise reach.
Nothing is curated out: leg A(iv) now runs the real restore pass and then crosses the grace window
**with no assertion in between**, so it fails if restore marks a live segmented object.

---

## 1. What this iteration changed, finding by finding

The previous attempt's design (two-variant `ChunkMap`, one shared resolver, staged publication)
survived review — C1/T1/T3 PASS, the adversary reproduced the red→green and could not refute the
consumer sweep. It was rejected on **20 T4 blockers, 25 surviving mutants, and an E0046 on the
#634 stack**. All 20 are dispositioned below (16 fixed, 4 recorded-rejected in
`review-rejected.md`), all 8 adversary findings are fixed, mutants are down to **1 (equivalent)**,
and the #634 fold is verified green.

### The one design change (fixes 5 of the 20 in one place)

`MapResolution::Retired` was being treated as "no chunks" by every consumer — `read.rs:514`,
`server/lib.rs:369`, `custodian/src/resolve.rs:35`, and by bare `continue`s in
`rebalance`/`reconstruction`. Decision 7(h) says a maintenance pass *drops the stale resolution*;
the previous patch dropped **the object**. The adversary's strongest finding showed why that is
#508-attempt-4 in a new spelling: `restore.rs:222→:266` manufactures `orphan:` evidence for an
object it cannot resolve, and `desired_state.rs:157-165` turns the same absence into a positive
`Satisfied`.

New: `metadata::resolve_live_chunk_map` / `resolve_live_chunk_homes`
(`crates/core/src/metadata.rs:1876`, `:1969`) — **total** resolvers. On `Retired` they re-read the
root and resolve the generation that replaced it (up to `MAX_RESOLVE_RESTARTS = 3`), then **fail
closed** with `ChunkMapError::MapResolutionUnstable`. `Ok(None)` now means exactly one thing: no
live *committed* generation (deleted, or not committed) — the condition every one of these loops
already skipped on before segmentation existed. The homed variant hands back the root it restarted
against, so a repoint re-plans against the live generation instead of losing its CAS.
`custodian::resolve::{chunks_of, homes_of}` (`crates/custodian/src/resolve.rs:34`, `:52`) are the
maintenance plane's single entry; rebalance/reconstruction/backfill/gc/restore all go through them,
so no site drops a generation with a bare `continue` and every drop/restart is emitted on the
durability seam.

### The remaining fixes

| # | Finding | Fix | Where |
|---|---|---|---|
| 1,5 | `read.rs` Retired → false 404 | restart; framing taken from the served generation | `crates/core/src/read.rs:505-520` |
| 2 | `resolve.rs` drops a superseded generation | total resolver (above) | `crates/custodian/src/resolve.rs` |
| 3,9 | unchecked `u64` chunk-length sums | `checked_chunk_bytes` + `SegmentLengthOverflow`; root tiling gets `SegmentSpanOverflow` | `crates/core/src/metadata.rs:880`,`:909`,`:663` |
| 4 | `flip_batch` cannot express a fresh key | `RootPrecondition::{Fresh, Supersede}` → `require_absent` + version 1 | `crates/core/src/metadata.rs:2141`,`:2294` |
| 6 | gateway GET 404s on an overwrite race | both GET paths restart; range/ETag framed from the served generation | `crates/server/src/lib.rs:344-373`,`:432-470` |
| 12,16 | no seeded coverage of the crash between segment writes and the flip | DST property 10 (interrupted / raced / ambiguous-recovery arms) | `crates/dst/tests/custodian.rs:1501` |
| 13,14 | rebalance evacuation of a segmented chunk untested | new leg: drains a server the segmented object sits on, asserts convergence + `seg:` repoint + untouched root + `orphan:` evidence | `crates/custodian/tests/segmented_map_consumers.rs:836` |
| 15 | DST never raced a repoint against a supersede/delete | DST property 11 (3 interleavings) | `crates/dst/tests/custodian.rs:1642` |
| 17 | decode never checked the table's span against `size` | `InodeRecord` decodes through `InodeRecordWire`; `SizeSpanMismatch` | `crates/core/src/metadata.rs:1074`,`:1087` |
| 18 | epoch parsed with `from_str` (`+7`, `07`) | `parse_canonical_u64` | `crates/core/src/metadata.rs:978`,`:988` |
| 19 | one static fence, no per-batch cursor | `segment_progress: Option<&dyn Fn(&SegmentBatchInfo) -> WriteBatch>` merged into **each** batch | `crates/core/src/metadata.rs:2155`,`:2257` |
| 20 | backfill skipped before `checked_fragments` | classification moved **before** the segmented decision; the skip reports `unfilled` | `crates/custodian/src/backfill.rs:95-137`,`:243` |

### Recorded rejections (4, one class)

`crates/custodian/{reconstruction.rs:564,:579, rebalance.rs:284,:301}` — "writes the destination
before any orphan pre-mark". Declined as out of scope with an in-code deferral marker
(`crates/core/src/metadata.rs:2017`, `// deferred: #636`) and rows in
`results/issue_635/review-rejected.md`.

**Why, with the cost shown rather than an adjective.** I checked whether the *flat* path has a
guard the segmented path drops: `grep -n "require_absent\|desired_key\|orphan_key" crates/custodian/src/{rebalance,reconstruction}.rs`
returns only the **vacated-position** `orphan_key` puts (`rebalance.rs:307`, `reconstruction.rs:583`).
So on `origin/main` the flat repoint already writes the destination fragment first and its comment
says so (`rebalance.rs:258-259`: "a crash here leaves only collectable garbage"). This slice's
segmented path is the same shape **plus** a stricter precondition. Implementing the pre-mark half
alone would write `orphan:<P_new>` records that no drain yet consumes — X47 closes only when the
retirement drain re-reads each segment's current placement at drain time (`0016:2416-2430`), which
is #636's `retire:bytes:{generation}`. Doing it here means: a pre-mark batch + an adoption delete on
win + a "leave it" branch on loss in **two** call sites, plus the flat path (it has the same hole,
so fixing only the segmented one would make the two shapes disagree) ≈ 120–160 lines across
`rebalance.rs`, `reconstruction.rs`, `metadata.rs` and their tests, in a slice already at 45 files —
and it would still not close the race without #636's half. The exposure is unchanged from today's
flat behaviour, which is the honest reason, not the size.

---

## 2. The #634 stack base — what I did and why the shipped file cannot carry `scan_page`

The reviewer's C2/C3 FAIL and C4 NEEDS-HUMAN were: on `origin/main + #634` the added test's
`MemMeta` lacks `scan_page`, so the stack stops at **E0046**.

**The constraint is a hard fork, not an oversight.** `scan_page` is a *required* trait method on
#634's branch (`crates/traits/src/lib.rs`, `) -> Result<ScanPage>;`, no default body). A file that
implements it does **not** compile on `origin/main` (E0407, "not a member of trait
`MetadataStore`"); a file that omits it does not compile on main+#634 (E0046). No `cfg`
distinguishes them, and the double must live **inside the added test file**, because `run-verify.sh`
reverts modified production files and deletes added non-test files on the RED leg
(`engine/scripts/run-verify.sh:418-427`) — a double in `wyrd-testkit` would vanish exactly when the
red is measured.

**What base is actually operative for this bundle:** `results/issue_635/` has **no `stack-base`
file**, so `gates.py:352-360` exports no `$PDCA_VERIFY_BASE` and `run-verify.sh` resolves
`origin/main` (`:186-206`); the Do worktree is at `b0cd199` = `origin/main`; `origin/pdca-integration/main`
does not exist on the remote; #634 is an **open draft PR** (#645), not merged. So a patch carrying
`scan_page` would fail C4-verify's GREEN leg outright and would not apply-and-build on the branch
publish opens the PR against. I shipped against the operative base and **verified the fold
separately**:

```
git worktree add --detach $PDCA_SCRATCH/pdca-builder-635-fold origin/enhancement/634-scan-page-seam
git apply -3 patch.diff            # applies cleanly, every hunk
# + 4 delegating bodies (the one #634 prescribes: wyrd_testkit::test_double_scan_page)
cargo test -p wyrd-core -p wyrd-custodian -p wyrd-server   # ALL GREEN
cargo test -p wyrd-custodian --test segmented_map_consumers # 8 passed
```

The fold delta is **+36 lines, 8 `scan_page` lines across 4 doubles** — `MemMeta`
(`crates/custodian/tests/segmented_map_consumers.rs`), `Shuffling` and `MovingRoot`
(`crates/core/src/metadata.rs` `#[cfg(test)]`), `SupersedeMidResolve`
(`crates/server/src/lib.rs` `#[cfg(test)]`), each body identical to the one #634 already added to
~34 existing doubles (e.g. `crates/custodian/tests/gc.rs:73-80`). The scratch worktree was removed
(`git worktree remove --force` + `rm -rf`).

**For the human:** if you want the shipped patch to be the *stacked* one instead, the operative
base has to change first (merge #645, or have the driver stamp `stack-base` and cut the worktree
off the integration branch); then the four `scan_page` bodies go in and this patch is stack-green
as measured above. It cannot be both at once.

---

## 3. Mutation (the C5 gate that failed at 25 survivors)

`scripts/mutants-in-diff` on the final `patch.diff`: **246 mutants, 1 missed, 105 caught, 140
unviable** (was 193 tested / 25 missed). The survivors named in the carry-forward are dead:

* the **flat repoint's map/version writes** (`metadata.rs:1775` in v1) — killed by
  `a_flat_repoint_rewrites_the_map_and_bumps_the_version_by_one`;
* the **segment fence** (`metadata.rs:1972` in v1, `fenced_segment_batch` → `Default::default()`) —
  killed by `a_segment_write_batch_that_loses_its_fence_writes_no_segment`;
* the **byte-budget split** and the **`MAX_ROOT_SEGMENTS` ceiling** (6 + 2 survivors) — killed by
  `the_batch_split_is_byte_budgeted_and_never_drops_a_record`,
  `a_chunk_that_exactly_fills_a_segment_still_shares_it`,
  `the_root_segment_ceiling_is_enforced_at_publication_on_both_sides`;
* `SegmentBoundsMismatch`'s `||` and the resolver's group/epoch `||` — killed by
  `a_segment_whose_span_disagrees_with_the_root_fails_closed` (each side separately) and
  `a_row_from_another_group_or_epoch_is_never_folded_into_this_map`.

**How the ceiling and the split became testable at all.** At the real constants the batch split
first fires at ~113 000 chunks and the ceiling at ~500 000 — fixtures a unit test cannot afford,
and a mutation run repeats the suite once per mutant. So the three capacity constants are now
**parameters of the production bodies** (`plan_with`, `staged_batches`, `batch_ranges`), with the
public entries passing the real ones and `staged_batches` clamping the budget to `MAX_BATCH_BYTES`
so no caller can raise it. The tests drive the *production* code at small parameters; the real
constants are separately pinned by `the_capacity_constants_are_the_ones_the_arithmetic_assumes`.

**The one survivor is equivalent, and it is checkable in two lines:**
`crates/custodian/src/backfill.rs:158` `delete field size from struct InodeRecord expression` —
the very next lines are `..record.clone()` (`:165`), which supplies the identical `size`. Deleting
the explicit field changes no behaviour, so no test can kill it. I left the (pre-existing) line
rather than churn it for a gate number.

---

## 4. The brief's open questions, answered

1. **Encoding** — implemented exactly as pinned; `the_segmented_encoding_is_the_settled_json_both_ways`
   asserts the literal bytes both ways, so leg A's hand-written fixture stays honest. No deviation.
2. **Where the resolver lives** — `crates/core/src/metadata.rs` (never duplicated; `custodian` has a
   3-line adapter that adds only telemetry).
3. **Backfill** — **resolve, classify, then decline to rewrite**, and the decline is now a decision
   *with* an assertion: classification (`checked_fragments`) runs **before** the segmented branch, so
   a malformed placement still raises the operator signal, and the skip reports the count of chunks
   it declined to fill. Two tests bind it: the byte-identity one with an empty-placement segmented
   chunk (which discriminates the guard) and a telemetry one asserting the remaining-gauge counts the
   segmented population rather than silently excluding it.
4. **Fence-as-parameter committer** — kept (the first option). The flip carries the caller's
   preconditions **and** mutations in one batch, and phase 1 now takes a per-batch caller
   contribution too, which is what makes #636's `segments_written` cursor expressible. Root
   preconditions are `Fresh` (require_absent, version 1) or `Supersede(prior)` (CAS, version+1).

---

## 5. What I ruled out

* **Killing the four repoint findings by implementing X47's pre-mark here** — see §1; cost quantified,
  and half the protocol is worse than none because the evidence it writes has no consumer yet.
* **Shipping `scan_page` in the added test file** — would fail C4-verify's GREEN leg on the operative
  base (E0407). §2.
* **Adding `wyrd-metadata-redb` to `wyrd-custodian`'s dev-deps** so leg A could use a real store and
  need no double at all (which would have made the #634 fold a no-op for this file): a modified
  `Cargo.toml` is **reverted on the RED leg**, so the added test would fail to *build* pre-fix — the
  assertion red is worth more than the fold convenience, and the brief forbids it explicitly.
* **A `batch_budget_bytes` public field** to make the split testable: a knob a caller could set wrong.
  Private parameterised bodies with a clamp achieve the same and cannot be misused.
* **Asserting the envelope with a 113 000-chunk fixture**: ~11 MB of JSON per run in debug, repeated
  once per mutant by the C5 gate (246 mutants × ~2-4 s ≈ +10-16 min on a 5-minute gate).

## 6. Limitations the human should weigh

* **No production path publishes a segmented map yet** — by design (`0016:2287-2312`); #636 wires the
  session in. The committer is therefore exercised by tests only, over a real redb store.
* `unlink` / `commit_chunk_map_superseding` **fail closed** on a segmented prior
  (`crates/core/src/metadata.rs:1337`,`:1437`,`:1504`): retiring a segmented generation is the staged
  obligation #636 owns. Typed error, never a partial delete.
* The `seggrp:` marker's two-arm lifetime is #636's; this slice ships the record, the key helper and
  the bounded-range predicate (`segment_group_adopted`) the arms gate on.
* `high_water_marks` now resolves through the live resolver as well; its pre-existing
  `IN_PROCESS_CHUNK_CEILING` filter gained a regression test on both sides of the bound.
