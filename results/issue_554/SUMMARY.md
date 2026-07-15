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

Review of issue #554: make the deployed custodian run garbage collection after grace while preserving live data and deferred evidence under incomplete fleet views.

Target-state caveat: `$PDCA_TARGET` contains the preceding iteration (it lacks the hoisted run-loop uniqueness checks and revised grace rationale), while `patch.diff` applies cleanly to its `HEAD`; affected citations below are therefore grounded on the independently applied patch image, not treated as target defects.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 — C1 Spec | PASS | The acceptance decision is whether deployed-role reclamation, grace protection, and whole-fleet evidence preservation cover the stated leak without inventing reader-liveness machinery; the brief makes those boundaries falsifiable and the patch image exposes them at `crates/server/src/custodian.rs:530`. |
| C2 — C2 Reproduction (red pre-fix) | PASS | Reverting the two production source files while retaining the new test reproduced RED as the declared E0061 signature compile failure at `crates/server/tests/custodian_gc.rs:407`; this is an honest but structural red rather than the preferred bytes-remain assertion. |
| C3 — C3 Change | PASS | The safety decision is whether deletion may run only with unique fleet identities and a complete operator fleet; the production entry refuses duplicate identities at `crates/server/src/cli.rs:875` and threads the operator count at `crates/server/src/cli.rs:1049`, while GC gates on it at `crates/server/src/custodian.rs:582`. |
| C4 — C4 Verification (red→green) | NEEDS-HUMAN | Accept the broader gate as independently verified only on a host permitting loopback sockets — the patch suite passed 8/8 at `crates/server/tests/custodian_gc.rs:423`, but `cargo xtask ci` stopped at `list_delete_over_grpc` because this sandbox's loopback bind returned `PermissionDenied`, so the asserted full-CI green is provisional. |
| C5 — C5 Causal adequacy | PASS | The causal decision is whether to activate the existing collector rather than guard the leak symptom; the distinct production GC pass at `crates/server/src/custodian.rs:582` removes the missing run-loop wiring cause, with no capability probe or fallback guard. |
| T1 — T1 Structure | PASS | The change stays within server composition and deployed-role tests, and the separate fenced pass preserves scrub/reconstruction fault isolation at `crates/server/src/custodian.rs:569`. |
| T2 — T2 Shape | PASS | The interface decision is whether the run loop needs operator topology cardinality separate from connected peers; the explicit parameter at `crates/server/src/custodian.rs:454` represents the startup-degraded case without changing storage traits. |
| T3 — T3 Runtime | PASS | The runtime safety obligations are exercised in-process: grace boundary, live-byte preservation, expired leases, runtime outage, startup-partial fleet, and duplicate identities all passed in the eight-test suite beginning at `crates/server/tests/custodian_gc.rs:423`. |
| T4 — T4 Contribution | NEEDS-HUMAN | Decide whether any closed/rejected or concurrently open work overlaps these five affected paths — local merged history was checked by affected path, but closed/rejected remote work cannot be mechanically settled from the artifact-only sandbox, and overlap matters for integrating the fleet-wiring contract. |
| T5 — T5 Judgment | NEEDS-HUMAN | Maintainers must accept the 60 s floor, indefinite fleet-wide reclaim pause under one absent configured peer, and sequencing behind #490 — these affect reader safety and operational disk growth, and the patch itself states the bound is unproven at `crates/server/src/custodian.rs:90` and the pause at `crates/server/src/custodian.rs:564`. |
| V — Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether the in-memory production-wiring proof is sufficient evidence for deployment — it directly demonstrates reclaim and preservation but does not observe real backend timing, operator topology, or disk behavior; run `cargo test -p wyrd-server --test custodian_gc` and, on a loopback-capable host, `cargo xtask ci` before sign-off. |

### Advisory — adversary

# check-advisory-adversary.md — issue 554 / deployed-custodian-runs-gc (iteration 5)

Adversarial pass. I applied `patch.diff` to the base (`dc503cd`) in an isolated clone and
re-ran the evidence independently; probes and verdicts below.

