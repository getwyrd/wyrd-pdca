# Build notes — issue #635 (segmented-chunk-map), iteration 6

> Withheld from the reviewer; written for the human at sign-off.
> Line numbers are on the **patched** worktree (`$PDCA_WORKTREE` =
> `/home/eddie/development/wyrd/wyrd.pdca-wt-l0`, base `origin/main` @ `9120f7a`, which
> **carries #634**) unless a base ref is named. `patch.diff` is `git diff` against that base.

## 0. What this iteration is, and what it inherits

Iteration 5 was rejected on the **base**, not on the design: the brief named a wave-fold
branch that never existed, so Do built and verified without #634 and shipped store doubles
that could not compile against the real `MetadataStore` (`scan_page` is a **required**
method with no default body since PR #645). This iteration's brief fixes that at the root —
#634 is merged to `main` (`9120f7a`), there is no stack, no fold branch, and I confirmed
before starting that **neither `$PDCA_BASE` nor `$PDCA_VERIFY_BASE` is set and no
`stack-base` file exists in the bundle** (the brief's `Falsifiability` 2 STOP condition).
`run-verify.sh --print-base` ⇒ `origin/main`. Build base == test base == PR base.

**Starting point.** The brief's own carry-forward says iteration 5 "passed C1 Spec, C3
Change, T1 Structure and T2 Shape" and enumerates exactly what is still open ("do not
re-derive the settled parts"; "Their tests are in `iteration-v5/` for reference"). I
therefore took `iteration-v5/patch.diff` as the starting tree — it applied to `9120f7a`
**cleanly with `git apply --3way`**, 47 files — and did the five carry-forward items on top.
Re-deriving 10 000 lines of settled, gate-green design from scratch would have re-opened
questions the brief explicitly closes, and would have produced a *different* patch for the
reviewer to re-read from zero.

## 1. The five carry-forward items — what each became

| # | Carry-forward | What I did | Where | Proof it binds |
|---|---|---|---|---|
| 1 | **THE rejection**: doubles missing `scan_page` on the real base | one delegating body per double, mirroring the cited peer `crates/custodian/tests/gc.rs:73-80` verbatim | `crates/core/src/metadata.rs:5556`, `:5861`, `:6319`; `crates/custodian/tests/segmented_map_consumers.rs:103`; `crates/custodian/tests/backfill.rs:480`; `crates/server/src/lib.rs:918` | the workspace compiles on `9120f7a` (`cargo check --workspace --all-targets` clean); **no `Cargo.toml` is touched** — `wyrd-testkit` was already a dev-dep of all three crates |
| 2 | **Publish must refuse before it writes** | `publish` assembles **both** phases' batches (segment split *and* flip) before committing anything; the flip batch is a pure function of the plan + the caller's contribution, so nothing about it needs a durable segment | `crates/core/src/metadata.rs:3068-3079` | new test `a_deterministically_refused_publication_writes_no_segment_at_all` (`:4418`) — **red** with the old order: *"an unfenced flip: a refused publication leaves ZERO durable seg: records"* |
| 3 | **A resumed publication verifies the durable prefix** | `verify_resume_prefix` re-reads `seg:<nonce>:<epoch>:<resume_from-1>` and compares it byte-for-byte with the record the re-derived plan puts at that index; two new typed errors | `crates/core/src/metadata.rs:2997-3032` (probe), `:501-530` (`ResumePrefixMismatch` / `ResumePrefixAbsent`), called from `write_segments` (`:2974`) and `publish` (`:3074`) | new test `a_resumed_publication_refuses_a_durable_prefix_that_is_not_its_own` (`:4483`) — **red** without it: *"a resume onto another list's prefix must refuse: Committed"*, i.e. exactly the silent-at-publication / terminal-at-read outcome the probe describes |
| 4 | **Blast radius** (the containment table) | `high_water_marks` no longer resolves any root: flat maps' ids come off the record, segmented ids come from the **`seg:` namespace**, walked with `scan_page` | `crates/core/src/metadata.rs:3453-3499` (`high_water_marks`), `:3389-3420` (`segment_chunk_floor`), `:3363` (`SEG_FLOOR_PAGE`) | three new tests, all **red against the iteration-5 shape** (see §3), plus two that bind the new walk's own branches (`:6531` paging, `:6505-6516` the attributed skip) |
| 5 | **The five `review-batch.md` findings** | record-rejected, with the flat peer read on the base and cited | `results/issue_635/review-rejected.md` § "Round 5" | verified mechanically: `review-branch`'s own `load_rejected` + `is_rejected` suppress **5 of 5** findings |

## 2. The containment decision (item 4) in full — what I changed and what I did NOT

The brief settles the table; the only judgement left was *how* to make
`high_water_marks` total. I did the cheapest thing that is also the honest one:

* The `inode:` walk keeps `decode(&value)?` (unchanged from the base) and reads chunk ids
  **only from a flat map**. A segmented root carries no chunk ids at all, so there is
  nothing to lose by not resolving it — and resolving it is precisely what made one damaged
  object return `Err` from the call `Gateway::recover` makes before serving
  (`crates/server/src/lib.rs:123-124`).
* A new `segment_chunk_floor` (`crates/core/src/metadata.rs:3389`) walks the `seg:`
  namespace and takes the max id it finds. That is a **strict over-approximation** (it
  counts retired-but-undrained generations too, whose fragments are still on disk), which
  is the safe direction for an allocator floor.
* It is **paged** (`scan_page`, 64 records/page ≈ 6 MB heap ceiling) rather than scanned,
  because `scan` is complete-or-fail-loud at `SCAN_CAP` and `seg:` is the one namespace a
  single object can put 512 records into — a `scan` there would be a *new* way to make
  startup fail, i.e. the same blast radius by another route. This is the one place I use
  #634's primitive; the resolver still uses the bounded per-group `scan`, exactly as the
  brief instructs.
* An **undecodable `seg:` value** is attributed (`tracing::warn!` + a monotonic counter)
  and skipped rather than propagated. This is the `PendingMalformed` shape the brief names
  ("refuse to certify, **attribute**, keep going"): the alternative trades one unreadable
  value for every healthy object's availability. Flagged here because it is the one place I
  chose "keep going" over the rubric's *never a silent skip* — it is not silent, and the
  containment table's `high_water_marks` row is explicit that totality wins here.

**What I deliberately did not change:** GC / restore / `reconciliation_status` still *abort*
when a map cannot be resolved (the `?` propagates out of `referenced_fragments`). The
containment table permits exactly this ("Aborting the pass is acceptable — it is what
`decode(&value)?` already does today"), and what it forbids — deleting anything — is what
leg A(vii)(d) asserts on the fragments themselves. Continuing-while-protecting would mean
inventing a protected set for an object whose chunk list is precisely what cannot be read;
that is not a smaller change, it is a less honest one.

**Cost of the alternative I rejected** (uniform fail-closed, iteration 5's shape): zero diff
— it is what the tree already did — and it costs the whole gateway. The probe is in the
patch as a test: revert `high_water_marks` to the resolver-based body and
`Gateway::recover` fails with `SegmentAbsent { nonce: "fedcba…", epoch: 11, index: 1 }`
(`crates/server/src/lib.rs:1184` (the `recover()` expect)), i.e. a gateway that will not start because one of three
objects lost a segment.

## 3. The three questions, answered (forced refutation)

**(a) Genuine red?** Yes, and measured on both the base and the *previous iteration's*
shape:

* **The binding leg (A), through the project's own gate.** `./engine/scripts/run-verify.sh`
  (the `C4-verify` row in `pdca.toml`), which cuts its own `../wyrd-verify-l0` worktree off
  `origin/main`, applies `patch.diff`, then reverts production and keeps the added test:
  * GREEN leg: `cargo test -p wyrd-custodian --test segmented_map_consumers` — **9 passed,
    0 failed**.
  * RED leg: **0 passed, 9 failed**, and the red is **assertions, not a build error** —
    e.g. `segmented_map_consumers.rs:566: reconcile_step must resolve a segmented chunk
    map, not fail on it: Some("… invalid type: map, expected a sequence …")` and
    `:1109: one damaged object must not fail the id floor the gateway starts from`. The
    file compiled against the base, which is the whole point of seeding by raw record bytes
    and naming no symbol this slice adds.
  * Verdict line: `run-verify.sh: PASS — red without the fix, green with it.`
  * `--classify` returns exactly **one** `ADDED_TEST crates/custodian/tests/segmented_map_consumers.rs`,
    so nothing else joins the RED invocation and no compile error can destroy that red.
* **The three new leg-B/containment tests, each reverted individually** (co-located, so
  `C4-verify` cannot flip them — I reverted by hand, ran, and restored):
  * revert `publish`'s ordering ⇒ `a_deterministically_refused_publication_writes_no_segment_at_all`
    **fails** at `metadata.rs:4459`.
  * remove the resume probe ⇒ `a_resumed_publication_refuses_a_durable_prefix_that_is_not_its_own`
    **fails** at `metadata.rs:4530` (`… must refuse: Committed`).
  * restore iteration-5's `high_water_marks` ⇒ all three containment tests **fail**:
    `the_id_floor_is_total_over_a_damaged_segmented_object` (core),
    `a_damaged_segmented_object_fails_closed_without_taking_the_store_down` (server,
    `… must not stop the gateway from starting: SegmentAbsent { …, index: 1 }`),
    `a_damaged_segmented_object_never_costs_the_store_its_other_objects` (custodian leg A).
  * the new walk's two own branches, broken **one at a time**: stop after the first page ⇒
    `the_id_floor_pages_through_the_whole_segment_namespace` fails `left: 64, right: 131`
    (a floor *below* live ids — the re-mint hazard) while the totality test still passes;
    propagate the decode error instead of attributing it ⇒ the totality test fails
    (`an unreadable seg: value may not fail the id floor either`) while the paging test
    still passes. Each assertion binds exactly one behaviour.

**(b) Production path?** Yes. Leg A drives the real `reconcile_step`,
`reconcile_after_restore`, `reconciliation_status`, `backfill::reconcile`,
`wyrd_core::read::read_object` and `metadata::high_water_marks` — no double of any of them;
the only doubles are the *store* and the *D servers* (the seams, as `tests/gc.rs` does). The
gateway legs drive `Gateway::get_object_streaming` / `get_object_range` / `recover` over a
real redb store and a real FS chunk store. Leg B drives the real `SegmentedPublication`
against `RedbMetadataStore::in_memory()`; the only stand-in is the *caller* supplying the
publication fence, which is #636's by design.

**(c) Fixture includes the fault?** Yes, and this is where iteration 5's gap was. The new
leg A(vii) fixture seeds a **third** object in the *same store* as the healthy flat and
healthy segmented ones: committed, segmented, root naming two segments, and
`seg:fedcba…:11:000001` **never written** — the failing element is in the fixture, not
curated out. The healthy objects are then asserted to still read *with it present*, and the
damaged object's own fragments are asserted still on disk after a restore pass and a GC pass
past the grace window. Likewise the resume probe's fixture is a genuinely divergent durable
prefix (attempt 1's records, attempt 2's list), not a synthetic mismatch flag.

**One honest caveat on leg A(vii):** sub-legs (c) and (d) are green on the *base* too — a
segmented value fails `decode` there, so a read of the damaged object errors and GC deletes
nothing for the wrong reason. Their binding red is against the **iteration-5** shape, which
is where the defect actually lived. Sub-legs (a) and (b) are red on the base as well.

## 4. Where each leg of the success criterion lives

* **A(i)–(v)** `crates/custodian/tests/segmented_map_consumers.rs:511`, `:647`, `:707`.
* **A(vi)** core read `:754`; reconstruction `:776`; rebalance `:898`; backfill `:1023`,
  `:1201`; the two **gateway** legs are co-located in `crates/server/src/lib.rs:1128`,
  `:1223` (they cannot live in the custodian binary — `wyrd-server` depends on
  `wyrd-custodian`, and a second *added* test target would join `C4-verify`'s RED
  invocation and destroy leg A's assertion red).
* **A(vii)** `crates/custodian/tests/segmented_map_consumers.rs:1098` (a,b,c,d) +
  `crates/server/src/lib.rs:1158` (the gateway half of b and c, where `ChunkMapError` is
  nameable) + `crates/core/src/metadata.rs:6442` and `:6531` (the floor's own unit).
* **B(i)–(viii)** co-located in `crates/core/src/metadata.rs`'s `mod tests` (52 tests) and,
  for the X51 interleaving, appended to the existing `crates/dst/tests/custodian.rs`.
* **C** `cargo xtask ci` via `./engine/xtask.sh ci`, with `typos` and the doc renderer
  present on this host (both registered `[[doctor.checks]]`), so the prose gates really ran
  rather than warn-skipping.

## 5. Answers to the brief's open questions, for the sign-off

1. **The pinned encoding** — implemented exactly as the brief spells it; leg B(ii) asserts
   the canonical JSON literally, in both directions.
2. **Where the resolver lives** — `crates/core/src/metadata.rs` (with the record and the key
   helpers). The custodian's `resolve.rs` is a thin, attributed wrapper over it, not a second
   resolver.
3. **`backfill.rs`** — decision: **resolve, then refuse to rewrite a segmented map**, and
   fail the pass with a message naming the inode and the reason (rewriting would flatten the
   root and strand every `seg:` record). Asserted both ways at `:1023` (an empty placement,
   the discriminating input) and `:1201`.
4. **A `Completing`-less committer** — taken, as the brief directs: the fence is a
   parameter and the caller contributes **preconditions *and* mutations** to the one flip
   batch. This is a T3 precursor seam and stays a NEEDS-HUMAN confirm.
5. **The containment table** — settled by the maintainer at Plan; implemented per §2.

## 6. Gate results on the final tree, and scratch discipline

Both gates re-run after the last edit, against the exact `patch.diff` in this bundle:

* `./engine/scripts/run-verify.sh` (C4-verify, own `../wyrd-verify-l0` worktree off
  `origin/main`): **GREEN 9 passed / RED 0 passed, 9 failed** ⇒
  `run-verify.sh: PASS — red without the fix, green with it.` (exit 0)
* `./engine/xtask.sh ci` (the whole Wyrd gate): `xtask ci: all checks passed` (exit 0),
  with the prose gates really running — `$ typos`, `lint_docs: OK`,
  `render_site: link audit OK` — plus `cargo fmt --check`, `clippy -D warnings`,
  `cargo-machete`, `cargo deny`, the statics/unsafe/deploy guards, the conformance vectors
  (5 valid + 6 invalid) and the madsim DST leg, where
  `segmented_resolve_never_tears_on_retirement`,
  `staged_publication_is_atomic_at_the_flip` and
  `segmented_repoint_never_races_a_supersede` all pass.

Everything throwaway lived under `$PDCA_SCRATCH` (`/var/tmp/pdca`) named
`pdca-builder-635-*` and was deleted at the end: the refutation backup
(`pdca-builder-635-metadata.rs.bak`), a copy of `scripts/review-branch` used to check that
`review-rejected.md` parses (`pdca-builder-635-rb.py`), and the four gate logs quoted above.

## 7. STOP discipline

No push, no branch, no PR. The patch is `patch.diff` in the bundle; the worktree
`wyrd.pdca-wt-l0` holds the same tree (staged, uncommitted).
