# Brief (pointer) — issue 143 / m3.5-scrub-custodian

> Plan artifact for an implementation slice whose design ALREADY lives in an
> accepted, immutable host artifact — proposal 0005 (Milestone 3 — custodians),
> PR-sequence slice **5** (`0005:528-530`). This brief POINTS at 0005 (it does not
> restate or re-decide the design; INTEGRATION §2/§6) and carries the fields the
> driver/Do parse, plus the structural-slice fields (invariant, posture, prior-art)
> this category needs. Do reads the **Planning artifact** as authoritative.
>
> This is a **structural / lifecycle slice** (it introduces a new shared durability
> seam — the reconstruction/repair queue — with two producers and a future
> consumer), so the invariant is stated wide and Scope names no mechanism. It passes
> the category-gated Plan-exit gate: Scope names no probe/guard/helper, and the
> invariant is not satisfiable by guarding one module (it spans scrub + the read
> path + the queue seam).

- **Slug:** m3.5-scrub-custodian
- **Planning artifact:** `docs/design/proposals/accepted/0005-milestone-3-custodians.md`
  — §"The four custodian loops" / **Scrub** (`0005:262-267`); §6.3 step 1 and the
  read-path mirror §6.2 (`0005:264-266`); the read-time-failure feed (`0005:174-176`);
  the durability-plane metrics **scrub coverage** + **scrub-detected corruption rate**
  (`0005:331-332`); the PR-sequence DoD (slice 5, `0005:528-530`). Supporting:
  **ADR-0011** (telemetry from first commit), **ADR-0012** (backend-agnostic OTel),
  architecture §6.3/§6.2/§8.3. Authoritative; Do cites it for every design claim.
