# Build notes — issue 365 / coordination-etcd-l5-backend (iteration 2)

_Withheld from the reviewer; for the human at sign-off._

## What iteration 1 got rejected for, and what this rebuild changes

The v1 mechanism (a `coordination-etcd` crate + a shared `coordination-conformance`
suite + a `server` composition swap, mirroring `metadata-tikv`/`metadata-conformance`)
was sound and is **kept**. The sign-off rejected the **etcd store's correctness**: as
written it would ship split-brain, and the shared suite exercised neither config nor any
cross-process guarantee. This iteration is a correctness rebuild of `store.rs` plus the
suite gaps. Each of the six carry-forward defects is addressed below with a `path:line`
on the target branch (`feat/m4-production-metadata-backend`; worktree
`$PDCA_WORKTREE`).

### 1. Split-brain / no lease keep-alive — FIXED
`elect_leader` (v1 `store.rs:196`) and `lock` (v1 `store.rs:231`) granted a 30 s lease
and never renewed it, and the trait hands back a `Copy`, token-only `Leadership`
(`crates/traits/src/lib.rs:490`) / `LockGuard` (`:498`) with **no** renew method — so a
caller *cannot* keep the hold alive and it silently lapsed after 30 s.
- New load-light unit `crate::hold::keepalive_interval`
  (`crates/coordination-etcd/src/lib.rs`, `pub mod hold`) computes the renewal cadence
  (`ttl/3`, floored at 1 s → two missed keep-alives of headroom), unit-tested on every
  machine (`renews_at_a_third_of_the_ttl_with_headroom`,
  `a_tiny_ttl_is_floored_at_one_second`).
- `EtcdCoordination::spawn_keepalive` (`crates/coordination-etcd/src/store.rs:130`)
  spawns a background task that renews the lease at that cadence; `elect_leader`
  (`store.rs:237`, retention at `:283`) and `lock` (`store.rs:313`, retention at `:352`)
  each retain the `JoinHandle` in `LocalState` (`LeaderHold`/`LockHold`, `store.rs:71`,
  `:63`) for the life of the hold.
  On `Drop` (`store.rs:159`) every keep-alive is aborted so the leases lapse and a peer
  takes over — the failover a crash would trigger, done cleanly. This mirrors
  `coordination-mem`, which holds a lock in its map until `unlock`
  (`crates/coordination-mem/src/lib.rs:194`); here the *kept-alive lease* is the hold.

### 2. Unconditional unlock — FIXED
v1 `unlock` (`store.rs:288`) deleted the lock key **by key**, unconditionally, so it
could release a newer holder's reacquired lock. New `unlock`
(`crates/coordination-etcd/src/store.rs:366`) never deletes by key: it aborts the
keep-alive and **revokes only our own lease** (`store.rs:380`), which atomically deletes
the key bound to *that* lease and nothing else. If our lease had already lapsed and a peer reacquired
under their lease, revoking our dead lease is a no-op on their hold. Release is
conditional-by-construction.

### 3. Re-fence path returns `Err` after a lapse — FIXED
v1 re-`elect_leader` proclaimed on the cached `LeaderKey` and returned `Err` once the
lease had lapsed (etcd deleted the key). New `elect_leader`
(`crates/coordination-etcd/src/store.rs:230`): if `proclaim` fails, it drops the stale
hold + keep-alive and **falls through to a fresh campaign** (`store.rs:275`), returning
a new rising token — never `Err` where `coordination-mem` hands back a `Leadership`
(`crates/coordination-mem/src/lib.rs:184`). (Re-fence branch at `store.rs:243`.) (With keep-alive retained this path is now
defensive, not the common case.)

