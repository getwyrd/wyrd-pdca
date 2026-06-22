# Build notes — issue 143 / m3.5-scrub-custodian

Target: getwyrd/wyrd @ main (worktree off `origin/main`, base `a157aba`).
Planning artifact read: `docs/design/proposals/accepted/0005-milestone-3-custodians.md`
(§Scrub `0005:262-267`; read-time-failure feed `0005:174-176`; durability metrics
`0005:331-332`; PR-sequence slice 5 `0005:528-530`; Option A `0005:524-527`).

## What the slice had to restore (the invariant)

A fragment whose checksum fails verification — found **proactively by scrub** or
**reactively on read** — is never absorbed silently: it is excluded from the decoder
**and** its chunk is enqueued for reconstruction on the **one shared** repair queue.
The slice spans three places, so a one-module guard cannot satisfy it: the scrub loop
(new), the read path (`core`), and the shared queue seam (new).

## Design — the one shared queue lives in `core`

The decisive constraint is the **dependency direction** (ADR-0010, `0005:421-422`):
`custodian → core`, never the reverse. The read path is in `core`; scrub is in
`custodian`. The only place both producers can reach the *same* queue is `core`. So
the queue seam is a new module **`crates/core/src/repair.rs`**:

- `repair_key(chunk)` / `REPAIR_PREFIX` — the ledger key (`repair:<chunk_id>`),
  modelled on the existing `pending:` / `orphan:` ledgers (`gc.rs:43`, architecture §5).
  Keyed by chunk, so a repair obligation is a **set**: scrub and a read both catching
  the same chunk collapse to one obligation (idempotent), never a duplicate rebuild.
- `enqueue_repair(meta, chunk, detected_by)` — both producers call this exact fn, so
  "the same queue" is true **by construction**, not by convention.
- `fragment_intact(bytes, chunk)` — the shared verify: decode (which verifies the
  header + payload crc32c) **and** confirm the decoded header names the expected chunk.
  Placing it in `core` (already owns the on-disk-format reader,
  `wyrd_chunk_format::decode`) is what keeps **`custodian` free of a chunk-format
  dependency** — scrub calls `core`'s verify, gaining no new on-disk-format knowledge.
- `queued_repairs(meta)` — the in-process read-back the tests use to prove the feed
  (the reconstruction-loop consumer is slice 6, out of scope `0005:531-536`).

`patch.diff` cites: `crates/core/src/repair.rs` (new), `crates/core/src/lib.rs:15`
(`pub mod repair;`).

## Scrub loop — `crates/custodian/src/scrub.rs` (new), modelled on `gc.rs`

`ScrubContext { meta, fleet }` mirrors `GcContext`. `scrub::reconcile` walks each
store's `list_fragments`, and for every fragment a **committed** chunk map references
(the same reference set GC computes — I made `gc::referenced_fragments` `pub(crate)`,
`gc.rs:179`, rather than duplicate ~25 lines), it fetches the bytes and calls
`repair::fragment_intact`. On a mismatch it **excludes** the fragment (never decodes
it into a chunk) and `enqueue_repair(.., "scrub")`. It emits `scrub_coverage` (per
referenced fragment verified, `0005:331`) and `scrub_corruption_detected`
(`0005:332`) on the `DurabilityTelemetry` `tracing`→OTel seam, exactly as `gc.rs`
emits its counters.

Scrub **never deletes** — reclaiming displaced bytes is GC (slice 4), rebuilding is
reconstruction (slice 6). It only *produces* obligations, matching the brief's
out-of-scope line.

## Dispatch through the real `reconcile_step` (the fence)

