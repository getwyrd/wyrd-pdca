# Brief (pointer) — issue 117 / m2.7-tier2-integration-throughput-bench

> A Plan artifact that is a **pointer**: the planning decision for this work already
> lives — reviewed and governed — in Wyrd's accepted M2 implementation proposal. This
> issue is **PR step 7** (the final step) of that proposal's suggested sequence. This
> file references the proposal and carries the fields the driver parses; Do reads the
> **Planning artifact** as the authoritative plan and does not need it restated here.

- **Slug:** m2.7-tier2-integration-throughput-bench
- **Planning artifact:** `../wyrd/docs/design/proposals/accepted/0004-milestone-2-networked-d-servers.md`
  — authoritative. Read in full; the directly governing sections are **§ "DST and
  integration tests (the heart of M2)" → "Tier-2 — integration against real backends,
  born at M2"**, **§ "Benchmarks"**, the **§ "Crate touch-points"** `xtask` row, and
  **PR step 7** of "Suggested PR sequence". Supporting: architecture
  `10-quality-risks-glossary.md` §13 (testing strategy, Tier 2) and ADR-0009.
- **Defect / goal:** the **Tier-2 integration tier born at M2 does not yet exist** —
  there is no end-to-end test that drives an S3-style write/read across **real
  networked gRPC D servers under containers** (real tonic / HTTP-2 framing / prost
  (de)serialization / connection lifecycle), and there is no aggregate write/read
  **throughput benchmark** across D-server counts. Steps 1–6 (#112–#116) built and DST-
  validated the data path in simulation; step 7 validates it against reality and makes
  the §10 Q6 throughput claim first measurable.
- **Success criterion:** a new Tier-2 integration test, run on a checkout with a
  container runtime available, stands up **multiple real networked gRPC D servers**
  under docker-compose / testcontainers and performs an end-to-end S3-style
  **write → read under `rs(6,3)`**, asserting the read is **byte-identical** to the
  written object — exercising real tonic transport, not an in-process server. AND
  `cargo xtask bench` runs an **aggregate write/read throughput benchmark across
  D-server counts** and records first data points (tracked, not gated). Both the
  integration test and the bench **compile and are wired into `cargo xtask`**.
  Demonstrable at C4-verify on a Docker-capable host. NOTE: the container test is a
  **nightly / heavier-runner CI job**, NOT part of the docs-skippable default
  `cargo xtask ci` lane (cf. M1.7: CI gates the bench's *compilation*, the run is
  noisy and untracked-as-gate); "green on the nightly container job" and the recorded
  throughput numbers are **supplementary** evidence that clears after merge, not the
  Check criterion.
- **Repo + branch target:** getwyrd/wyrd @ main   (INTEGRATION §2 — single line; no maintenance branches)
- **Surfaces:** data   (backend/transport only; no GUI surface)
- **Depends on:** none open — PR steps 1–6 (#112–#117 predecessors #112/#113/#114/#115/#116) are merged on `main`; this is the last M2 step.
- **Conflicts with:** none
- **Scope:** realize **PR step 7 only** — (a) a Tier-2 container integration test
  driving end-to-end write/read over real gRPC D servers, and (b) the tracked
  `cargo xtask bench` throughput benchmark across D-server counts, plus the
  `xtask` integration runner that launches/tears down the container D servers and the
  CI container job that runs it. The in-CI obligation is to prove the data path builds
  **no shared bottleneck** (the §10 Q6 basis), per the proposal. / **out of scope:**
  anything the proposal's "Out of scope" boundary defers (failure-domain-aware
  placement / durability math, write-back repair / scrub / rebalance, degraded-write
  tolerance, durability telemetry, mTLS/PKI, real etcd discovery, TiKV); any change to
  the `ChunkStore` trait, the `wyrd.v0` proto service, or the commit protocol (all
  fixed by steps 1–6); replacing or weakening the Tier-0/Tier-1 deterministic spine
  (Tier-2 complements DST, it does not replace it).
- **Test file:** `../wyrd/crates/chunkstore-grpc/tests/tier2_integration.rs`
  (the container-driven end-to-end write/read test; gated so the default
  `cargo test` does not require Docker — run via the new `cargo xtask integration`).
  The throughput benchmark ships under `../wyrd/crates/core/benches/` (e.g.
  `throughput.rs`, alongside the existing `erasure.rs`) and is run via
  `cargo xtask bench`. Final paths/crate are Do's call; these are the named homes.
- **Citations expected:** Do must cite path:line on `main` AND the Planning artifact
  (proposal 0004, PR step 7 / Tier-2 / Benchmarks sections) for every change.
- **Prior-art check (triage cycles):** searched the target checkout by area —
  `git -C ../wyrd ls-files | grep -i 'bench|integration|compose|docker'` returns only
  `crates/core/benches/erasure.rs` (M1.7 EC micro-bench); no Tier-2 / container /
  compose / testcontainers files exist. `gh pr list --state all` for
  tier-2/testcontainer/throughput/117 shows steps 1–6 merged and **no** open or
  closed/rejected step-7 work. Net-new; no duplication or superseded attempt.
- **Disposition hint:** likely-fix

## Sign-off note (expected NEEDS-HUMAN)

This step adds **new dependencies** — a container-test harness (e.g. `testcontainers`)
and tonic's container-exercised transitives — which triggers the **ADR-0003 three-test
dependency audit + `deny.toml` allowlist** review (INTEGRATION §4: "any new dependency
or license" is reviewer NEEDS-HUMAN, the human's to accept, not a model's). Expect the
reviewer to raise it; the human clears it at §6 before accept.

## STOP discipline

Draft only until Check sign-off. A draft PR MAY be opened for CI; the PR MUST NOT be
marked ready before sign-off accepts.
