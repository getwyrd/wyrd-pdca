# Build notes — issue 477 / gateway-cluster-coordinated-id-allocation

## What the invariant demanded (and why a guard can't satisfy it)

The brief names an **Invariant to restore**: every id the gateway mints — inode and every
chunk id — must be globally unique across all concurrently-active gateway processes over one
fleet (and across a single process's restarts). The brief's SELF-TEST is explicit that a
one-call-site guard cannot satisfy this: the property is over *all* id-minting across
*separate* processes seeded from the *same* persisted baseline, so the only fix is to make
allocation **coordinated through the shared store** or **coordination-free by construction**.
So this is not a "smallest diff / guard the symptom" call — the target is the smallest change
that restores the invariant. The change removes the per-process `AtomicU64` counters
(`lib.rs:67-68` on the target) as the id source and re-threads both the inode and chunk paths.

Two distinct collisions both had to close (both are demonstrated RED by the new test — see
below): with two gateways seeded from the same empty baseline,

* both per-process **inode** counters mint inode 1, so the second gateway's `commit_create`
  fails `require_absent(inode:1)` with a bogus `Conflict` and its new-key PUT is rejected; and
* both per-process **chunk** counters mint chunk id 1, so the second gateway's fragment write
  clobbers the first object's fragments on the shared chunk store under the colliding id.

## The fix (two halves, mirroring the sanctioned schemes)

**Inodes — coordinated through the shared store.** `commit_written`'s create branch now
allocates via `crate::cli::alloc_inode(&self.meta)` (target `cli.rs:1027`) instead of
`self.next_inode.fetch_add` — exactly the CAS-backed `meta:next_inode` allocator the CLI
cluster path uses (`cluster_store_put`, the cited peer at `cli.rs:1158`). Two active-active
gateways therefore draw *distinct* inodes from the one shared counter; the create CAS
resolves any residual dirent race cleanly. (Composition slice mirrored per the brief's
"Citations expected".)

**Chunk ids — coordination-free by construction (ADR-0019).** `mint_chunk_id` now returns
`(chunk_epoch << 64) | next_chunk_seq`, where `chunk_epoch` is a per-gateway random 64-bit
value (top bit set) drawn once in `new`, and `next_chunk_seq` is a per-gateway monotonic
`AtomicU64` from 0. This satisfies every constraint the brief binds:
- *cross-process unique*: two processes draw independent epochs, so their id ranges are
  disjoint — neither can write under an id the other committed or has in flight;
- *overwrite-safe* (the brief's hard constraint against re-introducing a same-inode overwrite
  clobber): `next_chunk_seq` is monotonic across the whole process and never reset per-object,
  so re-PUTting an existing key mints fresh ids rather than re-minting the prior version's —
  unlike the CLI's `(inode<<64)|seq`-from-0 minter, which the brief explicitly calls
  overwrite-unsafe for the gateway;
- *disjoint from the other id spaces*: the epoch's top bit set ⇒ every id ≥ 2^127, clear of
  the `< 2^64` in-process space `metadata::high_water_marks` scans and of the cluster path's
  `(inode<<64)|seq` ids (`core/src/metadata.rs:585-595`).

**`recover()` re-threaded, not deleted.** The chunk counter is gone (chunk ids need no
recovery — they're coordination-free), and inode recovery moves into the shared store:
`recover()` now seeds the persisted `meta:next_inode` allocator to `high_water_inode + 1` via
a new `cli::seed_next_inode_floor` helper (single-sources the `meta:next_inode` key + decimal
format next to `alloc_inode`). This *preserves* the #364 finding-1 guarantee the brief says is
"in scope to preserve": a restart, or an in-place upgrade from a store an older single-process
gateway wrote with no counter, still resumes strictly above every committed inode. It stays
`pub` so `serve_s3` (`cli.rs:1415`) and the existing restart tests keep calling it.

## Why random chunk ids, and why no new dependency

ADR-0019 makes the chunk id a u128 *precisely* so ids can be generated without central
coordination (random/UUID-style) — that is the sanctioned cluster-safe scheme, and the brief
points at it. The `epoch<<64 | seq` split is strictly stronger than full-random-per-id for the
*within-process* guarantee (monotonic `seq` ⇒ no within-process collision is even possible; a
pathological RNG can only ever cause a cross-process epoch collision, at 2^-64), and needs a
single entropy draw.

Entropy comes from `std::collections::hash_map::RandomState`, which the standard library seeds
from the OS RNG (the same source that hardens `HashMap`). This is deliberate: it adds **no new
crate** to the gateway binary. Adding `rand`/`getrandom` to `crates/server`'s `[dependencies]`
would have pulled a **new production dependency** into the `wyrd` binary (rand is only a
dev/test dep today), which INTEGRATION.md §4 flags as a reviewer NEEDS-HUMAN (the ADR-0003
three-test audit + `deny.toml` allowlist). The `RandomState` idiom avoids that entirely while
staying honest OS entropy. Cost of the rejected `rand` route, concretely: +1 line in
`crates/server/Cargo.toml`, a first-ever production edge to `rand`/`rand_core`/`getrandom`, and
a §4 dependency-audit NEEDS-HUMAN — versus the 8-line dependency-free helper shipped.

Determinism note: `rand`-vs-`RandomState` non-determinism would matter under madsim DST, but
the `Gateway` type is **not** exercised by `crates/dst` (verified: `dst/Cargo.toml` has no
`wyrd-server` edge and no `Gateway` use), so seed reproducibility is unaffected.

## Rejected alternatives

- **Guard a single call site** (e.g. detect a colliding chunk id and retry): the SELF-TEST
  rules this out — the property spans separate processes, so no local guard restores it.
- **Fix only chunk ids, keep the inode counter**: fails the criterion — two gateways still
  both mint inode 1, so the second create is a bogus `Conflict`. The new test binds *both*
  (see refutation (c)).
- **Inode-derived `(inode<<64)|seq` chunk ids (mirror the CLI verbatim)**: overwrite-unsafe
  for the gateway — the brief forbids it, and it would require restructuring to resolve the
  inode before planning chunks (the async/lifetime boundary the brief's Difficulty flags).
  Random epochs sidestep the restructure: chunk ids are still minted at plan time.
- **Delete `recover()` outright**: would regress #364 for the in-place-upgrade case (a store
  with inodes but no `meta:next_inode`), which the brief says to preserve.

## Test churn (expected — the brief warns effects reach "every test that leans on the counters")

- **New binding test** `crates/server/tests/gateway_multi_writer.rs`: two `Gateway` instances
  over one shared, read-back-observable in-memory metadata + chunk store (the Arc-backed
  `MemMeta`/`MemChunks` pattern the sibling gateway tests use — redb can't model two *live*
  processes because it takes an exclusive file lock). Both `recover()` from the same empty
  baseline, each PUTs a distinct key, both GET back byte-identical, plus a cross-read that each
  object is visible through the *other* gateway (proving the stores really are shared).
- **`s3_http_wire.rs::restart_without_recover_collides_showing_the_bug` → rewritten** to
  `restart_without_recover_is_safe_by_construction`. Its old premise (a per-process counter
  replays from 1 without `recover`) is *removed by the fix*: the persisted allocator now
  resumes a restart even without `recover`. The rewrite asserts that new, stronger guarantee.
- The other two restart tests (`..._no_collision`, `..._over_orphan_ledger_...`) still pass
  unchanged in body; only their now-stale rationale comments were refreshed (chunk-id
  disjointness is now by-construction, not from an orphan-ledger counter scan).
- `gateway_lease_expiry.rs` (pre-occupies `inode:1` to force a losing PUT) still passes:
  `alloc_inode` also starts at 1 on an empty store, so `inode:1` is still the first id.

## Forced refutation of the new test (recorded per builder contract)

- **(a) Genuine red?** Yes — reverted `lib.rs` + `cli.rs` (via `git stash`, keeping the test)
  and re-ran: `two_active_gateways_...` FAILED at `crates/server/tests/gateway_multi_writer.rs`
  line ~152, `gateway B must store a distinct key ... : Conflict` — the bogus re-minted-inode
  conflict. Restored the fix and it passes.
- **(b) Production path?** Yes — the test drives `wyrd_server::Gateway::{recover, put_object,
  get_object}` directly (the production types the patch changes), not a copy or mock. `MemMeta`
  / `MemChunks` are only the shared backends behind the real trait seams (the same stand-ins
  `gateway_lease_expiry.rs` uses); all id-minting logic under test is production code.
- **(c) Fixture includes the fault?** Yes — the fixture is exactly the colliding condition: one
  shared metadata store and one shared, read-back-observable chunk store, with both gateways
  recovered from the *same* empty baseline (so any per-process counter seeds identically). The
  read-backs bind BOTH failure modes: gateway B's PUT rejection (inode collision) and A's
  clobbered read-back (chunk collision, detectable because the two payloads differ) each fail
  the test independently — so it can't pass if only one half of the fix is applied.

## Verification

- `cargo test -p wyrd-server --test gateway_multi_writer`: RED before fix, GREEN after.
- Full `wyrd-server` suite (incl. `s3_http_wire`, `closed_write_path`, `e2e`,
  `consistency_observable`, `custodian_day_one`, `gateway_cluster`, `gateway_lease_expiry`):
  all green.
- `cargo fmt --check`: clean. `cargo clippy -p wyrd-server --tests`: no warnings.
- Full gate `./engine/xtask.sh ci` (= `cargo xtask ci`): run to green (fmt-check, workspace
  clippy `-D warnings`, build, workspace tests, machete, deny, conformance, statics, DST).
