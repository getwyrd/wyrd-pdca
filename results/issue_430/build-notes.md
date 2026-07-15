# Build notes — issue 430 / fragment-identity-validation (iteration 4)

## Summary of what this iteration does

The **production fix and its tests are carried forward from iteration 3 unchanged** — the
brief's iteration-3 carry-forward is explicit that the code was *not* rejected ("gates were
green … the main review passed C1/C3/C5/T1/T2/T3/T5. … Do not rework the identity-validation
predicate itself"). The round-3 rejection was Check-side only: the adversary advisory leaf
produced no artifact, and an independent re-run of the verification could not be performed at
sign-off (sign-off host shell failure). Neither is a Do defect.

So this iteration's job was to **re-establish first-hand red→green evidence** on the current
target base so the reviewer/human has a fresh, reproducible record — the exact thing the
last two sign-offs could not obtain. Every run below was executed **this session** in
`$PDCA_WORKTREE` (`/home/eddie/development/wyrd/wyrd.pdca-wt-l0`, detached at `dc503cd`,
`origin/main`) with the project toolchain (`cargo 1.97.0`, `rustup` `stable`), and the
regenerated `patch.diff` re-applies clean to that base (`git apply --check` → clean).

## The fix (unchanged; see citations on the target base)

A fragment is admitted into any read/repair/maintenance path only when its decoded header
proves the **FULL identity** the committed chunk map requested — `chunk_id`,
`ec_fragment_index`, and (for RS) an EC tuple (`ec_scheme_type`/`ec_k`/`ec_m`) consistent
with the committed `ChunkRef.scheme`. This is the smallest change that restores the brief's
**Invariant**: verification "against the chunk map", not half of it.

- **New shared predicate** `repair::header_matches_identity(header, expected: FragmentId,
  scheme: EcScheme) -> bool` (`crates/core/src/repair.rs:58` fn sig, body through `:82` on the
  patched tree) — mirrors the store-level precedent `FsChunkStore::verify` (chunk **and**
  index, `crates/chunkstore-fs/src/lib.rs:117-130`) and widens it with the committed EC tuple
  only the shared core layer can check.
- **Widened shared helpers** `fragment_intact` / `intact_shard` now take `(bytes, expected:
  FragmentId, scheme: EcScheme)` and gate through `header_matches_identity`
  (`crates/core/src/repair.rs:95`, `:113`).
- **Read path** — both inline decode gates now call `header_matches_identity`: the
  single-fragment path (`crates/core/src/read.rs:237`) and the RS fan-out
  (`crates/core/src/read.rs:331`). A rejected survivor is excluded from the decoder,
  its chunk pushed on the shared repair queue (`corrupt.push(chunk.id)`), exactly as the
  existing misplaced-fragment arm does.
- **Custodian call-sites** pass the expected identity + scheme through:
  reconstruction (`crates/custodian/src/reconstruction.rs:391`), scrub
  (`crates/custodian/src/scrub.rs:126` + the `schemes` map plumbed through
  `crates/custodian/src/gc.rs`), rebalance
  (`crates/custodian/src/rebalance.rs:266`).
- **Consumer test-call updates** across `read_repair.rs`, `mutation_regressions.rs`,
  `custodian/tests/*`, `dst/tests/custodian.rs`, `chunkstore-grpc/tests/*` — mechanical
  signature updates so the whole tree compiles.

Out of scope, untouched: backend `ChunkStore` implementations, new metrics, maintenance-loop
redesign, and #431's block-fault arm.

## Verification performed THIS session (the gap the last two sign-offs hit)

Runner: the project toolchain resolved the same way `engine/xtask.sh` / `run-verify.sh` do.
The whole-tree `cargo xtask ci` (C4-ci) still cannot **complete** on this host because
`chunkstore-grpc`'s `list_delete_over_grpc` cannot bind loopback (`PermissionDenied`) — the
same environmental limitation the round-1/2/3 carry-forwards recorded, outside the patch.
I therefore proved everything the patch *can* be held to on this host, with a focused, bounded
run per leg:

