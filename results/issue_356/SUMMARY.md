# Result — issue 356 / relocatable-fanout-route-by-placed-dserver

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect / goal: `FanoutChunkStore` implements `PlacementChunkStore` with the **default**
- Success criterion: A fragment whose committed placement repoints it to a
- Repo + branch target: getwyrd/wyrd @ main   (INTEGRATION §2 — everything targets `main`)
- Scope (one logical fix) / out of scope: make the fan-out honour the placed D server its `PlacementChunkStore`

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: likely-fix
- Confidence: medium
- Recommendation: (set by Do)

## 3. Correctness (Check — chain)
- C1 Spec: none — brief.md
- C2 Reproduction (red pre-fix): none — (no gate configured)
- C3 Change: none — patch.diff
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): pass — xtask ci: all checks passed
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass —                as its own file to earn the full red->green.
- C5 Causal adequacy: none — reviewer + human sign-off

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 Contribution: none — (no gate configured)
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

# Check review — issue 356 / relocatable-fanout-route-by-placed-dserver

**Mode:** advisory, artifact-only (build-notes.md withheld). Grounded read-only on
`$PDCA_TARGET = /home/eddie/wyrd/wyrd.pdca-wt` (patch already applied there; builds clean
per `check-gates.json` C4-ci). Target readable and current — no staleness caveat.

## What I re-derived (not what was claimed)

- **Causal chain, end to end.** `crates/core/src/read.rs:103-104,132-133` resolves each
  fragment's placed D server (`fragment_dserver` → `ChunkRef::placed_dserver`) and passes it
  to `get_fragment_at(dserver, id)`. Pre-fix, `FanoutChunkStore` carried the *empty*
  `impl PlacementChunkStore … {}`, so `_at` took the trait **defaults**
  (`crates/traits/src/lib.rs:306-319`) which **ignore `dserver`** and delegate to
  `get_fragment(id)` → `route(index)` = `stores[index % n]`. The resolved placement was
  dropped at the trait boundary — exactly the brief's defect. The patch overrides both
  `_at` methods to route by `route_dserver(dserver)` (`fanout.rs:72-74, 142-155`). That
  **removes** the cause; it is not a capability probe or runtime guard, so the C5
  symptom-guard smell-test does **not** fire.
- **Red pre-fix, by construction.** All three new tests place a fragment **off** its
  `index % n` home and read/write via the `_at` path. Under the pre-fix default that path
  routes by `index % n`, so each must miss (`Ok(None)`) or land on the wrong store — genuine
  red. Post-fix `route_dserver(dserver)` selects the named store — green. Corroborated by the
  deterministic gate `C4-verify = pass`. (I established red-ness by source analysis: the
  target is read-only and the fix is already applied, so I did not stash-and-rerun in place.)
- **Identity case preserved.** `placement(n) = 0..n` (`traits/src/lib.rs:298-300`) ⇒ an
  un-moved fragment has `dserver == index`, and `route_dserver(i) == route(i) == stores[i % n]`
  (`fanout.rs:59-61, 72-74`). The un-moved write/read order is unchanged — matches the brief's
  out-of-scope guard.
- **Scope.** Diff touches only `crates/chunkstore-grpc/src/fanout.rs`; the trait default and
  the id-indexed `GrpcChunkStore` impl are untouched, as the brief requires. Prior-art check is
  documented in the brief by affected file path (093732d/#139 deferred this slice; f98cba7/#225
  did not add it) — settled, not re-litigated.

