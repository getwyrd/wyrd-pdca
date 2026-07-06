# Build notes — issue 365 / coordination-etcd-l5-backend (iteration 6)

## Scope of this iteration

This is iteration 6, built on the iteration-5 tree (applied as baseline, then
fixed). Iterations 1–5 already delivered the net-new `coordination-etcd` crate, the
shared `coordination-conformance` suite, the `server` composition swap, and the
madsim DST tier. The iter-5 sign-off **rejected** on a concrete, recurring axis:
*the real-etcd conformance test did not compile under `--features etcd`* (E0599),
so **criterion (b) — the shared suite GREEN on real etcd — was unearned**. I
addressed every iter-5 item, and in doing so the now-runnable real-etcd suite
exposed a **genuine correctness defect the simulator had masked**, which I also
fixed. Criterion (b) is now demonstrably earned (`cargo xtask etcd-conformance`
exits 0 against a live etcd).

## Iteration-5 carry-forward items — each addressed

1. **E0599: real-etcd conformance did not compile.**
   `crates/coordination-etcd/tests/conformance.rs:104` calls `b.elect_leader(...)`,
   an inherent-looking call that resolves only with the `Coordination` trait in
   scope, but the `#[cfg(feature = "etcd")] fn run` imported only
   `wyrd_coordination_conformance` and `EtcdCoordination`. **Fix:** added
   `use wyrd_traits::Coordination;` inside `run` (conformance.rs:49-55). `wyrd-traits`
   is already a normal (non-dev) dependency of the crate, so it resolves in the
   integration test with no Cargo change. Verified: `cargo test -p
   wyrd-coordination-etcd --features etcd --test conformance --no-run` now compiles.

