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
