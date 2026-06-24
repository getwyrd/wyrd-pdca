# Build notes — issue 251 / reconstruction-read-around-fragment-read-fault

## Root cause

`reconstruction::assess` fetched each placed fragment with
`store.get_fragment(frag).await?` (`crates/custodian/src/reconstruction.rs:246`,
target branch). The `?` propagated **any** non-`NotFound` `Err` — including a
permanent block-layer read fault (`EIO` / dead sector / `dm-error`) — out of the
per-obligation assessment, through `reconcile`'s `assess(...).await?`
(`reconstruction.rs:136`) and `reconcile_step`'s `map_err(ReconcileError::Store)?`
(`reconciliation.rs:97`). One faulted placed fragment therefore aborted the whole
shared per-chunk drain, and a disk that goes bad *after* its data lands could never
be repaired.

The read path already tolerates exactly this — `read.rs:189` admits only
`if let Ok(Some(_))`, reading an unreadable fragment around and rebuilding from the
`k` survivors. `scrub.rs:102` honours the same seam distinction
(`is_integrity_fault` → repair-and-continue; other `Err` → propagate).
Reconstruction did not.

## Invariant restored (Tier C — ADR-0010 `IntegrityFault` seam contract)

A consumer walking **placed** fragments must preserve the
permanent-loss-vs-transient distinction:

- **permanent** durability fault (the device cannot return the bytes — a
  corruption/integrity fault, *or* a block-layer read fault such as `EIO`/dead
  sector) → **read around**, rebuild from the ≥`k` survivors;
- **transient** fault (unreachable / timed out / busy on a healthy server) →
  **propagate** to the retry policy; never silently converted into permanent
  fragment loss / a re-placement.

This is the smallest change that restores that invariant (the deciding axis named
in the brief), not merely the smallest diff.

## Fix

In `assess`, replace the bare `?` with an explicit classify-and-branch
(`reconstruction.rs`, the `get_fragment` match):

```rust
Some(store) => match store.get_fragment(frag).await {
    Ok(bytes) => bytes,
    Err(e) if is_permanent_read_fault(e.as_ref()) => None, // read around
    Err(e) => return Err(e),                               // transient: propagate
},
```

`is_permanent_read_fault` (new private helper) returns true for:

1. `wyrd_traits::is_integrity_fault(err)` — corruption (the existing seam
   classifier scrub already uses), and
2. `is_block_read_fault(err)` — an `EIO` (`raw_os_error() == Some(5)`) `io::Error`
   anywhere in the error's `source()` chain.

A permanent fault becomes `None` → handled by the *existing* missing-shard arm
(rebuild from survivors). A transient fault is propagated, identical to
`scrub.rs:108` (`Err(e) => return Err(e)`).

### Why classify in `assess` (guard placement) rather than at the seam

The block-read (`EIO`) classification could instead be pushed into the seam — e.g.
a new `wyrd_traits` error type or having `chunkstore-fs` wrap `EIO` as a typed
durability fault. Rejected for this slice:

- The brief scopes the change to `reconstruction::assess` and lists "redefining the
  `IntegrityFault` type's meaning" and touching the read/scrub paths as **out of
  scope**.
- Production reach is preserved without a seam change: `chunkstore-fs` already
  surfaces a real dead-sector read as a raw `io::Error` straight from `fs::read`
  (`crates/chunkstore-fs/src/lib.rs:241`, `Err(e.into())`), and `Box<dyn Error>`
  keeps it downcastable. `is_block_read_fault` walks `source()` and downcasts to
  `io::Error`, so the *live* `assess` classifies a real `EIO` the same as the
  in-process stand-in. The classifier is reused, not test-only scaffolding.

