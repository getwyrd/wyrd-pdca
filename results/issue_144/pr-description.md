# Reconstruction custodian — drain the repair queue and rebuild under-replicated chunks (M3.6)

> Realizes accepted proposal 0005 §"Reconstruction — the heart of M3"
> (`docs/design/proposals/accepted/0005-milestone-3-custodians.md:269-286`),
> §"Repair-vs-serve" (`0005:305-317`), and the three M3 repair metrics
> (`0005:326-332`). One logical fix: the reconstruction loop and its
> repair-vs-serve priority.

## Root cause
Scrub (#143) and the read path enqueue repair obligations onto the shared,
durable repair queue (`wyrd_core::repair`), but `main` has no consumer — the
custodian crate ships gc / scrub / reconciliation / telemetry and `reconcile_step`
dispatches no reconstruction loop. So a lost D server's fragments, or a chunk a
scrub/read checksum failure flags, stay under-replicated indefinitely and the
obligation is never drained.

## Fix
Add the reconstruction custodian, dispatched from the existing fenced control
point. Each pass drains the queue and, per affected chunk: gathers any `k`
surviving fragments and verifies each against its checksum (a corrupt or
misplaced shard is *excluded* and never decoded); rebuilds the missing shard(s)
scheme-driven from the chunk's per-chunk `EcScheme`; re-places them on healthy D
servers in failure domains distinct from the survivors'; and repoints the
placement record with **one** version-conditional `MetadataStore::commit` that
also drains the obligation and orphans the displaced fragment. The rebuilt
fragments are written *before* the commit, so a crash mid-repair leaves only
collectable garbage (GC reclaims it after the #142 grace window), never a torn or
hybrid chunk; the CAS on the prior inode record means a racing writer or
superseded custodian loses the commit rather than corrupting the record. Repair
is ordered by a `repair_priority` that rises as redundancy falls, and the three
M3 durability metrics are emitted on the telemetry seam. The custodian gains no
on-disk-format dependency (ADR-0010): the two format-touching primitives
(`repair::intact_shard`, `write::encode_ec_fragment`) live in `core`.

## Verified against
- `crates/custodian/src/reconstruction.rs:1-456` — the new loop: drain →
  gather+verify → scheme-driven rebuild → distinct-domain re-place → single
  version-conditional commit (`reconcile`/`assess`/`repair_chunk`); `0005:269-286`.
- `crates/custodian/src/reconciliation.rs:60-99` — `reconcile_step` gains a
  `reconstruction: Option<&ReconstructionContext>` dispatch slot alongside gc/scrub;
  reports `Changed` if any loop converged.
- `crates/core/src/repair.rs:53-66` — `intact_shard`, `fragment_intact`'s
  payload-returning sibling, so a survivor is decoded behind the shared verify
  without a chunk-format dependency; consumes the queue from
  `6a33a33ebd7cbcc29b9cc7530832314774ceadbb` (#143).
- `crates/core/src/placement.rs:113-120,206-256` — `Topology::domain_of` and
  `select_distinct_domains_excluding`, the shared selector extended to keep a
  rebuilt fragment off any surviving fragment's domain (`0005:276`, `0005:491`);
  it refuses rather than collide when no free domain remains.
- `crates/core/src/write.rs:106-122` — `encode_ec_fragment` stamps a rebuilt RS
  shard with the same v1 header fields the write fan-out uses, so a reader decodes
  and verifies it identically.

## Test
`crates/custodian/tests/reconstruction.rs:1-524` — driven through the real
`reconcile_step` seam over in-memory trait stores and a placement-aware fleet.
Kill a D server holding an RS(2,1) fragment → the chunk returns to full redundancy
across 3 distinct domains via one commit (version `1→2`, placement `[0,1,2]→[0,3,2]`,
obligation drained), with reads succeeding before (degraded) and after; a
checksum-failing shard is excluded and rebuilt around; the three repair metrics
are read back via Prometheus; and the priority function rises as redundancy falls.
Red→green proven both ways (build-notes.md): the C4-verify revert leg drops the
net-new dispatch so the kept test fails to compile (build-level red), and negating
the version-conditional commit in `repair_chunk` leaves the chunk under-replicated
so the kill test's assertions fire (assertion-level red). Whole gate
`./engine/xtask.sh ci` (incl. the DST sweep) exits 0.

Fixes #144
