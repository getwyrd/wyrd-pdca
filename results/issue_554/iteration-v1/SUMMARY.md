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

Review of issue #554: make the deployed custodian run garbage collection without reclaiming live or within-grace fragments.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The brief gives a falsifiable deployed-loop outcome and explicitly requires skipped-server evidence preservation, which is the safety context for partial-fleet GC (`crates/server/src/custodian.rs:510`). |
| C2 Reproduction (red pre-fix) | PASS | In an archived `HEAD` with only the new test added, `deployed_role_reclaims_orphaned_bytes_after_grace_elapses` failed at `crates/server/tests/custodian_gc.rs:378` because the orphaned fragment remained. |
| C3 Change | FAIL | A partial reachable-fleet pass may delete an expired pending ledger entry after seeing a copy on one reachable server, so a skipped server can later return with unreclaimable bytes: the patch supplies only the live fleet at `crates/server/src/custodian.rs:518`, while GC records any swept copy at `crates/custodian/src/gc.rs:155` and retires the chunk-wide pending evidence at `crates/custodian/src/gc.rs:165`. |
| C4 Verification (red→green) | NEEDS-HUMAN | Accept the focused red→green as reproduced (base assertion failure; patched 2/2 pass), but decide whether the full CI result needs rerun on a host permitting loopback binds — `cargo xtask ci` here stopped on sandbox `PermissionDenied`, not a patch failure, in `crates/chunkstore-grpc/tests/list_delete.rs:55`. |
| C5 Causal adequacy | FAIL | The deployed wiring fixes orphan collection but is not causally adequate for expired pending garbage during partial reachability because retiring its sole chunk-wide evidence can strand skipped copies (`crates/custodian/src/gc.rs:142`, `crates/custodian/src/gc.rs:165`). |
| T1 Structure | PASS | A distinct fenced GC pass preserves fault isolation and runs sequentially after reconstruction, the relevant control-flow boundary at `crates/server/src/custodian.rs:523`. |
| T2 Shape | PASS | The production entry signature remains unchanged and the context uses the existing `GcContext` seam at `crates/server/src/custodian.rs:518`. |
| T3 Runtime | FAIL | The test must cover an expired pending chunk with one server unreachable during GC and reachable later, because the current all-reachable fixture at `crates/server/tests/custodian_gc.rs:363` cannot expose permanent skipped-copy leakage. |
| T4 Contribution | NEEDS-HUMAN | Decide whether repository-local history is sufficient prior-art clearance — affected-path `git log --all` showed #551/#461/#450 and no competing #554 change, but closed/rejected remote work was not mechanically available, which matters for duplicate or superseded work. |
| T5 Judgment | NEEDS-HUMAN | Maintainers must choose the deployed grace value because the lease TTL is explicitly only a floor, not a proven reader-safety bound; an incorrect value can tear in-flight readers (`crates/server/src/custodian.rs:90`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether the shipped behavior is fit for real partial-fleet operation after the expired-pending evidence-loss defect is resolved, since the in-process all-reachable test proves orphan timing and live-data preservation but not outage recovery (`crates/server/tests/custodian_gc.rs:371`). |

### Advisory — adversary

# check-advisory-adversary.md — issue 554 / deployed-custodian-runs-gc

## Evidence attack (re-run at $PDCA_TARGET)

- Green leg re-verified: `cargo test -p wyrd-server --test custodian_gc` → 2/2 pass in 0.11s at
  the target. Red-leg mechanism verified structurally read-only: `origin/main`'s
  `run_reconstruction_until` signature is byte-identical to the patched one
  (`crates/server/src/custodian.rs:411-424` vs `origin/main:crates/server/src/custodian.rs:390`),
  and the test references no symbol the revert removes (`GRACE_MILLIS` is its own constant,
  `crates/server/tests/custodian_gc.rs:404`), so the reverted base compiles and fails by
  assertion — the honest red the brief demanded. The test drives the real production entry
  (`run_reconstruction_until`, custodian.rs:411), not a re-implementation; on the fixed tree only
  the GC pass can delete the doomed fragments (the object is unlinked, so scrub/reconstruction
  have no obligations over it) — no wrong-reason green found. **Attempted to refute the
  red→green proof and the production-path claim; could not.**

## Refutations of the fix

- NEEDS-HUMAN — **This patch activates a reclaim of in-flight CLI writes on the shared
  production backends: the CLI's pending lease is born expired in the collector's clock
  domain.** `cmd_put` stamps leases with logical time zero (`NOW_MILLIS = 0`,
  `crates/server/src/cli.rs:67`, used at cli.rs:493-494 and `cluster_store_put`
  cli.rs:1555-1556), so every CLI put's pending entry carries `lease_expiry_millis = 60_000`.
  The deployed custodian's clock is wall time (`clock = wall_clock_millis`, cli.rs:839,
  cli.rs:1202), so GC's expired-lease input matches it instantly
  (`entry.lease_expiry_millis <= now_millis`, `crates/custodian/src/gc.rs:254`). Concrete
  failing case: `wyrd put --metadata-backend fdb <big-file>` racing a running
  `wyrd custodian --metadata-backend fdb` over the same store (nothing prevents this on
  tikv/fdb — the redb exclusive lock argument at cli.rs:800-802 does not apply, and the
  custodian doc *insists* it open the same store the writers use, cli.rs:699-704): a GC pass
  landing between the put's phase 2 and phase 3 deletes the just-written fragments *and* the
  pending entry (gc.rs:142-157, 165-166); the put's phase-3 commit checks no lease liveness
  (`crates/core/src/write.rs:245-287`) and commits an object whose bytes are all gone — silent
  data loss, created by this wiring, impossible before it because deployed GC never ran. The
  rationale comment "The CLI runs no custodian sweep, so lease expiry is moot"
  (cli.rs:65-66) is falsified by this very patch, which edits the adjacent line (cli.rs:68)
  without reconciling it. Whether the fix is stamping wall clock in the CLI put (touches an
  out-of-scope producer, breaks the stated reproducibility choice), a commit-time lease check,
  or an operator constraint, is a scope/architecture call — not routing to Do unilaterally.