2. **`xtask etcd-conformance` retry loop misreported a compile error as a bootstrap
   flake and retried it 5×.** **Fix:** `run_etcd_conformance_test`
   (xtask/src/main.rs:325-405) now **separates build from run** — a `cargo test
   --no-run` build step whose failure returns immediately with a
   "failed to COMPILE … not an etcd bootstrap flake" error (no retry), and only the
   actual test *run* is retried with backoff (etcd's port genuinely can lag). A
   non-compiling test can no longer masquerade as transient.

3. **`election_name` reintroduced the hierarchical-prefix-collision class for
   elections.** etcd's election recipe campaigns under `<name>/<lease>` and observes
   the leader by prefix-scanning `<name>/`, so the election *name* is itself a key
   prefix — exactly like `registration_prefix`, which already `encode_segment`s its
   key. `election_name` formatted the raw key, so a candidate for the nested election
   `a/b` (`elect/a/b/<lease>`) surfaces when observing the parent election `a`
   (prefix `elect/a/`) — a cross-election leader bleed. **Fix:** `election_name`
   (keyspace.rs:58-70) now `encode_segment`s the logical key, collapsing it to one
   opaque `/`-free segment. **Regression:** added
   `keyspace::tests::election_prefix_isolates_hierarchical_keys` (keyspace.rs), a
   mirror of `discovery_prefix_isolates_hierarchical_keys`, that models etcd's
   `<name>/<lease>` candidate key and `<name>/` observe prefix. This is the
   headless red→green proof — it runs in the gating `cargo xtask ci` (keyspace has no
   etcd dependency and is compiled on every build).

## The deeper bug the now-runnable real-etcd suite caught (lock never acquired)

With the compile fixed, `cargo xtask etcd-conformance` *ran* for the first time and
**failed** at `contract_locks_are_mutually_exclusive_and_fenced` — a **free** lock
was refused (`c.lock("inode/7")` returned `Ok(None)` on a fresh key).

Root cause (a real-etcd-vs-simulator **fidelity gap**): the lock acquired iff
`Compare::value(key, NotEqual, LOCK_HELD)`, i.e. it phrased the guard as *absent*.
But **real etcd defines a value comparison against a missing key to return `false`
for every operator** ("no value to compare" — `applyCompare` in etcd's txn path),
so the `NotEqual` guard **never fires on a real cluster** and the lock could never be
taken. The madsim simulator (`service.rs:370`) evaluates `value != Some(cmp.value)`
with `value = None`, so it returns `true` for an absent key — **masking the bug**.
This is precisely the "simulator fidelity ≠ real etcd" class every prior reviewer
warned about, and precisely why criterion (b) demands *real* etcd.

**Fix** (store.rs, `lock`): phrase the guard as a **held-test** that reads
identically on both backends — `when value == LOCK_HELD` (held ⇒ no-op ⇒ refuse)
with the `put`-with-lease moved to `or_else` (not held, *including absent* ⇒
acquire); `succeeded()` now means "was already held". The `LOCK_HELD` doc
(store.rs:46-62) is rewritten to explain why the guard tests *held*, never *absent*.
etcd serializes txns by revision, so two racing acquirers can never both see
"not held" — mutual exclusion holds. Kept **one code path** for real etcd and the
simulator (both `Txn`s support only value comparisons, not create-revision compares,
so a create-revision guard was not an option — see the `madsim-etcd-client` `Compare`
API, which exposes only `Compare::value`).

Why not "just use create-revision compare" (the textbook absence idiom)? Because the
madsim `Compare` type (`madsim-etcd-client-0.6.0/src/kv.rs:334-343`) exposes **only**
`value(...)` — no `create_revision`. Using it would fork the code path (real-etcd-only
vs simulator-only), breaking the ADR-0006 "one implementation, two build modes"
discipline the whole crate rests on. The held-test phrasing needs only value
comparison, so it keeps the single path *and* is correct on real etcd.

## Verification (red→green, on the project's own runners)

- **Headless gate proof (election encoding):**
  `keyspace::tests::election_prefix_isolates_hierarchical_keys` is **RED** with the
  raw `election_name` (panic: `observing election "a" must not match a candidate of
  "a/b": N/elect/a/b/0000000000000007`) and **GREEN** with the `encode_segment` fix.
  Runs in `cargo xtask ci` (no etcd/protoc/docker).
- **Criterion (b) — real etcd (the brief's named test):**
  `cargo xtask etcd-conformance` (the project's own Tier-2 runner: brings up
  `deploy/etcd-single-node`, builds `--features etcd`, runs
  `crates/coordination-etcd/tests/conformance.rs`, tears down) — **RED** before the
  lock fix ("a free lock is granted" panic), **GREEN** after: `real etcd passed the
  shared Coordination conformance suite and the cross-instance properties (single
  leader, mutual exclusion, discovery)`, exit 0. Run on this host (protoc 3.21.12 +
  docker present).
- **Deterministic proof (no regression):** `RUSTFLAGS=--cfg madsim cargo test -p
  wyrd-dst --test coordination` — 12 tests green, including the demonstrated-red
  `process_local_store_fails_the_*` panics, `single_leader_is_exclusive_across_two_etcd_instances`,
  and `a_lock_is_mutually_exclusive_across_two_instances`.
- **Impl #1 no regression:** `cargo test -p wyrd-coordination-mem` — 7 green.
- **Shared suite / demonstrated-red crate:** `cargo test -p
  wyrd-coordination-conformance` — 8 green.
- **Composition seam:** `cargo test -p wyrd-server --test backend_selection` — 3 green
  (`coordination_backend_selects_by_config`).
- **Commit-hook readiness:** `cargo fmt --check` clean on touched crates; `cargo
  clippy --all-targets` clean under both default features and `--features etcd`.

## Invariants held

- `crates/traits/src/lib.rs` (the `Coordination` trait), `core`, and `custodian` are
  **not in the diff** — byte-for-byte unchanged. Selection is the `server`-composition
  swap in `crates/server/src/cli.rs` only (ADR-0008/0016; 0015 `:236-240`).
- **One shared contract suite, two backends:** the real-etcd conformance and the
  madsim DST tier drive the *same* `wyrd-coordination-conformance` helpers
  (`run_all` + `cross_instance_*`); no etcd-only fork.
- Fencing tokens rise monotonically across elections and locks (token from etcd mvcc
  revision); the lock fix does not change token derivation (still the txn header
  revision, which a fresh `or_else` put bumps).
- `coordination-mem` remains the process-local/dev backend (untouched semantics).

## Standing NEEDS-HUMAN (surface, do not absorb) — unchanged from prior iterations

- **DST-fidelity acceptance** (the #264/#258 mirror). This iteration is itself a data
  point: the madsim simulator masked a real-etcd-only lock defect (value-compare on a
  missing key), so DST alone is not sufficient — the real-etcd `etcd-conformance` job
  is load-bearing and must run before this backend enters the shipped graph. A human
  should accept this madsim-plus-real-etcd fidelity story.
- **etcd-client 0.14 dependency review** — ADR-0003 three-test audit, `deny.toml`
  allowlist, and the ships-no-TLS/auth `connect(endpoints, None)` posture. The
  `--features etcd` build needs system `protoc` (present on the sign-off host).
- **Sequencing governance** (0015 `:461-463`, `:707-709`): explicit M4 slice vs a
  preceding coordination milestone — board-visible either way; branch base unaffected.

## STOP discipline

Produced `patch.diff`, the named test
(`crates/coordination-etcd/tests/conformance.rs`, plus the deterministic
`crates/dst/tests/coordination.rs`), and this file. No PR marked ready or merged.