## Verdict table (5/5/1)

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | Success criterion is concrete and testable: a moved fragment fetched via `get_fragment_at(placed_dserver, id)` returns from the placement-named store, covering `EcScheme::None` + a Reed-Solomon move (brief.md:23-31). Binding clause unambiguous; `dserver % n` declared illustrative. |
| C2 Reproduction (red pre-fix) | PASS | Pre-fix `_at` used the defaults that route by `index % n` (`traits/src/lib.rs:306-319`); the three tests place off-index (`fanout.rs:326-334, 362-377, 402-408`), so each misses pre-fix. Verified by source analysis + gate `C4-verify=pass`; target read-only so no in-place stash rerun. |
| C3 Change | PASS | Single logical change in one file: adds `route_dserver` (`fanout.rs:72-74`), overrides `get_fragment_at`/`put_fragment_at` (`fanout.rs:142-155`), imports `DServerId` (`fanout.rs:30`), updates module docs. No collateral edits; matches scope. |
| C4 Verification (red→green) | PASS | Gate `C4-ci=pass` (fmt/clippy/build/test/deny/conformance) and `C4-verify=pass` (per-fix red→green) in check-gates.json; target compiles with the override in place (`fanout.rs:142-155`). |
| C5 Causal adequacy | PASS | Fix eliminates the root cause — the dropped-`dserver` default — by honouring the resolved id (`fanout.rs:143-154`), restoring the chunk-map-as-authority invariant (brief.md:32-41). Not a guard/probe, so the symptom-guard rule does not fire. Resolution mechanism `stores[dserver % n]` is brief-blessed illustrative and exact for in-range ids (see Validation row for the deployment caveat). |
| T1 Structure | PASS | Regression lives in the named home — the inline `#[cfg(test)] mod tests` in `crates/chunkstore-grpc/src/fanout.rs:302-418` — exactly the brief's Test file. |
| T2 Shape | PASS | Assertions pin the right property: moved fragment returned from the named store, not `index % n` — `EcScheme::None` total-miss case (`fanout.rs:321-343`), rotated Reed-Solomon all-moved case (`fanout.rs:353-391`), and the write side (`fanout.rs:397-418`). Mirrors `placement_record.rs:192`. |
| T3 Runtime | PASS | Tests are non-tautological: they write to a backend then read via `get_fragment_at` and assert byte-equality against the named store, with `assert_ne!` guarding that each `dserver` is a genuine move (`fanout.rs:365-369`). Exercises real routing, red pre-fix. |
| T4 Contribution | PASS | Adds first-ever coverage of relocatable `_at` routing on the fan-out — previously the empty impl had none; the three cases lock the moved-id behaviour against regression to `index % n`. |
| T5 Judgment | PASS | Test selection is sound and matches the brief's required EcScheme::None + RS coverage. Advisory note for the human: no explicit test pins the **identity** (un-moved) fragment through `get_fragment_at` — it is covered only structurally by `route_dserver(i)==route(i)`; a one-line identity assertion would make the "routes exactly as today" guard regression-proof. Not blocking. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decision owed: does the fan-out's `stores[dserver % n]` faithfully model the real fleet's D-server→store resolution? It is exact only when placed D-server ids stay in `0..n` (the fan-out's one-D-server-per-store model); if this fan-out is ever composed over a **subset** of a larger/non-contiguous global D-server id space, `dserver % n` would silently mis-route a moved id while these tests (contiguous small ids) still pass. The brief blesses `dserver % n` as illustrative — human must confirm the deployment's id-range assumption holds, and that the read-side fix alone closes the production miss (twin #346 is the disjoint write/drain side). |

## NEEDS-HUMAN items to clear (→ SUMMARY §6)

1. **Validation / fitness-to-purpose:** confirm `stores[dserver % n]` correctly resolves
   placed D-server ids in deployment (id-range assumption), and that the read-path fix
   alone restores end-to-end readability of a moved object. See Validation row.

## Advisory (non-gating)

- Consider one identity-path assertion (`get_fragment_at(index, fid(index))` == `route(index)`)
  to lock the "un-moved routes exactly as today" invariant the brief calls out (T5 row).

### Advisory — codex

- NEEDS-HUMAN — `crates/chunkstore-grpc/src/fanout.rs:73`: `route_dserver` maps the placement-resolved `DServerId` with `dserver % stores.len()`, which only honors the named D server if stable IDs are dense fan-out slots. The repo already permits opaque IDs such as `10, 20, 30` and selects those exact IDs into placement (`crates/server/tests/failure_domain_registration.rs:32`, `crates/server/tests/failure_domain_registration.rs:66`), so a moved/placed record naming one of those IDs can still be routed to the wrong backend unless the human accepts modulo aliasing as the fan-out identity contract.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [x] Validation — fitness-to-purpose — Decision owed: does the fan-out's `stores[dserver % n]` faithfully model the real fleet's D-server→store resolution? It is exact only when placed D-server ids stay in `0..n` (the fan-out's one-D-server-per-store model); if this fan-out is ever composed over a **subset** of a larger/non-contiguous global D-server id space, `dserver % n` would silently mis-route a moved id while these tests (contiguous small ids) still pass. The brief blesses `dserver % n` as illustrative — human must confirm the deployment's id-range assumption holds, and that the read-side fix alone closes the production miss (twin #346 is the disjoint write/drain side).
- [x] `crates/chunkstore-grpc/src/fanout.rs:73`: `route_dserver` maps the placement-resolved `DServerId` with `dserver % stores.len()`, which only honors the named D server if stable IDs are dense fan-out slots. The repo already permits opaque IDs such as `10, 20, 30` and selects those exact IDs into placement (`crates/server/tests/failure_domain_registration.rs:32`, `crates/server/tests/failure_domain_registration.rs:66`), so a moved/placed record naming one of those IDs can still be routed to the wrong backend unless the human accepts modulo aliasing as the fan-out identity contract.

## 7. Proven / not proven
- Proven by which oracle: gates overall = pass (stub oracles).
- Unproven / needs manual run: anything flagged in §6.

## 8. Ready-to-ship attachments
- patch.diff
- tracker-comment.md     (ALWAYS, every tracker item)
- build-notes.md         (builder rationale — for the human, not the reviewer)

## 9. Check sign-off                     ← human completes Check here
- Disposition confirmed / overridden:
- Outcome: merged-wider
- Iteration delta (if iterating):
- By / date: Eduard Ralph / 2026-06-30

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
- M3 enhancement (not a bug in this slice): when dynamic discovery lands, route the fan-out by a real D-server-id→store map instead of `dserver % n` — opaque/sparse/gapped ids alias under modulo; add an in-range guard at the M2/M3 boundary so a discovery-fleet placement is never silently mis-routed through the `% n` fan-out.
