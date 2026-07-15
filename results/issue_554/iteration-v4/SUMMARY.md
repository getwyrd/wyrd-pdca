# Result — issue 554 / deployed-custodian-runs-gc

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect / goal: The deployable custodian role never runs GC. Both `reconcile_pass` calls in
- Success criterion: The deployed custodian role reclaims collectable garbage: driven
- Repo + branch target: getwyrd/wyrd @ main
- Scope (one logical fix) / out of scope: one logical change: construct a `GcContext` over the metadata store + the

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: likely-fix
- Confidence: medium
- Recommendation: (set by Do)

## 3. Correctness (Check — chain)
- C1 Spec: none — brief.md
- C2 Reproduction (red pre-fix): none — (no gate configured)
- C3 Change: none — patch.diff
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): fail — xtask: `cargo test --workspace --exclude wyrd-dst` failed with exit status: 101
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

Review of issue #554: make the deployed custodian run garbage collection after grace while preserving live data and partial-fleet evidence.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The brief gives a falsifiable deployed-loop outcome, safety boundary, fleet-completeness rule, and explicit scope at `brief.md:5`. |
| C2 Reproduction (red pre-fix) | NEEDS-HUMAN | Decide whether the recorded compile-error red is sufficient evidence — the supplied red/green runner is absent and an independent base-tree reconstruction exhausted sandbox quota, so I could not reproduce the red leg; the patched test is green at `crates/server/tests/custodian_gc.rs:415`. |
| C3 Change | FAIL | Correct the factually stale grace rationale before acceptance — it still says there is “one timescale” although CLI and gateway use 60 s and 30 s respectively, leaving an explicit carry-forward obligation unmet at `crates/server/src/cli.rs:81`. |
| C4 Verification (red→green) | NEEDS-HUMAN | Re-run the complete CI and red leg on a host with loopback and the gate scripts — focused GC (6/6), fmt, and clippy pass, but workspace tests stop at loopback `PermissionDenied` in `crates/chunkstore-grpc/tests/list_delete.rs:55`, and the asserted runner is unavailable. |
| C5 Causal adequacy | NEEDS-HUMAN | Decide whether shipping GC before the #490 lease-liveness prerequisite is acceptable — deployed GC treats logical-zero CLI leases as expired, so an in-flight shared-backend write remains exposed at `crates/server/src/cli.rs:65`; this matters because the patch activates that collector. |
| T1 Structure | PASS | The production entry threads the operator endpoint count through the backend seam to the role, preserving one authoritative fleet-completeness input at `crates/server/src/cli.rs:1039`. |
| T2 Shape | PASS | The distinct fenced GC pass preserves scrub/reconstruction fault isolation and runs only after repair, at `crates/server/src/custodian.rs:569`. |
| T3 Runtime | PASS | Independently run `custodian_gc` passed all six scenarios, including exact-boundary reclaim and both runtime- and startup-partial deferral, grounded at `crates/server/tests/custodian_gc.rs:415`. |
| T4 Contribution | NEEDS-HUMAN | Decide whether contribution overlap is clear — affected-path history finds #551/#461/#450, but closed/rejected work could not be mechanically queried here, so uniqueness beyond merged local history remains unsettled (`crates/server/src/custodian.rs:448`). |
| T5 Judgment | NEEDS-HUMAN | Approve the 60 s unproven grace floor and whole-fleet outage policy — the value is a measurement call and one absent/decommissioned endpoint pauses all reclamation indefinitely at `crates/server/src/custodian.rs:90` and `crates/server/src/custodian.rs:421`. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether the in-process trait-store evidence is representative enough for deployed operation — it exercises production wiring but not a live backend/topology, which determines confidence in operational fitness (`crates/server/tests/custodian_gc.rs:13`). |

### Advisory — adversary

# check-advisory-adversary.md — issue 554 / deployed-custodian-runs-gc (iteration 4)

Adversarial pass. I attempted to refute the red→green evidence, the gate results, and the
fix itself, re-running everything at `$PDCA_TARGET`. Verdicts below; experiments were run on
throwaway copies, never the target.

## Refutations / findings

- NEEDS-HUMAN — **The gating C4 red is, on this host, an environment fault — but it is the
  third consecutive cycle the gate is red without naming a failing test.** I re-ran the exact
  gate command `cargo test --workspace --exclude wyrd-dst` at `$PDCA_TARGET`: **exit 0, 129
  test binaries green**, including the new `crates/server/tests/custodian_gc.rs` (6/6). The
  only failure I could produce was
  `crates/server/tests/cli_roundtrip.rs:43` (`put_then_get_round_trips_across_separate_invocations`)
  panicking with `wyrd: Disk quota exceeded (os error 122)` when `tempfile::tempdir()` lands
  on this sandbox's quota-limited `/tmp`; with `TMPDIR` on a writable filesystem it passes.
  That is the same environment-fault class as iteration 2's `list_delete_over_grpc`
  `PermissionDenied` (loopback). Per issue #236 this is **not** scored as a refutation of the
  fix — but the deterministic gate still blocks, `check-gates.json` C4's `path_line` again
  reports only "exit status: 101" without naming the failing test (the diagnostic iteration 3
  explicitly demanded), and I cannot inspect the gate host. A human must either fix the gate
  host (disk quota / tmpdir / loopback) and re-run C4, or adjudicate the discrepancy; my
  candidate culprit is `cli_roundtrip` under a tempdir-write restriction. Verdict on C4 is
  provisional (toolchain/environment unavailable to me).

