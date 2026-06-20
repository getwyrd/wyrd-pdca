# Brief (pointer) — issue 116 / m2.6-tier1-network-dst

> A Plan artifact that is a **pointer**: the plan already lives in an accepted Wyrd
> proposal (governed under GOVERNANCE / the ADR process), not in a brief authored
> here. Proposal 0004 §"DST and integration tests (the heart of M2)" — its **Tier-1**
> subsection and **suggested PR step 6** — *is* the plan; ADR-0009 is the defining
> ADR. This file references them and carries the fields the driver parses; Do reads
> the proposal as authoritative and does not restate it.

- **Slug:** m2.6-tier1-network-dst
- **Planning artifact:** `docs/design/proposals/accepted/0004-milestone-2-networked-d-servers.md`
  — authoritative. Read specifically: §"DST and integration tests (the heart of M2)"
  → **Tier-1** (the five enumerated properties), the §"Crate touch-points" rows for
  `testkit` / `dst` / `Cargo.toml`, and **suggested PR step 6**. Supporting normative
  source: `docs/design/adr/0009-deterministic-simulation-testing.md` (DST is the
  spine; "Jepsen-style fault injection begins as soon as there is a networked path";
  "a DST seed that finds a bug is committed as a permanent regression test").
- **Defect / goal:** The gRPC `ChunkStore` data path (M2.1–M2.5, all merged on `main`:
  the proto service, `GrpcChunkStore` client + D-server service, d-server discovery,
  parallel fan-out write, any-*k* read) has **no deterministic network-fault coverage**.
  `testkit` has `Clock`/`Disk` seams but no network seam, and `wyrd-dst` exercises only
  the in-process commit protocol. Grow a **network seam** in `testkit` and run the
  *real* `GrpcChunkStore` wire code on madsim's simulated network (via `madsim-tonic`,
  cfg-aliased), asserting the Tier-1 properties under seed-reproducible faults.
- **Success criterion:** `cargo xtask dst` (the `--cfg madsim` seed sweep,
  `MADSIM_TEST_NUM=50`) runs **green**, with the new network-DST tests asserting all
  five Tier-1 properties from proposal 0004 over the real `GrpcChunkStore` on madsim's
  simulated network: (1) parallel-write durability — all *n* fragments readable on
  their distinct D servers after a fan-out commit; (2) *k*-of-*n* over the network with
  drops/delays — byte-identical reconstruction when up to *m* fetches drop/delay;
  (3) re-read-on-corruption — a checksum-failing fragment is treated as absent and
  re-read elsewhere, read still succeeds; (4) fail-closed partial write — an injected
  drop/partition/timeout **aborts pre-commit**, leaving only leased garbage, never a
  half-committed chunk; **and** (5) the M0/M1 commit-protocol property suite
  (concurrent-writer-one-wins, atomicity, no-hybrid-read) re-runs **unchanged** with
  the gRPC `ChunkStore` under madsim — proving the trait seam is real. Determinism
  holds despite the new parallelism (`select`/`join` ordering is seed-driven). The new
  test file is **red before** the seam + tests land and **green after**; any
  bug-finding seed is committed as a permanent regression test (ADR-0009).
- **Repo + branch target:** getwyrd/wyrd @ main   (per INTEGRATION §2 — Wyrd has no
  maintenance branches; everything targets `main`. Note the target checkout currently
  sits on `docs/pdca-adoption-plan-pilot-status`; Do branches from a clean `main`.)
- **Depends on:** none open — M2.4 (#114) and M2.5 (#115), the stated needs, are both
  merged on `main`.
- **Surfaces:** data   (DST / backend test infrastructure + a `testkit` seam; no GUI).
- **Scope:** **suggested PR step 6 only** — one logical change: (a) add `madsim-tonic`
  cfg-aliased so the *same* `tonic` client code resolves to the simulated transport
  under `--cfg madsim` (the version matrix — `madsim-tonic 0.6.0+0.14` tracks
  `tonic 0.14` — is already settled in the Cargo.toml dep notes, so the real wire code
  is the primary path); (b) grow a **network seam** in `testkit` (drop / delay /
  partition / corruption fault points) alongside the existing `Clock`/`Disk` seams;
  (c) add network-DST tests in `wyrd-dst` asserting the five Tier-1 properties and
  re-running the M0/M1 commit suite over the gRPC `ChunkStore`; (d) include them in the
  `cargo xtask dst` seed sweep and commit any bug-finding seed.
  **Out of scope:** Tier-2 container/testcontainers integration and the throughput
  benchmark (proposal step 7 — a separate issue); degraded-write tolerance, write-back
  repair/scrub, mTLS, real-etcd discovery, and failure-domain-aware placement (all
  deferred by proposal 0004 §"Out of scope" to M3/M5–M7). The documented **fallback** —
  a ChunkStore-level in-sim fake instead of `madsim-tonic` — is the recorded retreat
  only if the real wire code cannot run under madsim; it is **not** the primary, since
  the version risk is already retired.
- **Repro instruction:** on the target branch, `./engine/xtask.sh dst` (or
  `cargo xtask dst` in the checkout) currently exercises only the in-process commit
  protocol (`crates/dst/tests/concurrency.rs`) — there is no network-fault campaign and
  no `GrpcChunkStore`-over-madsim coverage. The new test file fails (does not exist /
  no faults injected) before the seam + tests land, and passes across the 50-seed sweep
  after.
- **Test file:** `crates/dst/tests/network.rs` (new) — the Tier-1 network-DST campaign
  (the five properties) plus the M0/M1 commit-suite re-run over the gRPC `ChunkStore`,
  all under `#![cfg(madsim)]` and run by `cargo xtask dst`. Do may add a small
  shared harness module if the commit-suite re-run is better expressed as a
  store-parameterized variant of `concurrency.rs`; the network campaign itself ships in
  `network.rs`.
- **Citations expected:** Do must cite `path:line` on the target branch (`testkit`
  seam, `wyrd-dst` tests, workspace `Cargo.toml`) **and** the Planning artifact
  (proposal 0004 Tier-1 properties / step 6; ADR-0009) for every change.
- **Prior-art check (triage cycles):** searched by affected path across merged history,
  open PRs, and closed PRs. Merged M2 chain: #122 (M2.1 proto), #128 (M2.2 grpc), #129
  (M2.3 d-server), #130 (M2.4 write), #135 (M2.5 read) — all upstream of this; **none**
  touch `crates/dst/tests/network.rs`, a `testkit` network seam, or add `madsim-tonic`.
  No open/closed PR and no branch references M2.6 / network DST. `madsim-tonic` appears
  only as a forward-looking comment in the workspace `Cargo.toml` ("added in M2.6").
  Genuinely unbuilt — no duplicate, no superseded attempt.
- **Disposition hint:** likely-fix

## STOP discipline

Draft only until Check sign-off. A draft PR MAY be opened for CI; the PR MUST NOT be
marked ready before sign-off accepts.
