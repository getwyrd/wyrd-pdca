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

# Check review — issue 365 / coordination-etcd-l5-backend

**Task under review:** the `Coordination` (L5) trait has exactly one implementation
(process-local `coordination-mem`), so a real multi-node cluster cannot discover peers,
elect a single custodian leader, or fence stale holders across machines. This patch builds
the ADR-0006 REQUIRED second implementation — a networked `coordination-etcd` crate over
etcd — plus one **shared** contract suite both backends pass, and a `server`-composition
selection with the trait and all callers byte-for-byte unchanged. This is iteration 2:
iteration 1 was rejected because the etcd backend as written would ship split-brain
(no lease keep-alive, unconditional unlock, non-re-fencing election, untested config,
single-instance-only suite).

**Grounding note:** `$PDCA_TARGET` is not reachable from this review dir (env read is
sandbox-blocked; no wyrd checkout resolves under CWD or `../`), so citations are grounded on
`patch.diff` and I could not execute the suite. Verdicts are re-derived by inspection of the
patch plus the recorded gate results (`check-gates.json`: C4-ci = pass, gating; C4-verify =
pass, non-gating). The decisive fact for this cycle is structural and re-derivable from the
patch without a toolchain: **the etcd store (`store.rs`, 429 lines — the crate's entire reason
to exist) is compiled only under the OFF-by-default `etcd` feature, so no gate that ran ever
compiled or exercised it.** CI green therefore proves only (a) impl #1 was not regressed,
(b) the shared suite is non-vacuous against a hand-written broken stub, and (c) the load-light
`keyspace`/`fencing`/`hold` units pass — **not** that the etcd backend is correct.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | Patch delivers the four scoped items (new `crates/coordination-etcd` `patch.diff` store.rs/lib.rs; shared `crates/coordination-conformance`; `server` selection cli.rs:2098-2153; workspace wiring Cargo.toml:364-378) matching the brief's Scope; trait/`core`/`custodian` untouched (no diff to `crates/traits`). Well-specified against a detailed brief. |
| C2 Reproduction (red pre-fix) | PASS | Headless RED is genuine and re-runnable: `demonstrated_red.rs:699-811` drives every shared clause against a `BrokenCoordination` stub and asserts each panics — the suite bites. BUT this exercises a stub, **not** `EtcdCoordination`; the flippable regression (etcd store RED→GREEN vs the real suite) is endpoint-gated and cannot run here — the human owes the real-etcd RED/GREEN (see V). No gate configured for C2. |
| C3 Change | PASS | Change is coherent and self-consistent: etcd→trait mapping (leased register/discover, campaign-based election, CAS-txn try-lock, mod-revision config) implemented in `store.rs:1394-1648`; server composition is a generic `run_d_server<Co: Coordination>` selection, not a caller refactor (cli.rs:2248-2279). Iteration-1 code defects each have a corresponding fix in the diff. |
| C4 Verification (red→green) | **NEEDS-HUMAN** | Gating C4-ci passed but by construction **never compiles `store.rs`** (etcd feature off), so the crate's correctness is unverified by any gate that ran. Decision owed: run `cargo test -p wyrd-coordination-etcd --features etcd` with `WYRD_ETCD_ENDPOINTS` against a real etcd and confirm GREEN (the networked half of the two-impl pin). The iteration-1 false-green (endpoints set + feature off) is now fixed — it panics (`conformance.rs:1696-1704`). Note: `--features etcd` pulls `etcd-client 0.14` which needs system `protoc` vs the repo's no-system-protoc posture — the human's run environment must provide it. |
| C5 Causal adequacy | **NEEDS-HUMAN** | Root cause (only one impl / no cross-process L5) is addressed by a real second impl, and the iteration-1 split-brain defects are addressed **in code**: per-hold background keep-alive (`store.rs:1349-1369,1500-1502,1571`), unlock-by-lease-revoke never delete-by-key (`store.rs:1585-1602`), re-fence via proclaim-then-recampaign (`store.rs:1456-1530`). No capability-probe / runtime-guard smell (the `#[cfg(feature=etcd)]` is compile-time composition, not a probe). Decision owed: whether these mechanisms actually deliver single-leader + mutual-exclusion + no-split-brain **under real etcd** — the exact claim iteration 1 was rejected on — is a contested distributed-correctness judgment that rests on unexecuted code; a human must confirm against a live cluster (ties to C4/V). |
| T1 Structure | PASS | Crate layout mirrors the `metadata-tikv` precedent: dependency-free load-light modules (`keyspace`/`fencing`/`hold`) compiled+unit-tested everywhere, real `store` behind the feature; workspace membership + optional-dep wiring correct (Cargo.toml, server Cargo.toml:2057,2067). Shared suite is a separate crate both backends depend on (no etcd-only contract fork). |
| T2 Shape | PASS | `EtcdCoordination` implements the trait surface exactly with no trait edit (invariant held); `server` selection enum mirrors `MetadataBackend` (cli.rs:2098-2131). Two deliberate contract relaxations — lease `distinct` not `rising` (`lib.rs:523`) and config revision `strictly-rising` not `+1` (`lib.rs:596`) — are defensible (etcd lease ids/mod-revisions are opaque/cluster-wide) and mem still satisfies them a fortiori; flagged for human awareness, not a defect. |
| T3 Runtime | PASS | Runtime paths that actually run in CI are sound: mem shared-suite run (`coordination-mem/tests/conformance.rs:2037`), demonstrated_red, and load-light unit tests (keyspace aliasing, fencing monotonicity, keepalive cadence) — recorded green. Keep-alive tasks are aborted on `Drop` (`store.rs:1378-1391`) so leases lapse for failover. The networked runtime (spawned keep-alives, campaign blocking) is not exercised here — folded into C4/V. |
| T4 Contribution | PASS | Tests are genuine regression guards, not vacuous: mem run guards impl #1 against the suite lift; `demonstrated_red` proves non-vacuity via a stub that fails every clause; new config clause (`lib.rs:596`) closes the iteration-1 untested-config gap; cross-instance lock/election tests (`conformance.rs:1723-1743`) assert the cross-process guarantees mem cannot — though those, being networked, only contribute once a human runs them (C4/V). |
| T5 Judgment | **NEEDS-HUMAN** | Two judgment calls the human must weigh. (1) The iteration-1 requirement "there is no `xtask etcd-conformance` job … must be resolved before the real-etcd GREEN can be earned" is **still unmet** — the patch adds no Tier-2 automation, so the flippable GREEN has no reproducible CI home and lives entirely on a manual invocation; decide whether that is acceptable to sign off or whether the job must land first. (2) Keeping leases alive internally (background task) instead of a renewable-handle trait change is a sound call that preserves the byte-for-byte-trait invariant — confirm it is the intended resolution of iteration-1 fix-point 1 (a trait change was the alternative, itself a NEEDS-HUMAN). |
| Validation — fitness-to-purpose | **NEEDS-HUMAN** | Whole-purpose validation cannot be mechanized here. Owed at sign-off: (a) run the shared suite + cross-instance clauses against a real etcd and confirm "peers discovered through L5 / single leader / fenced mutual exclusion" actually hold; (b) the **DST-fidelity decision** (`madsim-etcd-client` vs contract harness — the #264/#258 mirror), named unresolved in the brief; (c) **etcd-client dependency review** — ADR-0003 three-test audit, `deny.toml` allowlist, TLS/auth posture, exact version, and the system-`protoc` posture (brief NEEDS-HUMAN); (d) the **sequencing governance** decision (explicit M4 slice vs preceding coordination milestone, 0015 :461-463/:707-709). Prior-art by affected path could not be settled mechanically here (target unreachable) — confirm no closed/rejected `coordination-etcd` work conflicts. |

### Advisory — adversary

# Adversarial review — issue 365 / coordination-etcd-l5-backend (iteration 2)

Skeptic's pass. Attacked the red→green evidence, the etcd backend's correctness, and the
gate verdict. Grounded on `$PDCA_TARGET` (`/home/eddie/wyrd/wyrd.pdca-wt-l0`). I am advisory;
I do not gate.

## The evidence — the gate does not exercise the production path

- **NEEDS-HUMAN — The deterministic GREEN never compiles or runs the etcd backend.**
  `crates/coordination-etcd/Cargo.toml` sets `default = []` and gates the store on
  `etcd = ["dep:etcd-client", …]`; `crates/coordination-etcd/src/lib.rs:325` guards
  `mod store;` with `#[cfg(feature = "etcd")]`; `crates/server/Cargo.toml`'s `etcd` feature
  and the `CoordinationBackend::Etcd` arm in `crates/server/src/cli.rs:145` are likewise
  `#[cfg]`-gated out. `check-gates.json` C4-verify records *"no pre-patch state to isolate a
  RED against"*. So the whole gated GREEN ("xtask ci: all checks passed") proves only three
  things: (1) `coordination-mem` still passes the shared suite (impl #1 not regressed),
  (2) the suite is non-vacuous against the `BrokenCoordination` stub
  (`crates/coordination-conformance/tests/demonstrated_red.rs`), (3) the dep-free
  `keyspace`/`fencing`/`hold` unit tests. It establishes **nothing** about
  `EtcdCoordination` (`store.rs`) — the split-brain keep-alive, the lease-scoped `unlock`,
  and the re-fence path that iteration 1 was rejected over are verified by *code reading
  only*. Criterion (b) "the shared suite is green on both backends (real etcd)" is unproven
  by any gate and rests entirely on a human running real etcd at sign-off. Do not read the
  reviewer's "pass" as validation of the etcd path.

- The RED half of the flippable regression is demonstrated against a *parallel* in-process
  stub (`BrokenCoordination`, `demonstrated_red.rs:32`), never against the real etcd path.
  That is legitimate for proving suite non-vacuity, but it is **not** a red→green on
  production code — the production object `EtcdCoordination` is compiled out of every
  headless run.

## The fix — concrete failing cases

- **NEEDS-HUMAN — Cross-instance election test leaks a live lease from the cancelled
  campaign; the single-leader proof can hang and fail against real etcd.**
  `crates/coordination-etcd/tests/conformance.rs:210` wraps the *first*
  `b.elect_leader("custodian")` in `tokio::time::timeout(750ms)` and asserts it does not
  resolve. But `elect_leader` (`store.rs:196`) grants a lease and calls `self.spawn_keepalive`
  (`store.rs:118`) — a detached `tokio::spawn` — *before* `client.campaign(...).await`
  (`store.rs:203`). etcd's `Campaign` first *puts* B's candidate key (bound to the lease),
  then blocks. When the 750ms timeout drops the `elect_leader` future, the candidate key is
  already written and the detached keep-alive task keeps its lease alive **forever** (a
  dropped future does not abort an already-spawned task, and no `LeaderHold` was inserted into
  `state`, so nothing can ever abort it). After `drop(a)` (`:222`), etcd promotes the
  lowest-create-revision candidate — B's *orphaned* candidacy, not the fresh
  `b.elect_leader("custodian")` at `:223` — and the fresh campaign blocks behind an orphan
  that no future will ever resign, so the `timeout(Duration::from_secs(40))` expires and
  `.expect("B's campaign resolves once A releases")` panics. This is a concrete failure of the
  exact cross-process single-leader test that is the crate's raison d'être: the promised
  real-etcd GREEN cannot be earned as written. (Depends on `etcd-client` Campaign
  cancellation semantics — a human with real etcd must adjudicate.)

- **`Drop` aborts keep-alives but never revokes the leases** (`store.rs:130-141`). On a clean
  drop, leadership/lock leases linger for their full `HOLD_TTL_SECS = 30` (`store.rs:63`)
  instead of being released promptly. Production impact: a cleanly shut-down custodian holds
  leadership for up to 30s after exit, widening the single-leader gap for no reason; and the
  election failover test (`conformance.rs:224`) leans on this lapse under a 40s budget, so it
  is timing-fragile even setting aside the leak above.

- **`renew` and `revoke` are exercised by no test on any backend.** The shared suite
  (`crates/coordination-conformance/src/lib.rs`) drives register/discover/elect/lock/unlock/
  config but never calls `renew` or `revoke`; the cross-instance etcd tests use only
  lock/elect. So `EtcdCoordination::renew` (`store.rs:145`) and `revoke` (`store.rs:160`) —
  the registration-renewal path the production `server` composition actually wires via
  `server.serve(coord, lease, renew_interval, …)` (`crates/server/src/cli.rs:571`) — carry
  zero contract coverage against etcd. A defect in the `renew` `stream.message()`/`ttl()>0`
  check would let D-server registrations silently lapse in production and be caught by
  nothing.

## The verdict — where the reviewer may have rationalized

- **NEEDS-HUMAN — Criterion (b) is asserted, not demonstrated.** Any brief/reviewer claim
  that "the shared suite is green on both backends" is, at Check time, unwarranted: the etcd
  half is `#[cfg]`-compiled out of every gate run and the sign-off task ("run real etcd") is
  itself listed as a NEEDS-HUMAN with no headless runner. The green tree is necessary but not
  sufficient evidence for the success criterion.

- **Config revision-monotonicity is not independently shown to bite.**
  `config_rejects_a_backend_that_never_persists` (`demonstrated_red.rs`) panics at the
  *earlier* "written value reads back" assertion (`BrokenCoordination::get_config` always
  returns `None`), never reaching the `r1 > r0` revision check in
  `contract_config_is_revisioned` (`crates/coordination-conformance/src/lib.rs:170`). A
  backend that persisted values but returned a frozen `config_revision` is therefore not
  demonstrated-RED — the very "config with a monotonic revision" clause the iteration-1
  rework was told to add is not proven non-vacuous.

- **The "rising lease id" property is no longer asserted anywhere.** Iteration 1 asserted
  `second.id > first.id`; the shared clause was relaxed to `assert_ne!` only
  (`contract_leases_are_distinct`, `lib.rs:105`) and the mem-specific tests
  (`crates/coordination-mem/tests/conformance.rs`) did not re-add a rising assertion.
  Relaxing to "distinct" is defensible per the trait's "opaque" wording
  (`crates/traits/src/lib.rs:484`), but the reviewer should confirm the drop of mem's
  rising-lease coverage is intended, not silent.

## Attempted refutations that held

- The trait surface is genuinely byte-for-byte unchanged (`crates/traits/src/lib.rs:434-501`):
  the fix keeps leadership/locks as `Copy`, token-only, so the keep-alive-in-the-store design
  is the only way to hold a lease alive without a renewable handle — the invariant holds.
- The `server` composition change (`cli.rs` `run_d_server<Co: Coordination>`) is a genuine
  selection-not-refactor: `DServer`/`dserver` callers are untouched; the default build errors
  loudly on `--coordination-backend etcd` rather than silently falling back
  (`from_config`, tested at `cli.rs:1030`).
- The single-instance shared clauses and the cross-instance *lock* test
  (`a_lock_is_mutually_exclusive_across_two_instances`) reason correctly over etcd's atomic
  compare-and-put; I could not construct a failing input for them.
- The iteration-1 defects (no keep-alive, unconditional unlock, non-re-fencing re-election,
  untested config, single-instance-only suite) are all addressed *in code* — my findings are
  new (the cancelled-campaign leak, Drop-without-revoke, untested renew/revoke) or concern the
  gate's inability to prove any of it.

### Advisory — codex

- `crates/coordination-etcd/src/store.rs:283` — `elect_leader` spawns the lease keep-alive before awaiting the blocking `campaign`; if the future is cancelled while queued behind another leader, the dropped `JoinHandle` does not abort the task, so the lease can be renewed forever without any `LocalState` entry to clean it up. The new cross-instance election test creates exactly this cancellation with `timeout` at `crates/coordination-etcd/tests/conformance.rs:204`, so the stale queued campaign/lease can block or steal the later takeover. Use an owned guard that aborts/revokes on cancellation, or only detach the keep-alive after campaign success.
- NEEDS-HUMAN — `Cargo.toml:101` — the production feature currently depends on `etcd-client = "0.14"`, whose build script requires a system `protoc`; `CARGO_TARGET_DIR=/tmp/pdca-advisory-tiwjhbaf/target cargo check -p wyrd-coordination-etcd --features etcd` failed before compiling this crate with “Could not find `protoc`”. This conflicts with the repo’s existing no-system-protoc posture and means the default green gate does not prove the shipped etcd feature builds in a clean environment.
- NEEDS-HUMAN — `xtask/src/main.rs:56` — there is still no `xtask etcd-conformance`/Tier-2 runner analogous to `tikv-conformance`; the only etcd tests are endpoint-gated and skipped when `WYRD_ETCD_ENDPOINTS` is unset. A human needs to decide whether manual `cargo test -p wyrd-coordination-etcd --features etcd` evidence is sufficient for sign-off, or require an automated real-etcd runner before claiming the success criterion.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C4 Verification (red→green) — Gating C4-ci passed but by construction **never compiles `store.rs`** (etcd feature off), so the crate's correctness is unverified by any gate that ran. Decision owed: run `cargo test -p wyrd-coordination-etcd --features etcd` with `WYRD_ETCD_ENDPOINTS` against a real etcd and confirm GREEN (the networked half of the two-impl pin). The iteration-1 false-green (endpoints set + feature off) is now fixed — it panics (`conformance.rs:1696-1704`). Note: `--features etcd` pulls `etcd-client 0.14` which needs system `protoc` vs the repo's no-system-protoc posture — the human's run environment must provide it.
- [ ] C5 Causal adequacy — Root cause (only one impl / no cross-process L5) is addressed by a real second impl, and the iteration-1 split-brain defects are addressed **in code**: per-hold background keep-alive (`store.rs:1349-1369,1500-1502,1571`), unlock-by-lease-revoke never delete-by-key (`store.rs:1585-1602`), re-fence via proclaim-then-recampaign (`store.rs:1456-1530`). No capability-probe / runtime-guard smell (the `#[cfg(feature=etcd)]` is compile-time composition, not a probe). Decision owed: whether these mechanisms actually deliver single-leader + mutual-exclusion + no-split-brain **under real etcd** — the exact claim iteration 1 was rejected on — is a contested distributed-correctness judgment that rests on unexecuted code; a human must confirm against a live cluster (ties to C4/V).
- [ ] T5 Judgment — Two judgment calls the human must weigh. (1) The iteration-1 requirement "there is no `xtask etcd-conformance` job … must be resolved before the real-etcd GREEN can be earned" is **still unmet** — the patch adds no Tier-2 automation, so the flippable GREEN has no reproducible CI home and lives entirely on a manual invocation; decide whether that is acceptable to sign off or whether the job must land first. (2) Keeping leases alive internally (background task) instead of a renewable-handle trait change is a sound call that preserves the byte-for-byte-trait invariant — confirm it is the intended resolution of iteration-1 fix-point 1 (a trait change was the alternative, itself a NEEDS-HUMAN).
- [ ] Validation — fitness-to-purpose — Whole-purpose validation cannot be mechanized here. Owed at sign-off: (a) run the shared suite + cross-instance clauses against a real etcd and confirm "peers discovered through L5 / single leader / fenced mutual exclusion" actually hold; (b) the **DST-fidelity decision** (`madsim-etcd-client` vs contract harness — the #264/#258 mirror), named unresolved in the brief; (c) **etcd-client dependency review** — ADR-0003 three-test audit, `deny.toml` allowlist, TLS/auth posture, exact version, and the system-`protoc` posture (brief NEEDS-HUMAN); (d) the **sequencing governance** decision (explicit M4 slice vs preceding coordination milestone, 0015 :461-463/:707-709). Prior-art by affected path could not be settled mechanically here (target unreachable) — confirm no closed/rejected `coordination-etcd` work conflicts.
- [ ] `Cargo.toml:101` — the production feature currently depends on `etcd-client = "0.14"`, whose build script requires a system `protoc`; `CARGO_TARGET_DIR=/tmp/pdca-advisory-tiwjhbaf/target cargo check -p wyrd-coordination-etcd --features etcd` failed before compiling this crate with “Could not find `protoc`”. This conflicts with the repo’s existing no-system-protoc posture and means the default green gate does not prove the shipped etcd feature builds in a clean environment.
- [ ] `xtask/src/main.rs:56` — there is still no `xtask etcd-conformance`/Tier-2 runner analogous to `tikv-conformance`; the only etcd tests are endpoint-gated and skipped when `WYRD_ETCD_ENDPOINTS` is unset. A human needs to decide whether manual `cargo test -p wyrd-coordination-etcd --features etcd` evidence is sufficient for sign-off, or require an automated real-etcd runner before claiming the success criterion.

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
- Iteration delta (if iterating): Rejected: this is iteration 2 and the crate's load-bearing correctness is still UNPROVEN — the etcd store (store.rs, the crate's entire reason to exist) is compiled only under the OFF-by-default `etcd` feature, so no gate that ran ever compiled or exercised it; the split-brain paths iteration 1 was rejected on are verified by code reading only. The rebuild must make correctness provable AND address every adversary finding below. Correctness must be proven (not asserted): - The etcd backend must actually be compiled and exercised. Success criterion (b) — shared suite + cross-instance clauses GREEN on REAL etcd (single leader / fenced mutual exclusion / peers discovered via L5) — must be demonstrably earnable, not resting on a manual invocation nothing runs. Land the missing Tier-2 automation (an `xtask etcd-conformance` runner analogous to tikv-conformance) so the real-etcd GREEN has a reproducible home. Adversary items that must be addressed: - Orphaned-campaign lease leak (codex confirms): elect_leader (store.rs:196/283) spawns the detached keep-alive BEFORE campaign(...).await. When the cross-instance election test's 750ms timeout (conformance.rs:204/210) drops the future, etcd has already put B's candidate key bound to the lease and the detached keep-alive renews it forever (dropped future != aborted task; no LeaderHold recorded to clean it up). After drop(a), etcd promotes B's orphaned candidacy, the fresh campaign blocks behind an orphan nothing resigns -> the 40s timeout panics. The single-leader test that IS the crate's raison d'etre cannot pass as written. Fix: owned guard that aborts/revokes on cancellation, or detach the keep-alive only after campaign success. - Drop aborts keep-alives but never revokes leases (store.rs:130-141): a cleanly shut-down custodian holds leadership up to HOLD_TTL_SECS=30 after exit, widening the single-leader gap and making the failover test timing-fragile. Revoke on clean drop. - renew and revoke have zero contract coverage on any backend, yet renew is the registration-renewal path production wires (cli.rs:571). Add contract coverage; a renew defect would silently lapse D-server registrations in production. - Clean-build failure (codex confirmed): the `etcd` feature pulls etcd-client 0.14 whose build script needs system protoc; `cargo check -p wyrd-coordination-etcd --features etcd` fails with "Could not find protoc", conflicting with the repo's no-system-protoc posture. Resolve so the shipped feature builds in a clean env. - Config revision-monotonicity clause not shown to bite (RED panics earlier at the read-back assertion, never reaching r1 > r0); make it demonstrably non-vacuous. - "Rising lease id" coverage was dropped (relaxed to assert_ne!); confirm intended or restore. Human judgment items still owed at the next sign-off (decisions, not code): etcd-client dependency review (ADR-0003 three-test audit, deny.toml allowlist, TLS/auth, version), DST-fidelity decision (madsim-etcd-client vs contract harness), and the sequencing- governance call (explicit M4 slice vs preceding coordination milestone).
- By / date: Eduard Ralph / 2026-07-04

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
