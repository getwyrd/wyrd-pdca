# Result — issue 350 / placement-backfill-migration

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect / goal: Drain the pre-M3 / mixed-era population of committed chunk maps whose
- Success criterion: After backfill reconcile passes over a metadata store
- Repo + branch target: getwyrd/wyrd @ main   (Wyrd has no maintenance branches; INTEGRATION §2)
- Scope (one logical fix) / out of scope: Steps 1 and 2 of the ADR-0040 decision-6 removal path, in one logical

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: new-feature
- Confidence: medium
- Recommendation: (set by Do)

## 3. Correctness (Check — chain)
- C1 Spec: none — brief.md
- C2 Reproduction (red pre-fix): none — (no gate configured)
- C3 Change: none — patch.diff
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): pass — xtask ci: all checks passed
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass — run-verify.sh: PASS — red without the fix, green with it.
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

# Check review — issue #350 / placement-backfill-migration

> **Re-validation correction (2026-07-02).** This advisory ran while the C4-ci gate
> was failing and `cargo` was unavailable in the review sandbox, so its **C4 verdict
> (FAIL)** and the "clippy lint under `all=\"deny\"`" root-cause guess below are
> **superseded**. The C4-ci failure was a sandbox toolchain artifact (the provisioned
> `cc`/`gcc` shim resolved to `zig cc`, whose clang frontend rejected `cc-rs`'s
> `--target=x86_64-unknown-linux-gnu` while building the bench-only `criterion → alloca`
> C dependency under `--all-targets`; `cargo-deny`/`cargo-machete` were also not
> installed). Re-run against the same base (`389b4d2`) with a real `cc` (gcc 15.2) and
> the two CLIs installed, **`cargo xtask ci` passes all checks** and the deterministic
> gate was regenerated to **overall = pass, C4-ci = pass** (`check-gates.json`). The
> non-toolchain verdicts below (T5 / Validation NEEDS-HUMAN — reachability, metric
> shape, unfenced `pub reconcile`, fitness-to-purpose) **stand** and remain the human's
> to clear.

**Task under review:** add a custodian **backfill pass** that drains the pre-M3 / mixed-era
population of committed chunk maps whose `placement` vector is empty — rewriting each such
committed chunk with an explicit full-length identity placement under the custodians'
prior-record CAS, skipping malformed (non-empty wrong-length) vectors, and emitting the
remaining empty-placement count on the durability-plane seam so the population is watchable
as it drains to zero (ADR-0040 decision-6 steps 1–2). Design-proposal brief; binding
conditions are (a) identity backfill under CAS, (b) malformed never rewritten, (c)
drain-to-zero signal.

> Advisory review. Deterministic gates block; I annotate. `cargo` execution is unavailable
> in this review sandbox, so gate-owned legs (clippy) are grounded on `check-gates.json` +
> the workspace lint config; everything else is re-derived by reading the target source at
> `/home/eddie/wyrd/wyrd.pdca-wt` (not stale: the #348 classifier is present at
> `crates/core/src/metadata.rs:159-198`, the patch is applied, and the per-fix test
> compiles and passes).

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | Brief's binding conditions (a)/(b)/(c) are concrete and testable; `backfill.rs` implements exactly them — empty→identity fill, malformed skip, remaining-count gauge. No spec ambiguity in what the code must do. |
| C2 Reproduction (red pre-fix) | PASS | NET-NEW born-at-tier suite `crates/custodian/tests/backfill.rs`; pre-patch the `wyrd_custodian::backfill` module does not exist so the file fails to compile — the demonstrable red the verification posture calls for. `check-gates.json` C4-verify confirms red-without / green-with. |
| C3 Change | PASS | Diff is coherent and self-contained: new `backfill.rs`, `pub mod`/re-export in `lib.rs:23,33`, net-new tests. Stays over the `traits`/`core` seams (backfill.rs:50-53), no read/write-path edit — matches the additive scope. |
| C4 Verification (red→green) | ~~FAIL~~ → **PASS** (re-run) | **SUPERSEDED — see the re-validation banner above.** The original verdict follows, kept for the record: Gating CI leg failed: `cargo clippy --workspace --exclude wyrd-dst --all-targets` exit 101 (`check-gates.json` C4-ci, gating=true, overall=fail). Not a stale-target artifact — target carries #348, patch applies, and the per-fix test compiles+passes under `warnings="deny"`, so the failure is a **clippy-only** lint (passes rustc, fails clippy-driver) tripping `[workspace.lints.clippy] all = "deny"` (Cargo.toml:147-151). Decision owed: builder must run that clippy line to see the exact lint and clear it; the functional red→green (C4-verify) is green, only the lint leg blocks. — **Correction:** the root cause was not a lint under `all="deny"` but the zig-cc/`alloca` toolchain artifact; re-run with a real `cc` (gcc 15.2) + `cargo-deny`/`cargo-machete` installed, `cargo xtask ci` passes all checks and `check-gates.json` was regenerated to overall=pass / C4-ci=pass. |
| C5 Causal adequacy | PASS | Symptom-guard smell-test does NOT fire: `chunk.checked_fragments()` (backfill.rs:94) is design-level data classification (empty/full/malformed), not a capability probe (`hasattr`/try-import) or a runtime guard papering over a load-time side effect. The pass removes the migration debt at its root by materializing the explicit identity vector, the actual ADR-0040 decision-6 step. |
| T1 Structure | PASS | New module mirrors its GC/scrub/reconstruction/rebalance siblings and re-exports through `lib.rs`. Note: `reconcile` is a free `pub` fn rather than the siblings' `pub(crate)` — deliberate, since it is not wired into `reconcile_step` (see T5). |
| T2 Shape | PASS | Signatures/types line up with the real seams: `InodeRecord{size,chunk_map,state,version}` (metadata.rs:200-210), `WriteBatch::new().require().put()` (traits lib.rs:387-410), `MalformedPlacement{expected:u16,actual:usize}` (metadata.rs:193-197). No shape mismatch. |
| T3 Runtime | PASS | All five legs are covered and green per C4-verify: identity fill + version bump, CAS-conflict-loses-and-retries, malformed untouched, idempotent full-length, drain-to-zero gauge. `emit_remaining` re-scans after the fill loop (O(2N)/pass) — benign, correctness-preserving. |
| T4 Contribution | PASS | One coherent logical change: pass + wiring + telemetry + tests, no drive-by edits. (Reachability caveat — the pass is not scheduled — is raised as a judgment call under T5, not a contribution defect, since the brief marks hosting illustrative.) |
| T5 Judgment | NEEDS-HUMAN | Two calls owed. (1) **Reachability:** `backfill::reconcile` is defined and tested but is NOT invoked by `reconcile_step` (reconciliation.rs:79-106 drives gc/scrub/reconstruction/rebalance only) — so no pre-M3 record actually drains in a running deployment this slice; confirm shipping an unscheduled pass satisfies #350's remit (brief marks the `reconcile_step` hosting "ILLUSTRATIVE"). (2) **Seam metric shape:** the new durability-plane emissions (`gauge.backfill_placement_remaining`, `monotonic_counter.backfill_chunks_filled`/`_malformed_placement`/`_conflict`, backfill.rs:170,178,191,206) match the ADR-0011 idiom but their names/shape are a maintainer sign-off item per the brief's Open Questions. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Human owns whether steps 1–2 as shipped genuinely advance the ADR-0040 decision-6 removal path: the drain signal is only meaningful once the pass is scheduled and observed reaching zero in a real store (unsatisfiable at build time), and closing `Fixes #350` while step 3 (#363) and the wiring remain open is a scope/fitness call. |

