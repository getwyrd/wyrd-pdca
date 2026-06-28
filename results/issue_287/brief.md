# Brief — issue 287 / gc-identity-placement-fallback

> The Plan artifact (docs 02 §PLAN). Human-authored. Do reads ONLY this file.

- **Slug:** gc-identity-placement-fallback
- **Defect:** The read path tolerates pre-M3 / mixed-era chunk maps with empty or short
  placement vectors by falling back to identity placement (`fragment index -> D-server
  id`, `read.rs:fragment_dserver`). GC's safety reference set does NOT apply that fallback:
  `referenced_fragments` (`crates/custodian/src/gc.rs:189`) iterates only
  `chunk.placement.iter().enumerate()`, so a committed pre-M3 object whose `ChunkRef`
  decodes with `placement: vec![]` (`#[serde(default)]`, `metadata.rs:93`) contributes NO
  protected fragments. A stale orphan record or expired pending-ledger entry pointing at
  the same fragment then lets GC delete live committed data. (scrub uses the same
  `referenced_fragments`, so it shares the gap.)
- **Success criterion:** A committed inode whose `ChunkRef` has `placement: vec![]` has its
  identity-fallback fragments included in GC's reference set, so GC does NOT delete a
  fragment that an orphan record references at `(dserver, FragmentId { chunk, index })`
  resolved by the identity fallback — i.e. GC's protected set matches the placement closure
  the read path actually resolves. Demonstrated by the regression below: red pre-fix (GC
  reclaims the fragment), green post-fix (it is skipped as referenced).
- **Invariant to restore:** A fragment that a **committed** chunk-map reference resolves to
  — under the SAME placement resolution the read path uses, identity fallback included —
  must NEVER be passed to `delete_fragment`. The GC reference set must be the closure of
  *readable* committed fragments, not only those listed explicitly in `placement`. Source:
  the GC module durability invariant (`crates/custodian/src/gc.rs:24,110` — "a fragment a
  placement record points at is never reclaimed") and proposal 0005 (M3 Custodians, the
  placement record / GC safety reference set,
  `docs/design/proposals/accepted/0005-milestone-3-custodians.md`); the readable-fragment
  resolution is `read.rs:fragment_dserver` and `reconstruction.rs:227-235` (both already
  apply the identity fallback). Stated over the category "committed-but-fallback-placed
  fragments," not the `vec![]` repro alone.
- **Repo + branch target:** getwyrd/wyrd @ main
- **Conflicts with:** 288
- **Ordering note:** Conflicts-with 288 because both land on the `crates/core` read/repair
  path and 287's "centralize placement expansion … use it consistently in read, scrub, GC"
  direction may touch `crates/core/src/read.rs` (`fragment_dserver`), which 288 edits
  directly — schedule them into different waves so neither builds blind on the other's
  base. No build-on dependency either way.
- **Surfaces:** data
- **Difficulty:** medium
- **Scope:** Make GC's (and the shared scrub) reference-set computation honor the
  identity-placement fallback for empty/short placement vectors, with scheme-aware fragment
  count expansion (`EcScheme::None` → 1 fragment; `EcScheme::ReedSolomon { k, m }` → `k+m`
  fragments), so the protected set equals the read path's resolved placement. Centralizing
  the `ChunkRef` placement expansion so read/scrub/GC/reconstruction share one definition
  is acceptable and preferred, but the binding requirement is GC's reference set, not a
  refactor. / out of scope: changing the read path's existing fallback behaviour; orphan /
  pending-ledger lease semantics; rebalance (touch only if the shared expansion helper
  naturally reaches it).
- **Repro instruction:** On `main`, seed a committed `InodeRecord` whose `ChunkRef` has
  `scheme: EcScheme::None` (or ReedSolomon) and `placement: vec![]`, plus a stale orphan
  record for `(0, FragmentId { chunk, index: 0 })` whose lease has expired; run GC. The
  fragment is reclaimed (`delete_fragment` called) because `referenced_fragments` returned
  the empty set for that chunk.
- **Test file:** crates/custodian/tests/gc.rs
- **Citations expected:** Do must cite path:line on the target branch for every change
  (`crates/custodian/src/gc.rs:189`, `crates/core/src/read.rs:99-105`,
  `crates/core/src/metadata.rs:93`, `crates/custodian/src/reconstruction.rs:227-235`).
- **Prior-art check (triage cycles):** Searched `crates/custodian/src/gc.rs` and
  `crates/core/src/read.rs` history — the placement record landed in `093732d`, GC
  reclaim/reference-set in `af4ab65`; the read-side identity fallback
  (`read.rs:fragment_dserver`) and reconstruction's mirror
  (`reconstruction.rs:227-235`) exist, but GC was never updated to match. No open PR
  touches these files (`gh pr list` empty). Net-new fix.
- **Disposition hint:** likely-fix

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a
draft PR MAY happen during the cycle (useful for CI feedback). The PR MUST NOT be
marked ready before sign-off accepts.

## Iteration 1 — carry-forward (from the previous attempt)
- Sign-off rationale: Rejected: the reviewer's two substantive flags must be addressed before this can ship. (1) Test breadth — the only regression covers EcScheme::None + placement: vec![] at index 0; the RS{k,m} k+m expansion and the short/partial-placement merge (.get(i).unwrap_or(i) where some i are present) ship untested and a regression there would pass CI silently. Add an RS case (orphan at index > 0) and a short-vector case. (2) Centralize the placement expansion — gc.rs now holds a 3rd copy of the expansion/identity-fallback logic alongside read.rs and reconstruction.rs:227-235. GC safety depends on matching the read path's protected set exactly, so factor this into a single shared ChunkRef helper (the brief's preferred approach) instead of inlining a 3rd copy, so placement semantics cannot drift across callers. The fix's core logic is sound; rebuild from the same brief with these addressed.
- Full previous attempt preserved in `iteration-v1/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).