- NEEDS-HUMAN — **Evidence provenance: the target worktree does NOT contain the patch under
  review.** `$PDCA_TARGET` holds a stale (iteration-4-shaped) state: the duplicate
  `--endpoints`/`--ids` refusal still exists ONLY inside `--reconcile-after-restore`
  (`crates/server/src/cli.rs:944` in the worktree — the patch's hoisted every-path refusal
  block is absent), and `crates/server/tests/custodian_gc.rs` is 828 lines vs the patch's
  933 — the two `deployed_run_loop_refuses_duplicate_{endpoints,ids}` tests are missing
  (`cargo test --test custodian_gc` in the worktree runs 6 tests, the patch has 8).
  `check-gates.json` (15:55) records the gating C4-ci as "all checks passed" — if that gate
  ran against this worktree, the green covers iteration-4 code, not this patch; if it ran on
  a fresh application of `patch.diff` elsewhere, the worktree is merely out of sync and the
  reviewer's citations may have been grounded on the wrong tree. I could not determine
  which from the artifacts available. Mitigation: I applied `patch.diff` cleanly to base
  `dc503cd` and independently ran `custodian_gc` (8/8 green), `custodian_day_one` (15/15
  green) and `closed_write_path` (1/1 green) — the patch itself is sound; the question is
  what the recorded gate evidence attests to.

