# Brief (pointer) — issue 144 / reconstruction-custodian

> Plan-pointer brief: per INTEGRATION §6, Wyrd plans through its own artifacts. The
> authoritative plan is **accepted proposal 0005** (Milestone 3 — custodians). This
> file points at it and carries the driver-parsed fields; it does NOT restate the
> design. Do reads the Planning artifact as authoritative and cites it.

- **Slug:** reconstruction-custodian
- **Planning artifact:** `docs/design/proposals/accepted/0005-milestone-3-custodians.md`
  — §"Reconstruction — the heart of M3" (lines 269–286) and §"Repair-vs-serve: dynamic
  priority, not a static throttle" (lines 305–317), plus §"The durability plane"
  (326–332, the three M3 repair metrics). Backed by architecture §6.3
  (`docs/design/architecture/06-runtime-view.md`), §8.9
  (`docs/design/architecture/08-crosscutting-concepts.md`), and ADR-0015
  (`docs/design/adr/0015-consistency-contract.md`, the version-conditional commit
  contract). AUTHORITATIVE — Do treats the proposal as the spec; this brief adds no
  design of its own.
- **Defect / goal:** Wyrd has no reconstruction loop — a lost D server's affected
  chunks stay under-replicated indefinitely, and read-path / scrub checksum failures
  enqueue repair obligations (M3.5, #143) that nothing consumes. Realize the
  **reconstruction custodian**: drain the shared repair queue, rebuild missing
  shard(s) from any `k` survivors, re-place them in distinct failure domains, and
  repoint the placement record with **one version-conditional commit** — plus
  **repair-vs-serve dynamic priority** so a near-floor chunk preempts foreground work.
- **Success criterion:** In the deterministic simulator, kill a D server (or inject a
  scrub/read checksum failure) so affected chunks go under-replicated; the
  reconstruction custodian gathers any `k` surviving fragments, rebuilds the missing
  shard(s) via the chunk's **per-chunk** `EcScheme`, re-places them on healthy D
  servers in **distinct failure domains**, and repoints each chunk's placement record
  with a **single version-conditional `MetadataStore::commit`** — after which the
  affected chunks are back to **full redundancy** and **every read succeeds throughout
  the repair** (no read errors, no torn/hybrid chunk). BINDING: full-redundancy
  restoration + reads-never-error + the location update is one version-conditional
  commit (a crashed repair leaves collectable garbage, never corruption). ILLUSTRATIVE
  (Do's call on mechanism): which queue-scan / priority-function shape is used, and the
  exact `reconstruction.rs` API surface.
- **Repo + branch target:** getwyrd/wyrd @ main   (INTEGRATION §2 — single line, no maintenance branches)
- **Depends on:** 142, 143
- **Ordering note:** #142 (M3.4 GC) makes the displaced/orphaned fragment GC-eligible
  after a reader-safe grace window; #143 (M3.5 scrub) built the shared durable repair
  queue (`wyrd_core::repair::{enqueue_repair, fragment_intact}`) this loop consumes.
  Both must be COMPLETE first.
- **Surfaces:** data (backend / custodian logic; DST is the substrate, no GUI)
- **Scope:** the reconstruction custodian and its repair-vs-serve priority, as
  proposal 0005 §reconstruction / §repair-vs-serve specify — i.e. (a) a custodian loop
  that consumes the shared repair queue, gathers any `k` surviving fragments, verifies
  their checksums (a checksum-failing shard is never decoded), reconstructs the missing
  shard(s) scheme-driven from the per-chunk `EcScheme`, places them on healthy D
  servers in distinct failure domains, and repoints the placement record with one
  version-conditional commit so readers flip atomically and the displaced fragment
  becomes GC-eligible; (b) dynamic repair priority that rises as redundancy falls
  (near-floor chunk preempts foreground), seated on the read-retry reserved seat M2
  (proposal 0004) left in the parallel read path — not a read-path redesign; (c) emit
  the three M3 repair metrics
  (under-replicated chunk count, repair-queue depth, time-to-repair). / **out of
  scope:** the rebalance loop (drain/decommission) — separate M3 slice; the full global
  admission / backpressure scheduler (proposal 0005 §8.9, lands incrementally — build
  the seat + priority function, not a fleet-wide scheduler); sharded scrub/repair
  (Open question, not M3); tenant-key handling (custodian reconstructs ciphertext
  below EC, ADR-0021 — no key access added); dashboards / alerting / UI (ADR-0013,
  deferred); multi-zone replication-lag metric (deferred to M5).
- **Repro instruction:** On `main` at `../wyrd`, the custodian crate
  (`crates/custodian/src/`) has gc / scrub / reconciliation / telemetry but no
  `reconstruction` module — repair obligations enqueued by scrub (#143) are never
  consumed. Drive a DST scenario that kills a D server holding fragments of an
  EC-coded chunk: without a reconstruction loop the chunk stays under-replicated and
  redundancy never recovers.
- **Test file:** `crates/custodian/tests/reconstruction.rs` (mirrors the existing
  `tests/gc.rs` / `tests/scrub.rs`), exercising the kill-and-reconstruct property in
  the simulator: a killed D server's affected chunks return to full redundancy in
  distinct failure domains via one version-conditional commit, with reads succeeding
  throughout. A bug-finding seed promotes to a permanent seeded regression
  (ADR-0009 rule).
- **Verification posture:** the DEFAULT flippable-test posture HOLDS — **C4-verify is
  binding and red→green applies.** The test ships as its own file
  (`crates/custodian/tests/reconstruction.rs`, the gate's ADDED_TEST discriminator), so
  C4-verify keeps it on the revert leg, drops the reconstruction production code, and
  requires the test to fail — exactly the gate's net-new path (run-verify.sh:125-148).
  Do drives the test through the **stable `reconcile_step` seam** (adding the
  `reconstruction: Option<&ReconstructionContext>` slot the loop "does not yet
  dispatch", `crates/custodian/src/reconciliation.rs`) and asserts on **outcome via the
  public repair-queue API** — `wyrd_core::repair::{enqueue_repair, queued_repairs}` —
  plus a full-redundancy/distinct-domain check, mirroring `tests/scrub.rs`. One
  PRE-DECLARED nuance so the reviewer isn't surprised: because the `reconstruction`
  dispatch param is genuinely net-new (no inert scaffolding on `origin/main`, unlike
  gc/scrub), the C4-verify revert-leg red is expected to be a **build-level** red (the
  reverted production removes the dispatch the kept test calls), which the gate honors
  as red. To prove the assertion is not vacuous, Do MUST ALSO show an **assertion-level
  red** in `build-notes.md`: with the dispatch wired, negate the placement/commit step
  (à la scrub's `fragment_intact` negation, `tests/scrub.rs:16`) and show the test fails
  on "chunk stays under-replicated / obligation not drained". Whole-gate confirmation:
  `./engine/xtask.sh ci` (incl. the DST sweep) exits 0 in `$PDCA_WORKTREE`.
- **Citations expected:** Do must cite path:line on `main` for every change AND the
  proposal-0005 section it realizes (e.g. `0005:269-286`, `0005:305-317`).
- **Prior-art check (searched by file path — merged history / open / closed PRs):**
  `crates/custodian/src/reconstruction.rs` does not exist on `main` (lib.rs lists only
  gc / leadership / reconciliation / scrub / telemetry and flags reconstruction as "a
  later slice"); the repair-queue substrate it consumes was merged by M3.5 (#143) and
  the GC of displaced fragments by M3.4 (#142). No prior or in-flight reconstruction
  loop found — this is the first implementation of the M3.6 slice.
- **Disposition hint:** likely-fix

## STOP discipline

Draft only until Check sign-off. A draft PR MAY be opened for CI; the PR MUST NOT be
marked ready before sign-off accepts.
