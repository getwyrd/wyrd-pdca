# Build notes — issue 251 / reconstruction-read-around-fragment-read-fault

Target branch: `getwyrd/wyrd @ main` (worktree base `4150ca5`). All citations are
`path:line` against that tree (`$PDCA_WORKTREE`).

## Root cause

`reconstruction::assess` fetched each placed fragment with `store.get_fragment(frag).await?`
(`crates/custodian/src/reconstruction.rs:246`, pre-fix). The bare `?` propagates **any**
non-`NotFound` error, so one block-layer read fault (`dm-error` / dead-sector `EIO`) on a
single placed fragment makes `assess` return `Err`, aborting the whole per-chunk
reconciliation drain — one faulted D server stalls repair for every queued chunk, and a
disk that goes bad *after* its data lands can never be repaired. The read path already
tolerates this (`crates/core/src/read.rs:189`, admits only `if let Ok(Some(_))`, reads the
unreadable fragment around and rebuilds from the `k` survivors); reconstruction did not.

## Fix

Replace the bare `?` with a classify-at-the-seam match (`reconstruction.rs:245-262`),
mirroring the precedent at `scrub.rs:102`:

- `Ok(bytes) => bytes` — normal path.
- `Err(e) if is_permanent_read_fault(e.as_ref()) => None` — a **permanent** durability
  fault is read around (treated as a missing shard, rebuilt from the ≥`k` survivors).
- `Err(e) => return Err(e)` — a **transient** fault is propagated to the retry policy,
  never converted into permanent loss / a re-placement.

`is_permanent_read_fault` (`reconstruction.rs`) is `wyrd_traits::is_integrity_fault(err)
|| is_block_read_fault(err)`:

- corruption/integrity faults reuse the existing seam classifier (`traits/src/lib.rs:107`);
- `is_block_read_fault` walks the `std::error::Error::source()` chain looking for an
  `io::Error` with `raw_os_error() == Some(EIO=5)` — the errno a dead sector / `dm-error`
  target raises.

This is the smallest change that **restores the invariant** (ADR-0010 / the `IntegrityFault`
seam contract, `traits/src/lib.rs:64`): the permanent-loss-vs-transient distinction is
preserved at this consumer's decision point, exactly as `scrub.rs:102` and `read.rs:189`
preserve it elsewhere.

## Iteration-1 carry-forward — the T5 production-reach gap, closed

The previous attempt was rejected because the `is_block_read_fault` source-chain walk was
only exercised by a **depth-0** fixture (a bare `Box<io::Error>`); the reviewer asked
whether a real dead sector actually reaches `assess` in a form the classifier catches, or
whether production wraps the `io::Error` so the classifier returns `false` and the fix
silently no-ops. Closed on **both** axes the carry-forward named:

1. **Cited the production path.** `chunkstore-fs::get_fragment` surfaces a non-`NotFound`
   `fs::read` error as `Err(e.into())` where `e: std::io::Error`
   (`crates/chunkstore-fs/src/lib.rs:241`). `.into()` resolves to the std blanket
   `impl<E: Error + Send + Sync + 'static> From<E> for Box<dyn Error + Send + Sync>`, which
   **boxes the `io::Error` directly** (depth 0); `.await?` at `lib.rs:244` propagates that
   `BoxError` unchanged up to `assess`. So a real EIO reaches `assess` downcastable at
   depth 0 with `raw_os_error() == Some(5)` preserved — the classifier catches it and the
   fix does **not** no-op for the fs D server (dev / NAS profile, the one the #195
   `dm-error` harness drives). The `permanent_eio_fault` fixture is therefore the exact
   production fault shape, not an artificial stand-in.

2. **Exercised the chain walk at non-zero depth.** Added `WrappedReadError`
   (`tests/reconstruction.rs`) — a backend error whose `Display` does *not* re-surface the
   errno and whose `io::Error` is reachable **only** via `source()` — plus a
   `wrapped_permanent_eio_fault` fixture (depth 1). The read-around scenario is now driven
   for **both** shapes via the shared `reads_around_a_permanent_read_fault(make_error)`
   helper: `reads_around_a_depth0_permanent_read_fault_on_a_placed_fragment` and
   `reads_around_a_wrapped_permanent_read_fault_on_a_placed_fragment`. The wrapped test
   forces `is_block_read_fault`'s `next = e.source()` loop to take a step before matching —
   so the chain-walk code is proven, and a future/networked backend that boxes the raw
   fault inside its own error type is covered, not just the depth-0 fs case.

Both red pre-fix, green post-fix (see below). The rejected `.ok().flatten()` candidate was
**not** reintroduced.

## Why not the alternatives

- **`.ok().flatten()` in `assess` (rejected, per brief + #195 iter-2).** Cost: it is a
  ~1-line swallow, but it erases the permanent-vs-transient distinction — a transient
  healthy-server error becomes `None`, the fragment is treated as lost, and the selector
  re-places it (a spurious permanent re-placement: version bump, drained queue, fragment
  moved off a healthy server). The brief's SELF-TEST calls this out explicitly. The
  discriminating guard `a_transient_fault_is_not_turned_into_a_spurious_re_placement`
  is **green** with the classify-and-propagate fix and **red** with `.ok().flatten()`
  (it would assert `Err`, an unchanged queue, version 1, placement `[0,1,2]` — all of
  which `.ok().flatten()` violates).

- **Changing `scrub` / `read` to share one classifier helper.** Out of scope (brief).
  `scrub.rs:102` only classifies `is_integrity_fault` (it propagates EIO); reconstruction
  needs the broader permanent line because it **mutates** state (re-places), so reading
  around a transient fault there is harmful in a way it is not on the one-shot read path.
  The fix is local to `reconstruction.rs`; no seam type is redefined.

## Verification (red → green)

Runner: `cargo test -p wyrd-custodian --test reconstruction` in `$PDCA_WORKTREE` (the same
`cargo` the gate's `cargo xtask ci` drives, bounded by the tool timeout).

- **Pre-fix** (production change reverted via `git stash`, tests present):
  `reads_around_a_depth0_permanent_read_fault_on_a_placed_fragment` and
  `reads_around_a_wrapped_permanent_read_fault_on_a_placed_fragment` **FAIL** — `assess`
  propagates the EIO (`panicked … a permanent read fault is read around, not propagated
  out of assess: Store(Os { code: 5 … })` and the `WrappedReadError` variant). 6 passed,
  2 failed.
- **Post-fix:** all **8** pass.
- `cargo fmt -p wyrd-custodian -- --check` clean; `cargo clippy -p wyrd-custodian --tests`
  clean (no warnings) — commit-ready for the target's `cargo xtask ci` fmt/clippy gates.

The unit under test is import-light (in-memory trait stores only; no GUI/IO toolkit at
load), so a headless gate runner loads it cleanly.

## Files changed

- `crates/custodian/src/reconstruction.rs` — classify-at-seam match at `:245-262`;
  `EIO`, `is_permanent_read_fault`, `is_block_read_fault` added before `enum RepairOutcome`.
- `crates/custodian/tests/reconstruction.rs` — `FaultGetStore`, `WrappedReadError`, the
  three fixtures, the shared read-around helper + its depth-0/wrapped wrappers, and the
  transient discriminating guard.
