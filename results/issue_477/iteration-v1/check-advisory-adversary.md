# Adversarial review — issue #477 / gateway-cluster-coordinated-id-allocation

Advisory only; never gates. Grounded on target source at
`/home/eddie/wyrd/wyrd.pdca-wt-l1` (branch `feat/obs-floor.1-keystone`; the patch is
applied in the worktree). Toolchain re-run of the red→green was **not** attempted from
scratch — I reasoned the proof against the source rather than reverting; treat the
"could not refute" claims as source-level, not a fresh `cargo` run.

## Refutations attempted and could NOT sustain (the fix survives these)

- **Wire truncation of the wide chunk id.** The new chunk ids are `≥ 2^127`
  (`crates/server/src/lib.rs:217-220`, epoch top-bit forced at `lib.rs:241`), and the new
  test uses an in-memory `MemChunks` (`crates/server/tests/gateway_multi_writer.rs:88-113`)
  that never crosses the gRPC D-server wire — the classic "test double hides the production
  encoding" hole. I chased it: the proto carries the id as **two `fixed64` halves**
  (`crates/proto/proto/wyrd/v0/chunk.proto:12-15`), and the on-disk fragment header writes
  the **full 16 bytes** (`crates/chunk-format/src/codec.rs:49`,
  `CHUNK_ID..CHUNK_ID+16`). Pending/orphan keys encode the id as decimal `u128`
  (`crates/core/src/metadata.rs:41,61` — `format!` over a `u128`) and parse back to `u128`
  (`metadata.rs:550-556,66-73`). No truncation anywhere on the data plane. **Refuted.**
- **The test is only an inode test; the chunk half is unproven.** Traced it: even a
  hypothetical inode-only fix lets gateway B's PUT commit (distinct inode) and then reach
  the read-back asserts (`gateway_multi_writer.rs:162-171`); a per-process chunk counter
  would still clobber A's fragments under a colliding chunk id, so `get_object("a") !=
  object_a` → RED. The test genuinely constrains **both** id halves. **Refuted.**
- **Tautology / passes for the wrong reason.** If `random_chunk_epoch`
  (`lib.rs:236-242`) were effectively constant, both gateways would mint `(epoch,0)` for
  their first chunk → clobber → RED. GREEN therefore genuinely requires distinct epochs and
  a distinct shared-CAS inode. Not a tautology. **Refuted.**
- **Migration / restart re-mint (the #364 invariant the patch must preserve).** With
  `recover` (`lib.rs:116-119` → `seed_next_inode_floor`, `crates/server/src/cli.rs:1069-1090`)
  a store carrying `inode:` keys but no `meta:next_inode` seeds the floor above the max
  inode; concurrent recover is idempotent/monotone via the CAS loop. Chunk ids draw a fresh
  epoch per process, disjoint from any `< 2^64` legacy id. **Refuted.**
- **Overwrite re-minting a prior version's chunk id (the brief's binding constraint).**
  `next_chunk_seq` is process-monotonic and never reset (`lib.rs:218`), epoch fixed per
  process, so an overwrite mints strictly fresh ids. **Refuted.**

## Advisory findings a human should weigh (not clean refutations)

- NEEDS-HUMAN — **The "active-active concurrent" test runs strictly sequentially.**
  `crates/server/tests/gateway_multi_writer.rs:151-157`: `gw_a.put_object(...).await`
  completes fully before `gw_b.put_object(...).await` — there is no `join!`/spawn. So the
  proof never triggers concurrent CAS contention on `meta:next_inode`
  (`crates/server/src/cli.rs:1027-1060`, the retry/backoff loop): both allocs are
  uncontended (A→1, B→2). The fix's *concurrent* inode correctness rests entirely on
  `alloc_inode`'s pre-existing CAS loop, which this new test does not exercise. It does
  satisfy the brief's literal criterion ("recovered from the same baseline before either
  commits"), but the stated **invariant** ("globally unique across all concurrently-active
  gateway processes") is broader than what is demonstrated. Human: confirm the sequential
  model is accepted as sufficient evidence for the concurrent invariant.
- NEEDS-HUMAN — **Chunk-id entropy is per-process (63 bits), not per-chunk-random as
  ADR-0019 envisions.** `crates/server/src/lib.rs:236-242`: all cross-process disjointness
  rides on a single 63-bit epoch (top bit forced at `lib.rs:241`); the low 64 bits are a
  deterministic per-process sequence from 0. Consequence a skeptic must name: if two
  *live* processes ever draw the **same** epoch, they collide **systematically on every one
  of their chunks** (`(epoch,0)` vs `(epoch,0)`, `(epoch,1)` vs `(epoch,1)`, …) — the
  original bug reinstated for that pair, not a single stray chunk. Per-pair probability is
  ~`2^-63` (astronomically small), but this is a **different risk profile** from ADR-0019's
  cited "u128 … random/UUID-style **per identifier**", and the brief's invariant is stated
  as an absolute ("MUST NOT mint the same … chunk id"). Human: confirm a per-process-epoch,
  probabilistic guarantee (vs per-chunk-random) is the accepted reading of the invariant.
- NEEDS-HUMAN — **`restart_without_recover_is_safe_by_construction` overstates: `recover`
  is still load-bearing for in-place upgrades.** `crates/server/tests/s3_http_wire.rs:588`.
  The test's g1 writes with the **new** gateway, so `meta:next_inode` is already present when
  g2 reopens *without* recover → safe. But that is not the case `recover` exists for: a
  store an **older** single-process gateway wrote has `inode:` keys and **no**
  `meta:next_inode`; started without `recover`, `alloc_inode` reads the absent key → returns
  id `1` (`crates/server/src/cli.rs:1032-1035`) → collides with the existing inode 1. The
  renamed test's title/claim ("safe by construction … even *without* an explicit recover")
  does not hold for the migration scenario `recover` is there to handle. Production always
  calls `recover` (`crates/server/src/cli.rs` serve path retains `gateway.recover().await?`),
  so this is a test-claim overreach, not a production defect — but a reviewer who reads the
  test as proving recover is optional would be misled.
- **`high_water_marks`' `max_chunk` (and its orphan-ledger scan) is now dead code.**
  `crates/core/src/metadata.rs:589-623`: the only caller, `Gateway::recover`
  (`crates/server/src/lib.rs:117`), now discards `_max_chunk`; no other caller uses it
  (grep confirms). The whole orphan-ledger projection (`metadata.rs:611-621`) and its
  doc rationale (`metadata.rs:558-588`) describe a chunk-id recovery path the gateway no
  longer takes. Harmless (wasted scan), but note that
  `restart_recovers_id_allocators_over_orphan_ledger_no_reclaim_loss`
  (`crates/server/tests/s3_http_wire.rs:644`) keeps its name while the mechanism it now
  validates is the coordination-free epoch, not the `high_water_marks` orphan scan — the
  test name no longer describes what it tests.
