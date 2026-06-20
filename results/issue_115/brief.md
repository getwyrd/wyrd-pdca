# Brief (pointer) — issue 115 / parallel-any-k-read

> A Plan artifact that is a **pointer**: issue 115 (M2.5) realizes a step of an
> already-accepted host plan, so this brief references that plan rather than
> restating it. Do reads the **Planning artifact** as authoritative and reads the
> spec fields from the `- **Label:** value` lines below.

- **Slug:** parallel-any-k-read
- **Planning artifact:** `docs/design/proposals/accepted/0004-milestone-2-networked-d-servers.md`
  — PR step **5** ("Any-*k*-arrive-first read") and the "Read — any-*k*-arrive-first"
  section; cross-read with architecture `docs/design/architecture/06-runtime-view.md`
  §6.2 (read path) and §6.6 (consistency). These are the authoritative, governed plan
  for this change. Do MUST cite them.
- **Defect / goal:** `core::read_chunk` fetches a chunk's fragments **serially in
  index order** (`crates/core/src/read.rs:80`, the `EcScheme::ReedSolomon` arm:
  `for index in 0..(k+m)` awaiting each `get_fragment` and breaking at `k`). M2.5
  replaces this with a **parallel any-*k*-arrive-first** read that reconstructs from
  the first `k` fragments that verify their checksums and cancels the rest — turning
  erasure coding into a tail-latency advantage instead of waiting on the slowest `m`.
- **Success criterion:** for an `rs(6,3)` chunk, a GET reconstructs **byte-identically**
  from whichever `k` fragments verify first; up to `m` missing / slow / corrupt
  fragments are read around; below `k` available it returns a **clean typed error**
  (no panic, no short/corrupt read); outstanding fetches are cancelled once `k`
  verify. `EcScheme::None` stays a single fetch. `cargo xtask ci` exits 0.
- **Repo + branch target:** getwyrd/wyrd @ main
- **Scope:** in `core::read_chunk` (`crates/core/src/read.rs`, the
  `EcScheme::ReedSolomon` arm), replace the in-order serial fetch with a concurrent
  fan-out over all `n = k+m` fragment indices, reconstructing from the first `k` that
  verify their checksums and abandoning the remaining fetches; a fragment that is
  missing (`Ok(None)`), fails its checksum/decode, is unreachable, or is slow is
  treated as **absent** and read around, consuming the existing transport-error
  policy. The `EcScheme::None` arm is unchanged. Determinism under simulation
  (ADR-0009) must hold — fragment-completion ordering stays seed-driven.
  / out of scope: the typed `TransportError` enum and the `Ok(None)` not-found
  contract (already on `main` — `crates/chunkstore-grpc/src/error.rs`, `client.rs`,
  M2.2); the parallel fan-out **write** (M2.4, merged); gateway-level distinct-D-server
  endpoint selection beyond the `ChunkStore` seam; M3 repair-vs-serve throttling (this
  change only leaves the read-retry path as the reserved seat); the network DST harness
  (proposal PR step 6) and integration/throughput bench (step 7).
- **Test file:** `crates/server/tests/dst_read_fanout.rs` — a new seed-reproducible
  DST test in the style of `crates/server/tests/dst_erasure.rs`. It must assert the
  any-*k*-arrive-first property: an `rs(6,3)` GET reconstructs byte-identically from
  any `k` surviving fragments **independent of arrival/index order**, with up to `m`
  fragments missing/corrupt/slow read around, and a clean typed error below `k`. It
  must fail against `main`'s serial read (which the parallel-read property
  distinguishes) and pass after the fix. (If Do finds the property is more naturally
  asserted by extending `dst_erasure.rs`, it may do so and note the deviation — the
  named file is the default home.)
- **Citations expected:** Do must cite `path:line` on `getwyrd/wyrd@main` AND the
  Planning artifact for every change.
- **Disposition hint:** likely-fix

## STOP discipline

Draft only until Check sign-off. A draft PR MAY be opened for CI; the PR MUST NOT be
marked ready before sign-off accepts.

## Iteration 1 — carry-forward (from the previous attempt)
- Sign-off rationale: V (fitness-to-purpose) accepted — the parallel any-k read delivers the M2.5 tail-latency intent. The implementation patch (read.rs FuturesUnordered fan-out) is not in question. Rework the tests, not the fix: - C5 / T5: deliver the named seed-reproducible DST test (dst_read_fanout.rs, in the style of dst_erasure.rs) that exercises the simulation harness. The shipped read_fanout.rs runs real multi_thread tokio and never touches the sim path, so ADR-0009 determinism-under-simulation (brief.md:33) is asserted only by comment (patch.diff:60-63), not exercised. - The DST test must assert any-k reconstruction is independent of arrival/index order across a seed sweep — not the single hung arrangement (indices 0,1,2) the current networked test covers. Order-independence + seed-reproducibility are the properties under-covered today (brief.md:40-48). - The networked read_fanout.rs may stay as a complementary over-the-wire check, but it does not substitute for the named DST oracle.
- Full previous attempt preserved in `iteration-v1/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).
