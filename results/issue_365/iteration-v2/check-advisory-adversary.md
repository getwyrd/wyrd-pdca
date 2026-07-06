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
