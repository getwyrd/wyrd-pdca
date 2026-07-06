# Build notes — issue 365 / coordination-etcd-l5-backend (iteration 5)

## Scope of this iteration

The `coordination-etcd` crate + shared suite were built across iterations 1–4 and
signed off as "materially complete" at iter-4 Check (store genuinely compiled +
driven under madsim; split-brain guard gated demonstrated-red; lease/fencing
refutations stuck). This iteration is a **targeted fix of the three iter-4
carry-forward items**, on top of the iter-4 patch (re-applied cleanly onto the
current target base `5d87cc4` = `origin/feat/m4-production-metadata-backend`).

The three iter-4 blockers, each addressed:

### (1) Criterion (b): single-leader must be checked on REAL etcd, not only the simulator

**iter-4 finding:** the headline single-leader / split-brain property ran ONLY on
the madsim simulator; the real-etcd conformance (`cargo xtask etcd-conformance`)
never ran a second `elect_leader` while the first instance held leadership, so
"single leader … on REAL etcd" (criterion (b)) was never checked there.

**Fix — and it de-forks the suite:** I lifted the three cross-instance clauses
(single leader, mutual exclusion, cross-process discovery) OUT of the dst test and
INTO the shared `wyrd-coordination-conformance` lib as public helpers:
- `cross_instance_single_leader_is_exclusive` (conformance `src/lib.rs`),
- `cross_instance_lock_is_mutually_exclusive`,
- `cross_instance_registration_is_discoverable`.

Both drivers now call the SAME helper — the real-etcd conformance
(`crates/coordination-etcd/tests/conformance.rs`, the `#[cfg(feature = "etcd")] run`)
AND the madsim simulator (`crates/dst/tests/coordination.rs`) — so there is no
fork of the contract (ADR-0006 "one contract, two implementations"; the exact
invariant the brief pins). The real-etcd `run` now drives all three cross-instance
clauses including single-leader (a tokio-`timeout`-bounded B campaign against a live
A), so `cargo xtask etcd-conformance` earns criterion (b).

The single-leader clause is runtime-agnostic: the shared helper takes the two
stores and a caller-supplied `campaign_b_bounded` closure, so mem drives it on
`pollster`, etcd-on-real-tokio uses `tokio::time::timeout`, and etcd-on-madsim uses
`madsim::time::timeout`. This is why the shared lib stays free of any runtime dep
(it must also serve `coordination-mem`, which runs single-threaded on pollster).

The demonstrated-RED for single-leader is preserved: the SAME shared helper run
against two process-local `coordination-mem` instances goes RED (mem grants every
lone process → B's bounded campaign resolves → `assert !is_none` fires "split-brain"
→ `#[should_panic]`), pinning the clause as non-vacuous.

Citations (target branch, post-patch):
- shared helpers: `crates/coordination-conformance/src/lib.rs` (`cross_instance_*`).
- real-etcd single-leader: `crates/coordination-etcd/tests/conformance.rs`
  (`cross_instance_single_leader_is_exclusive(&a, "custodian", …)` in the
  `#[cfg(feature = "etcd")] run`).
- simulator green + demonstrated-red: `crates/dst/tests/coordination.rs`
  (`single_leader_is_exclusive_across_two_etcd_instances`,
  `process_local_store_fails_the_single_leader_clause`).

### (2) Keyspace prefix-collision (`keyspace.rs`)

**iter-4 finding:** registration keys were not escaped / length-delimited, so a
member registered under a nested key `a/b` (etcd key `…reg/a/b/<lease>`) is a prefix
match for the parent's discovery prefix `…reg/a/` — `discover("a")` would leak every
member of `a/b`, `a/b/c`, … (a cross-key discovery bleed).

**Fix — the smallest change that restores the invariant** (per-key discovery
isolation): percent-encode the two characters that break the keyspace — `/` (which
forges the nesting) and `%` (so the encoding stays injective/reversible) — in
`encode_segment`, applied in `registration_member` / `registration_prefix`
(`crates/coordination-etcd/src/keyspace.rs`). The encoded logical key then contains
NO `/`, so the trailing `/` delimiter isolates it exactly: a member of `a/b`
(`…reg/a%2Fb/…`) can no longer prefix-match `discover("a")`'s `…reg/a/`.

Only registration keys are prefix-scanned (`discover`), so only registration is
encoded; lock / election / config keys are exact-match (or, for `config_prefix`,
an intentional whole-config scan) and need no change — keeping the diff to the
actual defect site.

Red→green proof (headless, in `cargo xtask ci`):
`crates/coordination-etcd/src/keyspace.rs::tests::discovery_prefix_isolates_hierarchical_keys`
— **RED** against the raw-key formatting (panics
`discover("a") must not match a member of "a/b": N/reg/a/b/0000000000000001`),
**GREEN** with the encode. Verified both directions through `cargo test -p
wyrd-coordination-etcd --lib` (see §Verification).

### (3) Transient proclaim-error anti-churn path had NO test

**iter-4 finding:** a proclaim RPC error while the lease is still live must return
`Err` yet RETAIN the hold and its renewing lease (`store.rs`'s "still leading"
re-proclaim path) — the anti-churn / anti-lease-leak fix from iters 1–3 — but no
test exercised it, so a dropped-hold regression could go undetected.

