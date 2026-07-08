# Build notes — issue 477 / gateway-cluster-coordinated-id-allocation (iteration 2)

## Context: what changed vs iteration 1

Iteration 1's **approach was accepted** at sign-off: coordination-free gateway id allocation
— shared-CAS inodes (`cli::alloc_inode`) + a per-gateway random 63-bit chunk epoch (ADR-0019).
C4 (`cargo xtask ci`) was green and T5 clear. The iterate was **not** to change the fix but to
**strengthen two tests** before merge. So the production diff (`lib.rs`, `cli.rs`) is unchanged
from the accepted v1; iteration 2 rewrites the tests to close the two carry-forward gaps and
adds direct evidence. The full v1 attempt is preserved in `iteration-v1/`.

The two carry-forward asks (verbatim intent):

1. **Make the active-active regression test genuinely CONCURRENT.** The v1
   `gateway_multi_writer.rs` ran `gw_a.put_object(..).await` *then* `gw_b.put_object(..).await`
   — strictly sequential, so the shared `meta:next_inode` CAS allocator handed out 1 then 2
   **uncontended**; the contended CAS path the fix rests on was never exercised.
2. **Correct `restart_without_recover_is_safe_by_construction`** (`s3_http_wire.rs`). Its name
   over-claimed: for the **migration case `recover` actually exists for** — a store an older
   single-process gateway left with `inode:` keys but **no** persisted `meta:next_inode` — a new
   gateway started *without* `recover` returns id 1 and collides. So "safe by construction" is
   false for that case; either rescope/rename to what it proves, or exercise the
   recover-from-legacy path so the name matches the mechanism.

Explicitly **not** relitigated (per carry-forward): the chunk-id probabilistic guarantee
(per-process 63-bit epoch) is accepted; the collision-detection gap is out of scope and filed
as getwyrd/wyrd#478.

## The production fix (unchanged from accepted v1) — for citation completeness

Target branch `feat/m4-production-metadata-backend`, in `$PDCA_WORKTREE`:

- **Removed** the per-process `next_inode`/`next_chunk` `AtomicU64` counters as the id source
  (`crates/server/src/lib.rs`, struct fields now `chunk_epoch: u64` + `next_chunk_seq: AtomicU64`
  at `lib.rs:75-76`).
- **Inodes — coordinated through the shared store.** `commit_written`'s create branch allocates
  via `crate::cli::alloc_inode(&self.meta)` (`lib.rs:189`) — the CAS-backed `meta:next_inode`
  allocator the CLI cluster path uses (`cluster_store_put`, the cited peer `cli.rs:1158`;
  allocator body `cli.rs:1027`). Two active-active gateways therefore draw **distinct** inodes.
- **Chunk ids — coordination-free (ADR-0019).** `mint_chunk_id` returns
  `(chunk_epoch << 64) | next_chunk_seq` (`lib.rs:217-220`); `chunk_epoch` is a per-gateway
  random 64-bit value (top bit set) from `random_chunk_epoch()` (`lib.rs:236`, OS entropy via
  `std::collections::hash_map::RandomState` — no new crate). Overwrite-safe: `next_chunk_seq`
  is process-monotonic, never reset per object, so a same-inode overwrite mints fresh ids (the
  brief's hard constraint against the CLI's overwrite-unsafe `(inode<<64)|seq`-from-0 minter).
- **`recover()` re-threaded, not deleted** (`lib.rs:116`): seeds the persisted allocator via the
  new `cli::seed_next_inode_floor` (`cli.rs:1069`), preserving #364 finding-1 for restart and
  the legacy-store in-place upgrade. Chunk ids need no recovery.

Type note: `seed_next_inode_floor`/`alloc_inode` return `Result<_, BoxError>` where cli's
`BoxError = Box<dyn Error + Send + Sync>` (implicitly `'static`), which is exactly
`wyrd_traits::Result`'s error, so `recover()`'s `?`/return type-check without conversion.

## Iteration-2 test changes (the substance of this iterate)

### (1) `crates/server/tests/gateway_multi_writer.rs` — now genuinely concurrent

`two_active_gateways_concurrently_store_distinct_objects_without_collision` (`:154`):
- `WRITERS = 8` distinct objects, **split across the two gateways** (even → A, odd → B), each on
  its own `tokio::spawn` task, on a `#[tokio::test(flavor = "multi_thread", worker_threads = 8)]`
  runtime, all released together from a `tokio::sync::Barrier` right before `put_object` — so the
  PUTs **race** on the one shared `meta:next_inode` allocator instead of "A fully completes, then
  B". Asserts every PUT commits, every object round-trips **byte-identical through both gateways**,
  and the shared store holds exactly `WRITERS` **distinct** inode ids (scan `inode:` → `HashSet`).
- New companion `shared_inode_allocator_hands_out_distinct_ids_under_contention` (`:252`): 16
  tasks call the production `cli::alloc_inode` against one shared store, released from a barrier;
  asserts 16 **distinct** ids — the CAS-retry invariant the gateway relies on, exercised directly.

**Evidence the CAS is actually contended (not merely "concurrent in principle").** I temporarily
instrumented `alloc_inode`'s conflict/retry branch with an `eprintln` and ran the concurrent
gateway test 5×: **3–11 CAS-conflict retries fired every run** (38 total). So the two gateways
genuinely collide-and-retry on the shared counter. Instrumentation reverted before shipping
(grep for `SCRATCH` is clean; patch.diff contains no such line). Stress: the test was run **25×
green, 0 flakes** — the fix resolves the contention correctly and the test is not order-fragile.

