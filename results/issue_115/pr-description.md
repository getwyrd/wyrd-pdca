# Parallel any-*k*-arrive-first read (M2.5)

The fifth step of M2 (proposal 0004, "Read — any-*k*-arrive-first" / PR step 5).
Turns erasure coding into a tail-latency *advantage*: a read reconstructs from
whichever *k* fragments verify first instead of waiting on the slowest *m*.

## Root cause

`core::read_chunk`'s `EcScheme::ReedSolomon` arm fetched a chunk's fragments
**serially in index order** — `for index in 0..(k+m) { get_fragment(..).await }`,
awaiting each fetch before issuing the next and breaking at *k*. Only one fetch is
ever outstanding, so the read's latency is the sum of the *k* fetches it happens to
hit in index order, and a single slow or unreachable D server at a low index stalls
the whole read.

## Fix

`crates/core/src/read.rs` — the `ReedSolomon` arm replaces the serial loop with a
`FuturesUnordered` fan-out over all `n = k + m` fragment indices:

- fire `get_fragment` at all *n* indices at once;
- push each shard that verifies its checksum/decodes, and `break` once *k* are in
  hand — dropping the outstanding fetches, which cancels them;
- a fragment that is missing (`Ok(None)`), fails its checksum/decode (`Err`), or is
  slow/unreachable (its future simply hasn't resolved) is treated as **absent** and
  read around — a corrupt shard is never handed to the decoder;
- below *k* valid fragments it still returns the typed
  `ReadError::InsufficientFragments` — no panic, no short/corrupt read.

The fan-out is single-task cooperative concurrency (it polls the futures, never
spawns), so fragment-completion ordering stays seed-driven and the read remains
deterministic under simulation (**ADR-0009**). The `EcScheme::None` arm is
unchanged — a single fetch. `futures-util` (alloc-only, already in the workspace)
supplies `FuturesUnordered`.

## Verified against

`docs/design/proposals/accepted/0004-milestone-2-networked-d-servers.md` (lines
259-265 "Read — any-*k*-arrive-first"; 442-444 PR step 5 DoD) and architecture
`docs/design/architecture/06-runtime-view.md` §6.2 (read path), §6.6 (consistency).

`cargo xtask ci` (fmt, clippy `-D warnings`, build, full test suite, `cargo deny`,
conformance, madsim DST) green.

## Test

- **`crates/server/tests/dst_read_fanout.rs`** (new, the named DST oracle) — a
  seed-reproducible `wyrd_testkit::Sim` test in the style of `dst_erasure.rs`. An
  `ArrivalStore` fake delays each `get_fragment` by a seed-ranked number of poll
  yields, so the *n* fetches complete in a seed-driven permutation. Over a 64-seed
  sweep + a pinned regression seed it asserts: byte-identical rs(6,3)
  reconstruction from whichever *k* arrive first (**arrival/index-order
  independent**); **peak in-flight == n** (the discriminator — the serial read
  peaks at 1); **exactly k fetches reach the inner store** (the slow *m* are
  cancelled); below *k* a clean typed `InsufficientFragments { have: 5, need: 6 }`;
  and `EcScheme::None` stays a single fetch. **Red on `main`** (`peak left: 1,
  right: 9`), **green on the fix**.
- **`crates/server/tests/read_fanout.rs`** (complementary) — an over-the-wire
  check: an rs(6,3) GET over real gRPC reconstructs from the first *k* and abandons
  hung D servers; below *k* surfaces the typed error.

`async-trait` enters `crates/server` dev-deps for the in-test `ChunkStore` fake.

## Out of scope

The typed `TransportError` enum / `Ok(None)` not-found contract (M2.2, already on
`main`); the parallel fan-out **write** (M2.4, merged); gateway-level distinct-D-server
endpoint selection beyond the `ChunkStore` seam; M3 repair-vs-serve throttling; the
network DST harness (proposal PR step 6) and the throughput bench (step 7).

Refs #115

🤖 Generated with [Claude Code](https://claude.com/claude-code)
