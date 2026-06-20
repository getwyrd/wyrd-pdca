# Build notes — issue #115 / m2.5-any-k-arrive-first-read

## Success criterion (the real end result)

A networked `rs(6,3)` GET returns byte-identical bytes by reconstructing from
whichever 6 fragments verify their checksums **first**; up to `m` missing / slow /
corrupt fragments are read *around* **without the read stalling on them**; below
`k` readable fragments returns a clean typed error (no panic, no corruption);
outstanding fetches are abandoned once `k` verify; `EcScheme::None` stays a single
fetch.

## Root cause

`core::read_chunk` (`crates/core/src/read.rs:88-120` on `main`, the
`EcScheme::ReedSolomon` arm) fetched fragments **in order, one `.await` at a
time** (`for index in 0..(k+m) … get_fragment(...).await`). A single slow/hung D
server at an early index blocks the whole serial loop *before* `k` good fragments
are gathered — even though only `k` of `n` are needed. EC was a tail-latency *tax*,
not an advantage. (Proposal 0004, "Read — any-*k*-arrive-first"; architecture
§6.2: "needs only *k* of *n* … reconstructs from whichever *k* arrive first … re-
read elsewhere on failure", and §6.6.)

## Fix (the smallest change that restores the §6.2 invariant)

Replace the serial loop with a concurrent fan-out (`crates/core/src/read.rs`,
`read_chunk` ReedSolomon arm in the patch):

- Build a `FuturesUnordered` of all `n = k+m` `get_fragment` futures, each tagged
  with its index.
- Drain it in **completion order**, pushing every fragment that yields
  `Ok(Some(_))` *and* decodes (`decode(..).is_ok()`) into `shards`. A fragment that
  is missing (`Ok(None)`), fails its checksum / can't decode (`Err`), or is
  slow/unreachable (future simply hasn't resolved) is treated as **absent** — the
  four-way taxonomy from the Planning artifact collapses to "absent → read around"
  at this layer (the typed `TransportError`, M2.2, lives a layer down and is
  already merged; this read consumes its `Result`).
- The moment `shards.len() == k`, `break` — dropping the `FuturesUnordered` cancels
  (abandons) the outstanding fetches.
- Below `k` after the stream drains: the existing
  `ReadError::InsufficientFragments` typed error, unchanged.

`EcScheme::None` is untouched (still a single fetch). `erasure::reconstruct` is
order-independent (`crates/core/src/erasure.rs:106-116` bins shards by global
index), so taking shards in *arrival* order rather than *index* order is correct —
confirmed by the existing `dst_erasure` suite still passing.

### Why `FuturesUnordered` (single-task, no spawn)

This mirrors the M2.4 write fan-out's deliberate design (`core::write_fragments`
uses `futures_util::future::try_join_all`, `crates/core/src/write.rs:163`). Both
poll the `n` futures **cooperatively on one task** — no `tokio::spawn`, no runtime
tie-in — so completion ordering is seed-driven and the read stays deterministic
under madsim (ADR-0009). The workspace `futures-util` pin
(`Cargo.toml:56`, `default-features=false, features=["alloc"]`) already exists and
its comment explicitly names "the parallel write fan-out / any-k read (M2.4/M2.5)";
`FuturesUnordered`/`StreamExt` are in the `alloc` feature, so **no dependency
change** to `core`.

### Alternatives weighed

- **`select_all` loop** (`futures_util::future::select_all`): also `alloc`-only and
  single-task, but it returns `(result, index, remaining_vec)` and rebuilds the
  remaining `Vec` each iteration — O(n²) churn and noisier control flow for the same
  semantics. `FuturesUnordered` expresses "first `k` to complete, then drop the
  rest" directly. Not a correctness difference; chose the clearer one.
- **A per-request client deadline to bound the hung case**: the proposal's taxonomy
  maps `DeadlineExceeded → absent`, but `GrpcChunkStore` sets no per-call deadline
  today (`crates/chunkstore-grpc/src/client.rs:61-73`) and the read-retry/deadline
  *throttle* is the M3 "reserved seat" (proposal 0004, out-of-scope; brief Scope).
  Adding a deadline here would be M3 scope creep. The any-`k`-arrive-first structure
  already delivers the success criterion without it: we never *wait* on the slow
  `m`; we only stop polling them once `k` verify. (The one residual: below-`k` with
  genuinely-hung — never-erroring — servers cannot terminate without a deadline.
  That is a `DeadlineExceeded` concern deferred to M3; the test exercises below-`k`
  via cleanly-down servers, which error promptly, matching today's transport.)

## Test — `crates/server/tests/read_fanout.rs` (new), the read-side mirror of `write_fanout.rs`

Networked, real-tonic, over loopback gRPC D servers — the same harness shape as the
merged `write_fanout.rs`, so it runs on the headless `cargo xtask ci` runner (no
display; tokio + tonic only, exactly like the existing server integration tests).

A `FaultStore` wraps `FsChunkStore`: `put`/`health` always delegate (so the write
fan-out commits and discovery sees the server live), but with `hang_get` set every
`get_fragment` does `std::future::pending().await` — a server that accepts the
request then stalls **indefinitely** (slow/hung, *not* cleanly down; a clean-down
quick error already drops through the serial loop, which the brief calls out).

- `rs_read_reconstructs_from_the_first_k_and_abandons_hung_d_servers` — 9 D servers,
  indices {0,1,2} hang reads, {3..8} serve. The fan-out routes index `i`→server `i`,
  so the serial loop hits hung index 0 first. A `tokio::time::timeout(10s)` around
  `read_object_from` turns the pre-fix stall into a clean **red** (live fan-out
  answers in ms, far inside the budget); post-fix reconstructs **byte-identical**
  from the 6 that arrive first and abandons the 3 hung fetches → **green**. This is
  the discriminating property.
- `below_k_readable_fragments_is_a_clean_typed_error` — write with all 9 live, then
  stop 4 cleanly (5 < k=6 readable); assert the read returns, downcasts to
  `ReadError::InsufficientFragments { have: 5, need: 6, .. }` — no panic, no corrupt
  bytes. An invariant guard (passes pre- and post-fix).

`async-trait` added to `crates/server/Cargo.toml` **dev-dependencies** only (for the
`FaultStore` `ChunkStore` impl); already in the workspace + `Cargo.lock` (used by
`core`/`traits`), so no lock change.

## Red→green proof (project toolchain, `cargo` under the wyrd checkout)

- **Red** (read.rs reverted to `main`'s serial loop):
  `rs_read_..._abandons_hung_d_servers` FAILS at `read_fanout.rs:203` —
  `Elapsed(())` after the 10s budget (serial read stalls on hung index 0);
  `below_k_...` passes. (1 passed, 1 failed.)
- **Green** (with the fix): both pass in 0.19s.
- No regression: `cargo test -p wyrd-core -p wyrd-server` all green, including
  `dst_erasure` (loss/corruption-beyond-`m` clean error across seeds) and the
  existing `read_path` / `erasure_path` / `write_fanout` suites — confirming the
  arrival-order reconstruction stays deterministic and correct.
- `cargo fmt --check` clean; `cargo clippy --all-targets -- -D warnings` clean on
  `wyrd-core` + `wyrd-server`.

## Citations

- Change: `crates/core/src/read.rs` (`read_chunk` ReedSolomon arm + module/fn docs +
  `futures_util` import) on `main`; `crates/server/Cargo.toml:36-41` (dev-dep);
  new `crates/server/tests/read_fanout.rs`.
- Precedent: `crates/core/src/write.rs:163` (M2.4 single-task fan-out),
  `crates/server/tests/write_fanout.rs` (cluster harness), `Cargo.toml:52-56`
  (futures-util pin naming M2.5).
- Plan: proposal 0004 "Read — any-*k*-arrive-first" / "Error taxonomy" (PR-step-5),
  DST Tier-1 properties 2 & 3; architecture §6.2 read path, §6.6.
