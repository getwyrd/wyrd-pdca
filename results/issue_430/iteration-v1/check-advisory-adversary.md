# check-advisory-adversary.md — issue 430 / fragment-identity-validation

Adversarial pass. I attempted to refute the evidence, the fix, and the verdict; I
**independently re-ran the red→green proof** in a scratch clone of the target base
(`dc503cd`) rather than trusting the C4-verify gate.

## Refutation attempts — and their outcomes

- **Attempted to refute the red→green evidence: could not.** In a scratch clone of base
  `dc503cd` with only `crates/core/tests/fragment_identity.rs` retained (all production
  and modified-test files reverted), both new tests fail **by assertion** — the read
  returns silently wrong bytes through the production `read::read_object` path
  (`crates/core/src/read.rs:435`), exactly the defect claimed. With the patch applied
  they pass. The red is honest: the test file compiles against the base production code
  (it never calls the widened helper signatures directly), so the red is behavioural,
  not a degenerate compile-error red. The deterministic-red shaping (serve only `k`
  fragments, one wrong-identity) holds — the decoder necessarily consumed the wrong
  shard pre-fix.

- **Attempted to break the fix with a legitimately-written fragment (mixed-era
  data-loss angle): could not.** If any historical writer had emitted RS fragments with
  unstamped EC header fields, `repair::header_matches_identity`
  (`crates/core/src/repair.rs:248-271`) would reject ALL existing RS data — a data-loss
  event. Checked: the very first RS write commit (`a0f9dad`) already stamped
  `ec_scheme_type`/`ec_k`/`ec_m`/`ec_fragment_index` (today
  `crates/core/src/write.rs:142-146`), and no production writer has ever emitted
  `EcSchemeType::Replication` (`crates/chunk-format/src/header.rs:79` — only codec unit
  tests use it). No legitimate on-disk fragment is rejected by the widened check.

- **Attempted to refute the untested half of the success criterion (read-around when
  ≥ k intact fragments remain): could not.** The new test file only exercises the
  below-`k` typed-error path. I wrote a scratch test (3 slots, slot 0 wrong-index,
  slots 1–2 genuine) against the patched code: the read returns the TRUE bytes via
  the `Ok(decoded) if header_matches_identity(...)` fan-out arm
  (`crates/core/src/read.rs:330-343`). Read-around works.

- **Attempted to break the custodian call-site threading: could not.**
  `reconstruction.rs` builds `frag` from the placement slot index
  (`crates/custodian/src/reconstruction.rs:359-363`) so `intact_shard(b, frag,
  chunk_ref.scheme)` (`reconstruction.rs:391`) verifies the identity the slot expects;
  the `EcScheme::None` early-return at `reconstruction.rs:346` keeps the scheme RS
  there. `rebalance.rs:266` indexes `plan.prior.chunk_map[plan.chunk_index]`, which is
  in-bounds by construction (`rebalance.rs:155,197`). Full `wyrd-core` + `wyrd-custodian`
  suites (23 targets) pass with the patch; `wyrd-chunkstore-grpc` test targets
  compile (`frag_id` is in scope in the tier tests as the patch assumes).

## Non-blocking observations (no `[impl]` rebuild warranted; recorded for the record)

- `crates/custodian/src/scrub.rs:100` — the new `if let Some(&scheme) =
  referenced.schemes.get(&frag.chunk)` **silently drops** a placed fragment from scrub
  coverage when its chunk has no scheme entry. Today this is unreachable — `placed` and
  `schemes` are populated in the same `Ok` arm of `referenced_fragments`
  (`crates/custodian/src/gc.rs:232-247`) — but the invariant is enforced only by
  co-location; a future refactor that decouples them would silently shrink scrub
  coverage rather than fail loudly. A `debug_assert!`/comment-free skip is a latent
  coupling, not a live defect.
- `crates/custodian/src/gc.rs:246` — `schemes.insert` is last-write-wins: two committed
  inodes sharing a chunk id under DIFFERENT schemes would make scrub verify one inode's
  fragments against the other's scheme. Chunk ids are minted uniquely per write, so this
  is a corrupted-metadata scenario where a (spurious) repair enqueue is a defensible
  outcome; not a failing case for this diff.
- Enqueue on the ≥-k read-around path is order-dependent (the fan-out may accept `k`
  good shards at `crates/core/src/read.rs:338` before ever examining the wrong-identity
  slot, so no repair obligation is recorded that pass). This matches the pre-existing
  misplaced-arm semantics the brief explicitly names as the model ("as the existing
  misplaced-fragment arm already does"); scrub independently catches it. Conformant.

## Verdict

Attempted to refute the red→green proof, the mixed-era/legitimate-data rejection angle,
the untested read-around branch, and the custodian identity threading; **could not**.
The evidence is genuine (behavioural red on the production path, independently
reproduced), the fix covers the named edge cases, and I found no concrete failing input.
No `NEEDS-HUMAN` findings.
