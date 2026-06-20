# Build notes — issue 115 / parallel-any-k-read (iteration 2)

## What this iteration changes vs. v1

The carry-forward was explicit: **the fix is accepted, rework the tests.** So:

- `crates/core/src/read.rs` — the M2.5 `FuturesUnordered` fan-out is **unchanged**
  from v1 (read.rs:18, 93-138 on the working tree). It fires `get_fragment` at all
  `n = k+m` indices, reconstructs from the first `k` that verify their checksums
  (read.rs:117-128), and breaks/drops the rest once `k` are in hand (read.rs:121-124).
  This is the change accepted under V (fitness-to-purpose) in the v1 sign-off.
- **New named DST oracle:** `crates/server/tests/dst_read_fanout.rs` — a
  seed-reproducible `wyrd_testkit::Sim` test in the style of `dst_erasure.rs`. This is
  the deliverable the v1 review demanded and the v1 attempt lacked.
- The v1 networked test `crates/server/tests/read_fanout.rs` is **kept as a
  complementary over-the-wire check** (the carry-forward sanctioned this: "may stay …
  but does not substitute for the named DST oracle"). It exercises the real gRPC
  `FanoutChunkStore`/`GrpcChunkStore` path the DST test cannot. It is no longer the
  sole oracle.
- Reverted a stray v1 working-tree edit to `wyrd.code-workspace` (unrelated to this
  change).

## The carry-forward, point by point

1. *"deliver the named seed-reproducible DST test that exercises the simulation
   harness"* → `dst_read_fanout.rs` drives the read through `wyrd_testkit::Sim`
   exactly as `dst_erasure.rs` does (`Sim::new(seed)`, seeded RNG, `pollster::block_on`,
   single-threaded, pinned regression seed). No `multi_thread` tokio, no network.
2. *"assert any-k reconstruction is independent of arrival/index order across a seed
   sweep — not the single hung arrangement (indices 0,1,2)"* → each of 64 seeds draws a
   fresh **arrival permutation** (`arrival_yields`, a Fisher-Yates shuffle of `0..n`
   from the seeded RNG); the winning `k` set therefore varies seed to seed, and the
   test asserts byte-identical reconstruction every time. ADR-0009 determinism is
   *exercised*, not merely asserted in a comment: completion ordering is literally the
   seed-driven permutation, and re-running a seed replays it.

## The hard design problem and the pivot (the part worth reading)

The natural first design — model each fragment's *arrival latency* as a number of
poll "ticks" (`Delay { remaining }`) and give the read a bounded poll budget so the
serial read "times out" deterministically — **does not work**, and I want to record
why so it isn't re-attempted.

Under a single-threaded cooperative executor (`block_on` / `FuturesUnordered`), the
tick model collapses to *serial advancement*: I measured the parallel read completing
in **~121 polls** for delays `{0,8,16,24,32,40}`, i.e. ≈ the **sum** (120) of the six
smallest delays, not the **max** (40) I'd predicted. The futures are registered
concurrently but advanced essentially one at a time, so parallel-cost ≈ Σ(k smallest
delays) and serial-cost ≈ Σ(delays of indices 0..k-1). Those overlap (both equal 120
whenever indices 0..k-1 happen to be the fastest), so **no poll budget cleanly
separates serial from parallel** — the discriminator would be flaky. (This is also the
"no wall-clock hang" trap the Do instructions warn about, surfacing in poll-space.)

Pivot: discriminate on **concurrency**, not latency. The one thing the fix changes
that is observable deterministically and finitely is *how many `get_fragment` calls are
in flight at once*:

- serial (`main`) awaits fetch `i` before issuing fetch `i+1` → **peak in-flight = 1**;
- parallel fan-out issues all `n` before any completes → **peak in-flight = n**.

`ArrivalStore` records this with a `Probe` (an in-flight counter + running max, plus a
`reached_inner` counter), giving each fetch a single guaranteed `Yield` (`rank+1 ≥ 1`)
so all `n` are simultaneously in flight on the first `FuturesUnordered` pass before any
completes. The assertion `peak == N` is **red on serial (peak 1), green on the fix
(peak 9)** — verified below — with finite yields, so `block_on` cannot hang. The same
`Yield` ranks give the seed-varied completion order for the arrival-independence and
cancellation properties, so one mechanism serves all three.

Cost of the rejected tick-budget approach, concretely: it needed two hand-tuned magic
constants (`STEP`, `BUDGET`) whose validity I could not prove across seeds (measured
parallel 119–122 vs. a predicted 41 — a 3× model error), and the serial/parallel cost
ranges *overlap*. The concurrency oracle needs **zero** tuned constants and is exact:
`1` vs `n`.

## Properties in `dst_read_fanout.rs`

- `any_k_arrive_first` (64 seeds + pinned): byte-identical reconstruction from the
  first `k` by arrival; `peak == N` (fan-out — the discriminator); `reached_inner == K`
  (the slow `m` are cancelled, never reaching the inner store).
- `below_k_is_a_clean_typed_error` (64 seeds + pinned): `m+1` deleted → only `k-1`
  survive → clean typed `ReadError::InsufficientFragments { have: 5, need: 6 }`, no
  panic/short read; `peak == N` (still fans out before failing closed). Success
  criterion: "below `k` … a clean typed error."
- `none_scheme_is_a_single_fetch` (64 seeds + pinned): `EcScheme::None` reads its one
  fragment with `peak == 1`, `reached_inner == 1`. Success criterion: "`EcScheme::None`
  stays a single fetch."

## Red → green evidence (project runner)

- Fix in place: `cargo test -p wyrd-server --test dst_read_fanout` → **4 passed**.
- Serial read restored from `HEAD` (`git show HEAD:crates/core/src/read.rs`):
  `any_k_arrive_first_across_seeds`, `below_k_…`, and the pinned-seed test **FAIL** with
  `peak left: 1, right: 9`; `none_scheme_…` still passes (the None arm is unchanged).
  Restored the fix afterward.
- Full gate `./engine/xtask.sh ci` (fmt --check, clippy -D warnings, build, whole test
  suite incl. both new tests, `cargo deny`, conformance, madsim DST) → **"xtask ci: all
  checks passed."**
- `cargo fmt --all -- --check` clean (the new file was run through `cargo fmt`; the
  target's pre-commit formatter would accept it).

## Why the test lives in `crates/server/tests/`

Same home as `dst_erasure.rs` (the brief's named default and style reference). It is a
plain integration test under `cargo test --workspace` — not the `--cfg madsim`
`wyrd-dst` crate — exactly like `dst_erasure.rs`, so the `ci` test step runs it. It is
import-light: no GUI/display, no tokio runtime, no network; `FsChunkStore` uses blocking
`std::fs` inside its async methods, so it resolves under `block_on` with no runtime
(headless-safe, and it cannot recur as an import-time crash).

## Citations (getwyrd/wyrd @ main working tree)

- Fix: `crates/core/src/read.rs:18` (import), `:93-138` (the `ReedSolomon` fan-out),
  `:121-124` (break/cancel at `k`), `:129-136` (typed `InsufficientFragments`),
  `:82-92` (the unchanged `None` single-fetch arm).
- Dev-dep: `crates/server/Cargo.toml:39-41` (`async-trait` for the in-test
  `ChunkStore` fake), and the corresponding `Cargo.lock` entry.
- Planning artifact: `docs/design/proposals/accepted/0004-milestone-2-networked-d-servers.md`
  lines 259-265 ("Read — any-*k*-arrive-first") and 442-444 (PR step 5 DoD);
  architecture `docs/design/architecture/06-runtime-view.md` §6.2 (read path), §6.6
  (consistency).
