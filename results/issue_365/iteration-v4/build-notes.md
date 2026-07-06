# Build notes — issue 365 / coordination-etcd-l5-backend (iteration 4)

_Withheld from the reviewer; for the human at sign-off._

## What this iteration is

Iteration 3's **architecture was accepted** ("the iteration-2 'store never compiled by
any gate' blocker is genuinely closed — store.rs compiled+driven under `cfg(madsim)` in
the dst tier") but it was **rejected on test-adequacy + two store-correctness defects**.
This iteration keeps the accepted architecture (one `store.rs`, driven on the
`madsim-etcd-client` simulator in the ci-gated `dst` tier AND on real etcd via the
Tier-2 job) and addresses **every** iteration-3 finding. It is built on top of the
iteration-3 tree; the diff below is against the M4 integration base
(`feat/m4-production-metadata-backend`).

Target branch code facts cited against the worktree
`$PDCA_WORKTREE = /home/eddie/wyrd/wyrd.pdca-wt-l0`.

## The five iteration-3 findings, each addressed

### 1. Single-leader test could not catch split-brain (the class that got iters 1&2 rejected)

Iter-3's `only_one_of_two_instances_leads_then_hands_off` asserted only
`lead_b.token > lead_a.token`, which two *sequential* campaigns satisfy even under a
concurrent (split-brain) grant.

Fixed at `crates/dst/tests/coordination.rs:225-268`: B campaigns on its own node and sets
a shared `AtomicBool b_won` **only when its campaign actually resolves**. Before A drops,
the test asserts `!b_won` (`:254-257`) — a store that granted A and B concurrently would
flip it early and fail. Then A drops and B must win with a fencing term. So the
single-custodian-leader property is now verified by a gated test that a concurrent grant
fails.

### 2. Cross-instance clauses had no demonstrated-red

The cross-instance properties are now written as **helpers over `&impl Coordination`**
(`coordination.rs:117-179`: `cross_instance_single_leader_is_exclusive`,
`cross_instance_lock_is_mutually_exclusive`,
`cross_instance_registration_is_discoverable`). The **same helper** runs:

- GREEN against two networked etcd instances on the simulator
  (`:270-305`), and
- RED (`#[should_panic]`) against two independent process-local `coordination-mem`
  instances, which share no state (`:313-337`:
  `process_local_store_fails_the_single_leader_clause`,
  `…_mutual_exclusion_clause`, `…_cross_process_discovery_clause`).

