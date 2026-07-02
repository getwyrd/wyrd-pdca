# Check review — issue 330 / scrub-detect-missing-placed-fragment

**Task under review:** On `getwyrd/wyrd@main`, no production maintenance path turns a
*committed-referenced but simply-absent* fragment into a repair obligation — scrub only
walks whatever `list_fragments()` returns, so a fragment missing from its placed D server
is never even visited and its `Ok(None)` fetch arm just `continue`s. The fix must make a
production scrub pass enqueue such a chunk on the shared repair queue (red→green), without
false positives for orphan/unreferenced fragments or in-flight / pending-GC fragments.

**Grounding note:** `$PDCA_TARGET` is not readable in this sandbox (env inspection and
cross-checkout git/cargo are blocked; three checkouts exist on the machine — I did not
wander into them). Per the fallback rule I ground the citations below on `patch.diff`
itself. The prior iteration's C4 gate failures (`check-gates.json`: `C4-ci` = "cargo: not
found", `C4-verify` = "RED with fix") are **toolchain/target-state caveats, not patch
defects** — cargo/rustc were absent in the Do sandbox, so the suite never compiled. I do
**not** treat that as a blocking C4 FAIL; it is re-run work owed to the human (below).

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | Brief gives a load-bearing success criterion (missing placed fragment → shared repair-queue obligation), explicit false-positive guardrails, and a documented prior-art check by affected path (`scrub.rs`/`gc.rs`/`read.rs`; #347/#361, #287 merged, no open PR). Spec is unambiguous and scoped. |
| C2 Reproduction (red pre-fix) | PASS | `tests/scrub.rs` `detects_a_missing_placed_fragment_…` (patch.diff:174-207) commits a reference on d0 that holds no bytes and asserts `Changed` + `queued_repairs == [chunk]`; reverting the new arm to `Ok(None) => continue` (patch.diff:113-117) yields `Satisfied`/empty → test fails. Static re-derivation only — not executed (toolchain absent). |
| C3 Change | PASS | `scrub::reconcile` now drives the pass off the committed reference set grouped by placed D server (`by_dserver`, patch.diff:75-90) and fetches each placed fragment by id; the `Ok(None)` arm enqueues via `repair::enqueue_repair(ctx.meta, frag.chunk, "scrub")` (patch.diff:113-116) instead of `continue`. Change is present and lands on the cited site. |
| C4 Verification (red→green) | NEEDS-HUMAN | Gate `C4-ci` failed as "cargo: not found" and `C4-verify` as "RED with fix" — both are **toolchain-absence artifacts** in the Do sandbox, not observed patch defects. I could not execute here either. Decision owed: re-run `cargo xtask ci` + `run-verify.sh` (or `cargo test -p custodian --test scrub`) in a Rust-toolchained checkout of the target and confirm the two new tests are red pre-fix / green post-fix. |
| C5 Causal adequacy | PASS | Root cause (scrub blind to absence because it iterated `list_fragments()`, which cannot enumerate an absent id) is *removed*, not guarded: the pass is re-driven off `referenced_fragments` so `get_fragment` is asked for exactly the placed ids (patch.diff:66-90). Not a capability-probe/runtime-guard (symptom-guard smell-test does not fire). Residual: the read-path missing gap (`read.rs`, brief:15-17) is left to follow-up, consistent with the brief's "restores it for the missing-fragment case." |
| T1 Structure | PASS | Touches only the intended seam: `custodian/src/scrub.rs` (detector), `custodian/tests/scrub.rs` (2 tests), `traits/src/lib.rs` (doc). Does **not** edit `gc.rs:referenced_fragments`, shrinking the #348 conflict surface (brief:44-48). |
| T2 Shape | PASS | Idiomatic Rust: `HashMap<DServerId, Vec<FragmentId>>` grouping, `let Some(frags) = … else { continue }`, awaited fetches; new `emit_missing` mirrors the existing `emit_corruption` audit shape (patch.diff:133-152). |
| T3 Runtime | PASS | Static inspection only — no panics/unwraps on the hot path, `get_fragment`/`enqueue_repair` awaited, `?`-propagated. Behavioral equivalence for *present* referenced fragments preserved (present∩referenced == the old listing-gated set), so corruption detection is not regressed. Actual execution pends the toolchain (folds into C4). |
| T4 Contribution | PASS | Adds the flippable missing-fragment leg plus the in-flight/pending-write guardrail test (patch.diff:211-260). Note: no *dedicated new* test for the orphan/unreferenced guardrail — it is structurally guaranteed by the reference-set drive and relies on the pre-existing suite; worth a human eye that existing orphan coverage still holds. |
| T5 Judgment | PASS | Sound approach: inverting scrub from listing-driven to reference-set-driven is the correct, more-robust fix and correctly leaves the killed/partitioned-server case (dserver absent from `ctx.fleet` → skipped) out of scope per brief:55-58. Human may wish to confirm on a *verifying* backend that reference-set drive preserves corruption findings, since this is a durability-critical loop. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decision owed: confirm the change actually restores the durability invariant end-to-end — a placed-but-absent committed fragment becomes the *same* durable obligation corruption produces, with no false positives for orphan/pending-GC/in-flight fragments — and that it is fit to ship on the durability-maintenance seam (scheduled in a different wave from #348). Fitness-to-purpose is human-owned by design. |
