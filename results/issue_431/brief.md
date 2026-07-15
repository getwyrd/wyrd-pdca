# Brief — issue 431 / read-block-fault-repair-obligation

> The Plan artifact (docs 02 §PLAN). Do reads ONLY this file.

- **Slug:** read-block-fault-repair-obligation
- **Defect:** The Reed-Solomon foreground read path reads AROUND a permanent block-layer
  read fault and returns the object, but enqueues no repair obligation for the damaged
  shard. In the RS fan-out's error handling (`crates/core/src/read.rs:306-380`), a typed
  integrity fault is recorded as corrupt and enqueued (`read.rs:362-365`), but every other
  fetch error — including a permanent `BlockReadFault` — falls into the final arm and is
  emitted as `FaultClass::Transient` and read around with NO obligation (`read.rs:379`;
  the arm's own comment says "#431 owns the block-fault repair question"). Yet
  `wyrd_traits::BlockReadFault` (`crates/traits/src/lib.rs:164-199`) is documented as
  PERMANENT damage — retrying the same fetch cannot help — and the custodian already
  classifies it as a permanent read fault (`crates/custodian/src/reconstruction.rs:475`,
  via its own private source-chain walker `:485-495`, which matches `BlockReadFault`
  through the synthetic EIO it exposes via `source()`).
  A degraded read succeeds from the remaining `k` shards while the cluster silently
  carries avoidable durability debt until some later scrub happens upon the same fault.
- **Success criterion:** A foreground RS read that encounters a block-layer read fault on
  one shard (while ≥ k others remain readable) still returns the correct bytes AND lands
  the affected chunk on the shared repair queue (`repair::queued_repairs`) with a
  non-corruption `detected_by` reason (e.g. `"read-block-fault"`), WITHOUT incrementing
  the corruption-specific fault signals (`FaultClass::Corrupt` / `IntegrityFault`
  emissions, `read.rs:178-186`). Demonstrated by the new test file below: red on base
  (queue stays empty today), green with the fix, under C4-verify.
- **Falsifiability:** RED is producible in-process on the base toolchain: on
  `origin/main`, a test-double store returning `Err(BlockReadFault)` for one fragment
  (the classifier `wyrd_traits::is_block_read_fault`, `traits/src/lib.rs:339`, matches
  it) leaves the repair queue EMPTY after a successful read — the new test's queue
  assertion fails. Plain `#[test]`, no cfg gate; C4-verify's red leg reverts
  `read.rs` and keeps the added test file.
- **Invariant to restore:** Durable damage detected at read time is never absorbed
  silently — the read path feeds the SAME shared repair queue scrub feeds (proposal
  0005:174-176; the existing read-path producer contract pinned by
  `crates/core/tests/read_repair.rs`). "Durable damage" is decided by the system's single
  decision point for permanence — `wyrd_traits::is_block_read_fault`
  (`traits/src/lib.rs:325-350`) — not re-derived inline; a block fault is permanent
  damage to read around AND rebuild, distinct from checksum corruption.
- **Repo + branch target:** getwyrd/wyrd @ main
- **Conflicts with:** 430
- **Ordering note:** #430 and #431 both edit the RS fan-out match in
  `crates/core/src/read.rs` (~:306-380) — no build-on dependency, but a shared file:
  schedule in different waves. Suggested order: 430 first, 431 after (this bundle's
  change is one arm + the enqueue plumbing and rebases trivially).
- **Surfaces:** data
- **Difficulty:** medium
- **Scope:** one logical fix in `crates/core/src/read.rs`: distinguish
  `is_block_read_fault` from generic transient errors in the fragment-fetch error
  handling and record a repair obligation for the block-fault case with a
  non-corruption reason, preserving the existing distinction that block faults are not
  checksum corruption (no corruption-metric increment; the enqueue seam is
  `repair::enqueue_repair`, `crates/core/src/repair.rs:78` — `detected_by` is free-form,
  today `"scrub" | "read"`; the read-path drain sites are `read.rs:431-433` and `:476-477`).
  / out of scope: #430's fragment-identity validation (a different arm of the same
  match), reclassifying any OTHER non-integrity error (timeouts/unavailable stay
  transient and un-enqueued — `read.rs:366-380`'s telemetry-only handling stands),
  widening the permanent-fault class beyond EIO/`BlockReadFault` (deferred per #251, see
  `traits/src/lib.rs:331-332`), and custodian-side behaviour (reconstruction already
  handles block faults, `reconstruction.rs:475-497`).
- **Repro instruction:** On `origin/main`: RS(2,1) object over a test-double store where
  one D-server's `get_fragment` returns `Err(BlockReadFault::new(...).into())` and the
  other two serve intact fragments; call `read::read_object` (or `read_path`). The read
  returns the bytes; `repair::queued_repairs(meta)` returns `[]` — the permanent fault
  produced no obligation (only `FaultClass::Transient` telemetry fired, `read.rs:379`).
- **External dependencies:** none
- **Test file:** crates/core/tests/read_block_fault_repair.rs   (NEW file — the C4-verify
  gate earns its red only from an added `*/tests/*.rs`; do not append to an existing
  suite. ASSERTION SHAPE: `repair::queued_repairs` returns chunk ids ONLY
  (`repair.rs:91-98`) — to prove the non-corruption `detected_by` reason, read the repair
  key's VALUE back through the `MetadataStore` (`repair_key(chunk)` → the stored
  `detected_by` bytes). The no-corruption-signal leg is satisfied by that recorded reason
  (≠ the corruption producers' `"read"` / integrity classes); a tracing-capture assertion
  over the counters is optional hardening, not the binding red.)
- **Citations expected:** Do must cite path:line on `main` for every change. Composition
  peers Do MAY open: the integrity-fault arm it mirrors
  (`crates/core/src/read.rs:362-365` — enqueue + read-around, but with a DIFFERENT
  fault class/reason), and the test-double harness in
  `crates/core/tests/read_repair.rs:74-151` (`MemChunks` / `IntegrityFaultingStore` —
  clone the shape with a block-faulting store).
- **Prior-art check (triage cycles):** searched by file path — `git -C ../wyrd log` over
  `crates/core/src/read.rs`: 1d2a469 ("name the failing fragment and its D-server") added
  the telemetry in the final error arm and EXPLICITLY deferred this fix ("#431 owns the
  block-fault repair question", read.rs:367); baadd11 handled block faults in the
  CUSTODIAN read-around, not the foreground read. Still unfixed on today's `main`
  (verified by Read). No closed/rejected PR found for this path.
- **Disposition hint:** likely-fix

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a
draft PR MAY happen during the cycle (useful for CI feedback). The PR MUST NOT be
marked ready before sign-off accepts.

## Iteration 1 — carry-forward (from the previous attempt)
- Sign-off rationale: Auto-iterate (round 1): Check found implementation-level items only, no architectural judgment required — C4 Verification (red→green) — Accept the aggregate gate only after rerunning `cargo xtask ci` on a host permitted to bind loopback — focused red→green passed independently, but this sandbox stopped an unrelated gRPC test with `Operation not permitted`, so the asserted complete green was not reproducible here (`crates/chunkstore-grpc/tests/list_delete.rs:55`).; T4 Contribution — Decide whether affected-path prior art is clear for contribution — local merged/all-ref history for `crates/core/src/read.rs` shows the earlier telemetry work but no equivalent fix, while closed/rejected remote work could not be mechanically established from the supplied artifacts.
- Full previous attempt preserved in `iteration-v1/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 2 — carry-forward (from the previous attempt)
- Sign-off rationale: Auto-iterate (round 2): Check found implementation-level items only, no architectural judgment required — C4 Verification (red→green) — Accept the aggregate gate only after rerunning `cargo xtask ci` on a host permitted to bind loopback — focused red→green passed independently, but this sandbox stopped an unrelated gRPC test with `Operation not permitted` (`crates/chunkstore-grpc/tests/list_delete.rs:55`).; T4 Contribution — Decide whether affected-path prior art is clear for contribution — local merged/all-ref history shows no equivalent fix, but closed/rejected remote work cannot be mechanically established from the supplied artifacts (`crates/core/src/read.rs:396`).
- Full previous attempt preserved in `iteration-v2/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 3 — carry-forward (from the previous attempt)
- Sign-off rationale: Auto-iterate (round 3): Check found implementation-level items only, no architectural judgment required — C4 Verification (red→green) — Accept aggregate verification only after `cargo xtask ci` runs on a host permitted to bind loopback — focused red→green passed independently, but this host stopped the unrelated gRPC test at `crates/chunkstore-grpc/tests/list_delete.rs:55` with `Operation not permitted`.; T4 Contribution — Decide whether affected-path prior art is clear for contribution — local merged/all-ref history for `crates/core/src/read.rs` shows telemetry and corruption-repair predecessors but no equivalent block-fault fix, while closed/rejected remote work could not be mechanically established from the supplied environment (`crates/core/src/read.rs:396`).
- Full previous attempt preserved in `iteration-v3/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).
