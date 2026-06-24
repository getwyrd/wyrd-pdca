# Give each filesystem write its own scratch file

## Summary
Writing the same fragment to the filesystem chunk store from two places at
once could fail spuriously — an idempotent client retry, or a repair pass
writing a fragment a foreground write was also storing, could return a "No
such file or directory" error even though the data was valid and a complete
fragment was published. The fix gives every write its own private scratch
file so concurrent writes of the same fragment all succeed.

## What to look at
- `crates/chunkstore-fs/src/lib.rs` — `put_fragment` and the scratch-path
  helper it uses. Previously the scratch file was named from the fragment id
  alone, so two writes of one fragment shared it; now the name carries a
  per-store sequence number, making it unique per write. The atomic rename
  onto `<index>.frag` remains the only point at which a fragment becomes
  visible.
- `open` now sweeps stale scratch (`*.tmp`) left by a crashed process, since
  unique names no longer overwrite one another the way the old fixed name did.
- To exercise it: open an `FsChunkStore` over a temp dir and fire many
  concurrent `put_fragment` calls for one fixed `FragmentId` with identical
  bytes — see the new test under `crates/chunkstore-fs/tests/`.

## Root cause
`put_fragment` staged each write in a scratch file keyed on the `FragmentId`
alone (`<chunk>/<index>.tmp`), then renamed it onto the final path. Two
concurrent writes of the same id therefore shared one scratch file, so their
`fs::write`s could clobber each other and the second `fs::rename` could find
the scratch already moved by the first and fail `NotFound`.

## Fix
The scratch path is now unique per write — the chunk directory and fragment
index plus a monotonic per-store sequence — so no two writes ever name the
same scratch file. The atomic rename stays the sole publish/serialization
point: a concurrent same-id write can neither see nor overwrite another's
partial bytes, last-writer-wins is a no-op (identical bytes), and writes of
different fragments remain independent. The per-store counter needs no new
dependency and no process-global state. Because unique names no longer
self-clean, `open` reaps stale `*.tmp` scratch under the store's chunk
directories; this is safe because a single process owns a store's root and no
write is in flight at open. Fragment files (`.frag`) and the listing logic are
untouched, so scratch never surfaces as a fragment.

## Verification
- **Claim:** N concurrent `put_fragment` calls for the same `FragmentId`
  all complete successfully, the published `<index>.frag` verifies, and
  listing ignores scratch files.
  - **Checked:** `crates/chunkstore-fs/src/lib.rs:45-48` (target branch) — the
    pre-fix scratch path is `<chunk>/<index>.tmp`, shared across calls for one
    id; `:82-83` — `fs::write(&temp, …)` then `fs::rename(&temp, final_path)`,
    the rename that raced and could fail `NotFound`. The fix makes that scratch
    name unique per write and removes only the write's own scratch on failure.
  - **Test:** `crates/chunkstore-fs/tests/concurrent_put.rs` (new) — 64 writers
    released together by a barrier over 16 rounds, all writing one fixed id;
    asserts every put is `Ok`, the fragment round-trips and verifies, and the
    store lists exactly that one id. Pre-fix (scratch reverted to the shared
    name) it fails on the first burst with `No such file or directory`;
    post-fix it passes.
- **Claim:** scratch files never surface as fragments.
  - **Checked:** `crates/chunkstore-fs/src/lib.rs:221-222` (target branch) —
    fragment listing accepts only names ending exactly `.frag`, so any `.tmp`
    scratch stays invisible; unchanged by this fix and re-asserted by the test
    above.
- **Claim:** per-write scratch uniqueness holds independent of timing.
  - **Test:** a unit test in `crates/chunkstore-fs/src/lib.rs`
    (`scratch_names_are_unique_per_seq_and_invisible_to_listing`) asserts
    structurally that distinct sequence values yield distinct scratch names,
    that scratch is never parsed as a fragment, and that it is recognised as
    reapable.

Fixes #203