That pins the clauses non-vacuous: their green rests on real cross-process coordination,
not simulator fidelity alone. I additionally re-verified the mutual-exclusion clause bites
on the **real store** (broke `store.rs`'s lock txn to `if false && !resp.succeeded()` →
`a_lock_is_mutually_exclusive_across_two_instances` went RED "the lock A holds must refuse
B across instances"; reverted → green).

### 3. `xtask etcd-conformance` returned a false green when tooling was missing

Iter-3 warned-and-returned `Ok(())` locally when docker/protoc were absent (exit 0 having
proved nothing). Fixed at `xtask/src/main.rs:282-315`: **missing docker or protoc now
always `Err`** (locally as well as in CI) — the job can never exit 0 without actually
running the real-etcd suite. The deterministic, always-runnable proof of the same store
is the `dst` tier (in `ci`); this job is the real-etcd fidelity backstop and is invoked
deliberately, not from `ci`. (Producing the real-etcd green in a lane with docker+protoc,
and the etcd-client dependency audit, remain NEEDS-HUMAN — see below.)

### 4. `config_revision` semantics diverged across backends

Iter-3's etcd `config_revision` returned the etcd **header** revision — the cluster-global
mvcc counter bumped by *every* write — so a config watcher would wake on unrelated
coordination traffic, and the shared clause (`r1 > r0`) could not catch it.

- **Normalized** the etcd semantics to config-only advancement
  (`crates/coordination-etcd/src/store.rs:388-410`): `config_revision` now returns the
  **max `mod_revision` over the config keyspace**, which advances iff a config key is
  written — matching mem's config-scoped counter (differing only in absolute values).
- **Tightened** the shared clause (`crates/coordination-conformance/src/lib.rs:211-227`):
  after the `r1 > r0`/`r2 > r1` checks, an **unrelated `register` must NOT advance the
  config revision**. Satisfiable by both backends; a global-write-counter fails it.
- **Demonstrated-red** for it (`crates/coordination-conformance/tests/demonstrated_red.rs`:
  `ConfigCountsEveryWrite` + `config_catches_a_global_write_counter_revision`): a store
  that persists values correctly and rises monotonically on config writes but *also*
  counts an unrelated `register` — it passes read-back and monotonicity, then trips the
  new config-only assertion (`expected = "must NOT advance the config revision"`).

### 5. Re-election treated every proclaim `Err` as lease-expiry (churn + lease leak)

Root cause removed, not guarded. Loss is now concluded **only** from an authoritative
signal, never inferred from a proclaim RPC error:

- The keep-alive task sets a shared `lost: Arc<AtomicBool>` the instant it observes the
  lease genuinely gone (renewal refused / stream closed / TTL 0)
  (`store.rs:63-90, 95-145`).
- The re-election path (`store.rs:243-289`) decides via `keepalive.is_lost()`:
  - not lost → re-proclaim for a fresh term; **a proclaim error is propagated
    (transient) with the hold intact** — no `stop_without_revoke`, no re-campaign;
  - lost → drop the dead hold and campaign fresh (a lapsed leader re-earns a term).

So a transient blip can never churn a still-valid leadership or leak a lease behind an
orphaned re-campaign. Tested by
`a_lapsed_leader_recampaigns_after_its_lease_is_lost` (`coordination.rs:410-468`), which
**deterministically** revokes A's leadership lease via an admin client (found through the
election prefix's `kv.lease()`) — a connection-stable stand-in for "partitioned past its
TTL", chosen after a clog-based version proved flaky across seeds (a 9s partition resets
the sim's h2 connection unpredictably, yielding non-deterministic `session expired`). The
keep-alive records the loss within its 2s renewal period; A then re-campaigns and wins a
fenced term. **Load-bearing, verified**: reverting the `is_lost()` check turns this RED on
all 50 seeds with `ElectError("session expired")` (A proclaims on its deleted key, which
the fixed store correctly propagates as a transient error).

## Red → green, through the project runner

- `cargo run -p xtask -- dst` (the `dst` tier, part of `ci`; 50-seed sweep): **green** —
  `crates/dst/tests/coordination.rs` 11/11 (shared suite on the simulator; split-brain
  guard + handoff; cross-instance exclusivity/mutual-exclusion/discovery green; the 3
  process-local demonstrated-red should_panics; lease expiry; cancelled-campaign
  orphan-safety; lapsed-leader re-campaign). concurrency/custodian/network dst files also
  green — no regression.
- `cargo test -p wyrd-coordination-conformance`: 8/8 (7 demonstrated-red should_panics
  incl. the new config-only one, + the `Good` control).
- `cargo test -p wyrd-coordination-mem`: shared `run_all` (incl. the new config-only
  clause) + mem-specific clauses green — no regression to impl #1.
- `cargo test -p wyrd-coordination-etcd` (default, feature-off): pure keyspace/fencing/hold
  units green.
- `cargo test -p wyrd-server --test backend_selection`:
  `coordination_backend_selects_by_config` green.
- `cargo fmt --all --check` and `cargo clippy … -D warnings` (feature-off AND under
  `--cfg madsim`): clean.

### Load-bearing regressions demonstrated on the fix itself (not just a stub)

- Break `store.rs` lock txn (`if false && !resp.succeeded()`) →
  `a_lock_is_mutually_exclusive_across_two_instances` RED; revert → green.
- Remove the `is_lost()` gate → `a_lapsed_leader_recampaigns_after_its_lease_is_lost` RED
  on all 50 seeds; revert → green.

## Invariants held

- **Trait + consumers byte-for-byte unchanged.** No edit to `crates/traits`, `crates/core`,
  `crates/custodian`, `coordination-mem/src/lib.rs`, or any gateway caller. Backend
  selection stays a `server`-composition swap (`server/src/cli.rs`). The
  `Leadership`/`LockGuard` "no renewable handle" gap is worked around **inside** the store
  (retained keep-alives), not by a silent trait edit — whether the trait should grow a
  renewable/resign handle stays a NEEDS-HUMAN.
- **One shared contract suite, two backends** — `coordination-conformance::run_all`, driven
  by both mem and etcd; no etcd-only fork. Cross-process guarantees (unstatable for two
  process-local instances) live in the two-instance etcd tests, with demonstrated-red
  proving they are non-vacuous.
- **Fencing tokens monotone; leases expire** — proven deterministically on the simulator.
- **coordination-mem stays the process-local/dev backend** (unchanged).

## A note the human should see (not caused by this change)

`cargo run -p xtask -- ci` currently fails at the workspace-test step on
`wyrd-custodian`'s `an_aborted_repair_is_not_counted_as_a_successful_repair`
(`crates/custodian/tests/reconstruction.rs:1501`). This is a **pre-existing** flaky test:
it fails identically on the **pristine base tree with all my changes stashed** when the
suite runs in parallel, and passes in isolation / single-threaded. It is a global
OpenTelemetry/metrics-registry cross-test contamination in `custodian` (a crate this issue
leaves byte-for-byte untouched and is out of scope). Fixing it would violate the scope
invariant; flagged here rather than absorbed. Every other workspace test and the full
`dst` tier are green.

## Alternatives ruled out (with cost)

- **A trait change to expose a renewable/resignable hold** (`LeadershipGuard` with async
  `renew`/`resign`). Cleanest "remove the cause," but edits `crates/traits/src/lib.rs:490,
  498` AND every caller (`custodian`, `server`, mem, the shared suite) — ≥4 crates,
  violating the byte-for-byte-untouched invariant and the trait-change NEEDS-HUMAN. The
  retained keep-alive + authoritative `lost` flag gives the same correctness (hold survives
  its life; releases on unlock/drop/cancel; loss is authoritative) with zero caller blast
  radius. Surfaced as NEEDS-HUMAN so the board can still choose the trait route.
- **A GET-confirm on every proclaim error** (get the leader key; absent ⇒ re-campaign).
  Would recover from genuine loss one round-trip sooner, but re-introduces "act on a
  proclaim error" and adds a GET to the proclaim-error path. The `lost`-flag design is
  strictly simpler and already correct: a transient proclaim error is propagated (caller
  retries), and by the retry the keep-alive has set `lost` → re-campaign. Cost avoided:
  an extra GET + a second error-classification site.
- **Clog-based lease-loss test** (partition A past TTL). Rejected after measuring: flaky
  across the 50-seed sweep (1/50-ish seeds failed with `session expired`) because a >6s
  partition resets the simulated h2 connection nondeterministically. The admin-revoke
  variant drives the exact same production path (keep-alive detects loss → re-campaign)
  deterministically on every seed.
- **Bundling `protoc` via `protobuf-src`** to make `--features etcd` build here: adds a
  C++/build-time toolchain — the exact new-dependency posture routed through the ADR-0003
  audit / NEEDS-HUMAN. The madsim path needs no protoc, so correctness is proven without
  that call.

## NEEDS-HUMAN (surfaced, not absorbed)

- **etcd client dependency review** (ADR-0003 three-test audit + `deny.toml` allowlist +
  TLS/auth posture) for `etcd-client 0.14` (real, feature-off) and `madsim-etcd-client 0.6`
  (dev/DST only). `cargo deny check` passes, but that is not the audit.
- **DST-fidelity decision — chosen: `madsim-etcd-client`.** The #264/#258 mirror. It is
  deterministic, in-CI, and drives production code; the human confirms it as the accepted
  DST story (the real-etcd `etcd-conformance` job is the fidelity backstop).
- **Real-etcd green must be produced in a lane with docker+protoc** before this backend
  enters the shipped graph. The automation is now honest (fails loud without tooling); the
  CI image for that lane must provide docker + `protoc`.
- **Sequencing governance** (0015 `:461-463`, `:707-709`): explicit M4 slice vs a preceding
  coordination milestone — a board decision; the branch base does not depend on it.
- **Pre-existing flaky `wyrd-custodian` metrics test** (see the note above): out of scope
  here, but the human/board should track it since it currently reddens `xtask ci` in
  parallel independent of this change.
