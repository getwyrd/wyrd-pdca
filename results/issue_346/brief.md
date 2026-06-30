# Brief — issue 346 / rebalance-evac-identity-placement-fallback

> The Plan artifact (docs 02 §PLAN). Human-authored. Do reads ONLY this file.
> The field labels below are parsed by the driver — keep the `- **Label:** value`
> shape. The success criterion is the load-bearing field: it is the sentence
> Check tests "did this work" against.

- **Slug:** rebalance-evac-identity-placement-fallback
- **Defect:** Rebalance evacuation planning does not apply the identity-placement
  fallback that the read path, GC, scrub, and reconstruction all use, so a pre-M3 /
  mixed-era chunk is left on a draining D server and lost on decommission. Two failure
  modes in `crates/custodian/src/rebalance.rs`: (1) `plan_evacuations` (`:150-160`)
  iterates the **raw** `ChunkRef.placement` vector, which is empty for a pre-M3 record
  (`#[serde(default)]`, `crates/core/src/metadata.rs:93`); the iterator yields nothing,
  `evac` is empty, and the chunk is `continue`-skipped — no `EvacPlan` is produced even
  though its fragment is live and on a draining server. (2) `EvacPlan.placement` is
  stored raw (`:175`) and consumed downstream in `evacuate_chunk` —
  `new_placement = plan.placement.clone()` (`:221`), `plan.placement[index]` (`:224`),
  `new_placement[index] = target` (`:245`), and the version-conditional write
  (`:253`) — so an **empty** placement panics (index out of bounds) and a **short** one
  commits a malformed short placement record. Expanding only the filter is a half-fix.
- **Success criterion:** After the rebalance reconcile loop runs over a committed inode
  whose `ChunkRef` has `placement: vec![]` and whose identity-resolved D server (index
  `i`) is marked draining, an `EvacPlan` is produced and committed that moves the
  fragment to a healthy, non-draining, distinct-domain server, AND the committed chunk's
  placement record is **full-length** (`== fragment_count()`) with the moved index no
  longer naming a draining server. The regression test asserting this is red pre-fix (no
  plan produced / panic on the raw-vector clone-index path) and green post-fix.
  BINDING: a plan is produced for the pre-M3 chunk, and the committed placement is
  full-length and repointed off the draining server. Resolving via `placed_dserver` /
  materializing through `0..fragment_count()` is ILLUSTRATIVE — the mechanism is Do's.
- **Invariant to restore:** Every placement-consuming custodian path resolves a chunk's
  fragment locations through the **same authoritative identity-placement-fallback
  resolution** (an absent or short `placement[i]` resolves to D-server `i`), and any path
  that re-places fragments commits back a **complete, full-length** (`fragment_count()`)
  placement record — so a mixed-era chunk has exactly one placement closure across read,
  GC, scrub, reconstruction, AND rebalance. Source: `crates/core/src/metadata.rs:110-124`
  — `ChunkRef::placed_dserver` is documented as "the single authoritative
  placement-resolution definition" for the read path, GC, scrub, and reconstruction; its
  caller list omits rebalance, which is precisely the defect (the omission is the tell).
  Internal invariant of proposal 0005 "the placement record" (`docs/principles.md` §6,
  placement/durability category).
- **Repo + branch target:** getwyrd/wyrd @ main   (INTEGRATION §2 — everything targets `main`)
- **Depends on:** (none)
- **Conflicts with:** (none)
- **Ordering note:** #346 (write/drain side) and #356 (read/fan-out side) are the two
  twins of the same #292 placement-fallback class, but they touch **disjoint files**
  (`custodian/src/rebalance.rs` here vs `chunkstore-grpc/src/fanout.rs` there) with no
  build-on dependency and no shared resource — they run in the **same wave, in parallel**.
  Both build on the already-merged placement-record machinery (093732d / #139,
  `ChunkRef::placed_dserver` + `fragment_count`), which is on `main`; no in-batch
  prerequisite.
- **Surfaces:** data
- **Difficulty:** medium
- **Scope:** rebalance evacuation planning (and the placement vector it hands to the
  commit) must consider and write the same fully-resolved placement closure every other
  placement-consuming path uses — so a pre-M3 / mixed-era fragment on a draining server
  is selected for evacuation, and the record the move commits is a complete, full-length
  placement vector (not a panic on an empty vector or a corrupt short one). Apply the same
  resolution to the `survivor_domains` computation so spread is preserved for mixed-era
  chunks. / out of scope: the read-side fan-out routing (#356); changing the
  identity-fallback definition or placement semantics themselves; the GC/drain sibling
  (#287); introducing a new shared `ChunkRef::fragments()` helper (separate issue) — use
  what already exists.
- **Repro instruction:** In `crates/custodian/tests/rebalance.rs` (helpers `four_domains`,
  `write_rs_2_1`, `elect`, the reconcile-loop pattern of
  `drains_a_d_server_and_evacuates_to_a_distinct_domain_through_reconcile_step`): commit an
  inode whose `ChunkRef` carries `placement: vec![]`, mark the identity-resolved D server
  (index `i`) draining, run the rebalance / reconcile loop. Today no `EvacPlan` is produced
  (the chunk is skipped); forcing selection on the raw vector panics or commits a short
  placement. Cover `EcScheme::None` (the single fragment at index 0) and a `ReedSolomon`
  case with the draining fragment at index > 0.
- **Test file:** crates/custodian/tests/rebalance.rs
- **Citations expected:** Do must cite path:line on the target branch for every change.
- **Prior-art check (triage cycles):** searched by file path. `crates/custodian/src/rebalance.rs`
  was introduced whole by 185f66a ("custodian: rebalance loop drains and decommissions D
  servers") and never applied the fallback; the placement-record slice (093732d / #139,
  "Record fragment placement so a moved fragment is still found") added `placed_dserver`
  but deferred wiring rebalance to it. No open/closed PR and no branch
  (`git branch -a` for 346/evac/rebalance) addresses the rebalance evacuation fallback.
  Genuine open defect; sibling-but-distinct from the merged drain/GC work (#287).
- **Disposition hint:** likely-fix

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a
draft PR MAY happen during the cycle (useful for CI feedback). The PR MUST NOT be
marked ready before sign-off accepts.
