# Brief — issue 349 / mixed-era-placement-test-matrix

> The Plan artifact (docs 02 §PLAN). Human-authored. Do reads ONLY this file.
> The field labels below are parsed by the driver — keep the `- **Label:** value`
> shape. The success criterion is the load-bearing field: it is the sentence
> Check tests "did this work" against.

- **Slug:** mixed-era-placement-test-matrix
- **Defect:** The mixed-era placement resolution — `ChunkRef::placed_dserver`'s
  identity-placement fallback for empty (pre-M3) and short `placement` vectors
  (`crates/core/src/metadata.rs:110-124`) — is exercised **unevenly** across the
  consumers that interpret `ChunkRef.placement`, so several resolution paths and
  scheme sizes are unguarded against regression. Per the #292 audit's matrix
  (consumer × {empty(None), empty(RS), short, full, RS{6,3}}): **Read**
  (`crates/core/tests/placement_record.rs`) covers only full RS{6,3} — empty/short
  are gaps; **Scrub** (`crates/custodian/tests/scrub.rs`) covers only full(None) —
  empty/short/RS{6,3} are gaps; **Reconstruction**
  (`crates/custodian/tests/reconstruction.rs`) covers only full RS{2,1} — empty/short
  and the empty→re-placement case are gaps; **Rebalance**
  (`crates/custodian/tests/rebalance.rs`) covers only full RS{2,1} — the empty-placement
  case is the regression just fixed by #346 and is currently unlocked; **GC**
  (`crates/custodian/tests/gc.rs`) has the empty/short matrix (#287, sub-cases 4a/4b/4c)
  but only at RS{2,1} — RS{6,3} is a gap. The DST scenario `mixed_era_read`
  (`crates/server/tests/dst_erasure.rs:210`) mixes *schemes* but writes **full**
  placement from the write path, so it never exercises an empty/short vector. Net: the
  authoritative resolution has no uniform regression net, and two safe-today behaviours
  (rebalance evacuating a pre-M3 chunk; reconstruction re-placing one to a full-length
  record) are unpinned.
- **Success criterion:** Every `ChunkRef.placement` consumer — Read, GC, Scrub,
  Reconstruction, Rebalance — has explicit, **passing** test coverage for empty
  (pre-M3) and short and full placement across `EcScheme::None` and Reed-Solomon,
  including at least one **RS{6,3}** case per consumer; PLUS a DST scenario in
  `dst_erasure.rs` seeded with an explicit **empty-placement** chunk that asserts read
  AND maintenance resolve it **identically across seeds**; PLUS a reconstruction case
  that reconstructs a chunk whose committed `placement` is **empty** and asserts the
  re-placed record is a **full-length** vector (length `fragment_count()`) with the
  rebuilt fragment in a distinct domain — not a short/empty write. The new cases pass on
  current `main` (which carries the merged resolvers #139 / #287 / #346 / #356), and
  EACH is demonstrated load-bearing by a temporary negation that reddens it (see
  Verification posture). BINDING: the named matrix cells exist as tests and pass, and a
  negation of the underlying resolver reddens them. ILLUSTRATIVE: the specific fixtures
  reused and whether a cell ships as `#[tokio::test]` vs a DST seed — Do's call, provided
  the cell exercises the stated resolution.
- **Invariant to restore:** (coverage-LOCKING, not behaviour-restoring — this slice
  changes no production code.) The property the matrix pins: *every placement-consuming
  path resolves a chunk's fragment locations through the same authoritative
  identity-placement-fallback resolution (an absent or short `placement[i]` resolves to
  D-server `i`), and any path that re-places fragments commits back a complete,
  full-length (`fragment_count()`) placement record* — one placement closure across read,
  GC, scrub, reconstruction, AND rebalance. Source: `crates/core/src/metadata.rs:110-124`
  (`ChunkRef::placed_dserver`, "the single authoritative placement-resolution
  definition"); ADR-0040 "mixed-era placement expansion" (accepted, merged PR #355) and
  proposal 0005 "the placement record" (`docs/principles.md` §6, placement / durability
  category). This field documents what the tests assert; it is NOT a structural defect to
  fix (the resolvers already exist and are merged), so the Plan-exit structural gate does
  not apply.
- **Repo + branch target:** getwyrd/wyrd @ main   (INTEGRATION §2 — everything targets `main`)
- **Depends on:** (none)
- **Conflicts with:** (none)
- **Ordering note:** All of this matrix's resolution prerequisites are already **merged
  to `main`**: the placement record + `placed_dserver` (#139, 093732d), the GC fallback
  (#287, PR #340), the rebalance evacuation fallback (#346, PR #357), the relocatable
  fan-out (#356, PR #358), and ADR-0040 (PR #355). So there is **no in-batch dependency**
  — #349 builds directly on `main` (the local `origin/main` mirror may need a `git fetch`
  to see #357/#358; the worktree's fetch handles that). #349 is deliberately INDEPENDENT
  of the still-open `ChunkRef::fragments()` consolidation (#347): the matrix asserts
  *observable resolution*, which is identical whether a consumer resolves via
  `placed_dserver` directly (today) or via `fragments()` later — so #347 landing first
  would not invalidate these tests. The batch's other id, #350, is left UNPLANNED (it
  depends on #347 and on this matrix; see the planner's note) — there is no #349↔#350
  shared file (#349 edits test files only), so no conflict.
- **Surfaces:** data
- **Difficulty:** medium — additive, test-only, but **wide cross-file reach**: ~6 test
  files across 3 crates (`crates/core/tests/`, `crates/custodian/tests/`,
  `crates/server/tests/`). Each cell is independent with no effect propagation into
  production code, which keeps it off `high`; the breadth keeps it off `low`.
- **Scope:** add the missing mixed-era placement matrix cells so each consumer that reads
  `ChunkRef.placement` is regression-locked across {empty(None), empty(RS), short, full}
  and at RS{6,3}, plus the empty-placement DST scenario and the reconstruction
  re-placement pin — building on the #287 GC suite (`crates/custodian/tests/gc.rs`,
  sub-cases 4a/4b/4c) as the template and reusing the existing fixtures. / out of scope:
  changing any consumer's resolution logic or the identity-fallback definition; the
  `ChunkRef::fragments()` consolidation (#347); strict-maintenance malformed-length
  rejection (#348); the backfill migration / observability counter / identity-fallback
  removal gate (#350); any production diff at all — this slice is tests only.
- **Repro instruction:** On `main` (post-#346/#356 fetch), build the matrix from the
  existing fixtures, one cell per gap:
  - **Read** — `crates/core/tests/placement_record.rs` (fixtures: `Fleet`,
    `rs_plan`, the reopen helper): add empty-placement (`placement: vec![]`) and
    short-placement resolution cases for `EcScheme::None` and Reed-Solomon, asserting the
    object reads byte-identical via identity fallback.
  - **Scrub** — `crates/custodian/tests/scrub.rs` (fixtures around `:144-160`,
    `placement: vec![...]`): add empty(None), empty(RS), short, and an RS{6,3} case
    mirroring the existing full(None) case.
  - **Reconstruction** — `crates/custodian/tests/reconstruction.rs` (fixtures
    `write_rs_2_1`, `four_domains`, `elect`, the `..._through_reconcile_step` pattern):
    add an empty-placement reconstruction case AND the **re-placement pin** — reconstruct
    a chunk committed with `placement: vec![]`, assert the committed re-placed record has
    `placement.len() == fragment_count()` with the rebuilt fragment in a distinct domain
    (guards the unpinned safety at `reconstruction.rs:230`/`:388-418`); add an RS{6,3} case.
  - **Rebalance** — `crates/custodian/tests/rebalance.rs` (fixtures `four_domains`,
    `write_rs_2_1`, `elect`, the reconcile-loop pattern of
    `drains_a_d_server_and_evacuates_..._through_reconcile_step`): add the empty-placement
    evacuation case that locks #346 (`EcScheme::None` single fragment, and an RS case with
    the draining fragment at index > 0) and an RS{6,3} case.
  - **GC** — `crates/custodian/tests/gc.rs`: add RS{6,3} cells alongside the existing
    RS{2,1} 4a/4b/4c sub-cases.
  - **DST** — `crates/server/tests/dst_erasure.rs` (model on `mixed_era_read:210` and the
    across-seeds / pinned-seed harness `:273-289`): add a scenario seeded with an explicit
    **empty-placement** committed chunk and assert read + the maintenance resolution agree
    across seeds (and at the pinned `REGRESSION_SEED`).
- **Test file:** the matrix ships across these homes (each must hold its new cells, all
  green on `main`): `crates/core/tests/placement_record.rs`,
  `crates/custodian/tests/scrub.rs`, `crates/custodian/tests/reconstruction.rs`,
  `crates/custodian/tests/rebalance.rs`, `crates/custodian/tests/gc.rs`,
  `crates/server/tests/dst_erasure.rs`.
- **Verification posture:** NET-NEW coverage where "red" is criterion-ABSENCE — the cells
  are **born green** on `main` because the resolvers they exercise are already merged
  (#139/#287/#346/#356); there is no prior failing assertion to flip. So Check must NOT
  treat the absence of a red→green flip as a failure. FORCING FUNCTION: to prove each cell
  is load-bearing (not a tautology that passes regardless), Do MUST capture a
  **demonstrated red per consumer** via a temporary local negation — e.g. revert the
  consumer's resolution from `placed_dserver`/`fragments()` expansion back to raw
  `chunk.placement` iteration or `index % n`, confirm the new cell reddens, then restore.
  Record each negation→red→restore in `build-notes.md`. The rebalance empty-placement cell
  specifically must red against the pre-#346 raw-vector path; the reconstruction
  re-placement pin must red if `assess` is fed raw `chunk.placement` instead of the
  expanded vector. The DST empty-placement cell rests its red on a seeded empty vector that
  the old `index % n`/raw path mis-resolves. Confirmer of the deferred (over-the-wire /
  seed-sweep) green: `cargo xtask ci` in the worktree at Check (`./engine/xtask.sh ci`),
  supplementary to the in-process cells.
- **Citations expected:** Do must cite path:line on the target branch for every change
  (including the resolver lines each negation touches, to evidence load-bearingness).
- **Prior-art check (triage cycles):** searched by affected file path across merged
  history and open/closed PRs. The matrix's resolvers all landed and merged
  (`placed_dserver`/#139 093732d; GC fallback #287/PR #340; rebalance #346/PR #357;
  fan-out #356/PR #358; ADR-0040/PR #355). Existing coverage: `gc.rs` carries the
  empty/short sub-cases (4a/4b/4c) at RS{2,1} only; `placement_record.rs` carries full
  RS{6,3}; `scrub.rs`/`reconstruction.rs`/`rebalance.rs` carry only their full cases;
  `dst_erasure.rs::mixed_era_read` mixes schemes with full placement. No open/closed PR or
  branch adds the empty/short × consumer × RS{6,3} matrix or the empty-placement DST
  scenario — this issue (item 4 of 5 of the #292 audit) **is** that work. Sibling
  enhancements remain disjoint and OPEN: #347 (`fragments()` helper, source-side), #348
  (strict-maintenance rejection), #350 (migration/removal gate).
- **Disposition hint:** likely-fix

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a
draft PR MAY happen during the cycle (useful for CI feedback). The PR MUST NOT be
marked ready before sign-off accepts.
