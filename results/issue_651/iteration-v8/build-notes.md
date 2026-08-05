# build-notes — issue 651, iteration 8 (withheld from the reviewer)

**Base:** `origin/main` = `d50f0ca` (the brief's target). Built in `$PDCA_WORKTREE`
(`/home/eddie/wyrd/wyrd.pdca-wt-l0`); the human's `../wyrd` checkout was never touched.
All `path:line` citations below are against that worktree with `patch.diff` applied.

**Started from iteration 7 as the brief instructs** (`results/issue_651/iteration-v7/patch.diff`,
applied verbatim to the worktree first, `git apply` clean). Everything below is the delta on top
of it: criterion (4), plus the one round-7 finding the brief orders fixed.

## Size

| | this iteration | budget |
|---|---|---|
| files | 8 | ≤ 8 |
| added lines | 1,674 | — |
| **semantic added** (non-blank, non-comment) | **937** | ≤ 950 |

Per file (added / semantic): discriminator 855/551 · `cli.rs` 193/139 · `restore.rs` 356/125 ·
`restore_reconcile.rs` 140/86 · `desired_state.rs` 81/22 · `segmented_map_consumers.rs` 32/13 ·
`06-runtime-view.md` 1/1 · `m4-first-deployment-blueprint.md` 16/0. Measured with the same
script that reproduces the brief's "iteration 7 = 749 semantic" figure exactly, so the two
numbers are on one basis. Delta over v7: +188 semantic (production 46, discriminator 141, the
inverted test +1).

## What criterion (4) actually needed — one predicate, applied in both halves

Both halves keyed the fleet by `FragmentId` and asked *"do bytes with this id exist?"*. The fix
is to ask *"can these bytes be shown to be **this reference's**?"*, and the whole question turns
out to be answerable from the map the pass already builds:

`canonical: HashMap<FragmentId, Vec<DServerId>>` (`crates/custodian/src/restore.rs:303`) is
built from `referenced.placed`, a `HashSet`, so its value is the **deduplicated set of D servers
any committed reference names for that fragment id**. That is exactly the ambiguity oracle:

* **mark half** (`restore.rs:340`) — `any` → `all`. A copy elsewhere is a stale duplicate only
  when **every** named server holds the id; while one does not, these bytes may be that
  reference's last copy, so they are kept (`displaced_kept`) and never marked. One named server
  (the non-colliding case) makes `all` and `any` the same test, so nothing else moves.
* **report half** (`restore.rs:443`, via `attributable`, `restore.rs:665`) — fleet-wide evidence
  counts for a reference only while its own server is the **only** one the namespace names for
  that fragment. Under a collision the reference's `anywhere` collapses to its `placed`, so an
  empty placement is `dangling`, not `misplaced`.

**Why this shape and not the alternatives** (each was worked out, not hand-waved):

* *Rebuild `canonical` per-reference from `committed.chunks` (a `Vec<Vec<DServerId>>` per
  fragment).* Cost: ~10 more lines and a second source of truth for the same data, since
  `all(present)` over a multiset is identical to `all(present)` over its set. Rejected: no
  behaviour differs, and it would have made the two halves disagree if the two `inode:` scans
  ever did.
* *A separate "ambiguous chunk ids" set built by counting references per id.* Cost: an extra
  `HashMap<ChunkId, usize>` pass over `committed.chunks` (~8 lines) and a **wrong** answer for
  the legitimately-shared case — two references with identical placements are the *same* chunk,
  and by-count ambiguity would report both as lost. The dedup-set form gets that right for free.
* *Report the collision as a new `RestoreReport` field / CLI cell.* Declined by the brief's
  Scope, and it would have cost the `Vec<ChunkId>` shape of `dangling`/`misplaced` plus a fourth
  NEEDS-HUMAN paragraph in `cli.rs` (~35 lines). The collision surfaces through the existing
  verdict plus the audit seam instead (`emit_ambiguous_evidence`, `restore.rs:788`).

## The one round-7 finding, fixed (not re-litigated)

`crates/custodian/tests/restore_reconcile.rs:950` — v7 asserted `report.dangling.is_empty()`
"because the bytes are one hop away". Inverted: with a shared id and the reference's own
placement empty, the chunk is `dangling` and `misplaced` must be empty. The doc comment now
carries the reason (a `misplaced` verdict is the one a repair acts on, and that repair would
restage object 1's fragment into object 2's placement). It is deliberately **absent** from
`review-rejected.md`, and that file says so explicitly so a reviewer cannot read the absence as
an oversight.

## Two judgement calls a human should look at

1. **The `malformed` clause in `attributable` (`restore.rs:671`) ships untested.** A chunk id
   whose placement is malformed *anywhere* in the namespace is also unattributable: that
   reference's true placement is unknown, so a copy anywhere may be its. It is one line, it is
   strictly fail-safe (it can only turn a `misplaced` into a `dangling`, never the reverse), and
   it cannot regress anything on the base — `committed_chunks` skips malformed placements, so
   the clause only fires for an id that is *both* malformed in one object and validly placed in
   another. A focused test (`commit_single_chunk(&meta, 1, 62, 1)` + `commit_chunk(&meta, 2, 62,
   1, 1, vec![0, 1, 0])`, bytes on d0, assert `dangling == [62]`) is ~25 semantic lines and would
   have put the patch at ~962, over the 950 budget. I chose the correct-but-uncovered line over
   dropping the clause, because dropping it leaves a real hole in the very invariant the brief
   names. If the human would rather have the test than the headroom, it is a 25-line follow-up.
2. **`emit_ambiguous_evidence` fires only when the attribution *changed the verdict*** (`anywhere
   < k <= by_id_alone`, `restore.rs:466`). Evidence withheld on a chunk that is dangling either
   way, or on a chunk that reads fine at its placement, is silent. That is a noise choice, not a
   correctness one: the emitted case is exactly the one where "DANGLING" arrives with the wrong
   operator story attached, which is what the runbook change at
   `docs/design/architecture/m4-first-deployment-blueprint.md:601` documents.

## Scope held

No `gc.rs` / `scrub.rs` edit (verified: the fleet-wide-by-id displaced rule exists only in
`restore.rs`). No `crate::resolve` module, no custodian-level walk — `#681`. Nothing written to
a chunk map — `#682`. No change to how ids are minted — `#652`. No new report class or CLI cell
for a colliding id, no owner attribution threaded through `Expected`/`emit_dangling`/
`emit_misplaced` (declined in `review-rejected.md` with the issue reference, per AGENTS.md's
reviewer protocol). No new dependency, no dev-dependency, no cfg gate, no DST leg — this patch
*narrows* a destructive path (it marks strictly fewer fragments than the base), so the rubric's
"a new destructive path lands with seeded Tier-0 DST coverage" does not apply.

## Verification — run through the project's own runners, not hand-rolled

**`./engine/scripts/run-verify.sh` (C4-verify, `PDCA_BUNDLE=results/issue_651`) → PASS.**
`--classify` first: `ADDED_TEST crates/custodian/tests/segmented_map_restore.rs` + `CRATE
crates/custodian` + `CRATE crates/server`, so the invocation is
`cargo test -p wyrd-custodian --test segmented_map_restore`.

* GREEN with the fix: `7 passed; 0 failed`.
* RED with production reverted, test kept: `1 passed; 6 failed`. The one that passes is
  criterion (4c) — by design, "no collision ⇒ no change" is base behaviour.
* The base's own answers, quoted from that leg, are the defect verbatim:
  * (4a) `RestoreReport { … dangling: [], misplaced: [45312] … }` — the base says the bytes are
    one hop away for an object none of them belong to.
  * (4b) `RestoreReport { stranded_marked: 1, … }` — the base marks the second object's only
    copy. That is the data-losing leg, reproduced.
  * (1)/(2) `SegmentedMapUnsupported { operation: "restore::committed_chunks" }` — the whole
    pass still `?`s out on the base.

**`./engine/xtask.sh ci` (C4-ci, the whole Wyrd gate) → exit 0.** fmt, clippy `-D warnings`,
build, the full test suite incl. DST, `cargo deny check` (0.20.2, the tool version that cost v7
its verdict), the conformance vectors, and the prose gates — `typos`, `lint_docs: OK`,
`render_site: wrote 98 page(s)` all **ran** rather than warn-skipping, which matters because
this slice edits two docs. So the patch is commit-ready for the target's own hooks, not merely
gate-green.

### The three forced questions

* **(a) Genuine red?** **Yes**, measured, not asserted: the C4-verify RED leg reverts
  `restore.rs`, `desired_state.rs`, `cli.rs` and every modified test file, keeps the
  discriminator, and 6 of its 7 tests fail — including both criterion-(4) legs, on *assertions*
  (`left: 1 / right: 0`, `left: [] / right: [45312]`), never on a missing symbol. The
  discriminator names no symbol this patch introduces, which is why the reverted tree still
  compiles.
* **(b) Production path?** **Yes.** The tests call `wyrd_custodian::reconcile_after_restore` and
  `wyrd_custodian::desired_state::reconciliation_status` — the production functions this patch
  changes — from an integration-test crate, over the same public entries the `wyrd custodian`
  command calls. The in-memory doubles are the `MetadataStore` / `ChunkStore` **seams** (the
  crate's established idiom, `tests/restore_reconcile.rs`), not stand-ins for the logic under
  test: every verdict, mark and audit line in the assertions is produced by `restore.rs` /
  `desired_state.rs` themselves.
* **(c) Fixture includes the fault?** **Yes.** (4a)/(4b) seed the *collision itself* — two
  committed objects carrying one chunk id with different placements — and (4b) additionally
  seeds the displaced copy on a D server no placement names, which is precisely the fragment the
  base marks (proven above: `stranded_marked: 1`). Nothing is curated out: the object whose
  placement is empty stays in the fleet as an empty `MemDServer`, and the unreadable-object
  fixture asserts its own fault is real (`seed_damaged` re-resolves the seeded root and requires
  `is_err()`), so a leg can never pass because the fault quietly stopped being one.

## Housekeeping

`review-rejected.md` was rewritten for this iteration: the three standing rejections and the
scope declines are re-keyed to **this** patch's line numbers (they had drifted by ~20–90 lines),
a fifth decline covers the owner-attribution gap the brief declines by name, and the round-7
`restore_reconcile.rs` finding is recorded as FIXED rather than rejected. Scratch work lived
under `$PDCA_SCRATCH` (`pdca-builder-651-*.diff/.log`) and is removed.
