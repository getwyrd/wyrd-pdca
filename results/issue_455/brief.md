# Brief — issue 455 / e2e-closed-write-path

> The Plan artifact (docs 02 §PLAN). Human-authored. Do reads ONLY this file.
> The field labels below are parsed by the driver — keep the `- **Label:** value`
> shape. The success criterion is the load-bearing field: it is the sentence
> Check tests "did this work" against.
>
> Design foundation (read in place on `feat/m4-production-metadata-backend`, not copied):
> proposal 0015 (`docs/design/proposals/accepted/0015-milestone-4-production-metadata-backend-revised.md`),
> architecture §7.4 (single-zone day-one durability loop), ADR-0008 (backend = composition,
> not refactor), ADR-0014 (redb dev / TiKV prod). Umbrella campaign: #367 (first-deployment
> gate), #366 (observability floor / live exporter), #257 (Tier-1/2 real-store campaign).

- **Slug:** e2e-closed-write-path
- **Defect:** The write→durability loop's halves are each proven in isolation but never
  **joined**. `s3_gateway_cluster.rs` (#454 / PR #459) proves a gateway S3 PUT fans chunks to
  D-servers and a GET reads them back; `custodian_day_one.rs` (#450) proves the under-replicated
  backlog gauge rises-then-returns-to-zero through the deployable custodian role — but it
  **hand-writes** the object into the store via `wyrd_core::write::write_new_object_placed`
  (`crates/core/src/write.rs:333`), never via a gateway PUT. So "the loop closes" — that a
  gateway-written object, over the *shared* cluster metadata store, is a **custodian-visible
  repair obligation** — is unproven. This is exactly the failure the issue names: a
  `--metadata-backend tikv` custodian can open a store nothing wrote and see zero repair work.
- **Success criterion:** An in-process test drives a **gateway S3 PUT** that writes object
  metadata to a **shared** cluster metadata store and fans erasure-coded chunks across loopback
  D-servers (real gRPC); a **custodian opened over that SAME store** then observes the object as a
  **non-zero repair obligation**; killing one D-server raises the under-replicated backlog gauge
  to ≥1 and it returns to 0 after the custodian reconstructs; and a GET reads the object back
  **byte-identical**, reconstructed from D-server fragments. Demonstrable by C4-verify
  (`cargo xtask ci` on the new test, in isolation at Check). Do NOT scope the criterion to the
  live `deploy/small-multi-node/` Docker run — that is off-Check (see Verification posture).
- **Falsifiability:** RED is producible on the loopback topology Do is pointed at (loopback gRPC
  D-servers + a shared redb/in-memory metadata store; e.g. rs(6,3), so killing 1 of 9 D-servers
  is repairable). The load-bearing join assertion — *a custodian reading the store a gateway PUT
  wrote sees the object as a repair obligation, so the backlog gauge reads ≥1 after a D-server is
  killed* — goes RED if the gateway's write path and the custodian's repair scan disagree on
  placement (the custodian would see **0** obligations, the very "empty store" symptom). Do MUST
  capture a **demonstrated** red load-bearingly (a temporary negation/stub proving the custodian's
  obligation count derives from the gateway-written placement), not rest red on the test's prior
  non-existence. The live-stack green (real TiKV metadata, 9 Docker D-servers, live Prometheus
  exporter) is deferred off-Check — it is NOT the environment Do is graded on here.
- **Invariant to restore:** The **write path and the repair path share one placement contract**:
  the metadata a gateway PUT records over the cluster metadata store must be sufficient for a
  custodian reading the *same* store to compute that object's repair obligations — neither path
  may record or read a placement shape the other cannot honor. This is the empirical test of
  ADR-0008's stated consequence, *"backend choice becomes a composition concern in `server`, not a
  refactor"* (`docs/design/adr/` ADR-0008; architecture §7.4 day-one durability loop; ADR-0014
  redb=dev / TiKV=prod). SELF-TEST: a one-module guard cannot satisfy it — it is a cross-crate
  contract spanning the gateway write, the custodian repair scan, and the D-server placement.
- **Repo + branch target:** getwyrd/wyrd @ feat/m4-production-metadata-backend   (M4 integration
  branch per INTEGRATION §2; the slice PR opens against it, not `main`)
- **Depends on:** 454
- **Conflicts with:**
- **Ordering note:** #455 depends on #454 (gateway composes over cluster backends). #454's fix
  already **merged** onto the target branch `feat/m4-production-metadata-backend` as PR #459
  (`Fixes #454`, commit 99889ba), so the dependency is materially satisfied by the base the slice
  builds on. The `Depends on: 454` field is kept for honest ordering: if #454 is re-briefed in this
  batch it must land in an earlier wave. No file conflict foreseen — this slice adds a new test
  file and, at most, reconciles the shared placement contract.
- **Surfaces:** data   (backend/logic only; no frontend — the S3 surface is driven in-process)
- **Difficulty:** medium   (a new cross-crate integration test composing gateway + custodian +
  D-server fleet over one shared store; blast-radius stays in `crates/server/tests` unless closing
  the loop surfaces a small placement-contract reconciliation across gateway/custodian — rated up
  from `low` for that cross-crate reach)
- **Scope:** Join the gateway **write path** and the custodian **repair path** over one shared
  cluster metadata store into a single closed-loop proof — so a gateway-written object is a
  custodian-visible repair obligation and survives a D-server loss (gauge rises→returns-to-zero),
  and round-trips byte-identical. / **out of scope:** the live `deploy/small-multi-node/` Docker
  demonstration (runs off-Check under the #367 first-deployment campaign); the live-TiKV / live-etcd
  backend arms (the in-process proof uses redb/in-memory + `MemCoordination`; TiKV/etcd are
  off-Check); building or changing the live Prometheus scrape / OTLP collector run (#366); any
  gateway/custodian production-wiring change beyond what closing the loop strictly requires.
- **Repro instruction:** on `feat/m4-production-metadata-backend`, no test drives a gateway PUT
  into a store a custodian then sweeps — `crates/server/tests/custodian_day_one.rs` hand-writes its
  objects (`wyrd_core::write::write_new_object_placed`, `crates/core/src/write.rs:333`) and
  `crates/server/tests/s3_gateway_cluster.rs` never stands up a custodian. Add the joined test and
  run `cargo test -p wyrd-server --test closed_write_path` (red before the join is proven, green
  after); the whole gate is `./engine/xtask.sh ci`.
- **External dependencies:** **none** beyond the base Rust toolchain for the Check slice — the
  in-process proof uses in-workspace deps (`aws-sdk-s3`, `tonic`, `tempfile`) already exercised by
  the two peer tests. The DEFERRED live demonstration needs Docker + the `deploy/small-multi-node/`
  stack (3× TiKV/PD, 3-node etcd, 9 D-servers, Prometheus) — off-Check, NOT required to build or
  verify this slice. Do MUST NOT silently substitute a live-stack dependency into the Check slice.
- **Test file:** crates/server/tests/closed_write_path.rs
- **Verification posture:** DEFAULT does not fully hold — declare two postures:
  (a) **Net-new coverage** (born-at-tier): "red" is criterion-*absence* — there is no prior joined
  assertion to flip. What IS built AND exercised at Check: the joined in-process closed-loop test
  over a shared redb/in-memory store + loopback gRPC D-servers, driving the **shipping** composition
  surfaces (the gateway PUT path — `serve_s3_role` / `Gateway` — and the custodian sweep +
  durability gauge). Do MUST capture a *demonstrated* red where feasible (a temporary negation
  proving the custodian's obligation count depends on the gateway-written placement), not rest red
  on non-existence.
  (b) **Deferred / off-Check** (inert at Check): the live `deploy/small-multi-node/` durability
  demonstration — real TiKV metadata, 9 Docker D-servers, kill a node, watch the under-replicated
  gauge rise then return to zero — is confirmed by an operator running the #367 first-deployment
  campaign, NOT at Check. FORCING FUNCTION (deferred ≠ unbuilt): the DATA-PLANE half of that live
  run is built and runnable *today* — the compose file builds `wyrd-single-zone:local`
  (`crates/chunkstore-grpc/tests/dserver/Dockerfile`, `--features tikv,etcd`) and stands the full
  topology up, so a real S3 PUT→GET round-trip surviving a D-server kill is exercisable off-Check.
  But the **observable durability *signal* on the live stack is NOT yet built**: the custodian
  exposes no live Prometheus scrape port ("in-process Prometheus read-back only",
  `deploy/README.md`); the live scrape endpoint is **#366 (observability floor), open and
  unbuilt**. So the fully-observable live demonstration (the gauge seen through a live exporter)
  **depends on #366** and is a **separate work item**, NOT something this slice builds or waves
  through. What #455 delivers is the in-process, regression-guarded proof that the loop's LOGIC
  closes; the gateway/custodian roles, the OTLP push surface, and the in-process Prometheus
  registry that this test drives all exist and are exercised here.
- **Production reach:** the in-process proof shares **one** metadata-store instance between the
  gateway PUT and the custodian sweep (a directly-held / Arc'd store, or a sequentially-shared redb
  file), standing in for the live TiKV keyspace both roles would share in production; the live
  path (`--metadata-backend tikv` gateway + custodian over one networked TiKV store) is exercised
  off-Check on the deploy stack (#367 campaign). The shared store exercises the placement-contract
  seam **load-bearingly** — the custodian's obligation count is computed from the object the gateway
  actually wrote, not a hand-authored fixture (that hand-authored fixture is precisely what this
  slice removes from the loop-closure claim).
- **Citations expected:** Do must cite `path:line` on `feat/m4-production-metadata-backend` for
  every change. **Peer callsites to mirror (composition slice):**
  - Gateway S3 PUT over cluster backends → `serve_s3_role` (`crates/server/src/cli.rs:1289`), as
    driven by `crates/server/tests/s3_gateway_cluster.rs` (loopback D-servers via `spawn_dserver`;
    PUT/GET via stock `aws-sdk-s3`; `count_fragments` proves fan-out). For the store-sharing S3 PUT
    over a **directly-held** `Gateway` (so one store can be written then read), mirror
    `crates/server/tests/e2e.rs` (`Gateway::new(store, chunks, coord)` + S3 ops).
  - Custodian sweep + durability gauge → `run_reconstruction_over_backend`
    (`crates/server/src/cli.rs:751`), `require_aligned_topology` (`cli.rs:800`),
    `live_reconstruction_view` (`crates/server/src/custodian.rs:200`), `connect_fleet`
    (`custodian.rs:143`), `run_reconstruction_until` (`custodian.rs:309`) — as driven by
    `crates/server/tests/custodian_day_one.rs` (the gauge rises-then-returns-to-zero surviving a
    killed D-server).
  - The **join** replaces `custodian_day_one.rs`'s hand-written object
    (`wyrd_core::write::write_new_object_placed`, `crates/core/src/write.rs:333`) with the real
    gateway PUT, over one shared store. Do MAY open those cited peer callsites to copy the
    composition; pick the wiring (library `Gateway` vs `serve_s3_role` role) that lets ONE store be
    written by the gateway and read by the custodian — redb holds an exclusive OS file lock
    (`crates/server/src/cli.rs:582-584`), so a running-gateway + running-custodian pair over one
    redb *file* needs a shared-access realization; leave that mechanism to Do.
- **Prior-art check (triage cycles):** searched by affected file path across merged history, the
  M4 branch commits, and closed PRs. `crates/server/tests/s3_gateway_cluster.rs` (#454 / PR #459)
  = gateway PUT→D-servers→GET, **no custodian**. `crates/server/tests/custodian_day_one.rs` (#450)
  = custodian gauge rise/return, **object hand-written**, no gateway. `crates/dst/tests/custodian.rs`
  = the DST campaign. **None** joins a gateway PUT to a custodian sweep over a shared store — the
  loop-closure is genuinely unproven. No superseding/duplicate/rejected prior work found.
- **Disposition hint:** likely-fix

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a
draft PR MAY happen during the cycle (useful for CI feedback). The PR MUST NOT be
marked ready before sign-off accepts.

## Iteration 1 — carry-forward (from the previous attempt)
- Sign-off rationale: Why rejected (issue_455): the closed-loop join is not actually proven to close on its own. The test hand-feeds the repair obligation via `repair::enqueue_repair(&meta, chunk_id, "health")` (patch line 296) before the custodian observes it. The custodian must *derive* the obligation from the gateway-written placement, not be handed a pre-enqueued queue entry — otherwise the load-bearing assertion (`under_replicated == 1.0`) can pass even if the gateway→custodian derivation is broken. This is the exact "empty store" failure the issue exists to catch, and the manual enqueue papers over it. What to change next: - Remove the manual `enqueue_repair`. Have the custodian compute the under-replicated obligation from the placement the gateway PUT actually recorded + the observed D-server loss (drive the shipping obligation-discovery path, not an injected queue entry). - Capture a DEMONSTRATED red (brief.md:37-41, 90-92): a temporary negation — e.g. drop the D-server kill, or write a placement the custodian cannot read — must flip the load-bearing assertion to red, proving the obligation count derives from the gateway write. Absence-only red (test did not exist before) is not sufficient here; the C4-verify gate FAILED (test passes without a fix) and the reviewer flagged C2. Cleared at this sign-off (still hold on the rebuild): T2 Shape (directly-held `Gateway::put_object` is brief-permitted), T3 Runtime (xtask ci green in the gate env; only the reviewer sandbox could not bind loopback), T5 Judgment (no duplicate prior art). Deferred: §6 Validation — fitness-to-purpose to be re-reviewed after iterate-do, once the loop is shown to close without the manual enqueue.
- Failing gate: C4 per-fix red->green: this patch's test red pre-fix, green post-fix (advisory) — run-verify.sh: FAIL — the test PASSES without the fix, so it does not catch the bug (no red).
- Full previous attempt preserved in `iteration-v1/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).