Why 8 writers + barrier rather than a bare `tokio::join!(a, b)`: `MemMeta`'s async methods never
yield (they lock a `std::Mutex` and return), so `join!` on one task would run A's whole
allocation before polling B — still uncontended. Genuine OS-thread parallelism (`spawn` on a
multi-thread runtime) is required to make the allocations overlap; the barrier + several writers
make the overlap reliable rather than timing-lucky (measured above).

### (2) `crates/server/tests/s3_http_wire.rs` — the over-claiming test, split honestly

- **Rescoped + renamed** `restart_without_recover_is_safe_by_construction` →
  `restart_without_recover_is_safe_when_prior_process_persisted_the_allocator` (`:596`). It now
  states the *condition* under which it holds: process 1 ran this same coordinated code, so it
  **persisted** `meta:next_inode` past A; process 2 then resumes above A even without `recover`.
  The doc explicitly hands the migration case off to the new test below.
- **New** `recover_seeds_the_allocator_over_a_legacy_store_without_meta_next_inode` (`:658`) —
  makes `recover` load-bearing for the case it exists for. It builds the legacy on-disk shape
  faithfully (PUT through the current gateway, which writes `inode:1` **and** advances
  `meta:next_inode`, then **deletes** `meta:next_inode` via `WriteBatch::delete` — the one
  artificial step reproducing what an older single-process gateway left: inodes present, counter
  absent), then asserts BOTH halves:
  - **(1) without `recover`** the migrating new-key PUT re-mints inode 1 and is rejected
    (`Conflict`) — recover *is* load-bearing here; and
  - **(2) with `recover`** (`seed_next_inode_floor` over `high_water_marks`) the allocator is
    seeded above the legacy inode, the PUT commits, and A + B both round-trip byte-identical.
  This is a self-contained red/green over the `recover` call, so the name now matches the
  mechanism and the earlier false generality is gone.
- Refreshed the now-stale rationale comments on `restart_recovers_id_allocators_no_collision` and
  `restart_recovers_id_allocators_over_orphan_ledger_no_reclaim_loss` (chunk-id disjointness is
  by-construction now, not an orphan-ledger counter scan); bodies unchanged, still green.

## Why not add a madsim/DST property (the brief's optional ask)

The brief permits an *additional* DST property but pins the binding red→green to the in-process
test. I did not add a DST leg: `crates/dst` has no `wyrd-server`/`Gateway` edge (the `Gateway`
type isn't in the DST harness), so a DST property would require wiring the gateway into a new DST
target — a materially larger change than the deterministic in-process contention the two new
tests already give (8-way gateway race + 16-way allocator race, CAS-retries measured firing).
Cost of the DST route: a new `dst` target + `wyrd-server` dependency edge in `dst/Cargo.toml`
(a first-ever DST→server edge) + a nemesis harness — versus zero new edges for the in-process
tests. The reviewer's ask was specifically to make *this* test concurrent, which is done.

## Forced refutation (recorded per builder contract)

Binding test = `gateway_multi_writer.rs::two_active_gateways_concurrently_..._without_collision`.

- **(a) Genuine red?** YES. The project verifier `engine/scripts/run-verify.sh` applies
  `patch.diff` to a clean worktree off `feat/m4-production-metadata-backend`, runs the added test
  **green** with the fix, then **reverts `lib.rs`+`cli.rs`, keeps the test**, and re-runs: it goes
  **RED** — `two_active_gateways_concurrently_...` panics at `gateway_multi_writer.rs:205`
  ("every concurrent PUT ... must commit ... : Conflict") — the second gateway re-minting a
  per-process inode the first already committed. `run-verify.sh: PASS — red without the fix,
  green with it.` (The companion allocator test stays green in both, as expected — `alloc_inode`
  is unchanged; the binding red comes from the gateway-level test.)
- **(b) Production path?** YES. The test drives `wyrd_server::Gateway::{new, recover, put_object,
  get_object}` and `wyrd_server::cli::alloc_inode` directly — the exact production symbols the
  patch changes/relies on — not a copy or mock. `MemMeta`/`MemChunks` are only the shared, real
  trait-seam backends (the same Arc-backed stand-ins the sibling gateway tests use); every
  id-minting decision under test is production code.
- **(c) Fixture includes the fault?** YES. The fixture is the colliding condition itself: ONE
  shared `MemMeta` + ONE shared, **read-back-observable** `MemChunks`, both gateways recovered
  from the **same empty baseline** (so any per-process counter seeds identically), and the PUTs
  are launched **concurrently under a barrier** so they actually contend. The byte-identical
  read-backs bind BOTH failure modes (inode-collision PUT rejection; chunk-collision clobber),
  and the `inode:` scan asserts N distinct inodes were handed out under the contention — so the
  test cannot pass if either half of the fix is missing or if the allocator mishandled contention.

## Verification performed (project runner)

- `engine/scripts/run-verify.sh` (the per-fix C4-verify runner): **PASS — red without the fix,
  green with it** (re-run against the final regenerated `patch.diff`).
- `cargo test -p wyrd-server` (full crate, in `$PDCA_WORKTREE`): all binaries green, incl.
  `s3_http_wire` (17), `gateway_multi_writer` (2), `gateway_cluster`, `gateway_lease_expiry`,
  `closed_write_path`, `e2e`, `consistency_observable`, `custodian_day_one`, `dst_*`.
- `cargo fmt --check -p wyrd-server`: clean. `cargo clippy -p wyrd-server --tests -- -D warnings`:
  no warnings. (The whole-tree C4-ci `cargo xtask ci` gate re-runs fmt/clippy/build/test/deny/
  conformance/statics/DST at Check.)
- Concurrency evidence: CAS-conflict retries measured firing 3–11×/run over 5 runs; 25/25 green
  stress.
