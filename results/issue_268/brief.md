# Brief — issue 268 / grpc-block-read-fault-distinct-wire-category

> The Plan artifact (docs 02 §PLAN). Human-authored. Do reads ONLY this file.
> The field labels below are parsed by the driver — keep the `- **Label:** value`
> shape. The success criterion is the load-bearing field: it is the sentence
> Check tests "did this work" against.

- **Slug:** grpc-block-read-fault-distinct-wire-category
- **Defect:** A block-layer read fault (`EIO` / dead sector) raised by a *remote* D
  server's `get_fragment` cannot be reconstructed on the client as its own fault category.
  On `main` the server maps only `is_integrity_fault` to `Code::DataLoss`
  (`server.rs:83-89`); every other failure — including an `io::Error` with
  `raw_os_error() == Some(5)` — flattens to `Status::internal`. The client rebuilds a typed
  seam error only for `Code::DataLoss` (`client.rs:25-33`), so a remote `EIO` arrives with
  no `io::Error` in its `source()` chain: `reconstruction::is_block_read_fault` finds
  nothing, `assess` classifies a real remote dead sector as **transient**, and the chunk is
  retried forever instead of being read around and rebuilt from the ≥k survivors. The #251
  read-around therefore does not reach networked D servers (the dominant production
  transport). *(Iteration-1 carry-forward: the v1 fix made remote `EIO` ride the existing
  `Code::DataLoss` → `IntegrityFault` carrier. That restored the read-around but collapsed a
  dead sector onto **corruption**, so remote scrub `emit_corruption`s a block-read fault at
  `scrub.rs:102-104` while the local path does not (`scrub.rs:108` `Err(e) => return Err(e)`)
  — a remote-vs-local telemetry divergence the human rejected. See §Iteration.)*
- **Success criterion:** Over a real gRPC client↔server channel, a `get_fragment` that
  fails at the device with an `EIO` block-read fault is reconstructed client-side into a
  **block-layer read fault** that, at every consumer, is classified IDENTICALLY to the
  local/fs path:
  (a) `reconstruction::is_permanent_read_fault` (hence `assess`) treats it as **permanent**
      — read around it and rebuild from the ≥k survivors; AND
  (b) it is **NOT** an `is_integrity_fault`, so the scrub consumer does **not** record it as
      a corruption finding — it takes the SAME branch a local `EIO` does at `scrub.rs:108`
      (`Err(e) => return Err(e)`), never `emit_corruption` / a "scrub" repair enqueue.
  A genuinely transient remote fault (`UNAVAILABLE` / `DEADLINE_EXCEEDED`) still reaches the
  retry policy as a `TransportError` (no spurious re-placement). The wire MECHANISM (a
  dedicated `Status` code vs. a typed detail vs. a new seam fault type) is ILLUSTRATIVE; the
  BINDING condition is that the fault **category** — corruption vs. block-read fault vs.
  transient — survives the wire round-trip INTACT (remote == local at *every* consumer:
  reconstruction read-around, scrub telemetry, and the retry policy), demonstrated over the
  gRPC seam (not an in-process trait mock).