1. **GREEN leg** (fix applied): `cargo test -p wyrd-core --test fragment_identity`
   → `test result: ok. 3 passed; 0 failed`.
2. **RED leg** (revert ONLY the two core production files `read.rs` + `repair.rs`, keep the
   added test): same command → **all 3 FAIL by ASSERTION** — the read returns the wrong
   shard's bytes (`left: [128,129,…]`) instead of the true object (`right: [0,1,2,…]`),
   panicking at `fragment_identity.rs:204` / `:289` / `:373`. Not a compile error — the test
   drives only the public surface (`read::read_object` + `repair::queued_repairs`), which is
   unchanged, so the red is behavioural exactly as brief.md:83-86 requires.
3. **Mutation experiment** (kills the round-2 adversary survivor): replace the RS arm body
   with `{ let _ = (k, m); header.ec_scheme_type == EcSchemeType::ReedSolomon }` (drop the
   `ec_k`/`ec_m` compare). `fragment_identity` → case 3 FAILS, cases 1 & 2 pass; the unit
   `repair::tests::intact_shard_accepts_the_expected_fragment_and_rejects_wrong_identity`
   FAILS. So case 3 + the unit test **bind the k/m conjuncts specifically** — the
   dead-untested hole from round 2 is closed. Reverting the mutant → all green.
4. **Whole-workspace compile**: `cargo test --workspace --all-targets --no-run` → exit 0.
   Every consumer test edit (chunkstore-grpc, custodian, dst) compiles; the ONLY thing this
   host can't do is *run* the loopback-binding grpc tests.
5. **Commit-hook gates**: `cargo fmt --all -- --check` → clean (exit 0);
   `cargo clippy -p wyrd-core -p wyrd-custodian -p wyrd-chunkstore-grpc -p wyrd-dst
   --all-targets -- -D warnings` → clean (exit 0). Patch is commit-ready for the target's
   own hooks.

## Refutation (forced self-check — three questions, answered from THIS session's runs)

- **(a) Genuine red?** YES. Reverting `read.rs` + `repair.rs` to base (test kept) makes all
  three tests fail **by assertion** (transcript above: wrong bytes at both data positions);
  re-applying the fix → all three green. The red is behavioural, not a compile artifact.
- **(b) Production path?** YES. The tests call `read::read_object` — the real read entry —
  and serve the wrong fragment through the `ChunkStore` trait exactly as an adversarial
  backend would. They hit the same production gate `header_matches_identity` at the RS
  fan-out decode site; no copy/mock/re-implementation. The unit test calls the real
  `repair::intact_shard`.
- **(c) Fixture includes the fault?** YES. Each case serves exactly `k = 2` fragments with
  ONE of them the wrong-identity fragment actually stored at slot 0 (not curated out), so the
  RS fan-out (which stops at `k`) is forced to consume it — the deterministic-red shape the
  brief mandates (brief.md:37-42).

## Notes for the human at sign-off (§6)

- **C4-ci completeness**: `cargo xtask ci` cannot finish on this host (`list_delete_over_grpc`
  loopback-bind `PermissionDenied`). This is an environment limitation, not a patch defect —
  everything the host *can* run is green (focused red→green, whole-tree compile, fmt, clippy).
  A CI host that permits loopback binds will run the full gate. `External dependencies: none`
  per the brief, so this is not a NEEDS-HUMAN external-dependency marker — it is a host
  capability gap in the full-suite run only.
- **T4 (no closed/rejected upstream PR)**: the brief's Prior-art check already searched
  `git log` over `repair.rs` / `read.rs` and found no index-plus-scheme fix; closed/rejected
  remote PR state is not mechanically available offline in this bundle. Flagged for the human
  to confirm at sign-off (same as prior iterations — a remote-visibility gap, not a code one).
