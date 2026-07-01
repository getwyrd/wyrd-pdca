# Brief (pointer) — issue 347 / chunkref-fragments-placement-expansion-helper

> Plan artifact (pointer form). The planning decision already lives in a governed
> host artifact — **ADR-0040** — so this brief POINTS at it and carries only the
> fields the driver parses. Do reads ADR-0040 as the authoritative plan; this file
> does not restate it. The load-bearing field is the success criterion.

- **Slug:** chunkref-fragments-placement-expansion-helper
- **Planning artifact:** `docs/design/adr/0040-mixed-era-placement-expansion.md` (ADR-0040,
  *decisions 1 & 2*) — read in place in the `../wyrd` checkout. Decision 2 specifies the
  helper normatively: `ChunkRef::fragments() -> impl Iterator<Item = (u16, DServerId)>`
  over the full index space, **liberal** (infallible, applies the identity fallback
  unconditionally, does *not* validate length), with `fragment_count()` / `placed_dserver()`
  as its primitives. Decision 1 gives the invariant. This ADR is authoritative; Do cites it.
- **Defect / goal:** There is no single "walk every fragment to its holding D-server"
  definition. Three consumers open-code the identical `(0..fragment_count()).map(|i|
  placed_dserver(i))` expansion — GC `referenced_fragments` (`crates/custodian/src/gc.rs:197`),
  reconstruction `assess` (`crates/custodian/src/reconstruction.rs:230`), and rebalance
  `plan_evacuations` (`crates/custodian/src/rebalance.rs:165`, fixed under #346, PR #357).
  That duplication is how the rebalance raw-`placement` divergence (#346) went unnoticed.
  Goal: introduce the one `ChunkRef::fragments()` expansion helper (ADR-0040) and route
  those three hand-rolled read-expansion consumers through it, so placement expansion has
  a single definition.
- **Invariant to restore:** Every interpretation of a committed chunk map that needs the
  `(fragment index, holding D-server)` set resolves it through **one** placement-expansion
  definition over the full `0..fragment_count()` index space (identity fallback per index),
  not an open-coded per-caller walk of the raw `placement` vector. Source: ADR-0040
  decision 1 (the normative expansion rule) and decision 2 (one expansion helper) —
  authoritative, `../wyrd/docs/design/adr/0040-mixed-era-placement-expansion.md:55-73`.
  (Category-wide over every committed-chunk-map interpreter; not satisfiable by guarding a
  single module — all three read-expansion consumers must draw from the one helper.)
- **Success criterion:** `ChunkRef::fragments()` exists (BINDING: signature & semantics per
  ADR-0040 decision 2 — an iterator of `(u16, DServerId)` over `0..fragment_count()`,
  each index resolved via `placed_dserver`), and the three read-expansion consumers
  (`gc.rs` `referenced_fragments`, `reconstruction.rs` `assess`, `rebalance.rs`
  `plan_evacuations`) obtain their `(index, dserver)` expansion from it — no open-coded
  `(0..fragment_count()).map(|i| placed_dserver(i))` walk remains in those three sites.
  A unit test in the shipped test file asserts `fragments()` yields exactly the per-index
  `placed_dserver` resolution for `EcScheme::None` and `ReedSolomon{k,m}` across **empty**,
  **full** (`len == fragment_count()`), and **short** placement vectors. The change is
  behaviour-preserving (a pure centralization); the exact routing expression at each
  consumer is Do's call (ILLUSTRATIVE) as long as the binding conditions hold.
- **Repo + branch target:** getwyrd/wyrd @ main   (INTEGRATION §2 — everything targets `main`;
  Wyrd has no maintenance branches)
- **Depends on:** none   (the one sibling prereq, #346 rebalance, is already MERGED — PR #357;
  `plan_evacuations` exists on `main` in fixed hand-rolled form, ready to route)
- **Conflicts with:** none co-scheduled in this run
- **Ordering note:** #347 is item 2 of the #292 audit follow-ups (ADR-0040 Consequences).
  Downstream siblings build ON this helper and edit overlapping files, so if they are ever
  co-scheduled they belong in a LATER wave than #347: #348 (strict/malformed-length handling
  + `checked_fragments()`/`placement_is_valid()` companion — edits metadata.rs + the
  maintenance loops), #349 (mixed-era test matrix — edits the placement test area), and the
  newly-split #360 (the CI grep-gate that fails the build on raw `ChunkRef.placement`
  iteration — must land AFTER this migration so it has a green tree to protect). #350
  (backfill migration) is independent of this helper. None are part of this bundle.
- **Scope:** add `ChunkRef::fragments()` (ADR-0040 decision 2) in `crates/core/src/metadata.rs`
  alongside `fragment_count()` / `placed_dserver()`; route the three hand-rolled
  read-expansion consumers (GC `referenced_fragments`, reconstruction `assess`, rebalance
  `plan_evacuations`) through it; document `fragments()` and update the `placed_dserver` doc
  comment (`metadata.rs:110-118`) to list all callers including rebalance.
  / out of scope: the strict/malformed-length maintenance handling and the
  `checked_fragments()`/`placement_is_valid()` companion (#348); the **write / repoint**
  sites that construct a full-length placement vector (ADR-0040 decision 5 — `write.rs:84/103/234`,
  `rebalance.rs:239/242/263/271`, `reconstruction.rs:388/406/418` — they build/index `.placement`,
  some on the *plan* structs not `ChunkRef`, and MUST stay); the read path's per-index
  `placed_dserver` use (`read.rs:104` — resolves one index at a time, stays); and the CI
  grep-gate (**#360**). No behaviour change.
- **Difficulty:** low   (blast-radius: ~5 files, all localized single-site edits; the helper
  returns exactly what the hand-rolled loops produced, so effects do not propagate — a pure,
  behaviour-preserving centralization. A diff-reviewer holds one new helper + three one-line
  routing edits + a doc update + one unit test.)
- **Verification posture:** NET-NEW coverage over a net-new API — the `fragments()` unit
  test's "red" is criterion-ABSENCE (the helper does not exist / does not compile against the
  pre-fix tree), not a prior failing assertion flipping. It is red pre-fix (no `fragments()`)
  and green post-fix, and runs fully at Check under `cargo xtask ci` — nothing deferred
  off-Check. The consumer routing is behaviour-preserving and stays covered by the existing
  custodian GC / rebalance / reconstruction tests (which must remain green).
- **Test file:** `crates/core/tests/placement_record.rs` (the existing placement-record test
  home) — add the `fragments()` equality-vs-`placed_dserver` matrix (`EcScheme::None` and
  `ReedSolomon{k,m}` × empty / full / short placement vectors).
- **Citations expected:** Do must cite `path:line` on `main` AND ADR-0040 for every change.
- **Prior-art check (triage cycles):** searched by affected file path across merged history,
  open PRs, and closed PRs — `ChunkRef::fragments()` is NOT present (only unrelated match is a
  chunkstore-fs conformance test); no open/closed PR implements #347. `metadata.rs` placement
  primitives were added by #139/#287 (merged). #346 (rebalance identity-fallback fix) merged as
  PR #357. Result: net-new helper, no duplication, prereq already landed.
- **Disposition hint:** likely-fix

## STOP discipline

Draft only until Check sign-off. A draft PR MAY be opened for CI; the PR MUST NOT be
marked ready before sign-off accepts.