### 4. Config untested in the shared suite — FIXED
New shared clause `contract_config_is_revisioned`
(`crates/coordination-conformance/src/lib.rs:155`), added to `run_all` (`:220`). It
pins the **cross-backend** contract (value reads back, overwrite visible, revision
**strictly rises** — not the mem-only `+1`), so it is honoured by *both* backends. It is
driven by `coordination-mem` headlessly (green now) and rejected against a broken stub
in `demonstrated_red.rs:134` (`config_rejects_a_backend_that_never_persists`, green now).
This is binding criterion (a)'s "config with a monotonic revision", now exercised.

### 5. Single-instance-only suite — FIXED (as etcd-specific clauses, correctly)
The shared suite runs each clause against **one** `&impl Coordination`, so it can only
assert single-instance properties — and it *must*, because it also runs against
process-local `coordination-mem` (two mem instances share no state, so a cross-process
assertion would be unsatisfiable for mem and would wrongly fork the contract). The
cross-process guarantees therefore live where they belong — the networked backend's own
tests: `a_lock_is_mutually_exclusive_across_two_instances`
(`crates/coordination-etcd/tests/conformance.rs`) and
`only_one_of_two_instances_leads_at_a_time` stand up **two independent
`EtcdCoordination` instances** and assert one wins / the other is refused (lock) or
blocks then takes over after release (leadership). These are endpoint-gated (real etcd)
— see the NEEDS-HUMAN below.

### 6. Lease-id contract fidelity — FIXED
v1 `contract_leases_are_unique_and_rising` asserted `second.id > first.id`; etcd lease
ids are opaque `i64`, monotone only within a session, so that would flake. Relaxed to
`contract_leases_are_distinct` (`crates/coordination-conformance/src/lib.rs:82`,
`assert_ne!`), which is exactly what the trait promises ("Opaque lease identifier",
`crates/traits/src/lib.rs:484`). `coordination-mem` (rising ids) satisfies distinctness
a fortiori; the broken-stub rejection still bites (`demonstrated_red.rs:98`).

### Bonus: false-green Tier-2 guard — FIXED
v1's etcd `tests/conformance.rs` **silently passed** when `WYRD_ETCD_ENDPOINTS` was set
but the crate was built without `--features etcd` (the store was never compiled). New
`etcd_requested_but_not_built()`
(`crates/coordination-etcd/tests/conformance.rs`, `#[cfg(not(feature = "etcd"))]`)
**panics** in that configuration — asking for etcd on a build that cannot serve it is an
operator error, never a pass. A no-endpoint run still skips cleanly (the default gate
stays green).

## Red → green: what is proven where (headless), and what is NEEDS-HUMAN

Verified through the project runner (`./engine/xtask.sh ci`, i.e. `cargo xtask ci`) —
**all gates green** (fmt/clippy-`-D warnings`/build/test/deny/conformance), plus a
targeted run of the coordination crates:

- `demonstrated_red.rs`: **6/6 green** — every shared clause (now incl. config) REJECTS
  a non-implementing `BrokenCoordination`. This is the RED half of the flippable
  regression, pinned headlessly, driving the *production* suite functions (not a copy).
  It is genuinely load-bearing: were any clause vacuous, its `rejects(...)` assertion
  would fail.
- `coordination-mem` drives the whole shared `run_all` (incl. the new config clause):
  **green** — the lift did not regress impl #1.
- `coordination-etcd` unit tests: **9/9 green** — `keyspace` (5), `fencing` (2), the new
  `hold` (2). These are the load-light production units the store calls.
- `server` `coordination_backend_selects_by_config`: green (default build, mem arm).

**NOT headless-verifiable (irreducibly networked / infra) — surfaced as NEEDS-HUMAN, not
faked:**

