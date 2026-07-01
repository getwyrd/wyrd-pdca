# Result — issue 349 / mixed-era-placement-test-matrix

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect / goal: The mixed-era placement resolution — `ChunkRef::placed_dserver`'s
- Success criterion: Every `ChunkRef.placement` consumer — Read, GC, Scrub,
- Repo + branch target: getwyrd/wyrd @ main   (INTEGRATION §2 — everything targets `main`)
- Scope (one logical fix) / out of scope: add the missing mixed-era placement matrix cells so each consumer that reads

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

# Check review — issue 349 / mixed-era-placement-test-matrix

> Advisory, artifact-only, decorrelated. Inputs seen: `patch.diff`, `brief.md`,
> `check-gates.json`. `build-notes.md` deliberately withheld. **No Write/Edit.**

## Grounding posture (read first)

- `$PDCA_TARGET` could **not** be resolved in this sandbox: environment and
  `/proc` introspection are blocked, and there are ~20 candidate `wyrd`
  worktrees under `/home/eddie/wyrd/` which I am instructed not to wander into.
  Per the grounding rule I ground **against `patch.diff` alone**. This is a
  *target-state caveat*, **not** a patch defect — I raise **no** "cannot
  apply / does not compile" FAIL on this basis.
- Test-side citations below are `patch.diff:<line>`. Resolver source-line
  citations (`metadata.rs:119-124` `placed_dserver`; `read.rs:103-105`;
  `gc.rs:197-204` `referenced_fragments`; `reconstruction.rs:230-232`/`388-418`;
  `rebalance.rs:165-167`; `write.rs:171`; `scrub.rs:30`) are **as asserted by the
  patch comments / brief** and were **not** independently re-grounded against a
  confirmed target.
- This slice is **test-only**: every hunk lands in `crates/*/tests/`
  (`placement_record.rs`, `gc.rs`, `rebalance.rs`, `reconstruction.rs`,
  `scrub.rs`, `dst_erasure.rs`). No production diff — confirmed by reading every
  hunk header in `patch.diff`. Matches the brief's "tests only" scope.
- **C5 symptom-guard smell-test: does not fire.** The patch adds no capability
  probe (no `hasattr`/`try-import` analogue) and no runtime guard around an
  optional capability — it adds test coverage only, no production code path.

