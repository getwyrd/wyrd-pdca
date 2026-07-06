# Result — issue 365 / coordination-etcd-l5-backend

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect / goal: 
- Success criterion: a **shared `Coordination` conformance / contract suite** — lifted
- Repo + branch target: getwyrd/wyrd @ `feat/m4-production-metadata-backend`
- Scope (one logical fix) / out of scope: 

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: likely-fix (net-new crate + shared suite; a real, exercisable second
- Confidence: medium
- Recommendation: (set by Do)

## 3. Correctness (Check — chain)
- C1 Spec: none — brief.md
- C2 Reproduction (red pre-fix): none — (no gate configured)
- C3 Change: none — patch.diff
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): pass — xtask ci: all checks passed
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass —                no pre-patch state to isolate a RED against; C4-ci gates the whole tree (#88).
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

# Check review — issue 365 / coordination-etcd-l5-backend (iteration 6)

**Task under review:** the `Coordination` (L5) trait has exactly one implementation
(process-local `coordination-mem`), so a real multi-node cluster cannot discover peers,
elect a single custodian leader, or fence stale holders across machines. Build the
ADR-0006 REQUIRED **second implementation** — a networked `coordination-etcd` crate
implementing every trait method over etcd — plus **one shared conformance suite both
backends pass**, selectable by `server` composition with **no caller edits**.

Grounded on the applied target worktree `/home/eddie/wyrd/wyrd.pdca-wt-l0` (patch present:
`crates/coordination-etcd`, `crates/coordination-conformance` exist, files dated with the
patch). `cargo`/`git` invocations were blocked by the sandbox (approval-gated), so the
real-etcd build/run could not be re-executed here — those citations are grounded by source
reading of the applied tree and cross-checked against `patch.diff`.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | Brief pins a falsifiable target: a second, networked `Coordination` over etcd + one shared contract suite green on both backends + a `server`-composition swap with no caller edits (brief.md:40-58). Scope and out-of-scope (process roles → #364/#366) are bounded. |
| C2 Reproduction (red pre-fix) | PASS | This is net-new capability, so the flippable RED is the violating-stub counterpart: `coordination-conformance/tests/demonstrated_red.rs` has one `#[should_panic]` stub per clause (NeverDiscovers, NeverRevokes, NonExclusiveLock, ConstantToken, FrozenConfigRevision, ConfigCountsEveryWrite, DuplicateLeaseIds), and `dst/tests/coordination.rs:272-297` runs the cross-instance clauses against two process-local mem instances → RED. Non-vacuity is demonstrated, not asserted. |
| C3 Change | PASS | `coordination-etcd/src/store.rs` implements all 10 trait methods with etcd semantics (leased register/discover, campaign-based election, txn-based fenced locks, config via max mod_revision); `keyspace.rs` isolates concerns; `server/src/cli.rs:3508-3688` selects the backend by config through a generic `run_d_server`. No edit to `traits`/`core`/`custodian` (grep-confirmed) — the byte-for-byte invariant holds. |
| C4 Verification (red→green) | PASS | Gating C4-ci is green (check-gates.json:36), and the store is genuinely compiled+driven in-gate under `--cfg madsim` (etcd-client aliased to madsim-etcd-client) by `dst/tests/coordination.rs` — closing the iter-1/2 "store never compiled by any gate" blocker deterministically. The iter-5 hard defect (real-etcd conformance failed to compile: missing `use wyrd_traits::Coordination;`) is fixed by inspection at `coordination-etcd/tests/conformance.rs` (import present at the single-leader clause). Caveat: the real-etcd RUN (criterion (b)) is off-CI and could not be re-executed here → carried to V, not a blocking FAIL. |
| C5 Causal adequacy | NEEDS-HUMAN | Root cause (trait pinned by only one impl) is addressed by building the real second impl, not by a symptom guard — no capability-probe/load-time smell fires (the `is_lost()` / `--features etcd` split are correctness/build-config, not a runtime capability probe). Decision owed: **DST-fidelity acceptance** (#264/#258 mirror) — is the deterministic madsim-etcd-client simulator proof an accepted stand-in for real-etcd distributed correctness, or must real-etcd green be produced before this backend enters the shipped graph? This is the axis that drove rejections 1-5; impact is split-brain in production if the simulator hides a real-etcd fidelity gap. |
| T1 Structure | PASS | Tests are sited correctly: shared clauses in `coordination-conformance/src/lib.rs` (driven by both backends via `run_all`), non-vacuity in `.../tests/demonstrated_red.rs`, deterministic distributed proof in `dst/tests/coordination.rs`, real-etcd home in `coordination-etcd/tests/conformance.rs`, composition in `server/tests/backend_selection.rs`. No etcd-only fork of the contract. |
| T2 Shape | PASS | Clauses assert the load-bearing properties, tightened per prior rejects: single-leader asserts B's campaign stays PENDING while A leads (lib.rs:264-279; dst coordination.rs:202-205), config asserts config-ONLY advancement not just r1>r0 (lib.rs:211-228), renew/revoke covered (lib.rs:91-115), orphan-safety + lapse-recovery + transient-proclaim-error each have a dedicated madsim test. |
| T3 Runtime | PASS | The madsim `dst` tier (single-instance suite + cross-instance clauses + demonstrated-red + expiry/orphan/lapse/transient) runs under `cargo xtask ci` and is green per C4-ci. Real-etcd runtime is off-CI (folded into V). |
| T4 Contribution | PASS | The new tests are the flippable regression: they go RED against the violating stubs and two-mem-instance drivers and GREEN against the real store; `xtask etcd-conformance` now hard-fails (not false-green) when docker/protoc are missing and separates `--no-run` build failure from bootstrap-flake retry (xtask/src/main.rs:293-365), closing the iter-3/5 false-green and misreported-compile defects. |
| T5 Judgment | NEEDS-HUMAN | The test artifact is materially complete and non-vacuous by inspection; the residual judgment is the meta-adequacy call the brief routes to reviewer+human: accept that in-gate coverage is the deterministic SIMULATOR plus an operator-run (not CI-gated) real-etcd conformance for a production L5 backend. Decision owed: is that split sufficient, or must real-etcd conformance be wired into a gate before accept? (Same axis as C5, from the test-adequacy side.) |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Binding criterion (b) — the shared suite + cross-instance single-leader/mutual-exclusion/discovery GREEN on REAL etcd — is earnable but not re-verified here (cargo blocked in this sandbox). Operator must run, on this docker+protoc host, from `/home/eddie/wyrd/wyrd.pdca-wt-l0`: (1) `cargo test -p wyrd-coordination-etcd --features etcd --test conformance --no-run` (confirms the iter-5 compile fix), then (2) `cargo xtask etcd-conformance` (brings up deploy/etcd-single-node, runs the shared suite + single-leader on real etcd, tears down) — must exit GREEN. Also owed at sign-off: **etcd-client 0.14 dependency review** (ADR-0003 three-test audit + deny.toml allowlist + the ships-no-TLS/auth `Client::connect(endpoints, None)` posture, store.rs:207), **DST-fidelity acceptance**, and the **sequencing-governance** call (explicit M4 slice vs preceding coordination milestone). Prior-art: this is the 6th iteration of the same issue; iterations v1-v5 preserved — the recurring axis is criterion (b) real-etcd green, which the human must confirm is now truly earned. |

### Advisory — adversary

# Adversarial review — issue 365 / coordination-etcd-l5-backend (iteration 6)

Lens: refute the red→green evidence and the reviewer's verdict. Grounded on the
target source at `/home/eddie/wyrd/wyrd.pdca-wt-l0`. This is the 6th iteration; the
patch has been rejected five times on a small set of recurring axes. I re-ran those
axes rather than re-litigate them from the diff alone.

## Evidence I re-ran (not read)

- **Criterion (b) — real-etcd GREEN — now genuinely earns.** The recurring reject
  (iters 3/4/5: test doesn't compile under `--features etcd`; single-leader never
  checked on real etcd; `xtask` false-green). I brought up `deploy/etcd-single-node`
  on live docker etcd v3.5.16 and ran
  `WYRD_ETCD_ENDPOINTS=… cargo test -p wyrd-coordination-etcd --features etcd --test conformance` —
  it **compiled and passed** ("real etcd passed the shared Coordination conformance
  suite and the cross-instance properties (single leader, mutual exclusion,
  discovery)"), 2.06s. The iter-5 E0599 is fixed by `use wyrd_traits::Coordination;`
  at `crates/coordination-etcd/tests/conformance.rs:54`.
- **The single-leader clause cannot pass for the wrong reason.** At
  `crates/coordination-etcd/tests/conformance.rs:109-115` /
  `crates/coordination-conformance/src/lib.rs:264-279`, the bounded wait is
  `timeout(2s, b.elect_leader).ok().map(|r| r.unwrap())`: a B **win** fails the
  `is_none()` assert (split-brain caught), a B **error** *panics* on the `unwrap`, and
  only B genuinely staying pending yields `None`/pass. On live etcd B stayed pending
  for the full 2s while A led — the headline safety property is checked on a real
  cluster, not just the simulator.
- **The store is compiled + exercised by a gate.** The iter-2 "store never compiled by
  any gate" blocker is closed: `crates/dst/Cargo.toml:53-64` aliases `etcd-client` →
  `madsim-etcd-client` under `--cfg madsim`, and `crates/dst/tests/coordination.rs`
  (`#![cfg(madsim)]`) drives the same production `store.rs` deterministically in `ci`.
- **`xtask etcd-conformance` false-green is fixed.** `xtask/src/main.rs:293-312` now
  `Err`s (not warn-and-`Ok`) when docker/protoc are missing, and `:332-365` splits
  `--no-run` (compile = hard fail, not retried) from the run (retried) — the exact
  iter-5 "compile error masqueraded as bootstrap flake" defect.
- **The correctness defects from iters 1–2 are addressed and gated:** keep-alive that
  actually renews for the hold's life (`store.rs:117-166`); cancel-safe campaign guard
  (`store.rs:321-350`, tested `dst/tests/coordination.rs:333`); lease-scoped
  conditional `unlock` (`store.rs:401-413`); loss concluded only from the keep-alive's
  `is_lost`, never from a proclaim error (`store.rs:281-312`, tested `:381`, `:449`);
  config-only revision advancement via max `mod_revision` over the config prefix
  (`store.rs:433-456`, pinned by `conformance/src/lib.rs:219-228`); election-key
  encoding closing the iter-4/5 prefix-collision (`keyspace.rs:67-69`, regression at
  `keyspace.rs:138-164`).

**I attempted to refute criterion (b), the single-leader property, the split-brain
guard, the config-only-advancement clause, the lease-scoped unlock, and the
prefix-collision fix on live etcd + by inspection, and could not.** For an issue with
this rejection history that is the material signal.

## Residual findings (advisory; scoped to this diff)

- NEEDS-HUMAN — **Liveness gap: a leader whose *key* is lost while its *lease* survives
  is stuck erroring, not recovering** (`crates/coordination-etcd/src/store.rs:293-312`).
  The "still leading" path keys off `is_lost()` (lease liveness) only. If A's leader key
  disappears while A's lease is still renewing (the exact fault the iter-4-mandated test
  `a_transient_proclaim_error_keeps_the_hold_and_its_lease`,
  `dst/tests/coordination.rs:449` injects), every subsequent `elect_leader` re-proclaims
  the dead key and returns `Err` **forever** — A never re-campaigns (is_lost stays
  false) even though another instance can legitimately win. This is safe (A returns
  `Err`, never a false leadership → no split-brain) but a **liveness** hole: A cannot
  reclaim leadership without dropping the store. The one gated test on this path asserts
  only the single `Err` + lease-still-alive, not recovery. It is not naturally reachable
  on etcd (key and lease live/die together), so I rate it low-severity — but a human
  should confirm the custodian's reaction to a persistent `elect_leader` `Err` is
  "step down / restart," not "spin." Not a rebuild blocker.

- NEEDS-HUMAN — **Standing dependency/decision items are genuinely still owed** (the
  brief's own §"Known NEEDS-HUMAN" and iter-2..5 carry-forwards): (1) the `etcd-client
  0.14` review — ADR-0003 three-test audit, `deny.toml` allowlist, and the ships-no-TLS
  `Client::connect(endpoints, None)` posture at `store.rs:207`; (2) the DST-fidelity
  acceptance (madsim-etcd-client vs a contract harness, the #264/#258 mirror); (3) the
  sequencing-governance call (explicit M4 slice vs preceding milestone). These are
  human decisions the patch cannot and does not resolve; they are correctly left to
  sign-off, not code defects.

## Verdict claims I could not overturn

- `check-gates.json` C4-verify's "no pre-patch state to isolate a RED against" is a
  fair characterization for a net-new crate: the demonstrated-red is supplied instead
  by the `#[should_panic]` two-`coordination-mem`-instance clauses
  (`dst/tests/coordination.rs:272-297`), which pin the cross-instance clauses as
  non-vacuous. I could not show any cross-instance clause passes vacuously.
- The "byte-for-byte unchanged trait/callers" invariant holds in the diff: the change
  is a `server`-composition selection (`crates/server/src/cli.rs:130-189`), the trait
  seam is untouched.

Net: no correctness refutation stuck. The two NEEDS-HUMAN items above are for
adjudication at sign-off, not grounds to reject the fix itself.

### Advisory — codex

- NEEDS-HUMAN — `etcd-client` enters the production feature graph at [Cargo.toml:101](/home/eddie/wyrd/wyrd.pdca-wt-l0/Cargo.toml:101), and the implementation currently dials it with default options at [crates/coordination-etcd/src/store.rs:207](/home/eddie/wyrd/wyrd.pdca-wt-l0/crates/coordination-etcd/src/store.rs:207). Before shipping the `etcd` feature, sign-off should explicitly adjudicate the ADR-0003 dependency review plus TLS/auth posture.
- NEEDS-HUMAN — The real-etcd conformance job is deliberately not part of `ci` ([xtask/src/main.rs:21](/home/eddie/wyrd/wyrd.pdca-wt-l0/xtask/src/main.rs:21), [xtask/src/main.rs:25](/home/eddie/wyrd/wyrd.pdca-wt-l0/xtask/src/main.rs:25)), while `check-gates.json` only records `cargo xtask ci`; human sign-off should confirm `cargo xtask etcd-conformance` actually ran green against live etcd before accepting criterion (b).

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [x] C5 Causal adequacy — Root cause (trait pinned by only one impl) is addressed by building the real second impl, not by a symptom guard — no capability-probe/load-time smell fires (the `is_lost()` / `--features etcd` split are correctness/build-config, not a runtime capability probe). Decision owed: **DST-fidelity acceptance** (#264/#258 mirror) — is the deterministic madsim-etcd-client simulator proof an accepted stand-in for real-etcd distributed correctness, or must real-etcd green be produced before this backend enters the shipped graph? This is the axis that drove rejections 1-5; impact is split-brain in production if the simulator hides a real-etcd fidelity gap.
- [x] T5 Judgment — The test artifact is materially complete and non-vacuous by inspection; the residual judgment is the meta-adequacy call the brief routes to reviewer+human: accept that in-gate coverage is the deterministic SIMULATOR plus an operator-run (not CI-gated) real-etcd conformance for a production L5 backend. Decision owed: is that split sufficient, or must real-etcd conformance be wired into a gate before accept? (Same axis as C5, from the test-adequacy side.)
- [x] Validation — fitness-to-purpose — Binding criterion (b) — the shared suite + cross-instance single-leader/mutual-exclusion/discovery GREEN on REAL etcd — is earnable but not re-verified here (cargo blocked in this sandbox). Operator must run, on this docker+protoc host, from `/home/eddie/wyrd/wyrd.pdca-wt-l0`: (1) `cargo test -p wyrd-coordination-etcd --features etcd --test conformance --no-run` (confirms the iter-5 compile fix), then (2) `cargo xtask etcd-conformance` (brings up deploy/etcd-single-node, runs the shared suite + single-leader on real etcd, tears down) — must exit GREEN. Also owed at sign-off: **etcd-client 0.14 dependency review** (ADR-0003 three-test audit + deny.toml allowlist + the ships-no-TLS/auth `Client::connect(endpoints, None)` posture, store.rs:207), **DST-fidelity acceptance**, and the **sequencing-governance** call (explicit M4 slice vs preceding coordination milestone). Prior-art: this is the 6th iteration of the same issue; iterations v1-v5 preserved — the recurring axis is criterion (b) real-etcd green, which the human must confirm is now truly earned.
- [x] **Liveness gap: a leader whose *key* is lost while its *lease* survives
- [x] **Standing dependency/decision items are genuinely still owed** (the
- [x] `etcd-client` enters the production feature graph at [Cargo.toml:101](/home/eddie/wyrd/wyrd.pdca-wt-l0/Cargo.toml:101), and the implementation currently dials it with default options at [crates/coordination-etcd/src/store.rs:207](/home/eddie/wyrd/wyrd.pdca-wt-l0/crates/coordination-etcd/src/store.rs:207). Before shipping the `etcd` feature, sign-off should explicitly adjudicate the ADR-0003 dependency review plus TLS/auth posture.
- [x] The real-etcd conformance job is deliberately not part of `ci` ([xtask/src/main.rs:21](/home/eddie/wyrd/wyrd.pdca-wt-l0/xtask/src/main.rs:21), [xtask/src/main.rs:25](/home/eddie/wyrd/wyrd.pdca-wt-l0/xtask/src/main.rs:25)), while `check-gates.json` only records `cargo xtask ci`; human sign-off should confirm `cargo xtask etcd-conformance` actually ran green against live etcd before accepting criterion (b).

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
- By / date: Eduard Ralph / 2026-07-04

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
- issue_365 — etcd-client TLS/auth hardening + ADR-0003 dependency audit: accepted as the posture for this slice, but track as follow-up. `store.rs:207` dials `Client::connect(endpoints, None)` (no TLS/auth) and `etcd-client 0.14` now enters the production feature graph (`Cargo.toml:101`). Owed later: ADR-0003 three-test audit + `deny.toml` allowlist + real TLS/auth before production etcd exposure.
- issue_365 — downstream consumer: issue_366's rebuild must wire THIS etcd `Coordination` (leased register/discover for fleet membership; campaign election + txn-fenced locks for single-active fencing) to replace its probe-and-drop membership stand-in and local `MemCoordination` fencing. 365 delivers the backend; process-role wiring is 366's Do scope.
