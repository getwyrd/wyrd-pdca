# Design proposal — issue 350 / placement-backfill-migration

> The Plan artifact for the **exception**: a change significant enough to warrant a
> GEPS-style design proposal. Authored interactively at Plan (the planner leaf).
> Do reads ONLY this file and implements it; Check runs the regular gated check on
> the code. The `- **Label:** value` lines are parsed by the driver.

- **Slug:** placement-backfill-migration
- **Kind:** enhancement (design proposal)
- **Goal:** Drain the pre-M3 / mixed-era population of committed chunk maps whose
  `placement` vector is empty (decoded via `#[serde(default)]`,
  `crates/core/src/metadata.rs:94`) by adding a custodian **backfill pass** that
  rewrites each such committed chunk with an explicit full-length identity placement,
  plus an observability signal on the durability-plane seam (ADR-0011) so the
  remaining empty-placement population is watchable as it drains to zero — steps 1
  and 2 of the ADR-0040 decision-6 removal path for the identity-placement fallback.
- **Success criterion:** After backfill reconcile passes over a metadata store
  containing pre-M3 committed inode records (committed chunks with an **empty**
  `placement`), (a) every such committed chunk's record carries an explicit
  full-length identity placement — `placement.len() == fragment_count()` and
  `placement[i] == i` for all `i` — committed **version-conditionally** (the same
  prior-record CAS the custodians use, `rebalance.rs:evacuate_chunk` ~`:269-289` /
  `metadata.rs:commit_chunk_map` ~`:242-263`), so a racing writer/custodian wins the
  CAS and the backfill retries on a later pass rather than clobbering; (b) a chunk
  whose placement is **malformed** (non-empty, `len != fragment_count()`, ADR-0040
  decision 3) is NOT rewritten; and (c) the count of empty-placement committed
  records remaining is emitted per pass on the durability-plane seam
  (`tracing`→OTel bridge, per the existing `gauge.` / `monotonic_counter.` emission
  pattern, e.g. `rebalance.rs:emit_domain_utilization` ~`:311`) and reads **zero**
  once the backfill has covered the store. Demonstrable at C4-verify by the test
  file below (red pre-patch, green post-patch). Hosting the pass as a
  `backfill::reconcile` step inside `reconciliation.rs:reconcile_step` is
  ILLUSTRATIVE; the BINDING conditions are (a)–(c).
- **Repo + branch target:** getwyrd/wyrd @ main   (Wyrd has no maintenance branches; INTEGRATION §2)
- **Depends on:** 348
- **Ordering note:** The backfill is itself a maintenance pass that rewrites
  committed placement, so ADR-0040 decision 4 binds it: it MUST classify the
  committed vector before acting (empty → backfill; malformed → skip + audit, never
  rewrite). #348 lands the single-source classifier
  (`placement_is_valid()`/`checked_fragments()` in `crates/core/src/metadata.rs`)
  plus strict-maintenance wiring in the same custodian files this pass sits beside —
  build on its accepted result and reuse the classifier rather than open-coding a
  second length check (open-coded placement logic is the defect class ADR-0040
  exists to foreclose). The issue's other named prerequisites are already merged on
  `main`: the rebalance fix #346 (PR #357), the `fragments()` helper #347 (PR #361),
  and the mixed-era test matrix #349 (PR #359). #330 is complete in a prior wave.
- **Surfaces:** data
- **Difficulty:** medium
- **Scope:** Steps 1 and 2 of the ADR-0040 decision-6 removal path, in one logical
  change: a custodian backfill pass (scan committed inode records; for each committed
  chunk with an empty placement, materialize the full-length identity vector and
  commit it under the prior-record CAS; skip malformed vectors) + the
  empty-placement-remaining signal on the durability seam. / out of scope: **step 3,
  the removal gate itself** — converting the empty-vector branch of
  `placed_dserver`/`fragments()` into a defensive error behind a metadata
  format-version bump is a LATER slice, tracked by follow-up issue **#363** and
  gated on the counter reaching zero in production plus a confirming scan
  (ADR-0040 decision 6: both preconditions are unsatisfiable at build time — until
  then the fallback stays load-bearing and the read path is UNCHANGED); rewriting or "repairing" malformed (non-empty
  wrong-length) vectors (they are #348's strict-maintenance concern — operator
  signal, never silent rewrite); any change to the write/commit path (M3+ writers
  already record full-length placement); pending/uncommitted inodes.