- NEEDS-HUMAN [impl] — **Duplicate `--endpoints` now reaches a deleting GC sweep with no
  validation — the exact live-data-loss case the restore path refuses.** The uniqueness
  checks live only inside the `--reconcile-after-restore` block
  (`crates/server/src/cli.rs:940-967`); the run-loop path (`cli.rs:1040-1073`) performs none,
  and `require_aligned_topology` checks lengths only. The restore-path comment itself names
  the hazard: a box listed twice under two ids "answers to both … a later GC sweep through
  the duplicate reaches them as B and deletes them" (`cli.rs:929-939`). Before this patch
  that "later GC sweep" never ran in deployment; this patch is what makes it run every
  interval. Concrete failing case: operator fat-fingers `--endpoints A,A --ids 1,2
  --failure-domains x,y`; the whole-fleet gate passes (`fleet.len() == operator_fleet_size`
  counts the duplicate on both sides, `crates/server/src/custodian.rs:582`); a repair
  displaces fragment `f` from id 1 to id 2 — same physical box, same `FragmentId` key — and
  commits an orphan record for `(1, f)` (`crates/custodian/src/reconstruction.rs:580-585`);
  after grace, GC sweeps fleet entry `(1, boxA)`: `referenced.protects(1, f)` is false (the
  placement now references `(2, f)`), the orphan evidence exists, so `delete_fragment(f)`
  removes the one physical copy the **live committed placement** points at
  (`crates/custodian/src/gc.rs:124`, `gc.rs:134-136`, `gc.rs:151-153`) — silent loss of live
  data. Fix is mechanical: hoist the two uniqueness refusals (`cli.rs:940-967`) so they also
  guard the run-loop path, plus a test. (Delete-orphan and expired-pending inputs are safe
  under duplicates — evidence-gated; the repair-displacement route is the one that kills.)

## Attempted refutations that failed (the strong signal)

- **"The red is only a compile error, so the green may pass for the wrong reason" — refuted
  by experiment.** The mechanical C4-verify red leg on a reverted base is an E0061 compile
  error (the `operator_fleet_size` parameter), disclosed in the test module doc
  (`crates/server/tests/custodian_gc.rs:421-427`) exactly as the brief's Test-file note
  requires. I established assertion-level binding independently on a scratch copy: (a) with
  the GC block's gate forced to `false` (GC never runs — base behaviour, signature kept),
  **5 of 6 tests fail by assertion** (`custodian_gc.rs:837`, `:964`, `:1089` among them);
  (b) with only the gate reverted to iteration-2's `unreachable.is_empty()`, the
  startup-partial test fails by assertion at `custodian_gc.rs:791`. The tests bind to the
  production path (`run_reconstruction_until` → `reconcile_pass` → `gc::reconcile`, real
  write path via `write_new_object_placed`, physical byte checks on the stores) — no
  parallel re-implementation, no tautology.
- **Whole-fleet gate arithmetic:** could `fleet.len() == operator_fleet_size` pass on a
  partial fleet? No — `live_reconstruction_view` only ever drops entries from `configured`
  (`crates/server/src/custodian.rs:233-256`) and `connect_fleet` never produces more entries
  than `endpoints` (`custodian.rs:188-204`), so equality holds iff every operator endpoint is
  connected and reachable; `cmd_custodian` passes `endpoints.len()` (`cli.rs:1046-1051`).
  (Duplicate endpoints defeat the *intent* — see the [impl] finding — but not the arithmetic.)
- **Chunk-wide `pending:` retirement:** retired only inside a whole-fleet pass, and a
  mid-sweep store fault propagates before the cleanup batch commits (`gc.rs:151-170`), so
  partial sweeps preserve evidence; deletes are idempotent on retry.
- **Boundary conformance:** the inclusive `now == orphaned_at + grace` reclaim (`gc.rs:136`
  `>=`) is now pinned (`custodian_gc.rs:1052-1096`), closing the iteration-2 ±1 ms nit.
- **Doc-claim check:** the two-timescale grace derivation is now stated correctly — gateway
  30 s at `crates/server/src/lib.rs:49`, CLI 60 s at `cli.rs:23`, floor = the longer
  (`custodian.rs:114-138`) — the iteration-1 adversary-3 misstatement is fixed.

## Pre-declared human items (not new findings — keep visible)

