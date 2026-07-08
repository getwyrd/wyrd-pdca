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