The brief requires scrub to run through `reconcile_step` (the fenced control point,
the anti-#141 guard). I added a `scrub: Option<&ScrubContext>` parameter
(`reconciliation.rs:57`) and the step now runs each supplied loop and reports
`Changed` if either converged. Cost of the alternative considered — a
`ReconcileInputs` struct — was higher *and* still forced editing every call site:
`Some(&ctx)` → `ReconcileInputs { gc: Some(&ctx), ..Default::default() }` at 5 GC
sites + 3 skeleton sites, vs. the mechanical `+ , None` I applied
(`tests/gc.rs`, `tests/skeleton.rs`). The `None,None` path returns `Satisfied` exactly
as before, so GC/skeleton semantics are unchanged (both suites still green: 4 + 3).

## Read path — feed the same queue without breaking 30 callers

`read_object_from(chunks, inode)` is called in ~30 places across crates **without a
`MetadataStore`** (benches, server/dst/grpc tests). Threading `meta` through it would
break every one. Cost, concretely: `grep -rn read_object_from crates` = 25+ call
sites, none of which carry `meta`. So I kept that public signature **byte-for-byte**
and instead:

- Added a private `read_object_collecting(chunks, inode, &mut corrupt)` that the
  public `read_object_from` delegates to (dropping findings — no store to enqueue on).
- `read_chunk` now takes `&mut Vec<ChunkId>` and pushes the chunk id when a **present**
  fragment fails its checksum (`Ok(Some(bytes))` + `decode` is `Err`) — the bit-rot
  case, distinct from a missing (`Ok(None)`) or transport-errored fragment.
- `read_object` (which **does** have `meta`, `read.rs:169`) calls the collecting
  variant and enqueues each finding via `repair::enqueue_repair(.., "read")` — **before**
  surfacing the read result, so even a sub-`k`, unrecoverable read (a `none`-scheme
  corrupt fragment) leaves a durable obligation behind.

This is the smallest change that restores the invariant on the read side; it touches
only the two functions that already had `meta` in scope and leaves all external callers
compiling (verified: `cargo build --workspace --tests` clean).

The read-path regression therefore lands in **`crates/core/tests/read_repair.rs`**
(the seam is in `core`, per the brief's "a `core` read-path test if the read path
enqueues directly").

## Tests + demonstrated red (legs 2 and 4 are load-bearing, not absence)

- `crates/custodian/tests/scrub.rs` — legs 1 (walk+verify, skips the unreferenced
  orphan), 2 (bit-flip → `Changed` + `queued_repairs == [chunk]` + fragment NOT
  deleted), 3 (coverage + corruption metrics read back via `gather_prometheus`).
- `crates/core/tests/read_repair.rs` — leg 4: an RS(2,1) read excludes a bit-flipped
  fragment, reconstructs byte-identical from the surviving k=2, **and** enqueues the
  chunk on `repair::repair_key` (the same key scrub uses); plus the unrecoverable
  `none`-scheme case still enqueues.

Demonstrated red (recorded here; the negations are reverted in the shipped patch):

- **Leg 2** — flipped `repair::fragment_intact` to `true` (silently absorb): scrub
  returned `Satisfied` with an empty queue →
  `detects_a_bitflip_excludes_and_enqueues_for_reconstruction` failed
  (`left: Satisfied, right: Changed`). Proves the verify is load-bearing.
- **Leg 4** — suppressed the body of `enqueue_repair` (no-op): the EC read **still
  returned the bytes** but the queue stayed empty → both `read_repair` tests failed on
  the queue assertion (`left: [], right: [chunk]`). Proves the enqueue, not the read,
  is what the tests pin.

Both reverted; all 5 new tests green, plus gc (4), skeleton (3), and the rest of the
core suite. `cargo fmt --check` and `cargo clippy --tests` clean on both crates.

## Notes for sign-off

- Cargo.lock change is a single line: `wyrd-chunk-format` added to `wyrd-custodian`'s
  **dev**-deps (the scrub test builds a real v1 fragment to inject a bit-flip). It is a
  workspace-internal crate — **no new external dependency / license** (ADR-0003).
- No on-disk-format change: the checksum is the existing chunk-format envelope's
  (`0005:552-554`); scrub borrows `decode`, adds no coding math.
- Out of scope and untouched: the reconstruction custodian (dequeue/rebuild/re-place/
  version-conditional commit) and repair-vs-serve priority — slice 6.
- Ran focused `cargo test` (Bash-tool timeout) for red→green; the full gate is
  `cargo xtask ci` via `./engine/xtask.sh ci` (Check re-runs it).
