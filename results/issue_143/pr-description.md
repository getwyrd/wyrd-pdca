# custodian: scrub stored fragments and enqueue bit rot for repair

## Root cause
M3 verified a fragment's checksum only *reactively*, at read time
(`crates/core/src/read.rs:11-16`): a corrupt shard was read around but the
finding was discarded, and nothing walked a D server's stored fragments to
catch bit rot *before* the data was needed — so detected corruption could be
silently absorbed with no durable repair obligation recorded anywhere.

## Fix
Add the scrub custodian loop (`crates/custodian/src/scrub.rs`) and the shared
reconstruction/repair queue it feeds (`crates/core/src/repair.rs`). One scrub
pass walks each store's fragments, keeps only those a committed chunk map
references, and verifies each one's self-describing checksum **against that
map** (`repair::fragment_intact` — both that the bytes decode cleanly and that
the decoded header names the expected chunk). A fragment that fails — bit
rot, or intact-but-misplaced — is excluded and its chunk enqueued on the
durable repair queue; scrub never deletes or rebuilds (that is the
reconstruction custodian, slice 6). The same queue is fed reactively: the
read path now collects every chunk whose read excluded a checksum-failing
fragment and enqueues it via the same `repair::enqueue_repair`, so the
"one shared queue" holds by construction. Scrub coverage and the
scrub-detected corruption rate are emitted on the `DurabilityTelemetry` seam.
The queue lives in `core` because `custodian → core` is the only legal
dependency direction; `reconcile_step` gains a `scrub: Option<&ScrubContext>`
input and both-`None` returns `Satisfied` exactly as before.

## Verified against
- `crates/core/src/read.rs:11-16` — the existing read-time ("never bad data")
  checksum verification this slice mirrors proactively and now also wires to
  the repair queue.
- `crates/core/src/read.rs:169-180` — `read_object`, the placement-aware entry
  that already holds `meta` in scope; the enqueue is added here without
  changing `read_object_from`'s public signature (its ~25 callers carry no
  `MetadataStore`).
- `crates/custodian/src/reconciliation.rs:57-61` — `reconcile_step`, the fenced
  control point scrub is dispatched through (extended with the `scrub` input,
  `gc`/skeleton call sites updated to pass `None`).
- `crates/custodian/src/gc.rs:179` — `referenced_fragments`, the
  committed-reference set scrub reuses (made `pub(crate)`) so it verifies
  exactly what a committed chunk map protects, never an orphan.

## Test
- `crates/custodian/tests/scrub.rs` — walk+verify (skips an unreferenced
  orphan), bit-flip detected/excluded/enqueued, an intact-but-misplaced
  fragment detected/excluded/enqueued, and coverage + corruption read back off
  the durability seam. The two corruption legs are flippable (negating
  `fragment_intact` lets the finding be absorbed and the assertions fire),
  proving the verify is load-bearing.
- `crates/core/tests/read_repair.rs` — an EC read excludes a corrupt fragment,
  reconstructs, and enqueues its chunk on the same queue; an unrecoverable
  single-fragment read still leaves the obligation behind.

Each asserting test fails pre-change and passes post-change; `cargo xtask ci`
is Check's to re-run as the gate.

Fixes #143