## Verdict table (canonical 5/5/1 order)

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | Success criterion is concrete and testable — the {empty(None),empty(RS),short,full,RS{6,3}} × {Read,GC,Scrub,Reconstruction,Rebalance} matrix + the empty-placement DST cross-seed cell + the empty→full-length re-placement pin (`brief.md:29-43`); no spec defect. |
| C2 Reproduction (red pre-fix) | N/A | Net-new coverage, **born-green by design**: the resolvers (#139/#287/#346/#356) are already merged, so there is no pre-fix failing assertion (`brief.md:117-124`). The surrogate "red" is the per-consumer negation→red→restore recorded in the **withheld** `build-notes.md`; its deterministic confirmer is the C4-verify gate (passed). |
| C3 Change | PASS | Additive, test-only, no production diff; cells land in the six named test homes and reuse existing fixtures (`patch.diff:1,203,334,490,891,1181`). Scope honoured (`brief.md:77-85`). |
| C4 Verification (red→green) | PASS | Gating `C4 Wyrd gate: cargo xtask ci` = **pass** and `C4 per-fix red->green` = **pass** (`check-gates.json:33-49`); the full-suite green also covers compile (duplicate-symbol / import correctness). Could not independently re-execute (no resolvable target/build env in sandbox) — trusting the deterministic gate, not a claim. |
| C5 Causal adequacy | NEEDS-HUMAN | Decision owed: is **every** cell load-bearing on the `placed_dserver` identity-fallback, or do some pass regardless? The empty/short cells red plausibly (raw-vector negation → out-of-bounds / too-few references), but the **full RS{6,3}** cells (`patch.diff:401` rebalance, `:806` reconstruction, `:1105` scrub) carry length-n vectors that resolve identically raw-or-expanded — they pin **scheme size**, not the fallback. And the DST "maintenance agrees" arm asserts a **local replica** `maintenance_resolved` (`patch.diff:1222`), not the real GC/scrub/reconstruction code. Human must confirm the matrix locks the fallback invariant uniformly. |
| T1 Structure | PASS | Cells live in the correct pre-existing test files per the brief's matrix homes (`brief.md:112-116`); no misplacement, no production reach. |
| T2 Shape | PASS | Consistent with existing fixtures/patterns (`Fleet`/`rs_plan`, `write_rs_2_1`, `elect`, `reconcile_step`, `#[tokio::test]`/`block_on`); new local helpers `ten_domains`/`write_rs_6_3` are per-file (separate test crates, no symbol clash — gate-confirmed) (`patch.diff:349,370,753,774`). |
| T3 Runtime | PASS | Tests execute and pass — `cargo xtask ci` ran the full suite green (`check-gates.json:33-40`). |
| T4 Contribution | NEEDS-HUMAN | Decision owed: matrix completeness + prior-art (the latter not mechanically settleable here — no PR/history access). Two concretes: (a) the **rebalance** empty-placement cells appear as **pre-existing context**, not added by this patch (`patch.diff:338`, `@@ …evacuates_a_pre_m3_chunk_with_empty_placement_reed_solomon_index_gt_zer`) — so a prior change already added them; confirm an **empty(None)** rebalance evacuation cell exists (brief asked for it, `brief.md:101-105`) and that nothing is duplicated with siblings #347/#350. (b) Confirm each named matrix column is present exactly once per consumer. |
| T5 Judgment | NEEDS-HUMAN | Decision owed (oracle = reviewer + human sign-off): accept the **born-green** posture and the **DST local-replica** maintenance arm as adequate proof of the one-placement-closure invariant, given the negation evidence lives in the withheld `build-notes.md` and cannot be re-grounded here. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decision owed: does this matrix deliver a *genuine uniform regression net* — would a future regression in **any** consumer's identity-fallback resolution actually redden a cell — or are there silent holes (e.g. rebalance empty(None), the full RS{6,3} cells' weaker fallback coupling, the DST replica)? This is the human fitness call at sign-off. |

## Notes feeding §6 (NEEDS-HUMAN items to clear)

1. **C5 / T5 — load-bearingness of full RS{6,3} and DST cells.** The fallback
   cells (empty/short) have a credible raw-vector negation. The three *full*
   RS{6,3} cells resolve the same with or without `placed_dserver` (vector is
   already length n), so their negation story is scheme-size, not fallback;
   the DST cell's "maintenance" arm tests a hand-written copy of the resolver
   formula (`maintenance_resolved`, `patch.diff:1222-1226`), while the real
   maintenance paths are exercised separately by the in-process GC/scrub/
   reconstruction cells. Confirm via `build-notes.md` that each negation was
   actually run per cell and reddened it.
2. **T4 — rebalance empty coverage + prior-art.** The empty-placement rebalance
   cases are pre-existing in the post-patch file (shown as diff context at
   `patch.diff:338`), contradicting the brief's "covers only full RS{2,1}"
   (`brief.md:20-21`). Either the brief is stale or a prior cycle landed them.
   Confirm: (a) an `EcScheme::None` empty rebalance evacuation cell exists;
   (b) no duplicate/competing coverage from open siblings #347/#350; (c) the
   prior-art-by-affected-path check (`brief.md:134-144`) holds — not mechanically
   settleable without PR/history access.
3. **Validation fitness-to-purpose** — as in the table: is the net uniform and
   hole-free?

## What I could and could not re-run

- **Could not** re-execute `cargo xtask ci` or the per-cell negations: no
  resolvable `$PDCA_TARGET` / build environment in this sandbox, and
  `build-notes.md` (the negation ledger) is withheld by design.
