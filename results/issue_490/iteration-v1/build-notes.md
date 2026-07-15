# Build notes — issue 490 / renew-pending-lease-resurrection

## What the defect is

`metadata::renew_pending` did a **blind** `put` of every `pending:<id>` entry
(base `crates/core/src/metadata.rs:533-537`). A streaming PUT protects its
already-written-but-uncommitted chunks from the custodian sweep/GC only via their
pending leases (`write.rs:409-415`). If the upload stalls past `lease_ttl_millis`
between two chunks and the sweep runs, an early chunk's `pending:<id>` entry is
reclaimed — a legitimately dead upload (`write.rs:417-418`). The very next chunk's
renewal then blind-`put` **re-created** the swept entry, and the upload committed an
inode whose chunk map points at fragments the GC is free to reclaim. And the callsite
(`write.rs:431-440`) `.await?`'d the renewal and **dropped** the `CommitOutcome`, so
even a `Conflict` would have sailed through.

## The fix (three obligations from the brief's Scope)

Target: `getwyrd/wyrd @ main`. Edits (line numbers on the patched tree):

- **(a) conditional + atomic renewal** — `metadata::renew_pending`
  (`crates/core/src/metadata.rs:541-567`). New `now_millis` param. Per chunk it reads
  the current `pending:<id>` back and pairs `require(key, current-value)` with
  `put(key, new-value)` in **one** `WriteBatch`, then commits once
  (`crates/traits/src/lib.rs:664` `require`). A sweep that deletes the entry *between*
  the read-back and the commit turns the precondition false → the whole batch is
  `Conflict`; a two-commit read-verify-then-put could not close that interleave.
- **(b) existence AND expiry are both conditions** — same function: `get` returning
  `None` (swept) → `Ok(Conflict)`; a present entry whose recorded
  `lease_expiry_millis < now_millis` (lapsed, not yet reaped) → `Ok(Conflict)`. The
  strict `<` is deliberate and load-bearing: the healthy renewal renews exactly at the
  deadline (`now == expiry`, e.g. `stream_lease_renewal.rs` renews chunk 0 at `t=TTL`
  when its expiry is `TTL`), so `<=` would wrongly refuse the healthy path and break
  the peer contract. `<` refuses only a genuinely-past lease. (The sweep reaps at
  `<= now`; the belt-and-suspenders `<` catch is for a lapsed-but-unswept entry — the
  swept case is already caught by absence.)
- **(c) surface the refusal** — `write::lease_write_chunk`
  (`crates/core/src/write.rs:436-452`) now binds the `CommitOutcome` and, on anything
  but `Committed`, returns `WriteError::LeaseLapsed` (new enum,
  `crates/core/src/write.rs:606-637`, mirroring `read::ReadError`) — a hard error that
  aborts `stream_write_data` **before** the next chunk is written toward commit.

Out of scope, untouched as instructed: `sweep_expired_leases` / GC reap logic, the
non-streaming write path, gateway retry/error mapping, #554 wiring.

## Alternatives ruled out

- **Guard the symptom at commit** (re-scan pending before `commit_create` and abort if
  any in-flight lease vanished): rejected. It removes the *symptom* one layer late, not
  the *cause* (the resurrection), and still leaves `renew_pending` re-creating swept
  authority for any other caller. It also cannot be made atomic against a sweep that
  interleaves between the re-scan and the commit — the same TOCTOU the brief explicitly
  forbids for renewal. The invariant to restore ("a lapsed lease is dead; renewal may
  only extend a lease that still exists") lives *in the renewal*, so the smallest change
  that restores it is the conditional batch, not a downstream probe.
- **Two commits (read-verify, then blind renew):** rejected per brief Scope (a) — a
  sweep can delete between the check and the put. Cost of avoiding it is zero here: the
  single `require`+`put` batch is the same number of commits as the old blind put.

## Test — `crates/core/tests/stream_lease_lapse.rs`

Mirrors the peer harness (`stream_lease_renewal.rs`): `RedbMetadataStore::in_memory()`
+ `FsChunkStore` on a tempdir, logical clock, `TTL=30_000`, 3×4-byte chunks, plain
`#[test]` over `pollster::block_on`, no cfg gate. The mid-upload seam is the **input
stream**: a `stream::unfold` generator that, on the step producing chunk 2, jumps the
shared `Rc<Cell>` clock to `2·TTL` and awaits `sweep_expired_leases` — reclaiming
chunk 1's lease (expiry `TTL`) while the upload is still inside `stream_write_data`.
It drives the **production** `write::stream_write_data`; no stand-in.

Binding assertions (red pre-fix, green post-fix), shaped per the brief's honesty note
(the sweep deletes only the ledger entry, not the fragment bytes, so a read-back would
still succeed — so I do **not** assert on read-back loss):
1. the upload `result.is_err()` — pre-fix it returns `Ok(plan)`;
2. the swept `pending:1` key is not resurrected — pre-fix the blind put re-creates it;
3. nothing committed — `read_path` returns `None`.
Plus a fault-injection guard: `assert_eq!(reclaimed, vec![1])` proving the sweep
actually tore out the in-flight lease (the fixture *includes* the fault).

## Refutation (forced self-check)

- **(a) Genuine red?** Yes. `engine/scripts/run-verify.sh` reverted the production
  files (kept the test) and the test FAILED at binding #1
  ("must abort, not produce a commit plan over reclaimed fragments"); with the fix it
  passes. Gate verdict: `PASS — red without the fix, green with it.`
- **(b) Production path?** Yes. The test calls `write::stream_write_data` /
  `write::sweep_expired_leases` / `metadata::pending_key` / `read::read_path` — the real
  functions the patch changes, no copy or mock.
- **(c) Fixture includes the fault?** Yes. The sweep runs *inside* the streaming call
  via the input-stream generator and reclaims chunk 1's real pending lease; the
  `assert_eq!(*reclaimed.borrow(), vec![1])` fails the test if the fault was not
  injected.

## Commit-readiness

- `cargo fmt --manifest-path crates/core/Cargo.toml --check` → clean.
- `cargo clippy -p wyrd-core --tests --all-targets` → clean (fixed a `useless_vec` on
  the test payloads).
- Peer contract `stream_lease_renewal.rs` → still green.
- `renew_pending` has one caller (`write.rs`); grep across `crates/` confirms no other
  callsite needed updating for the new `now_millis` param.
