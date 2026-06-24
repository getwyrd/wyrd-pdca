# Build notes — issue 207 / scrub-corruption-enqueue-and-continue

Target branch: `getwyrd/wyrd @ main` (worktree base `f12f218`, which already carries
the merged #203 this brief depends on — `crates/chunkstore-fs/src/lib.rs` has the
unique-temp `put_fragment`/`verify`/`get_fragment` region #207 edits).

## Root cause (two sentences)

`FsChunkStore::get_fragment` verifies on read and returns **`Err`** for a corrupt /
misfiled fragment (`crates/chunkstore-fs/src/lib.rs:170`), but scrub fetched with
`let Some(bytes) = store.get_fragment(frag).await? else …` (old `scrub.rs:70`) — the
`?` propagated that `Err` **out of the whole pass**, *before* the corruption branch
(`fragment_intact` → `emit_corruption` → `enqueue_repair`) could run. So for every
verifying backend (the on-disk D server, and the networked one through the gRPC seam)
bit rot was never enqueued, the pass aborted at the first rotten fragment, and the
durability telemetry reported all-green while data rotted.

## Why this is a structural / error-contract fix, not a one-module guard

The brief names an **Invariant to restore** and a Plan-exit self-test: a patch confined
to `scrub.rs` that merely stops `?`-propagating **fails**, because without a
*corrupt-vs-transient* distinction scrub cannot decide enqueue+continue (corruption) vs
propagate/retry (transient). So the fix has to span three seams and keep the two faults
distinguishable from the store all the way to scrub's decision point:

1. **The store error contract** (`traits` + `chunkstore-fs`) — corruption must be a
   *typed*, classifiable fault, distinct from an I/O / transient error.
2. **The gRPC classification seam** (`chunkstore-grpc` server + client) — the
   distinction must survive the wire (`DATA_LOSS`, reconstructed client-side), and a
   malformed-**PUT** verify failure must be a client (`INVALID_ARGUMENT`) fault, not a
   server-internal one that invites futile retries.
3. **Scrub's decision point** (`custodian/scrub.rs`) — classify, then enqueue+continue
   on corruption, propagate on transient.

## The chosen seam: `wyrd_traits::IntegrityFault` + `is_integrity_fault`

`traits/src/lib.rs:63-110` adds a small typed corruption error `IntegrityFault { id,
detail }` and a source-chain classifier `is_integrity_fault(&dyn Error)`. `traits` is the
**only** crate all four participants depend on (`custodian → core → traits`,
`chunkstore-fs → traits`, `chunkstore-grpc → traits`), so it is the one place a *single*
type can be produced by every backend and recognised by every consumer without violating
the dependency rule (ADR-0010). It is an error-contract *definition*, consistent with the
crate's "definitions only" charter.

- `chunkstore-fs/src/lib.rs:141,176` — both `put` and `get` verify failures now surface
  as `IntegrityFault` (the internal `FsChunkStoreError` is kept and carried as `detail`,
  so its existing unit test still holds).
- `chunkstore-grpc/src/server.rs:56-66,86-94` — `is_integrity_fault` ⇒ `get` → `DATA_LOSS`,
  `put` → `INVALID_ARGUMENT`; everything else stays `INTERNAL`.
- `chunkstore-grpc/src/client.rs:19-37,73` — a `DATA_LOSS` get status is reconstructed
  into `IntegrityFault` (the client *has* the `id`), so scrub sees the **same** type from
  a local and a networked store. Other codes stay `TransportError` (unchanged retry
  classification).
- `custodian/src/scrub.rs:67-104` — the fetch is now a `match`: `Ok(Some)` keeps the
  existing `fragment_intact` check (for non-verifying backends like the in-memory fake);
  `Ok(None)` skips; `Err if is_integrity_fault` → `emit_scrubbed` + `emit_corruption` +
  `enqueue_repair` + continue; any other `Err` propagates for the retry policy.

## Alternative rejected, with cost

**`TransportError::Corrupt(Status)` as the carrier scrub branches on** (the brief's first
illustrative option). Rejected for two concrete, checkable reasons:

1. **Scrub cannot see it.** `custodian` does not (and per ADR-0010 must not) depend on
   `chunkstore-grpc`, so scrub cannot `downcast_ref::<TransportError>()`. A corruption
   signal scrub can classify must live at or below the `traits` seam — which is exactly
   `IntegrityFault`. A `TransportError::Corrupt` variant would still need a *second*,
   trait-level signal for scrub, i.e. strictly more surface than `IntegrityFault` alone.
2. **It widens the blast radius.** `TransportError` is matched exhaustively in shipped
   test code — `crates/server/tests/dserver.rs:244-248` matches all four variants with no
   wildcard. Adding a variant forces an edit there (a non-test consumer pattern would
   break too). My approach adds **0** lines to that file; a `Corrupt` variant adds an arm
   to every exhaustive `match` over `TransportError`. (`error.rs`'s `From<Status>` is
   likewise left untouched — `DATA_LOSS` is intercepted in the client's `get` before
   `From` runs, so the existing `status_code_maps_to_the_specific_transport_variant` test
   and the catch-all stay valid.)

The read path (`crates/core/src/read.rs`) is **out of scope** (#198 owns the read-path
chunk-id recheck and the brief forbids editing it). My change only alters *which concrete*
is boxed into `BoxError` from `get_fragment`; the type is still `BoxError`, so `read.rs`
compiles and behaves identically (it already propagated the boxed error). Verified: the
DST `corrupt_fragment_is_read_around` and `scrub_detects_bit_rot_then_reconstructs_q2`
tests stay green.

## Tests (red → green)

Named test file `crates/custodian/tests/scrub.rs`:

- `fschunkstore_corruption_is_enqueued_and_the_pass_continues` — the central, end-to-end
  leg over the **real `FsChunkStore`** (added as a custodian dev-dependency; ADR-0010
  lets a test composition name a concrete). Three referenced fragments, **two** rotted on
  disk; asserts the pass returns `Changed` (not an aborting `Err`), **both** rotten chunks
  are enqueued (order-independent proof it *continued* past the first — leg c), the
  corruption + coverage metrics emit on the durability seam (leg b), the intact fragment
  is still served, and a corrupt fragment is never returned as valid bytes.
- `scrub_propagates_a_transient_get_fault_without_enqueuing` — the other half of the
  distinction: a non-integrity (transient) get error propagates and enqueues nothing.

gRPC legs in `crates/chunkstore-grpc/tests/round_trip.rs`:

- `get_of_a_rotten_fragment_is_an_integrity_fault_over_grpc` — rot a fragment on the
  server's disk; the client error is `is_integrity_fault` and is **not** a
  `TransportError` (distinguishable across the wire).
- `put_of_a_malformed_fragment_is_invalid_argument_over_grpc` — a malformed PUT carries
  `Code::InvalidArgument`, not `Internal`.

**Red proof:** stashing only the behavioural edits (`scrub.rs` + `server.rs`) while
keeping the new types compiling → `fschunkstore_corruption_is_enqueued_and_the_pass_continues`
panics with `Err(Store(IntegrityFault { … "payload checksum mismatch" }))` (the exact
abort the brief describes), and both round_trip legs fail (`Internal` vs `InvalidArgument`;
`is_integrity_fault` false). Restored → all green.

**Gate:** `./engine/xtask.sh ci` (the project's single-sourced `cargo xtask ci` — fmt
`--check`, clippy `-D warnings`, build, full test incl. DST, `cargo deny`, conformance)
→ `xtask ci: all checks passed`. `cargo fmt` applied so the patch is commit-hook clean.

## STOP

Draft only — no PR marked ready, no merge.
