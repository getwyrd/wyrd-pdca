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

Review task: fix gateway active-active id allocation so concurrent gateways over one shared metadata/chunk fleet do not mint colliding inode or chunk ids.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The decision is whether the patch targets the brief's active-active collision, and the changed surface does: gateway inodes move to shared allocation and chunk ids leave per-process restart counters (`crates/server/src/lib.rs:63`). |
| C2 Reproduction (red pre-fix) | PASS | The decision is whether the regression is flippable; copied onto a clean base, `gateway_multi_writer` fails at the concurrent PUT assertion with `Conflict`, grounding the active-active symptom (`crates/server/tests/gateway_multi_writer.rs:205`). |
| C3 Change | PASS | The decision is whether all id-minting paths were moved off identically seeded process counters; create commits now allocate inodes via the shared CAS allocator and writes mint high-epoch chunk ids (`crates/server/src/lib.rs:156`, `crates/server/src/lib.rs:189`, `crates/server/src/lib.rs:217`). |
| C4 Verification (red→green) | NEEDS-HUMAN | Full CI must be rerun on a host that permits loopback binds: targeted red→green passed, but `cargo xtask ci` failed here in an unrelated gRPC bind test with `Operation not permitted` before completing all gates (`crates/chunkstore-grpc/tests/list_delete.rs:55`). |
| C5 Causal adequacy | PASS | The decision is whether this removes the shared cause rather than guards a symptom; inode allocation is coordinated through `meta:next_inode`, recovery seeds that shared floor, and chunk ids are constructed in a disjoint random epoch (`crates/server/src/cli.rs:1027`, `crates/server/src/cli.rs:1069`, `crates/server/src/lib.rs:236`). |
| T1 Structure | PASS | The decision is whether the fix stays within the gateway allocation slice; changes are limited to gateway allocation, S3 serve recovery wording, and allocation/restart tests (`crates/server/src/lib.rs:116`, `crates/server/src/cli.rs:1447`). |
| T2 Shape | PASS | The decision is whether the tests exercise the required active-active shape; the new test uses two gateways over one shared metadata store and one shared read-back chunk store (`crates/server/tests/gateway_multi_writer.rs:139`). |
| T3 Runtime | PASS | The decision is whether the concurrent runtime path actually ran with the patch; `cargo test -p wyrd-server --test gateway_multi_writer` passed both contention tests, including the barrier-released multi-thread PUTs (`crates/server/tests/gateway_multi_writer.rs:153`). |
| T4 Contribution | PASS | The decision is whether this adds operational dependency risk; the patch uses existing traits/backends and standard-library entropy with no new service or crate dependency (`crates/server/src/lib.rs:232`). |
| T5 Judgment | NEEDS-HUMAN | Closed/rejected prior-art must be confirmed outside this sandbox: local affected-path history shows prior single-process gateway and CLI allocator work, but I could not mechanically query remote closed/rejected PR state here (`crates/server/src/cli.rs:1027`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Human sign-off must decide whether the accepted probabilistic chunk-epoch scheme is fit for production risk; the tests prove the exercised cases, but policy requires this validation row to remain human-cleared (`crates/server/src/lib.rs:223`). |

### Advisory — adversary

# Adversarial review — issue 477 / gateway-cluster-coordinated-id-allocation

Skeptic's pass. I tried to refute (a) the red→green evidence and (b) the fix's
correctness, grounding every cite on the target at `$PDCA_TARGET`. Toolchain to
re-run `xtask ci` / `run-verify.sh` was not invoked here; the reasoning below is
static and structural. **I could not refute the core fix.** The findings that
remain are test-adequacy concerns for a human, not correctness defects.

## Attacks on the fix's correctness — attempted and FAILED to refute

- **Wire/on-disk truncation of the new ≥2^127 chunk ids.** The new scheme mints
  `(chunk_epoch << 64) | seq` with the epoch's top bit forced set
  (`crates/server/src/lib.rs:217-219`, `:236-242`), so every id is in `[2^127, 2^128)`.
  I checked the two places a wide id could be silently narrowed: the gRPC contract
  carries the id as hi/lo `fixed64` halves (`crates/proto/proto/wyrd/v0/chunk.proto:10-15`),
  and the FS store keys fragments as `{:032x}` / parses with `u128::from_str_radix`
  (`crates/chunkstore-fs/src/lib.rs:378`, `:385-389`) — both hold a full u128. The
  real gRPC + FS wire path is exercised with a minted id in `closed_write_path.rs`
  (`crates/server/tests/closed_write_path.rs:230-268`, which reads the *actual*
  minted `chunk_map[0].id` rather than asserting `== 1`), so a truncation would have
  surfaced under C4. **Could not refute.**

- **Inode seeding off-by-one / reuse.** `recover` calls
  `seed_next_inode_floor(meta, max_inode+1)` (`crates/server/src/lib.rs:118`;
  body `crates/server/src/cli.rs:1069-1090`). Traced fresh store (`floor=1`, absent
  counter → no-op → first `alloc_inode` returns 1), legacy store (`inode:1..5`, no
  counter → seeds `"6"` → first new inode 6, above 5), and normal restart (counter
  already `max+1` → no-op). The raise-only, CAS-guarded loop is monotone and races
  safely against a concurrent `alloc_inode`. No off-by-one, no reuse. **Could not refute.**

- **Same-inode overwrite re-mint (the brief's binding constraint).** `mint_chunk_id`
  draws from a monotonic `next_chunk_seq` that never resets within a process
  (`lib.rs:218`), and a restart draws a *fresh* epoch, so an overwrite always mints
  ids disjoint from the prior version's — never the CLI `(inode<<64)|seq`-from-0
  re-mint the brief forbids. **Could not refute.**

- **Restart / orphan-ledger reclaim data loss.** Post-fix a restarted process draws
  a new epoch, so its chunk ids are disjoint from a deleted object's still-live
  orphan fragments by construction — no `high_water_marks` orphan scan needed for the
  gateway ids (which sit above `IN_PROCESS_CHUNK_CEILING = 1<<64`,
  `crates/core/src/metadata.rs:590`, so they are correctly ignored by that scan).
  **Could not refute.**

- **RED is genuine, not a tautology.** Pre-fix, `Gateway::new` seeds per-process
  `next_inode` at 1 for *both* gateways; the 4+4 concurrent writers claim inode sets
  `{1,2,3,4}` on each gateway, so the *set overlap* forces ≥4 `commit_create`
  `require_absent(inode:k)` conflicts **independent of scheduling** — the "every PUT
  must commit" assertion (`crates/server/tests/gateway_multi_writer.rs:205-208`) fails
  deterministically. The test drives production `Gateway::{recover,put_object,get_object}`
  and production `cli::alloc_inode`, not a re-implementation. **RED is robust.**

- **Same-key concurrent PUT across two gateways.** Both alloc *distinct* inodes, one
  wins the dirent CAS, the loser gets `Conflict`; the loser's fragments are leased
  under the `pending:` ledger (`write::intent`, `lib.rs:160`) and reaped by the sweep,
  and its chunk ids are epoch-disjoint so they do **not** clobber the winner. A leaked
  inode gap results, but no corruption. **Could not refute** (and the pre-fix version
  of this race *was* the corruption bug, now closed).

## NEEDS-HUMAN — advisory test-adequacy concerns (not fix defects)

- **NEEDS-HUMAN —** `restart_recovers_id_allocators_over_orphan_ledger_no_reclaim_loss`
  (`crates/server/tests/s3_http_wire.rs:762`) no longer exercises the mechanism its
  name asserts. Under the epoch scheme, object B survives the orphan reclaim purely
  because `g2` is a *fresh* `Gateway` that drew a different random `chunk_epoch` than
  `g1` — not because `recover()` recovered anything. B's inode comes from the
  `meta:next_inode` counter process 1 already advanced, and B's chunk id is
  epoch-disjoint by construction, so the three `recover()` calls at `s3_http_wire.rs:794`
  and `:831` are **no-ops for the property under test**: this test would pass
  identically with `recover` deleted. It no longer guards the recover-over-orphan-ledger
  path (recover no longer touches chunk ids or the orphan scan at all,
  `lib.rs:116-119`). Consider renaming/rescoping, as was done for the sibling test.

- **NEEDS-HUMAN —** the headline concurrency claim in
  `two_active_gateways_concurrently_store_distinct_objects_without_collision`
  (`crates/server/tests/gateway_multi_writer.rs:194-198`) is overstated. The `Barrier`
  releases the tasks at the *start* of `put_object`; each task then runs `plan_write`
  (RS(6,3) erasure-coding of 4 KiB → 9 shards), `write::intent` (a metadata commit) and
  `write::write_fragments` (9 `MemChunks` inserts) **before** ever reaching
  `alloc_inode`, so the actual allocator calls are not tightly aligned and the CAS-retry
  path may not be reliably hit here despite the docstring's "genuinely race on the
  shared `meta:next_inode` CAS allocator" (`:142`, `:229-233`). The iteration-1
  "genuinely concurrent" ask is in practice satisfied by the *companion*
  `shared_inode_allocator_hands_out_distinct_ids_under_contention`
  (`gateway_multi_writer.rs:251-252`), which calls `alloc_inode` immediately after the
  barrier. The headline test's RED/GREEN is still valid (RED is set-overlap
  deterministic, above), but it does not prove the contended-CAS property it claims to.

- **NEEDS-HUMAN —** cross-process chunk-id uniqueness rests entirely on
  `std::collections::hash_map::RandomState::new()` (`crates/server/src/lib.rs:236-242`)
  producing an independent 64-bit seed per process. Std documents RandomState's seeding
  as an unspecified implementation detail and does not guarantee a CSPRNG source on
  every target; if two gateways launch near-simultaneously on a platform with weak/
  correlated seeding, the assumed ~2^-63 epoch-collision floor degrades. The brief
  explicitly **accepts** the "per-process 63-bit epoch" probabilistic guarantee, so I
  am *not* relitigating the probabilistic model — only flagging that its entropy is
  anchored to a std internal rather than a documented RNG (`getrandom`), which a human
  may want to pin. Advisory, within accepted scope.

## Verdict claims I probed in `check-gates.json`

- `C4-verify` "red without the fix, green with it" — consistent with the deterministic
  set-overlap RED I reconstructed statically; I did not re-run it, so this is provisional
  on the recorded gate, but I found no reason to doubt it.
- `C4-ci` pass — the ≥2^127 chunk ids flow through the real gRPC + FS path in
  `closed_write_path.rs`, so a wire/FS truncation would have been caught; consistent
  with the recorded pass.

**Net:** a real attempt to break the fix on truncation, off-by-one, overwrite re-mint,
restart/orphan reclaim, and RED-validity all failed. The open items are three
test-adequacy notes for human adjudication; none is a data-correctness refutation.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [x] C4 Verification (red→green) — Full CI must be rerun on a host that permits loopback binds: targeted red→green passed, but `cargo xtask ci` failed here in an unrelated gRPC bind test with `Operation not permitted` before completing all gates (`crates/chunkstore-grpc/tests/list_delete.rs:55`).
- [x] T5 Judgment — Closed/rejected prior-art must be confirmed outside this sandbox: local affected-path history shows prior single-process gateway and CLI allocator work, but I could not mechanically query remote closed/rejected PR state here (`crates/server/src/cli.rs:1027`).
- [x] Validation — fitness-to-purpose — Human sign-off must decide whether the accepted probabilistic chunk-epoch scheme is fit for production risk; the tests prove the exercised cases, but policy requires this validation row to remain human-cleared (`crates/server/src/lib.rs:223`).

## 7. Proven / not proven
- Proven by which oracle: gates overall = pass (stub oracles).
- Unproven / needs manual run: anything flagged in §6.

## 8. Ready-to-ship attachments
- patch.diff
- tracker-comment.md     (ALWAYS, every tracker item)
- build-notes.md         (builder rationale — for the human, not the reviewer)

## 9. Check sign-off                     ← human completes Check here
- Disposition confirmed / overridden:
- Outcome: merged-wider
- Iteration delta (if iterating):
- By / date: Eduard Ralph / 2026-07-07

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
