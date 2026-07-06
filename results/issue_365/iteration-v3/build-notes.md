# Build notes — issue 365 / coordination-etcd-l5-backend (iteration 3)

_Withheld from the reviewer; for the human at sign-off._

## The one decision that unblocks this (and why iterations 1–2 were rejected)

Iterations 1 and 2 shipped a plausible `coordination-etcd` crate but were rejected
for the SAME root cause both times: **the etcd store was compiled only under an
OFF-by-default `etcd` feature that no gate ever turned on, so its load-bearing
correctness — single-leader election, fencing, mutual exclusion, no split-brain —
was verified by code-reading only.** The adversary refuted the *fix*, not just the
evidence (v1 would ship split-brain; v2 had an orphaned-campaign lease leak that
would deadlock the very single-leader test that is the crate's reason to exist).

This iteration makes the store **actually compiled and exercised, deterministically,
inside a gate that already runs in `ci`** — using the DST-fidelity option the brief's
own NEEDS-HUMAN names (`madsim-etcd-client` vs a contract harness). Concretely:

- Under `--cfg madsim` the crate's `etcd-client` dependency **aliases to
  `madsim-etcd-client`** (the same aliasing the project already uses for
  `tonic` → `madsim-tonic`). `madsim-etcd-client` is an in-simulator etcd that
  faithfully models **lease tick-expiry**, **min-create-revision leader election**,
  and the **mvcc revision** the fencing tokens ride (I read its `service.rs` to
  confirm — `tick()` at `service.rs:465` expires leases in simulated time and deletes
  their keys; `campaign()` blocks until you are the min-create-revision candidate).
- So `crates/dst/tests/coordination.rs` drives the **same production `store.rs`** the
  real-etcd job drives — not a copy — over the simulator, and `cargo xtask dst` (which
  is called by `run_ci`, xtask/src/main.rs:823) compiles and runs it across the
  50-seed sweep. **No `protoc`, no live etcd, no Docker** — and it is in CI.

This resolves the standing rejection: correctness is now *proven*, not asserted, and
it has a reproducible home. The real `etcd-client` path (`--features etcd`) is the
production wire, kept OFF by default (it needs `protoc` + a live etcd), and driven by
the new `cargo xtask etcd-conformance` job — exactly the `metadata-tikv` posture.

## Why the simulator proof is honest (not a fabricated stand-in)

The builder guidance forbids a mocked/parallel re-implementation that passes
vacuously. This is neither:

- It runs the **production `store.rs`** verbatim (one source, two dependency aliases);
  the simulator is the *dependency*, not a reimplementation of my logic.
- I demonstrated it is **load-bearing on the fix itself**, not just on a stub: I broke
  `lock`'s mutual exclusion (`if false && !resp.succeeded()`) and
  `a_lock_is_mutually_exclusive_across_two_instances` went RED
  ("the lock A holds refuses B across instances", coordination.rs:161); reverting
  turned it GREEN. So the DST clauses catch a real correctness regression in the
  store, headlessly.
- The single-node real-etcd job still exists as the fidelity backstop (a human runs it
  at sign-off / in the heavier CI lane).

## The store's correctness, and how each v2 finding is addressed

`store.rs` uses only primitives available on BOTH real etcd and the simulator: `kv`
(put/get/txn with **value** compares — the simulator's `Txn` has no create-revision
compare), `lease` (grant/keep_alive/revoke/time_to_live), and `election`
(campaign/proclaim). One code path.

1. **Orphaned-campaign lease leak (the v2 rejection).** The keep-alive is a
   `KeepAlive` guard (`store.rs:63`) that lives on the `elect_leader` future's stack
   until the campaign wins; only then is it moved into `LeaderHold`. If the campaign
   future is **cancelled** mid-`await`, the guard drops, signals its task, and the task
   **revokes the lease** — deleting the speculative candidate key at once. Proven by
   `a_cancelled_campaign_leaks_no_orphan` (coordination.rs): B's `elect_leader` is
   dropped by a `timeout`, and a THIRD instance C still wins after A releases (it does
   not deadlock behind an orphan). A detached-keepalive break makes this test hang→fail.

2. **Revoke on clean drop (not a 30s TTL wait).** `KeepAlive::Drop` (`store.rs:81`)
   signals a graceful revoke, so leadership/locks release promptly. `HOLD_TTL_SECS` is
   6s (not 30), so even a *lost* revoke self-heals fast. Proven by
   `only_one_of_two_instances_leads_then_hands_off`: dropping A hands leadership to B
   **immediately** (no time advance), with B's term fencing A's.

3. **Unconditional unlock (the v1 rejection).** `unlock` (`store.rs`) never deletes by
   key; it revokes only OUR lease, which atomically deletes only the key bound to that
   lease — so it can never release a newer holder's reacquired lock.

4. **Re-fence path after a lapse.** A repeat `elect_leader` on a key we already lead
   **re-proclaims** (a fresh, higher revision token); if proclaim fails (our lease
   lapsed) it drops the stale hold and campaigns fresh — it never returns `Err` where
   mem returns a `Leadership`. Covered by `contract_election_is_granted_and_fenced`
   (which now runs on the etcd store via the simulator).

5. **Config untested / clause didn't bite.** New shared clause
   `contract_config_is_revisioned` asserts revision monotonicity RELATIVELY (`r1 > r0`),
   so it holds for both mem's per-write counter and etcd's cluster-global mvcc revision.
   Non-vacuity is pinned by `FrozenConfigRevision` in demonstrated_red.rs, which
   **persists values correctly (read-back passes) but freezes the revision** — so the
   clause reaches `r1 > r0` and fails THERE (the exact "make it bite past read-back"
   the v2 review asked for).

6. **`renew`/`revoke` had zero coverage.** New shared clause
   `contract_renew_and_revoke` (register → renew live → revoke → gone → renew-after-
   revoke errors) runs on both backends; `NeverRevokes` proves it bites. `renew`
   handles the real-vs-sim difference honestly: it inspects the keep-alive response TTL
   (`store.rs`), so an expired lease is a renew error on both.

7. **"Rising lease id" relaxation.** The shared clause asserts **distinctness**
   (`assert_ne!`), which is exactly what the trait/etcd promise (opaque `i64` ids,
   monotone only within a session). mem's *additional* rising-id property is pinned in
   mem's own conformance (`lease_ids_rise_within_the_process`), not forced onto etcd.
   `DuplicateLeaseIds` proves the shared clause bites.

8. **Missing Tier-2 automation + false-green.** `cargo xtask etcd-conformance` now
   exists (xtask/src/main.rs), mirroring `tikv-conformance`; it checks for **both**
   Docker and `protoc` (fail-loud in CI, warn-skip locally) and drives
   `--features etcd` against `deploy/etcd-single-node`. The real-etcd test PANICS if
   `WYRD_ETCD_ENDPOINTS` is set but the crate was built without `--features etcd`
   (coordination-etcd/tests/conformance.rs) — a mis-wired job can never report false
   green. `protoc` is required only by this dedicated job, not by `ci`.

## One shared suite, two backends (no fork)

`crates/coordination-conformance` holds the trait-generic clauses (lifted out of
`coordination-mem/tests/conformance.rs`) plus a `run_all` runner. Both backends call
`run_all`: mem via `pollster::block_on`, etcd via the madsim runtime (and via a real
tokio runtime in the real-etcd job). The cross-process guarantees (single leader across
instances, mutual exclusion across instances, cross-process discovery, deterministic
lease expiry) are NOT shoehorned into the single-`&impl` shared clauses (they are
unsatisfiable for two process-local mem instances) — they live in the etcd backend's
own two-instance tests. That is the honest split, not an etcd-only fork of a shared
clause.

## Invariants held

- **Trait + consumers byte-for-byte unchanged.** The patch touches no `crates/traits`,
  `crates/core`, `crates/custodian`, nor `coordination-mem/src/lib.rs`, nor any gateway
  caller. Selection is a `server`-composition swap (`cli.rs`: `CoordinationBackend`
  enum + a generic `run_d_server`, cfg-gated etcd arm), mirroring `MetadataBackend`.
  The `Leadership`/`LockGuard` "no renew handle" gap is worked around INSIDE the store
  (retained keep-alives), not by a silent trait edit — whether the trait should grow a
  renewable/resign handle stays a NEEDS-HUMAN (see below).
- **Fencing tokens monotone across elections and locks** (etcd's one global mvcc
  revision; `fencing::token_from_revision`, unit-tested); leases expire natively
  (proven deterministically on the simulator's ticked lease clock).
- **coordination-mem stays the process-local/dev backend** (unchanged; its full suite
  still green — no regression to impl #1).
- **ADR-0035:** `coordination-etcd` is now DST-reachable, so I added it to
  `STATICS_SCAN_CRATES` (xtask). It holds no process-global mutable state (only a
  `Mutex` field), so the scan passes.

## Red → green, verified through the project runner

- `cargo run -p xtask -- ci` — **all gates green** (fmt, clippy `-D warnings`, build,
  test, cargo-machete, cargo-deny, conformance, statics, orchestrator-guard, AND the
  madsim `dst` tier — which now compiles+runs the etcd store).
- `crates/dst/tests/coordination.rs` — 6/6 green across a 50-seed sweep: shared suite
  on the simulator, single-leader + handoff, cross-instance mutual exclusion,
  cross-instance discovery, deterministic lease expiry, and cancelled-campaign
  orphan-safety. Demonstrated RED on the fix itself (broke mutual exclusion → red;
  reverted → green).
- `coordination-conformance` demonstrated_red — 6 should-panic clauses + 1 control,
  all green (every shared clause rejects a targeted violating store).
- `coordination-etcd` unit tests — keyspace/fencing/hold pure units green on the
  default feature-off build.
- `server` `coordination_backend_selects_by_config` green.
- Patch re-verified to `git apply --check` cleanly against the base tree.

## Alternatives ruled out (with cost)

- **A trait change to expose a renewable/resignable hold** (`LeadershipGuard` with
  async `renew`/`resign`). Cleanest "remove the cause", but it edits
  `crates/traits/src/lib.rs:490,498` AND every caller (`custodian` M3.3/#141, `server`,
  the mem backend, the shared suite) — ≥4 crates, violating the brief's
  byte-for-byte-untouched invariant and the trait-change NEEDS-HUMAN. The
  retained-keep-alive gives the SAME correctness (hold survives its life; releases on
  unlock/drop/cancel) with zero caller blast radius. Surfaced as a NEEDS-HUMAN so the
  board can still choose the trait route.
- **A create-revision-compare lock** (the textbook etcd lock). The simulator's `Txn`
  supports only value compares, so a create-revision lock would fork the code path
  (real-only) and the simulator proof would evaporate. The value-compare absent-check
  (`NotEqual(LOCK_HELD)` over an invariant single value) is exact on BOTH and keeps one
  path — cost: one documented invariant (`LOCK_HELD` is the key's only value) vs. a
  forked, unprovable lock.
- **Bundling `protoc` via `protobuf-src`** to make `--features etcd` build in this env:
  adds a C++/build-time toolchain to the workspace — the exact new-dependency posture
  the brief routes through the ADR-0003 audit / NEEDS-HUMAN. The madsim path needs no
  protoc, so I don't need to make that call to prove correctness. Left to the human.

## NEEDS-HUMAN (surfaced, not absorbed)

- **etcd client dependency review (ADR-0003 three-test audit + `deny.toml` +
  TLS/auth).** `cargo deny check` passes (licenses already allow-listed), but that is
  NOT the audit. New crates entering the graph: `etcd-client 0.14` (real, feature-off)
  and `madsim-etcd-client 0.6` (dev/DST only). The version pin, TLS/auth posture, and
  supply-chain audit are the human's call.
- **DST-fidelity decision — I chose `madsim-etcd-client`.** This IS the #264/#258
  mirror decision the brief flagged. It is deterministic, in-CI, and drives production
  code, but the human should confirm the simulator's fidelity is accepted as the DST
  story (the real-etcd `etcd-conformance` job is the fidelity backstop). No trait change
  was needed.
- **Sequencing governance (0015 :461-463, :707-709):** explicit M4 slice vs a preceding
  coordination milestone — a board decision; the branch base does not depend on it.
- **`protoc` in the `etcd-conformance` CI job:** the job now fail-loud-requires it; the
  CI image for that lane must provide it (documented in the job + crate Cargo.toml).
