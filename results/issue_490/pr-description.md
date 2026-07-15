# PR description

## Summary
**User impact:** An upload that stalls for longer than its lease lifetime (30 s by
default) could still appear to succeed: the client gets an OK, the object is
listed and readable — but its data lives in storage the background garbage
collector is already entitled to delete, so a later download can fail or return
missing data. When the stalled upload was an overwrite, it silently replaced a
perfectly healthy object with one that can rot out from under the reader. The
client is never told anything went wrong.

This PR makes such an upload fail closed: a stall past the lease lifetime now
aborts the upload or refuses the final commit with a conflict, so a broken object
is never published and the previous version stays intact and readable.

Reported in getwyrd/wyrd#490 (no tracker URL pattern is configured for this
project; the report is linked by the closing reference below).

## What to look at
The change is confined to the two moments the write path exercises authority over
an in-flight upload's lease: the mid-upload renewal and the final commit. Both
now verify — atomically with the write itself — that every lease is still alive,
and refuse otherwise; nothing changes for uploads that stay within their lease.
The core of it is in `crates/core/src/metadata.rs` (the conditional renewal and
the lease-guarded committers) and `crates/core/src/write.rs` (the surfaced abort
and the committer plumbing).

To exercise it: `cargo test -p wyrd-core --test stream_lease_lapse --test
stream_lease_renewal`. The added `stream_lease_lapse.rs` stalls a streaming
overwrite past its lease in three different windows (mid-upload, at end of
stream, and with leases present but expired) and asserts the upload is refused
and the original object still reads back byte-identical.

## Root cause
Until the commit publishes the inode, an in-flight chunk's fragments are
protected from the custodian sweep/GC only by an unexpired `pending:<id>` lease —
and GC reclaims the bytes keyed on expiry even while the entry is still present
(`crates/custodian/src/gc.rs:142-144`). Two seams traded on authority the sweep
had already revoked: `metadata::renew_pending` did a blind `put` of every pending
entry, so the next chunk's renewal re-created entries a sweep had just reaped
(and `lease_write_chunk` discarded the renewal's outcome entirely); and phase-3
commit (`commit_create` / `commit_overwrite`) required nothing of the pending
ledger, so a lapse the renewal never observes — after the last chunk, or between
the stream returning and the caller committing — still published.

## Fix
- **Renewal is conditional and atomic** (`crates/core/src/metadata.rs:640-664`):
  each `pending:<id>` entry is read back and the batch pairs
  `require(key, read-back-value)` with the `put`, refusing with `Conflict` when
  an entry is absent (swept) or lapsed. The check and the write are one batch, so
  a sweep interleaving between read-back and commit turns the precondition false.
  The refusal surfaces as a hard error (`WriteError::LeaseLapsed`,
  `crates/core/src/write.rs:459-478`) that aborts the upload before the next
  chunk is written toward commit.
- **Phase-3 commit is lease-conditional**: new `metadata::create_leased`
  (`crates/core/src/metadata.rs:319-347`) and
  `metadata::commit_chunk_map_superseding_leased`
  (`crates/core/src/metadata.rs:548-587`) thread a per-chunk
  `require(pending_key, read-back-value)` (`live_lease_guards`,
  `crates/core/src/metadata.rs:682-700`) into the **same** `WriteBatch` as the
  inode create / CAS, so a racing sweep yields `Conflict`, never a publish.
- **One boundary everywhere**: renewal and commit refuse at
  `lease_expiry_millis <= now` — exactly the sweep's reap condition
  (`crates/core/src/write.rs:610`) and GC's expired-lease input — so every lease
  consumer agrees a lease is dead at its deadline.
- **Signatures**: `commit_create` gains a `now_millis` commit-instant parameter
  (`crates/core/src/write.rs:254`; the gateway passes `now_millis()`,
  `crates/server/src/lib.rs:193`); `commit_overwrite` keeps its signature and
  reuses its existing `orphaned_at_millis` as the commit instant
  (`crates/core/src/write.rs:296-313`). Test suites that drove phase 3 without
  the intent phase now run it first, per the protocol's phase order.
- **Deliberately untouched**: `metadata::commit_chunk_map`
  (reconstruction/backfill — its chunks are already committed and hold no pending
  entries) and the sweep/GC themselves (the defect was resurrection/publish, not
  the reap).

## Verification
- **Claim:** Renewal never resurrects or extends a lapsed lease; a mid-upload
  lapse aborts the upload and publishes nothing.
  - **Checked:** `crates/core/src/metadata.rs:640-664` — read-back + `require` +
    `put` in one batch, refusing on absent or `expiry <= now`;
    `crates/core/src/write.rs:459-478` — the refusal aborts the upload.
  - **Test:** `crates/core/tests/stream_lease_lapse.rs:120`
    (`mid_upload_lapse_aborts_the_stream_and_publishes_nothing`) — sweeps a lease
    between two chunks, asserts the swept entry is not re-created, drives the
    commit on the `Ok` arm so "nothing published" is falsifiable. Fails pre-fix
    (blind renewal resurrects; the new version publishes), passes post-fix.
- **Claim:** Phase-3 commit publishes only if every chunk's lease is alive at the
  commit's own atomic decision point.
  - **Checked:** `crates/core/src/metadata.rs:319-347`, `:548-587`, `:682-700` —
    per-chunk lease guards ride in the same batch as the inode create / CAS.
  - **Test:** `crates/core/tests/stream_lease_lapse.rs:206` (end-of-stream sweep:
    the stream returns Ok on both trees, the commit must refuse) and `:275`
    (leases present but expired, no sweep at all). Both fail pre-fix
    (`Committed`), pass post-fix; both assert the original object reads back
    byte-identical.
- **Claim:** The boundary is the reaper's own — a lease is dead at exactly
  `now == lease_expiry_millis`.
  - **Checked:** `crates/core/src/metadata.rs:658`, `:694` (`<=`) against the
    sweep (`crates/core/src/write.rs:610`) and GC
    (`crates/custodian/src/gc.rs:142-144`).
  - **Test:** exact-deadline refusal at all three seams —
    `crates/core/tests/stream_lease_lapse.rs:324` (overwrite commit; red
    pre-fix), `crates/core/tests/stream_lease_renewal.rs:125` (renewal) and
    `:227` (create commit). Mutation-checked: flipping both `<=` to `<` turns all
    three red; reverting restores green.
- **Claim:** Healthy uploads are unchanged — a slow upload that renews before
  expiry, and a live overwrite, still commit and read back byte-identical.
  - **Test:** `crates/core/tests/stream_lease_lapse.rs:371` (live overwrite
    commits) and `crates/core/tests/stream_lease_renewal.rs:35` (the existing
    slow-upload renewal contract, still green).
- **Gate:** `cargo xtask ci` (fmt, clippy `-D warnings`, build, test incl. DST,
  deny, conformance) passes with the fix applied.

## Known limitations / follow-ups
- The buffered PUT path performs no mid-upload renewal, so a buffered PUT whose
  data phase outlasts the 30 s lease now fails deterministically with a conflict
  instead of publishing a corruptible object — safe, but a hard failure for very
  slow large buffered PUTs; a follow-up will renew on that path or route it
  through the streaming path.
- A GC pass whose reference snapshot predates a commit that won with then-live
  leases can still reclaim just-committed bytes in a now-instantaneous window
  (versus the unbounded window before this fix); closing it requires reordering
  GC's ledger retirement and is tracked in #557.

Fixes #490