- **Red→green evidence attacked; refutation FAILED (verified independently).** The
  full-revert red leg is a COMPILE ERROR (E0061: `run_reconstruction_until` takes 7
  arguments but 8 were supplied), not an assertion failure — I reproduced this by reverting
  `src/custodian.rs` + `src/cli.rs` and keeping the test file. The C4-verify row ("PASS —
  red without the fix, green with it") does not qualify this; the test module doc does
  (custodian_gc.rs:56-63) and the brief pre-authorizes it if flagged in build-notes (which I
  cannot read here — the human should confirm the flag landed). I then closed the
  pass-for-the-wrong-reason gap myself with three behavior-binding probes on the applied
  patch: (a) neutering the GC gate to `if false` → 5 of 8 tests fail by ASSERTION (all
  reclaim + both defer tests); (b) regressing the gate to iteration-2's
  `unreachable.is_empty()` (custodian.rs:582) → exactly
  `deployed_role_defers_gc_when_the_operator_fleet_is_startup_partial` fails by assertion;
  (c) stripping only the hoisted cli.rs refusal block → both `refuses_duplicate_*` tests
  fail (the base path panics on the empty fleet at cli.rs:899, as the test doc predicts).
  The tests exercise the production wiring, not a mirror.

- NEEDS-HUMAN — **The hoisted fleet-identity refusal is string-equality only; endpoint
  aliasing walks straight past it into the deleting GC sweep** (patch.diff cli.rs hunk, the
  `unique_endpoints` HashSet<&String> check). Concrete failing case:
  `wyrd custodian --endpoints http://localhost:50051,http://127.0.0.1:50051 --ids 1,2` —
  two textual identities for one physical box pass both refusals, and the exact hazard the
  patch's own comment names ("a LIVE fragment protected as (A, frag) is unreferenced seen as
  (B, frag), so GC would DELETE IT") is live again; likewise trailing-slash or DNS-alias
  variants. The same weakness pre-existed in the restore one-shot, but this patch is what
  arms the ALWAYS-ON run loop with deletion behind that check, and its comment claims the
  refusal guards "EVERY path". Fully closing it needs server-side identity attestation
  (canonicalizing URLs cannot resolve DNS aliasing) — an architectural call, hence no
  [impl]: the human should decide whether operator-attested endpoint uniqueness is an
  accepted trust assumption (documented), or a follow-up issue.

- NEEDS-HUMAN — **A startup-dropped peer is never re-dialed, so one degraded boot pauses GC
  for the entire process lifetime — even after the peer recovers.** `connect_fleet` drops a
  boot-unreachable peer once and `configured` never grows (custodian.rs:190-204); the gate
  `fleet.len() == operator_fleet_size` (custodian.rs:582) can then never be satisfied
  in-process, so the run-loop doc's recovery claim — "a missing server's garbage is reaped
  on a later whole-fleet pass" / "recovered in full on the next whole-fleet pass"
  (custodian.rs:562-567) — is unattainable after a startup drop: recovery requires an
  operator RESTART of the custodian. Concrete case: a custodian restarted mid-incident (the
  exact start-degraded scenario connect_fleet exists for, custodian.rs:168-175) reclaims
  nothing, silently (one `gc pass deferred` stderr line per interval), until someone
  restarts it again after the fleet heals. The startup-partial test masks this by invoking
  the loop twice (two fresh "processes", custodian_gc.rs:726+). Data-safety direction is
  conservative (defer, never false-collect) — this is a fitness/ops extension of the
  pre-declared pause-under-outage §6 trade-off, for the maintainer to accept or route to a
  re-dial follow-up.

- **Attempted to refute, could not:** (a) GC racing the repair loop's placement rewrites —
  the three passes run strictly sequentially in one task and GC re-derives the committed
  reference set inside its own pass (`gc.rs:100`, `gc.rs:124`), so a just-re-placed fragment
  is protected; (b) partial-sweep evidence loss — a mid-sweep store fault aborts before the
  single cleanup commit (`gc.rs:152-169`), so pending/orphan evidence survives (the residual
  is a lingering orphan record for an already-deleted fragment: conservative direction,
  gc-library behavior the brief scopes out); (c) whole-fleet gate arithmetic — fleet ⊆
  configured ⊆ operator endpoints holds on the cli path, and duplicates that could inflate
  `fleet.len()` are refused upstream; (d) boundary semantics — the inclusive `>=` reclaim
  instant is now pinned exactly (`deployed_role_reclaims_at_the_exact_grace_boundary`,
  fails if regressed to `>`); (e) pre-resolved iteration-2 items (lease-liveness
  document-and-ship pending #490, 60 s grace floor) — carried, not re-litigated.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [x] C4 — C4 Verification (red→green) — Accept the broader gate as independently verified only on a host permitting loopback sockets — the patch suite passed 8/8 at `crates/server/tests/custodian_gc.rs:423`, but `cargo xtask ci` stopped at `list_delete_over_grpc` because this sandbox's loopback bind returned `PermissionDenied`, so the asserted full-CI green is provisional.
- [x] T4 — T4 Contribution — Decide whether any closed/rejected or concurrently open work overlaps these five affected paths — local merged history was checked by affected path, but closed/rejected remote work cannot be mechanically settled from the artifact-only sandbox, and overlap matters for integrating the fleet-wiring contract.
- [x] T5 — T5 Judgment — Maintainers must accept the 60 s floor, indefinite fleet-wide reclaim pause under one absent configured peer, and sequencing behind #490 — these affect reader safety and operational disk growth, and the patch itself states the bound is unproven at `crates/server/src/custodian.rs:90` and the pause at `crates/server/src/custodian.rs:564`.
- [x] V — Validation — fitness-to-purpose — Decide whether the in-memory production-wiring proof is sufficient evidence for deployment — it directly demonstrates reclaim and preservation but does not observe real backend timing, operator topology, or disk behavior; run `cargo test -p wyrd-server --test custodian_gc` and, on a loopback-capable host, `cargo xtask ci` before sign-off.
- [x] **Evidence provenance: the target worktree does NOT contain the patch under
- [x] **The hoisted fleet-identity refusal is string-equality only; endpoint
- [x] **A startup-dropped peer is never re-dialed, so one degraded boot pauses GC
- [x] external dependency: disk-quota-headroom — the C4 gate fails at `dst_commit.rs` with

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
- By / date: Eduard Ralph / 2026-07-15

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
- issue_554: harness worktree-reset defect — lane worktree (wyrd.pdca-wt-l1) kept iteration-4 state into iteration 5, so the gating C4-ci green attested to the previous iteration's code; likely triggered by the host going on standby mid-process; consider making the driver reset/re-populate `$PDCA_WORKTREE` (and verify it matches patch.diff) before running gates.
- issue_554: file follow-up under milestone M7 (Failover and disaster recovery) — the custodian never re-dials a peer dropped at a degraded boot (`connect_fleet`, custodian.rs:190-204), so GC stays paused for the process lifetime until an operator restart; re-dial configured-but-dropped peers each pass, making the run-loop doc's "recovered on the next whole-fleet pass" claim true.
- issue_554: pdca harness bug (human diagnosis at sign-off) — the disk-quota exhaustion behind §6 item 8 is the harness failing to clean up after itself: accumulated `../wyrd-*/target` dirs and stale lane/verify worktrees (>200 G) exhaust the user quota; the harness should prune or bound its worktree/target footprint after cycles.
- issue_554: the endpoint-aliasing trust assumption (accepted at this sign-off, §6) should be structurally closed by M5 step-ca identity attestation — key the fleet-uniqueness refusal on attested peer identity instead of endpoint string equality.