- **Repro instruction:** Not a defect repro — feature-absence baseline on
  `origin/main` (+ the #348 fold): build an in-memory metadata store; commit an
  inode via `commit_chunk_map` whose `ChunkRef` has `placement: vec![]` (simulating
  a pre-M3 record decoded through `#[serde(default)]`); run the custodian
  reconcile over it; observe the committed record still carries an empty placement
  and no drain signal exists.
- **Test file:** crates/custodian/tests/backfill.rs
- **Verification posture:** NET-NEW coverage — a born-at-tier test file (no prior
  failing assertion to flip), but the red is *demonstrable*, not absence-only: Do
  must run the new test pre-patch on the base and capture it red (the pre-M3 record
  survives a reconcile pass with its placement still empty), then green post-patch.
  Cover at least: identity backfill of an empty-placement committed chunk
  (full-length, `placement[i] == i`, version bumped by CAS); CAS-conflict handling
  (a record mutated between read and commit is not clobbered — conflict → retried
  on a later pass); a malformed (non-empty, wrong-length) vector left untouched; an
  already-explicit full-length vector left untouched (idempotence); and the
  emitted remaining-count reaching zero after coverage. The mixed-era read-path
  behaviour itself is already pinned by the #349 test matrix and must stay green.
- **Citations expected:** Do must cite path:line on the target branch for every change.
- **Prior-art check (triage cycles):** searched by file path — no
  `crates/custodian/src/backfill.rs` exists; `git log --all --grep=backfill` finds
  only ADR-0040's text (052fd49); no open/closed PR implements the migration. The
  adjacent placement work that DID land: #346 (PR #357), #347 (PR #361), #349
  (PR #359), #356 (PR #358) — all merged; #348 is in flight (this bundle's prereq).
- **Disposition hint:** new-feature

## Motivation

Follow-up from the #292 mixed-era placement audit (item 5 of 5), answering Q5:
which compatibility paths are migration-only and when they can be removed. The
identity-placement fallback (the empty-`placement` branch of
`ChunkRef::placed_dserver`, `crates/core/src/metadata.rs:119-125`) exists solely
for pre-M3 / mixed-era records; M3+ writes always record a full-length vector. So
the fallback is migration-only and must not stay load-bearing indefinitely —
ADR-0040 decision 6 (Proposed) defines the removal path this issue tracks.
Reconstruction and rebalance already materialize full placement when they *touch*
a chunk (ADR-0040 decision 5); the backfill closes the long tail of committed
records that no loop ever touches. Without the observability signal there is no
way to know when the pre-M3 population has drained, so the removal gate could
never be safely flipped.

## Design

1. **Backfill pass (step 1).** A custodian pass — the natural shape is a
   `backfill` module beside GC/scrub/reconstruction/rebalance, run from
   `reconciliation.rs:reconcile_step` (`crates/custodian/src/reconciliation.rs:65`)
   like its siblings; that hosting is illustrative. Per committed inode record:
   for each committed chunk, classify the placement (reuse #348's single-source
   classifier): **empty** → build the identity vector
   `(0..fragment_count()).map(u64::from)` and commit the updated record
   version-conditionally on the prior record (the `require(prior)`/`put(next)`
   CAS batch pattern of `rebalance.rs:evacuate_chunk` ~`:269-289`); **full-length**
   → untouched (idempotent); **malformed** → untouched, surfaced per #348's
   strict-maintenance posture (audit, never silent rewrite). A lost CAS is benign:
   the record is re-examined on a later pass. No fragment moves — this rewrites
   metadata only, so the semantic resolution of every fragment is unchanged
   (identity in, explicit identity out; pinned by the #349 matrix).
2. **Observability (step 2).** Emit the number of empty-placement committed
   records remaining, per pass, on the durability-plane seam — the existing
   `tracing`→OTel bridge idiom (`gauge.` sample like
   `rebalance.rs:emit_domain_utilization` ~`:311`, plus the append-only audit
   target pattern), so an operator can watch the population drain to zero. Exact
   metric name is Do's call; the drain-to-zero observability is binding.
3. **Removal gate (step 3) — deliberately deferred.** Once the signal reads zero
   and a scan confirms no empty-placement committed record remains, the empty
   branch becomes a defensive error behind a metadata format-version bump
   (ADR-0040 decision 6). Both preconditions are runtime facts about a deployed
   store, unsatisfiable in this slice; flipping the gate is a separate future work
   item, tracked by follow-up issue **#363**. This slice therefore publishes with
   the standard closing trailer (`Fixes #350`) — #350's remit is steps 1–2, and
   step 3 has its own tracking item.

## Alternatives considered

- **Rely on touch-driven materialization only** (decision 5: reconstruction/
  rebalance rewrite placement when they act): rejected — the never-touched long
  tail never drains, so the fallback stays load-bearing forever.
- **Flip the defensive error in the same slice, dark behind a version flag:**
  rejected — Wyrd has no metadata format-version machinery today; introducing it
  for an inert branch inflates blast radius with no verifiable behaviour, and the
  gate's preconditions cannot hold at build time.
- **Backfill malformed vectors to identity too:** rejected — ADR-0040 decision 3:
  a non-empty wrong-length vector can only mean truncation/corruption; rewriting
  it fabricates placement and destroys the operator signal (#348's posture).

## Impact & compatibility

Additive: a new custodian pass + telemetry, contained in `crates/custodian` (new
module, wiring in `reconciliation.rs`/`lib.rs`). No read-path, write-path, or
on-disk chunk-format change; no new dependency. Resolution semantics of every
existing record are preserved (identity → explicit identity). Concurrency-safe by
construction: the version-conditional commit is the same second fence writers and
custodians already race through (`0005:200-203`, ADR-0015). The #349 mixed-era
test matrix must remain green. ADR-0040 is `Proposed`, not Accepted — no
immutability concern; this slice implements its decision 6 steps (a)/(b) without
editing the ADR.

## Open questions

- Metric shape: per-pass gauge of remaining empty-placement records vs. a
  scan-derived counter — Do's call within the existing seam idiom; maintainer
  confirms at sign-off (ADR-0011 seam changes are reviewer NEEDS-HUMAN territory
  per INTEGRATION §4 only if the seam contract itself changes; this adds an
  emission, it does not alter the bridge).
- Pass cadence/placement within `reconcile_step` ordering (before/after GC) — Do's
  call; the backfill is read-mostly and CAS-guarded, so ordering is not
  correctness-bearing.

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a
draft PR MAY happen during the cycle (useful for CI feedback). The PR MUST NOT be
marked ready before sign-off accepts.