- **Invariant to restore:** A corruption fault, a block-layer read fault, and a transient
  fault are each handled DIFFERENTLY by the durability consumers — corruption is a
  repair-and-continue + corruption-telemetry obligation, a block-read fault is a permanent
  read-around that is NOT corruption, and a transient fault is a retry — so the three MUST
  stay mutually distinguishable **along the whole path from the store to the consumer's
  decision point**, INCLUDING across the gRPC seam where a `Status` carries only `Code` +
  `String`. A remote dead sector must reach the scrub/telemetry consumer as a block-read
  fault, never collapsed onto either corruption (`IntegrityFault`) or the transient class.
  Source: the `IntegrityFault` seam-contract doc, `crates/traits/src/lib.rs:56-82` ("the two
  faults are handled differently … so they must stay distinguishable along the whole path
  from the store to the consumer's decision point"), tied to ADR-0010; and the permanent-vs-
  transient split `crates/custodian/src/reconstruction.rs:312-342`, `scrub.rs:90-108`, and
  the read path honour. This covers the *category* "permanent block-layer read fault over
  the wire, distinct from corruption," not the EIO repro alone.
- **Repo + branch target:** getwyrd/wyrd @ main
- **Depends on:**
- **Conflicts with:**
- **Ordering note:** Independent of 287/288 — touches `chunkstore-grpc` (server/client wire
  contract), the `traits` seam crate, and the custodian `reconstruction`/`scrub` consumers;
  neither 287 (`gc.rs`) nor 288 (`core/src/read.rs`) edit these. Can build in the same wave
  as either.
- **Surfaces:** data
- **Difficulty:** high
- **Do model:** opus-xhigh
- **Scope:** Make a remote block-layer read fault travel the gRPC seam as its OWN distinct
  permanent-read-fault category — distinct both from a transient fault AND from integrity
  corruption — so that every durability consumer classifies a remote dead sector exactly as
  it classifies a local one: reconstruction reads around it (permanent), scrub treats it the
  same as the local `EIO` path (NOT a corruption finding), and a transient status keeps its
  existing `TransportError` classification. Decide the closure of "permanent block-layer
  fault" (errno-5 / `EIO` only vs. a broader dead-sector class — #251 §6 item 2) ONCE, on
  the wire contract, rather than re-deriving it per consumer; default to the existing
  errno-5-only closure unless the host decision widens it. / out of scope: changing the
  local/fs classification or `is_block_read_fault`'s errno closure on the local path;
  changing `IntegrityFault` / corruption semantics; reconstruction accounting; the Tier-1
  `dm-error` harness itself (#195); reusing `Code::DataLoss` / `IntegrityFault` as the remote
  block-read-fault carrier (the v1 approach the human rejected — see §Iteration).
- **Repro instruction:** On `main`, stand up a `GrpcChunkStore` client against the in-crate
  D-server service wrapping an inner `ChunkStore` whose `get_fragment` returns an
  `io::Error` with `raw_os_error() == Some(5)`. Observe (1) the client-side error has no
  `io::Error` in its `source()` chain and `reconstruction`'s permanent-read-fault classifier
  returns `false` (the fault is classified transient → retried forever); and (2) it is also
  not surfaced as any block-read-fault category distinct from corruption at the scrub
  consumer.
- **Test file:** crates/chunkstore-grpc/tests/read_fault_seam.rs
- **Verification posture:** Flippable regression at Check — the test drives a real tonic
  client↔server over an in-process channel (the seam the existing `tier2_integration.rs`
  exercises, no Docker required) with a fault-injecting inner store; red pre-fix (remote EIO
  classified transient AND/OR indistinguishable from corruption), green post-fix. The test
  MUST exercise the WIRE round-trip (a `Status` crossing the seam), not a mock of the
  `ChunkStore` trait. Assert BOTH halves of the success criterion over the reconstructed
  client error: it satisfies the permanent-read-fault classifier (read-around) AND is NOT an
  `is_integrity_fault` (so scrub would not `emit_corruption`) — pinning that a dead sector
  stays distinguishable from corruption past the seam, which the v1 suite could not.
- **Citations expected:** Do must cite path:line on the target branch for every change
  (`crates/chunkstore-grpc/src/server.rs:83-89`, `crates/chunkstore-grpc/src/client.rs:25-33,:90`,
  the `crates/traits/src/lib.rs` seam type/classifier it adds or extends, and the
  `reconstruction.rs`/`scrub.rs` consumers it satisfies).
- **Prior-art check (triage cycles):** Searched `crates/chunkstore-grpc/src/{server,client}.rs`,
  `crates/traits/src/lib.rs`, and `crates/custodian/src/{reconstruction,scrub}.rs` on
  `origin/main`. `is_block_read_fault` / `is_permanent_read_fault` live in `reconstruction.rs`
  for the LOCAL path (landed in #251); #251 deliberately scoped `chunkstore-grpc` out and
  named this the deferred follow-up. The v1 attempt for THIS issue (preserved in
  `iteration-v1/`) was rejected at sign-off; its rejected approach (DataLoss=IntegrityFault
  carrier) is explicitly out of scope above. No open PR touches these files. This is the
  re-planned remote half of #251.
- **Disposition hint:** likely-fix

## Plan note — ADR / seam-contract change (NEEDS-HUMAN by design)

Introducing a THIRD seam fault category (a permanent block-layer read fault distinct from
`IntegrityFault`) extends the seam contract documented in `crates/traits/src/lib.rs:56-82`
and tied to ADR-0010 (and the durability-telemetry ADR-0011). Per INTEGRATION §2/§4 an
accepted ADR is immutable and any ADR/spec change is architecture-board authority — a
Check NEEDS-HUMAN, not a model's to accept. The human has pre-committed (iteration-1
sign-off rationale) to amending the ADR(s) to support a separate permanent-read-fault
category. **Decided (human, this cycle):** Do ships ONLY the code change + the
`crates/traits/src/lib.rs` seam-doc update + the regression test; it does NOT author an ADR.
If the change turns out to require a new superseding/companion ADR (or an ADR-0011 telemetry
update), the human authors/accepts it SEPARATELY — Do must not block on it and must not edit
an accepted ADR (INTEGRATION §2 immutability).

## Iteration 2 — carry-forward (from the previous attempt)

- Iteration-1 sign-off rationale (REJECTED): the v1 fix reused the `DataLoss` →
  `IntegrityFault` wire carrier, so a remote block-layer read fault is recorded downstream
  as a *corruption* finding (`scrub.rs:102` → `emit_corruption`, `scrub.rs:104`) and remote
  scrub diverges from the local/fs path (where a raw `io::Error(EIO)` is NOT an
  `IntegrityFault`). Mislabeling a dead sector as corruption is not acceptable — it must be
  its own distinct fault. Re-plan with a distinct typed permanent-read-fault shape that
  survives the gRPC seam (its own wire code / client mapping) and keeps a dead sector
  distinguishable from integrity corruption all the way to the scrub/telemetry consumer.
- Full previous attempt preserved in `iteration-v1/` (patch.diff, build-notes.md,
  SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged (it is out of scope).
  Satisfy this brief's Success criterion (the end result), both halves.

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a
draft PR MAY happen during the cycle (useful for CI feedback). The PR MUST NOT be
marked ready before sign-off accepts.