- The deployed 60 s grace VALUE (floor, not a proven reader-safety bound,
  `custodian.rs:118-123`), the pause-under-outage trade-off (any absent configured server
  pauses ALL reclamation, `custodian.rs:177-183`), and the accepted document-and-ship
  lease-liveness hazard (`cli.rs:9-20` comment) remain maintainer calls per the iteration-2
  sign-off. I cannot verify from this bundle that the required **#490 sequencing note**
  ("do not run `wyrd custodian` against a shared write-taking backend before #490 merges")
  made it into the PR description — build-notes is withheld from this lens; confirm at
  sign-off.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C2 Reproduction (red pre-fix) — Decide whether the recorded compile-error red is sufficient evidence — the supplied red/green runner is absent and an independent base-tree reconstruction exhausted sandbox quota, so I could not reproduce the red leg; the patched test is green at `crates/server/tests/custodian_gc.rs:415`.
- [ ] C4 Verification (red→green) — Re-run the complete CI and red leg on a host with loopback and the gate scripts — focused GC (6/6), fmt, and clippy pass, but workspace tests stop at loopback `PermissionDenied` in `crates/chunkstore-grpc/tests/list_delete.rs:55`, and the asserted runner is unavailable.
- [ ] C5 Causal adequacy — Decide whether shipping GC before the #490 lease-liveness prerequisite is acceptable — deployed GC treats logical-zero CLI leases as expired, so an in-flight shared-backend write remains exposed at `crates/server/src/cli.rs:65`; this matters because the patch activates that collector.
- [ ] T4 Contribution — Decide whether contribution overlap is clear — affected-path history finds #551/#461/#450, but closed/rejected work could not be mechanically queried here, so uniqueness beyond merged local history remains unsettled (`crates/server/src/custodian.rs:448`).
- [ ] T5 Judgment — Approve the 60 s unproven grace floor and whole-fleet outage policy — the value is a measurement call and one absent/decommissioned endpoint pauses all reclamation indefinitely at `crates/server/src/custodian.rs:90` and `crates/server/src/custodian.rs:421`.
- [ ] Validation — fitness-to-purpose — Decide whether the in-process trait-store evidence is representative enough for deployed operation — it exercises production wiring but not a live backend/topology, which determines confidence in operational fitness (`crates/server/tests/custodian_gc.rs:13`).
- [ ] **The gating C4 red is, on this host, an environment fault — but it is the
- [ ] **Duplicate `--endpoints` now reaches a deleting GC sweep with no
- [ ] C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance) FAILED (gating) — xtask: `cargo test --workspace --exclude wyrd-dst` failed with exit status: 101
- [ ] external dependency: disk-quota-headroom — the C4 gate (`cargo test --workspace

## 7. Proven / not proven
- Proven by which oracle: gates overall = fail (stub oracles).
- Unproven / needs manual run: anything flagged in §6.

## 8. Ready-to-ship attachments
- patch.diff
- tracker-comment.md     (ALWAYS, every tracker item)
- build-notes.md         (builder rationale — for the human, not the reviewer)

## 9. Check sign-off                     ← human completes Check here
- Disposition confirmed / overridden:
- Outcome: iterated-to-Do
- Iteration delta (if iterating): Rejected on two confirmed findings, not on the approach — the GC wiring, the operator_fleet_size whole-fleet gate, and the red→green evidence all stand; do not rework them. 1. Duplicate `--endpoints` reaches the deleting GC sweep unvalidated (adversary [impl], confirmed against the patch): the uniqueness refusals exist only in the `--reconcile-after-restore` block (cli.rs:940-967); the run-loop path this patch activates (cli.rs:1040-1073) performs none, and the whole-fleet gate counts the duplicate on both sides. Fix mechanically: hoist the two existing refusals so they also guard the run-loop path, and add a test that drives the loop with duplicated `--endpoints`/`--ids` and asserts refusal (the live-data-loss route is repair displacement between two ids naming the same box). 2. Stale grace rationale (reviewer C3 FAIL, an iteration-1 carry-forward obligation still unmet): the RESTORE_GRACE_WINDOW_MILLIS doc-comment (cli.rs:70-83) still claims "the one timescale the system already trusts" (there are two: 60 s CLI, 30 s gateway) and still defers the derivation to "#554's job" — false once this patch lands. Update that comment to match the correct two-timescale derivation the patch already added in custodian.rs. Carry the C4 environment evidence forward so it is not re-litigated: the gating C4-ci exit-101 is a gate-host environment fault (`/tmp` disk quota, os error 122), independently reproduced by the adversary at the target (full gate command exit 0, 129 binaries green) and by the sign-off host on issue 430's cycle. The pre-resolved maintainer calls (lease-liveness document-and-ship pending #490, 60 s grace floor, pause-under-outage) remain accepted — keep them pre-declared, do not rework.
- By / date: Eduard Ralph / 2026-07-15

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
- Gate host: C4-ci went red three consecutive cycles without naming the failing test; root cause confirmed as gate-host environment (`/tmp` disk quota, os error 122 — reproduced independently at the target and on the sign-off host during issue_430's cycle). Fix the gate host's TMPDIR/quota and make the C4 gate report the failing test name, not just "exit status: 101".