**Fix:** added `a_transient_proclaim_error_keeps_the_hold_and_its_lease`
(`crates/dst/tests/coordination.rs`). It injects the fault deterministically on the
simulator: A wins leadership, then an admin client DELETES A's leader key WITHOUT
revoking A's lease. A's keep-alive keeps renewing the lease (`is_lost()` stays
false), so the next `elect_leader` takes the "still leading" re-proclaim path — which
now errors, because the leader key is gone. The test asserts:
- `elect_leader(...).is_err()` — the store surfaces the transient error rather than
  swallowing it into a silent re-campaign;
- the lease is still alive 8 s later (> `HOLD_TTL_SECS = 6`) — the hold was retained
  and the keep-alive kept renewing, not aborted.

Non-vacuity demonstrated: mutating the store's "still leading" path to infer loss
from the proclaim error and silently re-campaign (return `Ok`) turns the test RED
(`a proclaim error on a still-live lease must surface as Err, not a silent
re-campaign`). Restored after the check (see §Verification).

## Verification (through the project runner, headless — no protoc, no live etcd)

- `cargo test -p wyrd-coordination-etcd --lib` — 9 unit tests pass, incl. the new
  `discovery_prefix_isolates_hierarchical_keys`. Demonstrated RED by reverting
  `encode_segment` (panic quoted above), then restored → GREEN.
- `./engine/xtask.sh dst` (i.e. `cargo test -p wyrd-dst` under `--cfg madsim`) —
  the 12 `tests/coordination.rs` tests pass, including the new transient test and
  the three cross-instance tests now driving the shared helpers, plus the three
  `#[should_panic]` demonstrated-red clauses. Transient test demonstrated RED under
  the "infer-loss" store mutation, then restored → GREEN.
- `cargo test -p wyrd-coordination-mem -p wyrd-coordination-conformance` — impl #1
  (mem) still passes the shared suite; the conformance-crate demonstrated-red still
  passes. **No regression to impl #1** (the brief invariant).
- `cargo fmt … --check` clean; `cargo clippy … --all-targets` clean in BOTH the
  default (feature-off) and `--cfg madsim` configs — commit-hook ready.

### What is NOT headless-verifiable here (honest gap → NEEDS-HUMAN at sign-off)

The `--features etcd` real-etcd conformance path cannot be compiled/run in this
worktree: `etcd-client 0.14` regenerates its protobufs at build time and needs a
system `protoc`, which this environment lacks (and the repo holds a no-system-protoc
posture). So criterion (b)'s literal "GREEN on REAL etcd" is earnable only via
`cargo xtask etcd-conformance` (docker + protoc, off-CI). That job now fails-loud
when docker/protoc are missing (no false-green — `xtask/src/main.rs`
`run_etcd_conformance`, unchanged this iteration), and the real-etcd `run` now drives
the single-leader clause. The DETERMINISTIC, always-runnable proof of the SAME
production store code (the store compiles two ways from one source; `--cfg madsim`
aliases `etcd-client` → `madsim-etcd-client`) is the dst tier, which IS in `ci` and
which I ran green above. The real-etcd wiring change (adding the shared single-leader
call to the `#[cfg(feature = "etcd")] run`) is a thin call into a helper that the
madsim tier exercises and proves; a human with docker+protoc should run `cargo xtask
etcd-conformance` at sign-off to witness the literal real-etcd green.

## Standing NEEDS-HUMAN (unchanged; not silently absorbed)

- **DST fidelity** for an etcd backend (madsim-etcd-client vs a contract harness —
  the #264/#258 mirror). The simulator green rests on madsim-etcd-client fidelity.
- **etcd-client 0.14 dependency review** before it enters the shipped graph: the
  ADR-0003 three-test audit, `deny.toml` allowlist, and the ships-no-TLS/auth
  `connect(endpoints, None)` posture.
- **Sequencing governance** (explicit M4 slice vs a preceding coordination
  milestone) — board-visible either way; the branch base does not depend on it.

## Alternatives considered

- **Inlining the single-leader assertion directly in the real-etcd conformance**
  (instead of lifting to the shared lib): rejected — it would re-state the split-brain
  assertion in a second place, exactly the "no etcd-only fork of the contract"
  invariant the brief forbids. Lifting to the shared lib costs one runtime-agnostic
  closure parameter and de-forks all three cross-instance clauses (the dst test lost
  ~70 lines of local helper it now imports).
- **Encoding lock/election/config keys too** (for symmetry): rejected — they are
  exact-match (or an intentional whole-config prefix scan), so they carry no
  prefix-collision, and encoding them would change on-cluster key bytes for no
  correctness gain. The fix stays at the one defect site (registration discovery).
- **Making the transient test error the proclaim via a lease revoke** (as the lapse
  test does): rejected — a revoke would also flip `is_lost()` true, routing the store
  to the fresh-campaign path instead of the re-proclaim path under test. Deleting
  ONLY the leader key keeps the lease (and `is_lost()==false`) so the store takes the
  exact "still leading" re-proclaim branch the guard protects.
