# Result — issue 477 / gateway-cluster-coordinated-id-allocation

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect / goal: The runnable gateway (`crates/server/src/lib.rs`) mints inode and chunk
- Success criterion: Two `Gateway` instances composed over one **shared**
- Repo + branch target: getwyrd/wyrd @ feat/m4-production-metadata-backend
- Scope (one logical fix) / out of scope: Remove the per-process `AtomicU64` counters (`lib.rs:67-68`) as the gateway's

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: likely-fix
- Confidence: medium
- Recommendation: (set by Do)

## 3. Correctness (Check — chain)
- C1 Spec: none — brief.md
- C2 Reproduction (red pre-fix): none — (no gate configured)
- C3 Change: none — patch.diff
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): pass — xtask ci: all checks passed
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass — run-verify.sh: PASS — red without the fix, green with it.
- C5 Causal adequacy: none — reviewer + human sign-off

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 Contribution: none — (no gate configured)
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Review task: fix gateway active-active id allocation so two gateways over one shared metadata/chunk fleet cannot mint colliding inode or chunk ids.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The brief's required decision is global uniqueness for gateway-minted inode and chunk ids across active gateways; the target source now routes create inodes through shared CAS allocation and chunks through a per-gateway epoch (`crates/server/src/lib.rs:184`, `crates/server/src/lib.rs:217`). |
| C2 Reproduction (red pre-fix) | PASS | In a disposable pre-fix clone with only the new regression test added, the second active gateway's distinct-key PUT fails with `Conflict`, grounding the claimed bug at `crates/server/tests/gateway_multi_writer.rs:154`. |
| C3 Change | PASS | The implementation changes the id-allocation surface named by the brief: recovery seeds the persisted inode allocator, creates call `alloc_inode`, and both buffered and streaming writes mint chunks through `mint_chunk_id` (`crates/server/src/lib.rs:116`, `crates/server/src/lib.rs:189`, `crates/server/src/lib.rs:287`). |
| C4 Verification (red→green) | NEEDS-HUMAN | The focused red→green is reproduced, but full `cargo xtask ci` could not be independently completed because this sandbox denies loopback binding in an existing gRPC test (`crates/chunkstore-grpc/tests/list_delete.rs:53`); human needs a normal host CI rerun to clear the full gate. |
| C5 Causal adequacy | PASS | The patch removes the contested per-process source rather than adding a capability probe or runtime guard: inodes use the shared `meta:next_inode` allocator and chunks use a fresh epoch plus monotonic sequence (`crates/server/src/cli.rs:1027`, `crates/server/src/lib.rs:96`, `crates/server/src/lib.rs:218`). |
| T1 Structure | PASS | The change stays within the gateway id-allocation path plus carry-forward tests, matching the scoped modules and not altering metadata/chunk store traits (`crates/server/src/lib.rs:56`, `crates/server/tests/gateway_multi_writer.rs:1`). |
| T2 Shape | PASS | The regression test models the specified shape: two `Gateway` instances share one metadata store and one read-back-observable chunk store before storing distinct objects (`crates/server/tests/gateway_multi_writer.rs:118`, `crates/server/tests/gateway_multi_writer.rs:134`). |
| T3 Runtime | PASS | The patched target's focused test passes, and code inspection confirms the streaming path uses the same chunk-id minter as buffered PUT (`crates/server/src/lib.rs:155`, `crates/server/src/lib.rs:276`). |
| T4 Contribution | PASS | Affected-file history shows the prior single-gateway counter caveat and existing CLI allocator, while this patch adds the missing gateway multi-writer regression rather than duplicating an existing test (`docs/design/architecture/m4-first-deployment-blueprint.md:196`, `crates/server/tests/gateway_multi_writer.rs:128`). |
| T5 Judgment | NEEDS-HUMAN | Local affected-file history was checked, but closed/rejected remote work could not be mechanically settled in this sandbox; human needs to confirm no prior closed attempt supersedes this patch before sign-off. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Always-human validation: decide whether probabilistic coordination-free chunk epochs satisfy the product's "globally unique" active-active guarantee, because acceptance depends on treating ADR-0019 random/UUID-style ids as sufficient (`crates/server/src/lib.rs:223`). |

### Advisory — adversary

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

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [x] C4 Verification (red→green) — The focused red→green is reproduced, but full `cargo xtask ci` could not be independently completed because this sandbox denies loopback binding in an existing gRPC test (`crates/chunkstore-grpc/tests/list_delete.rs:53`); human needs a normal host CI rerun to clear the full gate.
- [x] T5 Judgment — Local affected-file history was checked, but closed/rejected remote work could not be mechanically settled in this sandbox; human needs to confirm no prior closed attempt supersedes this patch before sign-off.
- [x] Validation — fitness-to-purpose — Always-human validation: decide whether probabilistic coordination-free chunk epochs satisfy the product's "globally unique" active-active guarantee, because acceptance depends on treating ADR-0019 random/UUID-style ids as sufficient (`crates/server/src/lib.rs:223`).
- [ ] **The "active-active concurrent" test runs strictly sequentially.**
- [x] **Chunk-id entropy is per-process (63 bits), not per-chunk-random as
- [ ] **`restart_without_recover_is_safe_by_construction` overstates: `recover`

## 7. Proven / not proven
- Proven by which oracle: gates overall = pass (stub oracles).
- Unproven / needs manual run: anything flagged in §6.

## 8. Ready-to-ship attachments
- patch.diff
- tracker-comment.md     (ALWAYS, every tracker item)
- build-notes.md         (builder rationale — for the human, not the reviewer)

## 9. Check sign-off                     ← human completes Check here
- Disposition confirmed / overridden:
- Outcome: iterated-to-Do
- Iteration delta (if iterating): Approach accepted (coordination-free gateway id allocation — shared-CAS inodes + per-process random chunk epoch). C4 is green on a real host rerun (xtask ci: all checks passed) and T5 is clear (no prior closed/rejected attempt supersedes). Iterating only to strengthen the tests before merge: - The active-active regression test (crates/server/tests/gateway_multi_writer.rs) MUST be genuinely CONCURRENT: drive gateway A and B under join!/spawn so it actually exercises contended CAS on the shared meta:next_inode allocator. Today A's PUT fully completes before B's (A->1, B->2, uncontended), so the concurrent invariant the fix rests on is never exercised. - Correct restart_without_recover_is_safe_by_construction (crates/server/tests/s3_http_wire.rs): its claim does not hold for the migration case recover exists for — an older single-process store with inode: keys and no meta:next_inode, started WITHOUT recover, returns id 1 and collides. Either rescope/rename the test to what it actually proves, or make it exercise the recover-from-legacy path so the name matches the mechanism. Do NOT relitigate: the chunk-id probabilistic guarantee (per-process 63-bit epoch) is accepted. The collision-detection gap is out of scope here and is filed as getwyrd/wyrd#478 (Foundations milestone).
- By / date: Eduard Ralph / 2026-07-07

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
- Bug to file (foundation milestone): custodian gc/scrub cannot detect chunk-id collisions — `fragment_intact` checks only chunk-id + self-checksum, both satisfied by a colliding overwrite; add a per-chunk content digest to the chunk map so scrub/read can flag a collided band. Detection/repair backstop only; prevention stays the coordination-free id scheme.
- ^ Filed as getwyrd/wyrd#478 (Foundations milestone).