- NEEDS-HUMAN — **The expired-pending-lease input has NO grace window beyond the TTL itself,
  and the gateway's writer never renews — a slow S3 PUT > 30 s is now collectable mid-flight.**
  The production gateway stamps `lease_expiry = now + 30_000`
  (`crates/server/src/lib.rs:49,162`) with no renewal loop; GC reclaims an unreferenced
  fragment the moment the lease expires ("the lease TTL is its grace", gc.rs:142-144), and
  commit does not re-check the lease (write.rs:245-287). With the custodian's default 30 s
  interval (cli.rs:684) a streaming multi-GB PUT whose fan-out outlives 30 s can have its
  fragments reclaimed and still commit. Pre-existing library semantics, but this diff is what
  makes the collector actually run against live gateways — the exact "deployed reality" class
  the brief targets. Fitness/architecture decision (lease renewal, TTL sizing, or commit-time
  lease check), pre-declared adjacent to the grace-value sign-off item.

- NEEDS-HUMAN — **The derivation claim behind the deployed grace value is overstated: there
  are TWO trusted lease timescales, and the patch silently picked the larger.**
  `GC_GRACE_WINDOW_MILLIS = crate::cli::LEASE_TTL_MILLIS` (60 s, custodian.rs:101) is
  documented as "the one timescale the system already trusts" (custodian.rs:94-96), but the
  production gateway's pending-lease TTL is 30 s (`DEFAULT_LEASE_TTL_MILLIS`, lib.rs:49).
  Picking 60 s is conservative in the safe direction for orphans, but the doc-comment's
  justification is factually wrong as stated, and the brief pre-declares the deployed VALUE as
  the maintainer's call (proposal 0005:585-586). Surface at sign-off regardless of what
  build-notes says (not visible to this reviewer).

