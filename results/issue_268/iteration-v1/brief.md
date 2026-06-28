# Brief — issue 268 / grpc-block-read-fault-wire-signal

> The Plan artifact (docs 02 §PLAN). Human-authored. Do reads ONLY this file.

- **Slug:** grpc-block-read-fault-wire-signal
- **Defect:** A block-layer read fault (`EIO` / dead sector) raised by a *remote* D
  server's `get_fragment` is flattened to a string and collapsed into a generic
  `Status::internal` crossing the gRPC seam. The client rebuilds a typed seam error only
  for `Code::DataLoss` (integrity), so a remote `EIO` arrives with no `io::Error` in its
  source chain. `reconstruction::is_block_read_fault` finds nothing, `assess` classifies a
  real remote dead sector as **transient**, and the chunk is retried forever instead of
  being read around and rebuilt from the ≥k survivors. The #251 read-around fix therefore
  does not reach networked D servers (the dominant production transport).
- **Success criterion:** Over a real gRPC client↔server channel, a `get_fragment` that
  fails at the device with an `EIO` block-read fault is reconstructed client-side into a
  **permanent durability fault** — `is_permanent_read_fault` (and thus
  `reconstruction::assess`) classifies it the same as the local/fs path: read around it
  and rebuild from ≥k survivors. A genuinely transient remote fault
  (`UNAVAILABLE`/`DEADLINE_EXCEEDED`) still reaches the retry policy as a `TransportError`
  (no spurious re-placement). The mechanism (a dedicated `Status` code vs. a typed detail)
  is ILLUSTRATIVE; the BINDING condition is the permanent-vs-transient classification
  surviving the wire round-trip, demonstrated over the gRPC seam (not an in-process trait
  mock).
- **Invariant to restore:** A corruption/permanent durability fault and a transient fault
  are handled differently (repair-and-read-around vs. retry), so they MUST stay
  distinguishable **along the whole path from the store to the consumer's decision point**
  — including across the gRPC seam, where a `Status` carries only `Code` + `String`.
  Source: the `IntegrityFault` seam contract, `crates/traits/src/lib.rs` ("they must stay
  distinguishable along the whole path from the store to the consumer's decision point");
  cited in-code as ADR-0010 (`crates/custodian/src/reconstruction.rs` `is_permanent_read_fault`).
  This covers the *category* "permanent block-layer read fault over the wire," not the EIO
  repro alone.
- **Repo + branch target:** getwyrd/wyrd @ main
- **Conflicts with:**
- **Ordering note:** Independent of 287/288 — touches `chunkstore-grpc` (server/client wire
  contract) and the `reconstruction` classifier, which neither 287 (`gc.rs`) nor 288
  (`core/src/read.rs`) edit. Can build in the same wave as either.
- **Surfaces:** data
- **Difficulty:** high
- **Do model:** opus-xhigh
- **Scope:** Make a remote block-layer read fault reconstructable across the gRPC seam so
  the client-side `ChunkStore` surfaces it as the same permanent-read-fault shape the
  local/fs path already produces, while transient statuses keep their existing
  `TransportError` classification. Decide the closure of "permanent block-layer fault" at
  the seam (errno-5-only vs. a broader dead-sector class — #251 §6 item 2) once, on the
  wire contract, rather than re-deriving it per consumer. / out of scope: changing the
  local/fs classification or `is_block_read_fault`'s errno closure on the local path;
  reconstruction accounting; the Tier-1 `dm-error` harness itself (#195).
- **Repro instruction:** On `main`, stand up a `GrpcChunkStore` client against the in-crate
  D-server service wrapping an inner `ChunkStore` whose `get_fragment` returns an
  `io::Error` with `raw_os_error() == Some(5)`. Observe that the client-side error has no
  `io::Error` in its `source()` chain and `is_permanent_read_fault` returns `false` (the
  fault is classified transient).
- **Test file:** crates/chunkstore-grpc/tests/read_fault_seam.rs
- **Verification posture:** Flippable regression at Check — the test drives a real tonic
  client↔server over an in-process channel (the same seam the existing
  `tier2_integration.rs` exercises, no Docker required), with a fault-injecting inner
  store; red pre-fix (remote EIO classified transient), green post-fix. This exercises the
  WIRE round-trip (a `Status` crossing the seam), not a mock of the `ChunkStore` trait, as
  the acceptance requires.
- **Citations expected:** Do must cite path:line on the target branch for every change
  (`crates/chunkstore-grpc/src/server.rs:83-89`, `crates/chunkstore-grpc/src/client.rs:25-34,:90`,
  and the `reconstruction`/`traits` consumer it satisfies).
- **Prior-art check (triage cycles):** Searched `crates/chunkstore-grpc/src/{server,client}.rs`
  and `crates/custodian/src/reconstruction.rs` history — `is_block_read_fault` /
  `is_permanent_read_fault` landed for the local path in #251 (`8c2adcf`,
  `crates/custodian/src/reconstruction.rs`); #251 deliberately scoped `chunkstore-grpc` out
  and named this as the deferred follow-up. No open PR touches these files
  (`gh pr list` empty). This is the unstarted remote half of #251.
- **Disposition hint:** likely-fix

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a
draft PR MAY happen during the cycle (useful for CI feedback). The PR MUST NOT be
marked ready before sign-off accepts.

## Iteration 1 — carry-forward (from the previous attempt)
- Sign-off rationale: Rejected: the fix reuses the DataLoss → IntegrityFault wire carrier, so a remote block-layer read fault (EIO / dead sector) is recorded downstream as a *corruption* finding (scrub.rs:102 → emit_corruption, scrub.rs:104) and remote scrub diverges from the local/fs path (where a raw io::Error(EIO) is not an IntegrityFault). Mislabeling a dead sector as corruption is not acceptable — it must be its own distinct fault. Re-plan with a distinct typed permanent-read-fault shape that survives the gRPC seam (its own wire code / client mapping) and keeps a dead sector distinguishable from integrity corruption all the way to the scrub/telemetry consumer, instead of collapsing both onto IntegrityFault. The brief currently sanctions the DataLoss=IntegrityFault carrier, so the brief (and ADR-0010 / related ADRs, if they require it) must change — the human is willing to amend the ADR(s) to support a separate permanent-read-fault category.
- Full previous attempt preserved in `iteration-v1/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).