- **Did** verify against `patch.diff`: test-only scope; correct matrix homes;
  per-consumer cell presence (Read empty(None) `:27`, empty RS{6,3} `:87`,
  short RS{6,3} `:140`; GC empty/short RS{6,3} `:222`/`:281`; Scrub empty(None)
  `:948`, empty(RS) `:998`, short `:1050`, full RS{6,3} `:1105`; Reconstruction
  empty re-placement pin `:524` with full-length assertion `:608-612`, short
  `:642`, full RS{6,3} `:806`; Rebalance RS{6,3} `:401`; DST empty cross-seed
  `:1231`/`:1304` + pinned seed `:1317`); the re-placement pin asserts
  `placement.len() == fragment_count()` and a distinct rebuilt domain
  (`patch.diff:608-628`) per criterion 3. Relied on the deterministic C4 gate
  for compile + green.

### Advisory — codex

- NEEDS-HUMAN — `crates/server/tests/dst_erasure.rs:263` defines the DST “maintenance” side as a test-local helper that directly calls `ChunkRef::placed_dserver`, and `crates/server/tests/dst_erasure.rs:317` asserts against that helper rather than running any real maintenance consumer. This is useful for pinning the shared resolver shape, but it may not satisfy the brief’s DST requirement to assert read and maintenance resolve the explicit empty-placement chunk identically across seeds.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [x] C5 Causal adequacy — Decision owed: is **every** cell load-bearing on the `placed_dserver` identity-fallback, or do some pass regardless? The empty/short cells red plausibly (raw-vector negation → out-of-bounds / too-few references), but the **full RS{6,3}** cells (`patch.diff:401` rebalance, `:806` reconstruction, `:1105` scrub) carry length-n vectors that resolve identically raw-or-expanded — they pin **scheme size**, not the fallback. And the DST "maintenance agrees" arm asserts a **local replica** `maintenance_resolved` (`patch.diff:1222`), not the real GC/scrub/reconstruction code. Human must confirm the matrix locks the fallback invariant uniformly.
- [x] T4 Contribution — Decision owed: matrix completeness + prior-art (the latter not mechanically settleable here — no PR/history access). Two concretes: (a) the **rebalance** empty-placement cells appear as **pre-existing context**, not added by this patch (`patch.diff:338`, `@@ …evacuates_a_pre_m3_chunk_with_empty_placement_reed_solomon_index_gt_zer`) — so a prior change already added them; confirm an **empty(None)** rebalance evacuation cell exists (brief asked for it, `brief.md:101-105`) and that nothing is duplicated with siblings #347/#350. (b) Confirm each named matrix column is present exactly once per consumer.
- [x] T5 Judgment — Decision owed (oracle = reviewer + human sign-off): accept the **born-green** posture and the **DST local-replica** maintenance arm as adequate proof of the one-placement-closure invariant, given the negation evidence lives in the withheld `build-notes.md` and cannot be re-grounded here.
- [x] Validation — fitness-to-purpose — Decision owed: does this matrix deliver a *genuine uniform regression net* — would a future regression in **any** consumer's identity-fallback resolution actually redden a cell — or are there silent holes (e.g. rebalance empty(None), the full RS{6,3} cells' weaker fallback coupling, the DST replica)? This is the human fitness call at sign-off.
- [x] `crates/server/tests/dst_erasure.rs:263` defines the DST “maintenance” side as a test-local helper that directly calls `ChunkRef::placed_dserver`, and `crates/server/tests/dst_erasure.rs:317` asserts against that helper rather than running any real maintenance consumer. This is useful for pinning the shared resolver shape, but it may not satisfy the brief’s DST requirement to assert read and maintenance resolve the explicit empty-placement chunk identically across seeds.

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
- By / date: Eduard Ralph / 2026-07-01

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