- **Defect / goal:** M3 has the read path's *read-time* checksum verification
  (`core/read.rs:11-16`, a corrupt shard is read *around* and never decoded) but **no
  proactive scrub**: nothing walks a D server's stored fragments to catch **bit rot
  before the data is needed**, and there is **no reconstruction/repair queue** for a
  corruption finding to land in. The custodian's `reconcile_step` fence + GC loop are
  built (#141/#142); the scrub loop (`0005:528-530`) is the next maintenance loop and
  is unimplemented. Realize the scrub loop and the shared repair-queue seam it (and
  the read path) enqueue onto.
- **Success criterion:** Demonstrable at C4-verify, in-process over the trait stores
  (Option A — no deployed custodian process exists yet, `0005:524-527`), dispatched
  through the real `reconcile_step` fenced control point:
  (1) A scrub pass walks each store (`ChunkStore::list_fragments`) and verifies each
  referenced fragment's self-describing checksum against the committed chunk map.
  BINDING (the walk + verify); the verification mechanism is ILLUSTRATIVE.
  (2) An **injected bit-flip** in a stored fragment is **detected**, the fragment is
  treated as lost and **excluded** (a checksum-failing shard is never fed to the
  decoder, `0005:263-264`), and the affected **chunk is enqueued for reconstruction**
  on a durable repair queue. BINDING — this is the central DoD leg; the queue's
  concrete representation (ledger key/encoding) is ILLUSTRATIVE.
  (3) **Scrub coverage** and **scrub-detected corruption rate** are emitted on the
  `DurabilityTelemetry` seam (`tracing` → OTel, Prometheus + OTLP, no backend
  hardcoded). The two metric surfaces are BINDING (ADR-0011/0012); the in-process
  read-back mechanism is ILLUSTRATIVE.
  (4) A **read-path** checksum failure feeds the **same** reconstruction queue
  (`0005:174-176`): a read that excludes a corrupt fragment also enqueues its chunk
  for repair onto the queue scrub feeds. BINDING (same-queue feed); the seam by which
  the read path records the finding is ILLUSTRATIVE.
- **Invariant to restore:** A fragment whose checksum fails verification — discovered
  **proactively by scrub** or **reactively on read** — is **never absorbed silently**:
  it is excluded from the decoder AND its chunk is enqueued for reconstruction on the
  one shared repair queue, so detected bit rot always becomes a durable repair
  obligation. (Stated over the corruption-finding CATEGORY, not the repro fragment;
  spans scrub + the read path + the queue seam — NOT satisfiable by guarding one
  module.) Source: proposal 0005 §Scrub (`0005:262-267`), §6.2/§6.3 read-vs-scrub
  mirror, the read-time-failure feed (`0005:174-176`); the silent-non-zero failure
  the request plane hides (`0005:326-327`); ADR-0011 (telemetry from first commit).
- **Repo + branch target:** getwyrd/wyrd @ main   (INTEGRATION §2: single line, no
  maintenance branches; host suggests `feat/m3.5-scrub-custodian`)
- **Depends on:** 142
- **Ordering note:** builds on the GC loop's `reconcile_step` dispatch + ledger
  pattern (#142, merged) and the `list_fragments` walk (#140, merged); all deps are
  already merged to `main`, so this is listed for provenance — it schedules freely.
- **Conflicts with:** none
- **Surfaces:** data
- **Scope:** the scrub maintenance loop — walk each store, verify referenced
  fragments' checksums against the committed chunk map, exclude a checksum-failing
  fragment, and enqueue its chunk for reconstruction on a durable repair queue shared
  with the read path; emit scrub coverage + scrub-detected corruption rate; and make
  a read-path checksum failure enqueue onto that same queue. / **out of scope:** the
  reconstruction custodian itself (any-`k` → recompute → re-place → version-conditional
  commit) and repair-vs-serve priority — that is slice **6** (`0005:531-536`), a later
  issue; this slice only *produces* repair obligations, it never dequeues or rebuilds.
  No new coding math, no on-disk-format change (the checksum is the existing
  chunk-format envelope's; `0005:552-554`).
- **Test file:** `crates/custodian/tests/scrub.rs` (the scrub legs, modelled on
  `crates/custodian/tests/gc.rs`); the read-path-feeds-same-queue leg (4) ships its
  regression where the enqueue seam lands (a `core` read-path test if the read path
  enqueues directly, else alongside the scrub test) — Do names the exact file in
  build-notes. Each asserting test must fail pre-change and pass post-change.
- **Verification posture:** mostly **NET-NEW coverage** (scrub does not exist; the
  repair queue is born here), so "red" is partly criterion-ABSENCE on a new file. For
  the load-bearing legs (2) and (4), capture a **demonstrated red** where feasible —
  e.g. a temporary negation of the checksum-verify / enqueue step so the injected
  bit-flip is silently included or not enqueued, proving the new seam is load-bearing
  — rather than resting red on non-existence. Coverage/corruption metrics (leg 3) are
  read back in-process via the telemetry seam as in `gc.rs`. All legs are observable
  at C4-verify; no off-Check/deferred green.
- **Citations expected:** Do must cite path:line on the target branch (`main`) AND
  the Planning artifact (0005 line refs) for every design claim and change.
- **Prior-art check (triage cycles):** searched by file path —
  `crates/custodian/src/scrub.rs` and `crates/custodian/tests/scrub.rs` do **not**
  exist; no `repair:`/reconstruction-queue ledger anywhere under `crates/`; no
  scrub-touching history across `--all`; no open PRs on getwyrd/wyrd. Net-new slice,
  no overlap.
- **Disposition hint:** likely-fix

## STOP discipline

Draft only until Check sign-off. A draft PR MAY be opened for CI; the PR MUST NOT be
marked ready before sign-off accepts.

## Iteration 1 — carry-forward (from the previous attempt)
- Sign-off rationale: T5 gap must be closed before accept (issue_143). `fragment_intact` guards two conditions — checksum-clean decode AND decoded `header.chunk_id == chunk` (patch.diff:203-205) — but the test set only exercises the checksum half (bit-flip in payload). Add a regression that exercises the misplaced-but-intact path: a fragment whose checksum passes but whose `header.chunk_id` names a different chunk than the committed chunk map references. It must be detected, excluded, and enqueued for repair (scrub leg in crates/custodian/tests/scrub.rs and/or the read-path leg in crates/core/tests/read_repair.rs). Keep the existing flippable demonstration. §6 C5 and V were not reached — left unconfirmed pending the rebuild.
- Full previous attempt preserved in `iteration-v1/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).