- NEEDS-HUMAN [impl] — **Neither of the brief's two secondary GC obligations is pinned by the
  new deployed-role test.** (a) The expired-pending-lease input (brief: "expired pending
  leases are likewise GC's input") is never exercised through `run_reconstruction_until` —
  both tests reclaim only delete-orphans (custodian_gc.rs:467-521, 526-553). (b) The FLEET-VIEW
  requirement — "the pass MUST preserve orphan records for servers it skipped" — holds by
  construction in the library (`cleanup` only deletes keys for fragments actually reclaimed,
  gc.rs:151-158), but no test drives the deployed loop with an unreachable server and asserts
  its orphan record survives for a later pass. Both are additive test cases the builder can
  write against the existing harness (drop one `ConfiguredDServer`, or leave a pending entry
  unexpired/expired); a regression in either currently passes green.

## Attempted and could not refute

- GC-races-reconstruction claim (custodian.rs:506-513): passes are sequential, GC recomputes
  `referenced_fragments` after reconstruction's commit, and a just-orphaned displaced fragment
  carries a fresh `orphaned_at` inside the grace window — could not construct a reclaim of a
  re-placed or displaced-but-in-grace fragment.
- Fault-isolation claim: GC runs as a third distinct fenced pass ordered last
  (custodian.rs:523-537); a `ReconcileError::Store` in GC cannot suppress scrub or
  reconstruction (they already ran), and `Fenced` still stops the loop — matches the #461 rule.
- Test-constant drift (`GRACE_MILLIS` duplicating the pub(crate) TTL, custodian_gc.rs:404):
  drift in either direction breaks a test loudly, not silently — no refutation.
- Grace boundary: gc.rs:136 reclaims at exactly `orphaned_at + grace`; the tests pin ±1 ms
  around the deadline, which adequately pins the no-reclaim-before / reclaim-after mechanism
  the brief scoped.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C4 Verification (red→green) — Accept the focused red→green as reproduced (base assertion failure; patched 2/2 pass), but decide whether the full CI result needs rerun on a host permitting loopback binds — `cargo xtask ci` here stopped on sandbox `PermissionDenied`, not a patch failure, in `crates/chunkstore-grpc/tests/list_delete.rs:55`.
- [ ] T4 Contribution — Decide whether repository-local history is sufficient prior-art clearance — affected-path `git log --all` showed #551/#461/#450 and no competing #554 change, but closed/rejected remote work was not mechanically available, which matters for duplicate or superseded work.
- [ ] T5 Judgment — Maintainers must choose the deployed grace value because the lease TTL is explicitly only a floor, not a proven reader-safety bound; an incorrect value can tear in-flight readers (`crates/server/src/custodian.rs:90`).
- [ ] Validation — fitness-to-purpose — Decide whether the shipped behavior is fit for real partial-fleet operation after the expired-pending evidence-loss defect is resolved, since the in-process all-reachable test proves orphan timing and live-data preservation but not outage recovery (`crates/server/tests/custodian_gc.rs:371`).
- [ ] **This patch activates a reclaim of in-flight CLI writes on the shared
- [ ] **The expired-pending-lease input has NO grace window beyond the TTL itself,
- [ ] **The derivation claim behind the deployed grace value is overstated: there
- [ ] **Neither of the brief's two secondary GC obligations is pinned by the

## 7. Proven / not proven
- Proven by which oracle: gates overall = pass (stub oracles).
- Unproven / needs manual run: anything flagged in §6.

## 8. Ready-to-ship attachments
- patch.diff
- tracker-comment.md     (ALWAYS, every tracker item)
- build-notes.md         (builder rationale — for the human, not the reviewer)

## 9. Check sign-off                     ← human completes Check here
- Disposition confirmed / overridden:
- Outcome: iterated-to-Do
- Iteration delta (if iterating): Rejected pending correction of the reviewer FAILs and the adversary refutations; the wiring shape and the red→green proof stand. - Fleet-view defect (C3/C5/T3): a partial reachable-fleet pass must never retire chunk-wide expired-pending evidence when servers were skipped — preserve the evidence so a skipped server's copy stays reclaimable on a later pass; add the T3 test (one server unreachable during GC, reachable later, its orphan/pending record survives). - Adversary 1 (CLI clock domain): the deployed collector must not reclaim in-flight CLI writes whose leases are stamped at logical time 0 and read as born-expired against the custodian's wall clock on a shared backend; also reconcile the now-falsified cli.rs:65-66 rationale comment. - Adversary 2: the expired-pending-lease input must get a grace window beyond the bare TTL so a slow (>30 s) gateway PUT is not collectable mid-flight the moment its unrenewed lease expires. - Adversary 3: fix the grace-window doc-comment — there are two trusted lease timescales (60 s CLI, 30 s gateway); keeping 60 s is fine but the "one timescale the system already trusts" justification is factually wrong as stated. - Adversary 4: pin both secondary obligations with tests — expired-pending reclaim exercised through run_reconstruction_until, and skipped-server evidence preservation — so a regression in either goes red. Note: if closing adversary 1/2 turns out to require a commit-time lease-liveness check (an architecture change the brief puts out of scope), stop and route back — that decision overlaps the #490 re-plan.
- By / date: Eduard Ralph / 2026-07-15

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
- Auto-iterate rule change (issue_554 evidence): auto-iterate should keep rebuilding until NO implementation-only defects remain — judgment/pre-declared NEEDS-HUMAN items (here the brief's T5 grace-value item, C5 FAIL) must not veto unattended rebuilds while impl-only findings exist (cf. the Validation carve-out, #293); 554 halted at attempt 1 with impl-only FAILs (C3/T3, adversary [impl]) still on the table.
