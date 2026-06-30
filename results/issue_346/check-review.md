# Check review — issue 346 / rebalance-evac-identity-placement-fallback

Advisory, artifact-only, decorrelated from the builder (build-notes.md withheld).
Grounded read-only on the target base at `$PDCA_TARGET`
(`/home/eddie/wyrd/wyrd.pdca-wt`). The target base matches the patch's `-` context
exactly (rebalance.rs:89/151-159/163-168/175 unchanged) — **base is not stale**, the
patch grounds cleanly, no target-state caveat applies. Git history is **not**
mechanically re-runnable in this sandbox (gated), which constrains the prior-art row.

## Verdict table (5/5/1)

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | Success criterion is concrete and testable: a plan IS produced for a `placement: vec![]` chunk whose identity-resolved D-server drains, and the committed record is full-length (`== fragment_count()`) repointed off the drainer (brief.md:22-31). Decision turns on data-loss-on-decommission; the spec binds both failure modes (skip + panic/short-commit), not just the filter. No ambiguity to resolve. |
| C2 Reproduction (red pre-fix) | PASS | Re-derived red from the base: `plan_evacuations` iterates raw `chunk.placement` (rebalance.rs:151-157); for a pre-M3 record `placement` is empty (`#[serde(default)]`, metadata.rs:93) → `evac` empty → `continue` skip (rebalance.rs:158) → `Reconciled::Satisfied`, so the new `assert_eq!(outcome, Reconciled::Changed)` fails red. Corroborated by the non-gating C4-verify gate (result=pass) in check-gates.json. |
| C3 Change | PASS | One logical change: materialize the full `0..fragment_count()` index space through the authoritative `placed_dserver` resolver and reuse it for `evac`, `survivor_domains`, and the cloned/committed placement (patch.diff rebalance.rs:+153-165, +190). In-scope per brief.md:55-64 (incl. `survivor_domains`); uses existing API, adds no `fragments()` helper as instructed. |
| C4 Verification (red→green) | PASS | Gating `C4-ci` (cargo xtask ci: fmt/clippy/build/test/deny/conformance) = pass and non-gating `C4-verify` red→green = pass (check-gates.json). Base==patch-context so the gate ran against the right tree. Types align: `fragment_count()->u16` feeds `placed_dserver(u16)->DServerId` (metadata.rs:103-124). No stale/unreadable-target blocker to fabricate. |
| C5 Causal adequacy | PASS | Root cause = rebalance was the one placement-consumer omitted from the single authoritative resolver (metadata.rs:110-124 caller list). Fix **removes** the cause — eager full materialization via `placed_dserver` — it is **not** a capability probe or runtime guard around a present capability, so the symptom-guard smell-test does **not** fire (the only `.get().unwrap_or()` lives in the pre-existing resolver, not added here). Both downstream hazards (empty→skip, short→panic/corrupt-commit at rebalance.rs:221-253) are closed by the full-length invariant, not papered over. |
| T1 Structure | PASS | Fix in the owning module (`custodian/src/rebalance.rs`), test in the brief's designated file (`custodian/tests/rebalance.rs`); reuses existing base helpers (`Fleet`, `read_inode`, `four_domains`, `elect`, `frag`, `write_rs_2_1`) rather than re-introducing them. |
| T2 Shape | PASS | Coherent diff: doc comment on `EvacPlan.placement` updated to state the full-length-via-fallback invariant (patch.diff:+9-15), one resolution computed once and reused; no dead code, no leftover raw-vector reads. clippy/fmt green via C4-ci. |
| T3 Runtime | PASS | Two real `#[tokio::test]`s exercise both brief legs — `EcScheme::None` (single fragment, index 0) and `ReedSolomon{2,1}` (draining fragment at index 1 > 0) — asserting `Reconciled::Changed`, exact full-length repointed placement, preserved n-distinct-domain spread, orphan-for-GC, and post-move readability. Executed green under C4-ci. |
| T4 Contribution | PASS | Tests assert the binding criteria (plan produced + full-length repointed record), not tautologies; they fail on both the no-fix and the filter-only half-fix (an empty raw vector yields no survivor domains / panics on clone-index). Genuine regression coverage for a data-loss defect. |
| T5 Judgment | NEEDS-HUMAN | Patch judgment itself is sound and in-scope (adopts the shared resolver, no scope creep into #356 fan-out or #287 GC). **Decision owed:** confirm the prior-art / non-duplication check by affected file path — I cannot re-run `git log`/`git branch -a` here (gated), so a human must verify no closed/rejected rebalance-fallback work exists and that the #356 (fanout.rs) / #287 (GC) siblings remain disjoint, per brief.md:75-81. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | **Decision owed at sign-off:** is the chosen evacuation behavior operationally fit — repointing a pre-M3 fragment onto the single free distinct domain, committing a full-length record, and orphaning the old fragment for GC — given concurrent readers (atomic CAS flip) and the GC grace window? Mechanical gates confirm correctness-of-mechanism, not production fitness; only a human owns "does this do the right thing for a real decommission." |

## Notes
- No FAIL findings: every cited defect/claim grounds on the target base, and the fix
  matches the brief's invariant (one placement closure across read/GC/scrub/recon/rebalance).
- C5 explicitly cleared the symptom-guard smell-test: the added code transforms the
  cause; it does not guard a present capability.