### Advisory — codex

- NEEDS-HUMAN — crates/custodian/src/backfill.rs:76 exposes a metadata-mutating `pub async fn reconcile` as the pass entrypoint without threading it through the fenced custodian control point; the existing control-loop contract authorizes maintenance in `crates/custodian/src/reconciliation.rs:74` before dispatch. The brief says hosting in `reconcile_step` is illustrative, so this may be intentional for this slice, but a human should explicitly accept the unfenced direct-call API or require wiring before sign-off.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [x] T5 Judgment — Two calls owed. (1) **Reachability:** `backfill::reconcile` is defined and tested but is NOT invoked by `reconcile_step` (reconciliation.rs:79-106 drives gc/scrub/reconstruction/rebalance only) — so no pre-M3 record actually drains in a running deployment this slice; confirm shipping an unscheduled pass satisfies #350's remit (brief marks the `reconcile_step` hosting "ILLUSTRATIVE"). (2) **Seam metric shape:** the new durability-plane emissions (`gauge.backfill_placement_remaining`, `monotonic_counter.backfill_chunks_filled`/`_malformed_placement`/`_conflict`, backfill.rs:170,178,191,206) match the ADR-0011 idiom but their names/shape are a maintainer sign-off item per the brief's Open Questions.
- [x] Validation — fitness-to-purpose — Human owns whether steps 1–2 as shipped genuinely advance the ADR-0040 decision-6 removal path: the drain signal is only meaningful once the pass is scheduled and observed reaching zero in a real store (unsatisfiable at build time), and closing `Fixes #350` while step 3 (#363) and the wiring remain open is a scope/fitness call.
- [x] crates/custodian/src/backfill.rs:76 exposes a metadata-mutating `pub async fn reconcile` as the pass entrypoint without threading it through the fenced custodian control point; the existing control-loop contract authorizes maintenance in `crates/custodian/src/reconciliation.rs:74` before dispatch. The brief says hosting in `reconcile_step` is illustrative, so this may be intentional for this slice, but a human should explicitly accept the unfenced direct-call API or require wiring before sign-off.

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
- By / date: Eduard Ralph / 2026-07-02

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
- issue_350 (codex advisory): `backfill::reconcile` ships as an unfenced `pub` metadata-mutating entrypoint (siblings are `pub(crate)` via `reconcile_step`); weigh whether the fenced-control-point contract should require wiring before such a pass is public.
- issue_350 (toolchain, not code): the C4-ci gate false-failed under the provisioned sandbox because `cc`/`gcc` was shimmed to `zig cc` (`~/.local/bin/cc` → an ephemeral `/tmp/pyzig` ziglang venv), whose clang frontend rejects `cc-rs`'s `--target=x86_64-unknown-linux-gnu` while building the bench-only `criterion → alloca` C dep that `cargo xtask ci`'s `--all-targets` clippy/build pulls in; `cargo-deny`/`cargo-machete` were also absent. Re-run with a real `cc` (gcc 15.2) + the two CLIs installed, the gate passes (regenerated to overall=pass via `pdca gates 350`). Process deltas to weigh: (a) make the dev toolchain provisioning non-ephemeral / not `zig`-shimmed for the Wyrd gate host so C4-ci reflects code not environment; (b) consider whether the gate should compile bench-only native-C dev-deps via `--all-targets` at all; (c) the advisory reviewer grounded its C4 verdict on `check-gates.json` while `cargo` was unavailable to it — a gate false-negative propagated into the review, so the reviewer note should mark cargo-derived verdicts as provisional when its own sandbox lacks cargo.
