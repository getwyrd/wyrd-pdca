# Adversarial review — issue 365 (coordination-etcd-l5-backend)

Lens: refute the red→green evidence and the reviewer's verdict; find the input that
breaks the etcd backend. Grounded on `$PDCA_TARGET` (branch head 5d87cc4).

## Attack on the evidence — what the gate actually proves

- **NEEDS-HUMAN — The deterministic gate never compiles or runs the production etcd
  implementation.** `crates/coordination-etcd/src/lib.rs` gates the whole store behind
  `#[cfg(feature = "etcd")] mod store;` and the feature is OFF by default
  (`crates/coordination-etcd/Cargo.toml:26` `default = []`). `crates/coordination-etcd/tests/conformance.rs:29`
  returns early with a clean-skip when `WYRD_ETCD_ENDPOINTS` is unset — which it is in
  CI. So `cargo xtask ci` (the only gating row, C4-ci) never touches the 338-line
  `store.rs`. The check-gates C4-verify row concedes this: *"no pre-patch state to
  isolate a RED against."* **Green CI ⇏ the etcd backend works.** Any reviewer claim that
  the red→green demonstrates the etcd path is correct is unwarranted: the etcd path is
  never executed by any automated check.

- **NEEDS-HUMAN — The promised automation to close that gap does not exist.**
  `crates/coordination-etcd/Cargo.toml:20` and `Cargo.toml` (root, `etcd-client` note)
  both point to *"a dedicated `xtask etcd-conformance` job (companion) [that] turns the
  feature on and drives the SHARED suite against a real etcd."* There is **no such job**:
  `grep etcd xtask/` finds only the `deploy`/`tikv` machinery; `xtask/src/main.rs:167`
  has a `tikv-conformance` target but no etcd analogue. The GREEN half therefore depends
  entirely on a human manually running `--features etcd` with an endpoint. This is worse
  than "deferred to sign-off" — there is no runnable target to defer *to*.

- **The RED half is demonstrated against a hand-written stub, never against
  `coordination-etcd`.** `crates/coordination-conformance/tests/demonstrated_red.rs`
  drives the five shared clauses against `BrokenCoordination` and asserts each panics. I
  tried to refute the non-vacuity and could not — each clause genuinely bites (constant
  lease id fails `second.id > first.id`; empty `discover` fails the `assert_eq!`; constant
  token fails every monotonicity check). But this proves *the suite* bites, **not** that
  `coordination-etcd` was ever red-then-green. The RED artifact (stub) and the GREEN
  artifact (real etcd) never meet on the production path inside any gate.

## Attack on the fix — concrete failing cases in `store.rs`

- **NEEDS-HUMAN — Won leaderships and held locks silently lapse after 30 s: no
  keep-alive is ever spawned.** `store.rs:196` (`elect_leader`) and `store.rs:231`
  (`lock`) each `lease_grant(HOLD_TTL_SECS=30, …)` and never renew it. The only
  `keep_alive` in the file is inside `renew()` (`store.rs:129`), the trait method for
  *registration* leases, invoked externally with a `Lease` handle. But `elect_leader`
  returns `Leadership { token }` and `lock` returns `LockGuard { token }`
  (`crates/traits/src/lib.rs:490,498`) — **token only, no lease handle** — so a caller has
  no way to renew leadership or a lock through the seam. Concrete failing case: a custodian
  wins leadership, does no coordination for 30 s, its lease lapses in etcd, and a peer can
  win a *second* leadership — while the original still believes it leads. The comment at
  `store.rs:33` ("kept renewed for the life of the hold in a real deployment") describes
  code that does not exist. The fast conformance suite completes in ≪30 s, so it can never
  observe this. This is a mem-vs-etcd semantic divergence (`coordination-mem` leaderships
  never lapse) that the shared suite is structurally unable to detect.

