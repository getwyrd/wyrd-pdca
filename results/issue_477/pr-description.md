# PR description

## Summary
**User impact:** Running more than one gateway process against the same storage
fleet — the intended M4 deployment of several interchangeable gateways behind one
address — could silently corrupt or lose data. Two gateways started against the
same store would independently hand out the same internal object ids, so one
gateway's freshly written object could be overwritten by another's under a
colliding id, and a write could be spuriously rejected as a conflict. A single
gateway was unaffected; the loss only appeared once two or more ran at once.

This PR makes every id a gateway mints unique across all concurrently-active
gateways over the same fleet, so multiple gateways can serve the same store
safely.

Reported in getwyrd/wyrd#477 (no tracker URL pattern is configured for this
project, so the report is linked by the closing reference below).

## What to look at
The change is entirely in how the gateway picks the internal ids for a stored
object (its inode and its chunk ids). Instead of counting up from a number each
process guesses on its own, inodes now come from a single shared counter in the
metadata store that all gateways coordinate through, and chunk ids are built so
that two processes cannot land on the same value in the first place.

To exercise it: stand up two gateways over one shared metadata store and one
shared chunk store, have each store a different object at the same time, then read
both back — both objects must return their original bytes. The new
`crates/server/tests/gateway_multi_writer.rs` test does exactly this.

## Root cause
The gateway minted inode and chunk ids from per-process `AtomicU64` counters,
seeded once at startup from the persisted high-water mark. Two gateways over one
fleet seeded identically and then advanced independently, so both minted the same
next inode and the same next chunk id; because chunk fragments are written before
the create commit, the losing gateway's fragments overwrote a committed object's
fragments under the shared chunk id (silent loss), or its create was rejected with
a bogus conflict.

## Fix
Inodes are now allocated from the shared store's `meta:next_inode` CAS allocator —
the same allocator the CLI cluster path already uses — so two active-active
gateways always draw distinct inodes. Chunk ids are coordination-free (ADR-0019): a
per-gateway random 64-bit epoch forms the high bits so two processes occupy
disjoint id ranges without any shared counter, and a per-process monotonic sequence
in the low bits guarantees an overwrite mints fresh ids rather than re-minting a
prior version's. Startup recovery seeds the persisted inode floor above every
on-disk inode, preserving the single-process restart guarantee (and covering an
in-place upgrade from a legacy store with no counter) while extending safety to the
multi-process case.

## Verification
- **Claim:** Two gateways over one shared metadata + chunk store, recovered from
  the same baseline, can each store a distinct object and both read back
  byte-identical — no colliding inode or chunk id.
  - **Checked:** `crates/server/src/lib.rs:189` — the create path allocates its
    inode via `cli::alloc_inode` (allocator body `crates/server/src/cli.rs:1027`),
    the shared CAS counter, not a private counter.
  - **Checked:** `crates/server/src/lib.rs:217` — `mint_chunk_id` builds
    `(chunk_epoch << 64) | seq` from a per-gateway random epoch
    (`crates/server/src/lib.rs:236`), so two processes never share a chunk-id range.
  - **Checked:** `crates/server/src/lib.rs:116` — startup recovery seeds the
    persisted inode floor via `cli::seed_next_inode_floor`
    (`crates/server/src/cli.rs:1069`) above every on-disk inode.
- **Test:** `crates/server/tests/gateway_multi_writer.rs` —
  `two_active_gateways_concurrently_store_distinct_objects_without_collision` drives
  two gateways' PUTs concurrently over one shared store and asserts every object
  round-trips byte-identical through both. It **fails pre-fix** (the second gateway
  re-mints a committed inode and its PUT is rejected with `Conflict`, at
  `gateway_multi_writer.rs:205`) and **passes post-fix**. A companion test contends
  the shared allocator directly and asserts it never hands two callers the same id.
- **Test:** `crates/server/tests/s3_http_wire.rs` — restart/recovery cases confirm a
  restarted gateway resumes above every committed inode (including the legacy-store
  upgrade where recovery seeds the counter), so no restart replays a committed id.
- **Gate:** `cargo xtask ci` (fmt, clippy `-D warnings`, build, test, deny,
  conformance) passes on the target branch.

Fixes #477
