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

Review of issue #554: make the deployed custodian run garbage collection without reclaiming live or grace-protected writes.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The decision boundary is explicit: deployed GC must reclaim expired garbage while preserving live/in-grace data and skipped-fleet evidence, with live-writer lease safety carried forward as mandatory rather than optional (`brief.md`, Iteration 1). |
| C2 Reproduction (red pre-fix) | PASS | In a disposable copy of `$PDCA_TARGET`, reversing only the two production-source hunks while retaining the test produced 3 assertion failures (orphan reclaim, pending reclaim, later reclaim after outage) and 1 safety pass at `crates/server/tests/custodian_gc.rs:379`. |
| C3 Change | FAIL | Shipping GC while explicitly leaving the CLI clock-domain hazard open permits collection of a still-in-flight write, and the stale “one timescale” rationale remains; this violates the carried-forward correction required before deployment (`crates/server/src/cli.rs:68`, `crates/server/src/cli.rs:81`). |
| C4 Verification (red→green) | NEEDS-HUMAN | Accept the independently reproduced focused red→green (base: 3 failed/1 passed; patch: 4 passed), but decide whether CI must be rerun on a host allowing loopback sockets—the full `cargo xtask ci` rerun stopped only at `list_delete_over_grpc` with `PermissionDenied`, so the asserted complete green gate is provisional (`crates/server/tests/custodian_gc.rs:552`). |
| C5 Causal adequacy | FAIL | The deployed collector is activated without resolving the known lease-liveness cause: expired-pending input has no post-expiry grace and can delete a slow gateway or logical-time CLI write immediately after expiry (`crates/server/src/custodian.rs:106`, `crates/server/tests/custodian_gc.rs:548`). |
| T1 Structure | PASS | A separately fenced, last-in-interval GC pass preserves fault isolation and avoids racing reconstruction; full-fleet deferral retains evidence when reachability is partial (`crates/server/src/custodian.rs:513`, `crates/server/src/custodian.rs:526`). |
| T2 Shape | PASS | The production entry signature is unchanged and the focused base leg fails by behavioral assertions rather than compilation, preserving the required deployed-role test shape (`crates/server/src/custodian.rs:539`, `crates/server/tests/custodian_gc.rs:550`). |
| T3 Runtime | FAIL | Runtime coverage proves orphan/grace/fleet behavior, but the pending test models only an abandoned lease and positively pins reclaim at expiry+1; it does not protect a live writer and demonstrates the unsafe boundary (`crates/server/tests/custodian_gc.rs:495`, `crates/server/tests/custodian_gc.rs:550`). |
| T4 Contribution | NEEDS-HUMAN | Decide whether contribution overlap is clear enough to proceed—affected-path history confirms merged #551/#461/#450 prior art, but this sandbox cannot mechanically establish the brief’s claim that no closed/rejected competing work exists (`crates/server/src/custodian.rs:505`). |
| T5 Judgment | NEEDS-HUMAN | Maintainers must choose a deployed orphan grace value and its reader-safety basis—the selected 60 s value is documented only as a floor, not a proven bound, so data-loss risk depends on an unresolved operational limit (`crates/server/src/custodian.rs:90`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether deployment is fit despite known in-flight-write reclamation risk; exercise a CLI PUT sharing metadata with a wall-clock custodian and a gateway PUT lasting beyond 30 s, and require both to commit with all fragments intact before sign-off (`crates/server/src/cli.rs:68`, `crates/server/src/custodian.rs:106`). |

### Advisory — adversary

# check-advisory-adversary.md — issue 554 / deployed-custodian-runs-gc (iteration 2)

Skeptic's pass. Grounded on `$PDCA_TARGET` = `/home/eddie/development/wyrd/wyrd.pdca-wt-l0`
(worktree has the patch applied; base = `dc503cd`).

## Refutations that landed

- NEEDS-HUMAN [impl] — **The whole-fleet GC gate is defeated by the start-degraded fleet
  assembly: a custodian started while one D-server is down runs GC over a partial fleet on
  its very first pass — the exact stranding the deferral exists to prevent.** The gate
  checks `unreachable.is_empty()` (`crates/server/src/custodian.rs:539`), but `unreachable`
  comes from probing only the servers that made it into `configured` — and `connect_fleet`
  **silently omits** any peer unreachable at startup (`custodian.rs:197-204`; production
  call `cli.rs:876-884`, which even prints "N reachable of M configured", `cli.rs:900-904`).
  Concrete failing case: crashed fan-out leaves `pending:CHUNK` with fragments on d0 and d1;
  d1 is down when the custodian (re)starts — the day-one incident `connect_fleet` is
  explicitly built to start through (`cli.rs:872-874`); `configured = {d0,d2,d3}` →
  `unreachable = []` → GC runs, sweeps d0's copy, and **retires the chunk-wide `pending:`
  entry** (`crates/custodian/src/gc.rs:155-157,165-167`); when d1 returns, its copy is
  unreferenced, has no orphan record and no pending entry, so GC conservatively keeps it
  **forever** (`gc.rs:146-148`) — permanently leaked bytes. This falsifies the doc claim "GC
  is DEFERRED whenever any server is unreachable this pass" (`custodian.rs:408-411`): true
  only for mid-run unreachability, not startup unreachability. The #551 restore pass names
  and defuses this exact `connect_fleet` trap by requiring the whole fleet (`cli.rs:906-918`);
  the GC gate does not. The T3 test cannot catch it because it hand-assembles `configured`
  with all four servers and only toggles `health()` (`crates/server/tests/custodian_gc.rs:738-807`)
  — it never exercises the `connect_fleet` boundary the gate's correctness depends on.
  Builder-iterable fix: gate GC on live-fleet == operator-configured fleet (thread the
  endpoint count / a completeness flag into `run_reconstruction_until`), plus a test that
  drives the loop with `configured` shorter than the operator fleet.

- NEEDS-HUMAN — **Carry-forward Adversaries 1 & 2 are documented, not closed — and shipping
  this fix ACTIVATES the hazard they name.** On base, GC never ran, so a born-expired CLI
  lease was latent; with this patch a deployed custodian on a shared backend will actually
  collect it: `wyrd put` stamps `NOW_MILLIS = 0` / `lease_expiry = 60_000` logical
  (`cli.rs:75-76`), the deployed loop's clock is `wall_clock_millis` (`cli.rs:847,1210`),
  `expired_pending_chunks` compares them directly (`gc.rs:254`), and an in-flight put's
  fragments are unreferenced (committed-only reference set, `gc.rs:210-213`) — so the GC
  pass deletes a live write's fan-out mid-flight and retires its lease. Likewise the pending
  input still gets zero grace beyond the bare TTL (`gc.rs:142-144`): a >30 s gateway PUT
  (`DEFAULT_LEASE_TTL_MILLIS`, `lib.rs:49`) is collectable the instant its unrenewed lease
  expires — now every ≤ interval, not never. The iteration-1 sign-off said "the deployed
  collector must not reclaim in-flight CLI writes" and to **stop and route back** if closing
  needed the out-of-scope lease-liveness change; the builder shipped with doc-comments
  (`cli.rs:64-74`, `custodian.rs:105-109`) and a build-notes routing instead. Note a
  scope-compatible mitigation existed and was not taken: lag the `now_millis` handed to the
  GC pass by a fixed headroom (no `gc.rs` change, no signature change) — conservative for
  both inputs and it gives the pending input grace beyond the bare TTL. Whether
  document-and-ship satisfies the carry-forward, versus holding for #490, is the human's call.

- NEEDS-HUMAN — **Fitness trade-off: full deferral means any single-server outage pauses ALL
  reclamation fleet-wide** (`custodian.rs:539,561-570`). During a long outage — precisely when
  reconstruction is orphaning displaced fragments — #554's leak resumes for the duration; a
  decommissioned-but-still-configured server pauses GC indefinitely. The brief's FLEET VIEW
  paragraph steered toward reachable-fleet reclaim with per-server evidence preservation;
  the shipped shape deviates (defensibly: the pending input is chunk-wide and `gc::reconcile`
  is out of scope), and declares it (`custodian.rs:521-524`). Maintainer should confirm
  pause-under-outage is acceptable operationally.

## Minor (not gating anyone's attention)

- The exact grace boundary (`now == orphaned_at + grace` reclaims, `gc.rs:136`) is unpinned:
  the tests probe only ±1 ms around it (`custodian_gc.rs:591,651`). Conformance nit.

## Refutations attempted that did NOT land

- **Red→green honesty**: the new test uses only base-visible symbols (`orphan_key`/
  `pending_key`/`encode`/`PendingEntry`, `crates/core/src/metadata.rs:40,60,267,275`;
  `run_reconstruction_until` signature unchanged) and defines its own `GRACE_MILLIS`
  (`custodian_gc.rs:502`), so the reverted-base red is an honest assertion failure, not a
  compile error; the loop under test is the same entry `cmd_custodian` drives
  (`cli.rs:1121-1141`). Could not refute.
- **GC racing/reclaiming reconstruction's re-placed fragments**: passes are sequential and
  the reference set is rebuilt inside the GC pass (`gc.rs:96-100`); a just-committed
  placement protects its fragments. Could not refute.
- **Mid-pass store fault retiring evidence**: `gc::reconcile` commits its cleanup batch only
  after sweeping the whole fleet (`gc.rs:163-170`); an error before that preserves every
  pending/orphan record (at worst a consumed-orphan metadata record leaks — pre-existing
  library behavior, not this diff). Could not refute.
- **Premature reclaim inside the grace window**: pinned by
  `deployed_role_keeps_orphaned_bytes_within_the_grace_window` (`custodian_gc.rs:621-660`).
  Could not refute.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C4 Verification (red→green) — Accept the independently reproduced focused red→green (base: 3 failed/1 passed; patch: 4 passed), but decide whether CI must be rerun on a host allowing loopback sockets—the full `cargo xtask ci` rerun stopped only at `list_delete_over_grpc` with `PermissionDenied`, so the asserted complete green gate is provisional (`crates/server/tests/custodian_gc.rs:552`).
- [ ] T4 Contribution — Decide whether contribution overlap is clear enough to proceed—affected-path history confirms merged #551/#461/#450 prior art, but this sandbox cannot mechanically establish the brief’s claim that no closed/rejected competing work exists (`crates/server/src/custodian.rs:505`).
- [ ] T5 Judgment — Maintainers must choose a deployed orphan grace value and its reader-safety basis—the selected 60 s value is documented only as a floor, not a proven bound, so data-loss risk depends on an unresolved operational limit (`crates/server/src/custodian.rs:90`).
- [x] Validation — fitness-to-purpose — Decide whether deployment is fit despite known in-flight-write reclamation risk; exercise a CLI PUT sharing metadata with a wall-clock custodian and a gateway PUT lasting beyond 30 s, and require both to commit with all fragments intact before sign-off (`crates/server/src/cli.rs:68`, `crates/server/src/custodian.rs:106`).
- [ ] **The whole-fleet GC gate is defeated by the start-degraded fleet
- [x] **Carry-forward Adversaries 1 & 2 are documented, not closed — and shipping
- [ ] **Fitness trade-off: full deferral means any single-server outage pauses ALL

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
- Iteration delta (if iterating): Rejected for the NEW adversary finding, not the iteration-1 themes: the whole-fleet GC gate is defeated at startup. `connect_fleet` silently omits any peer unreachable when the custodian starts (custodian.rs:197-204; cli.rs:876-884), so `configured` is already partial, `unreachable.is_empty()` passes, and the very first GC pass can retire chunk-wide `pending:` evidence for a fragment a still-down server holds — a permanent leak, the exact stranding the deferral exists to prevent. FIX: gate GC on live fleet == operator-CONFIGURED fleet (thread the operator endpoint count / a completeness flag into run_reconstruction_until, as the #551 restore pass does at cli.rs:906-918), plus a test that drives the loop with `configured` shorter than the operator fleet — the existing T3 test hand-assembles all four servers and cannot catch this. Also in the rebuild: - Re-run the full `cargo xtask ci` on a host permitting loopback sockets so the green gate is non-provisional (reviewer's independent rerun stopped only at list_delete_over_grpc with PermissionDenied). - Pin the exact grace boundary (now == orphaned_at + grace reclaims, gc.rs:136) in the tests — currently probed only at ±1 ms (adversary conformance nit). - State the pause-under-outage trade-off explicitly in the run-loop doc (any single unreachable/decommissioned-but-configured server pauses ALL reclamation indefinitely) so the maintainer decision is visible at next sign-off (§6 item 7). RESOLVED at this sign-off — do NOT rework: the lease-liveness hazard (adversaries 1 & 2) is accepted as document-and-ship. #490's lease-conditional commit (obligation d) fail-closes both the born-expired CLI lease and the slow-gateway-PUT scenarios (refused write, never a torn committed object); residual mid-pass race is tracked as #557. Do not build any of the out-of-scope lease mechanisms; carry a sequencing note into the PR description instead: do not run `wyrd custodian` against a shared write-taking backend before #490 merges. T4 contribution-overlap and the T5 60s grace value (floor, not proven bound) remain human calls — keep them pre-declared in build-notes so they land in §6 again.
- By / date: Eduard Ralph / 2026-07-15

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
