## Summary
**User impact:** a gateway would fail to start at all if its metadata store held
even one record startup recovery could not read — one corrupted object entry, or
one otherwise-valid record in a shape recovery didn't know how to handle, was
enough to take every healthy object in the store offline until someone repaired
it by hand.

This change makes startup recovery tolerant of records it can't fully read: it
skips over and reports the damaged ones instead of refusing to start, so the
rest of the store keeps serving. It also drops a piece of recovery bookkeeping
that computed a value nothing in the code actually used any more.

## What to look at
- `Gateway::recover()` (`crates/server/src/lib.rs`) is what runs before the
  gateway accepts any request. Try seeding a store with one healthy object plus
  one raw, unreadable metadata record and calling `recover()` — before this
  change it returns an error and the gateway never starts; after, it returns
  `Ok` and the healthy object is still readable.
- The new test file `crates/server/tests/gateway_recover_totality.rs` exercises
  exactly that scenario, plus a structurally-valid-but-unresolvable record shape
  and a corrupted allocator counter.

## Root cause
The startup scan that computes the next available object id decoded every
stored record with `?`, so any one record it couldn't decode aborted the whole
scan and, with it, gateway startup. The counter that tracks the next id had the
same problem. Separately, the scan also computed a second value (a chunk-id
floor) that used to feed an allocator removed in an earlier change, so it had
no remaining caller.

## Fix
The scan now derives each record's contribution to the id floor from its
*key* before it ever looks at the value, so a record whose value can't be read
still raises the floor correctly instead of aborting. Unreadable records are
attributed (logged with their key and fault) rather than silently skipped, so
the operator can still find and repair them. The allocator-counter repair path
gets the same treatment, replacing an unreadable counter under a
compare-and-set that never overwrites a concurrent writer's value. The unused
chunk-id floor computation, and the two extra store scans it required, are
removed.

## Verification
- **Claim:** `Gateway::recover()` returns `Ok(())` over a store containing one
  undecodable `inode:` record alongside a healthy object, and the healthy
  object still reads back unchanged.
  **Checked:** `crates/server/src/lib.rs:133-141` (`Gateway::recover`),
  `crates/core/src/metadata.rs:2153-2192` (`high_water_marks`).
  **Test:** `crates/server/tests/gateway_recover_totality.rs::recover_is_total_over_an_undecodable_inode_record`
  — fails on `origin/main` with `recover()` returning `Err`; passes with this change.

- **Claim:** a structurally valid but unresolvable ("segmented") record is
  contained the same way, still contributing its id to the floor.
  **Checked:** `crates/core/src/metadata.rs:2153-2192`.
  **Test:** `crates/server/tests/gateway_recover_totality.rs::recover_is_total_over_a_segmented_root`
  — fails on `origin/main` (explicit refusal), passes with this change.

- **Claim:** a corrupted allocator counter (`meta:next_inode`) does not stop
  recovery, terminates in bounded time, and leaves the counter at or above the
  recovered floor.
  **Checked:** `crates/server/src/cli.rs:1749-1813` (`seed_next_inode_floor`).
  **Test:** `crates/server/tests/gateway_recover_totality.rs::recover_is_total_over_a_corrupt_next_inode_counter`
  — fails on `origin/main`, passes with this change; a 60s worker-thread budget
  turns a hypothetical non-terminating retry into a test failure rather than a
  hang.

- **Claim:** the counter repair never overwrites a concurrent allocator's
  write (no rewind of a live allocator).
  **Checked:** `crates/server/src/cli.rs:1749-1813`, guarded by a
  compare-and-set on the exact bytes read.
  **Test:** `crates/server/tests/gateway_recover_totality.rs::the_counter_repair_yields_to_an_allocator_that_won_the_race`.

- **Claim:** no regression on the existing legacy-store recovery case.
  **Checked:** `crates/server/tests/s3_http_wire.rs:666`
  (`recover_seeds_the_allocator_over_a_legacy_store_without_meta_next_inode`) —
  passes unchanged.

- **Claim:** the unused chunk-id floor is fully removed, not just unwired.
  **Checked:** `git grep -n "_max_chunk" -- crates/` returns no matches.

Fixes #652