1. **`store.rs` does not compile in this environment.** `etcd-client 0.14.1` generates
   its protobufs at build time and needs a system `protoc`, which this worktree (the
   repo's stated no-system-protoc posture) lacks — `cargo check -p wyrd-coordination-etcd
   --features etcd` fails in the `etcd-client` build script. I wrote `store.rs` against
   the **verified** `etcd-client 0.14.1` API by reading its source directly
   (`~/.cargo/.../etcd-client-0.14.1/src/client.rs`, `rpc/lease.rs` — `LeaseKeeper`,
   `LeaseKeepAliveResponse::ttl`; `rpc/election.rs` — `campaign`/`proclaim`/`LeaderKey::rev`),
   but I **cannot claim it compiles**. This is a real dependency/infra decision (brief's
   "etcd client dependency choice" NEEDS-HUMAN): either provision `protoc` in the
   `etcd-conformance` CI job, or pick an etcd client that vendors its protos.
2. **Shared suite GREEN against real etcd** (success criterion (b)) + the two
   cross-instance clauses: need a live etcd. Manual validation:
   `WYRD_ETCD_ENDPOINTS=http://127.0.0.1:2379 cargo test -p wyrd-coordination-etcd
   --features etcd` against a throwaway single-node etcd — expect
   `trait_contract_against_etcd`, `a_lock_is_mutually_exclusive_across_two_instances`,
   and `only_one_of_two_instances_leads_at_a_time` to **run (not skip) and pass**.
3. **The `xtask etcd-conformance` automation does not yet exist.** I deliberately did NOT
   add it: `xtask` lives in the host repo and the `deploy/`+`xtask` surface is #256's
   (slice 5), explicitly out of this crate's scope. The false-green guard (above) makes a
   mis-wired job fail loud rather than silently green; standing up the job (which must
   build `--features etcd` **and** provide `protoc`) is the companion infra work.

I did **not** fabricate a headless stand-in for the networked run (a fake in-memory
"etcd") — that would pass vacuously and drive a copy, not production. The honest split is
exactly the builder guidance: ship the corrected patch, prove the load-light + non-vacuity
halves headlessly, and leave the networked + un-compilable halves as NEEDS-HUMAN.

## Invariants held
- Trait + consumers **byte-for-byte unchanged**: `git diff` touches no `crates/traits`,
  `crates/core`, `crates/custodian`, nor any caller — only the two new crates, the two
  test-driver files, `server` composition, and `Cargo.{toml,lock}`. The `Leadership`/
  `LockGuard` gap (no renew handle) is worked around **inside** the store (retained
  keep-alives), NOT by a silent trait edit; whether the trait should grow a renewable
  handle stays a NEEDS-HUMAN (brief's trait-change note).
- One shared suite, two backends; no etcd-only fork of the *shared* contract (the
  cross-process clauses are honestly etcd-specific, not a fork of a shared clause).
- Fencing tokens monotone across elections and locks (etcd's one global revision,
  `fencing::token_from_revision`, unit-tested); leases expire natively.
- `coordination-mem` stays the process-local/dev backend.

## Alternatives ruled out (with cost)
- **A trait change to expose a renewable/droppable hold** (a `LeadershipGuard` with an
  async `renew`/`Drop`). This is the "clean" removal of the cause, but it edits
  `crates/traits/src/lib.rs:490,498` **and every caller** — `custodian` (M3.3/#141
  leader-elects through the trait), `server`, the mem backend, and the shared suite —
  violating the brief's byte-for-byte-untouched invariant and the trait-change
  NEEDS-HUMAN. Cost: a cross-cutting trait+caller edit spanning ≥4 crates vs. the
  retained-keep-alive fix that is contained to `store.rs`. The internal keep-alive gives
  the *same* correctness (hold survives for its life; releases on unlock/drop) with zero
  caller blast radius, so it is the smaller change that restores the invariant. Surfaced
  as a NEEDS-HUMAN so the board can still choose the trait route later.
- **Bundling `protoc` via `protobuf-src`** to make `--features etcd` build here: adds a
  C++/build-time toolchain dependency to the workspace — precisely the kind of new
  external dependency the brief routes through the ADR-0003 audit / NEEDS-HUMAN, and a
  posture decision I must not make silently. Left to the human.