- **NEEDS-HUMAN — The re-fence path errors instead of re-campaigning once the hold
  lease has lapsed.** `store.rs:173-190`: a repeat `elect_leader` proclaims on the
  `LeaderKey` cached in `state.leaders`. If the 30 s lease from the previous bullet has
  lapsed, etcd has already deleted that leader key, so `proclaim(...).await?` returns an
  error and `elect_leader` yields `Err` rather than transparently re-acquiring. Concrete
  failing case: any caller that re-elects after an idle gap > 30 s gets a spurious error
  where `coordination-mem` returns a fresh rising token.

- **The etcd config path (`set_config`/`get_config`/`config_revision`) has zero test
  coverage even at sign-off.** `run_all` (`crates/coordination-conformance/src/lib.rs:136-147`)
  drives exactly five clauses — register/discover, leases, election, locks, fencing —
  **none of them config.** So `store.rs:294-337` is never exercised by the shared suite on
  *either* backend, and the etcd `config_revision` (max mod-revision over the config
  prefix) is untested against real etcd. This directly contradicts the brief's binding
  condition (a), which names *"config with a monotonic revision"* as a success criterion.
  A reviewer who reads "shared suite green on both backends" as covering config is
  mistaken.

- **The shared suite proves only single-instance semantics; the cross-process
  guarantees that are the crate's entire reason to exist are untested.** Every clause in
  `crates/coordination-conformance/src/lib.rs` operates on **one** `&impl Coordination`.
  `contract_locks_are_mutually_exclusive_and_fenced` (:75) acquires and contends on the
  *same* instance; `contract_election_is_always_granted_and_fenced` (:56) elects twice on
  the *same* instance (exercising the local-state proclaim short-circuit, not a real second
  campaigner). Nothing ever stands up two `EtcdCoordination` instances and checks that one
  process blocks/loses. `coordination-mem` passes the identical suite precisely because it
  is single-process — so the suite cannot distinguish "correct distributed backend" from
  "in-memory backend." The property that justifies the whole crate (cross-process single
  leader / mutual exclusion / peer discovery) is asserted nowhere.

- **`contract_leases_are_unique_and_rising` bakes in an etcd implementation detail as a
  contract.** The clause asserts `second.id > first.id` (`lib.rs:47`), and `store.rs:120`
  returns `lease_id as u64` straight from etcd's `lease_grant`. etcd lease IDs are
  documented as **opaque**; they are only monotone by virtue of the server's internal
  `idGen` within one session, and are `i64` cast to `u64` (a high-bit-set id becomes a
  huge u64). Concrete risk: across an etcd restart, id-generator reseed, or a version whose
  allocation differs, `second.id` can be ≤ `first.id` and this clause flakes/fails — a
  contract asserting more than the trait (opaque id) or etcd's API promises.

## Verdict — where the reviewer likely rationalized

- The check-gates "overall: pass" rests on C4-ci, which — per the two evidence findings
  above — exercises none of the etcd code. Reading C4-verify's "no pre-patch state" as
  benign is the rationalization: it is *because* the flippable regression's green half was
  moved entirely off the automated path. The mem-suite green + `demonstrated_red` green
  prove the trait was refactored without regressing impl #1 and that the suite is
  non-vacuous — real, but a strictly weaker claim than "the etcd second implementation is
  correct," which is what the brief's success criterion actually demands.

## Attempted refutations that held

- Tried to break the shared clauses on `coordination-mem` and the `keyspace`/`fencing`
  unit tests (NUL-delimiter aliasing, class-collision, `prefix_range_end` carry,
  `token_from_revision` non-positive guard): all sound; could not refute.
- Tried to make `demonstrated_red.rs` pass vacuously (a stub that slips through): each of
  the five clauses genuinely panics on `BrokenCoordination`; could not refute.
- Within a single instance and a fast run, the etcd register→discover, try-acquire lock,
  and proclaim-based re-fence logic are internally consistent; the defects above surface
  on the time axis (30 s lapse), the cross-process axis, and the untested-config axis —
  none of which the gate or the suite can reach.