Cost of the seam alternative (rejected): a new public type + variant in
`crates/traits/src/lib.rs`, a producing change in `crates/chunkstore-fs/src/lib.rs`
(the `Err(e) =>` arm at lib.rs:241), and a matching change in
`crates/chunkstore-grpc/src/client.rs` `classify_get_status` to map the wire status —
three crates, a public-API addition to the seam (NEEDS-HUMAN-adjacent), for behaviour
the one-line downcast already achieves. Not warranted here.

### Why NOT `.ok().flatten()` (explicitly rejected by the brief, #195 iter 2)

`.ok().flatten()` swallows **every** `Err` into `None`, so a transient
healthy-server fault is misclassified as permanent loss → the chunk is rebuilt and
the fragment spuriously re-placed (a permanent move off a fragment that was merely
unreachable). It fails the invariant's transient leg. The
`a_transient_fault_is_not_turned_into_a_spurious_re_placement` test is the guard
that catches exactly this regression.

## `EIO = 5`

Defined as a named `const` with a comment rather than pulling in `libc`, to keep the
loop's dependency surface unchanged (ADR-0010 dependency boundary). `EIO` is errno 5
on every Unix platform Wyrd targets; the in-process stand-in builds it via
`io::Error::from_raw_os_error(5)` — byte-identical to what `fs::read` raises on a
dead sector (verified: pre-fix panic shows `Os { code: 5, kind: Uncategorized,
message: "Input/output error" }`).

## Tests (`crates/custodian/tests/reconstruction.rs`)

Driven through the **public** `reconcile_step` fenced control point (`assess` is
private), mirroring the existing tests in the file. A new `FaultGetStore`
ChunkStore returns a caller-supplied `Err` from `get_fragment` (delegating every
other op to a healthy inner store so a rebuilt fragment can still be *placed* there),
and it is kept **in the reconstruction fleet** so `assess` actually calls
`get_fragment` and must classify the fault — not route around it by absence.

- `reads_around_a_permanent_read_fault_on_a_placed_fragment` — **flippable**
  (red→green). Server 1 returns `EIO`; pre-fix `reconcile_step` returns
  `Err(Store(Os{code:5}))` (RED, verified), post-fix returns `Reconciled::Changed`,
  drains the obligation, bumps the inode version by exactly one, and re-places the
  rebuilt fragment on a healthy distinct domain (`placement == [0,3,2]`).
- `a_transient_fault_is_not_turned_into_a_spurious_re_placement` — **discriminating
  guard** (green with the correct fix, RED with `.ok().flatten()`; not a red→green
  flip, per the brief's verification posture). Server 1 returns a transient
  `ConnectionReset`; asserts the pass propagates (`result.is_err()`), the obligation
  stays queued, and the inode/placement are untouched. Under `.ok().flatten()` the
  transient fault would be swallowed → rebuild → re-place fragment onto the free
  domain B → `Changed`/drained/version-bump, failing every assertion.

The unit under test is import-light (in-memory trait stores only — no GUI/display/
device dependency), so it is headless-runner-safe.

## Verification

- `cargo test -p wyrd-custodian --test reconstruction`: pre-fix 6 passed / 1 failed
  (the permanent-fault test); post-fix 7 passed.
- `cargo fmt -p wyrd-custodian -- --check`: clean.
- `cargo clippy -p wyrd-custodian --tests --all-features`: no warnings.

The full gate (`./engine/xtask.sh ci` → `cargo xtask ci`) is the Check gate; the
above is the Do-beat red→green sanity pass.

## Citations (target branch `getwyrd/wyrd @ main`)

- defect: `crates/custodian/src/reconstruction.rs:246` (`...await?`)
- propagation path: `reconstruction.rs:136`, `crates/custodian/src/reconciliation.rs:97`
- seam contract: `crates/traits/src/lib.rs:64`, `is_integrity_fault` at
  `crates/traits/src/lib.rs:107`
- precedents mirrored: `crates/custodian/src/scrub.rs:102`, `crates/core/src/read.rs:189`
- real-`EIO` production source: `crates/chunkstore-fs/src/lib.rs:241`
