# Adversarial review — issue_257 (iteration 8)

Advisory only; never gates. Grounded on `$PDCA_TARGET`
(`/home/eddie/wyrd/wyrd.pdca-wt-l0` @ `feat/m4-production-metadata-backend`).
Attack order: the evidence → the fix → the verdict.

## The evidence (the at-Check red→green)

- **NEEDS-HUMAN — The flagship at-Check seed drives `RedbMetadataStore`, not the TiKV
  code #257 exists to cover, and is a near-clone of the pre-existing `concurrency.rs` —
  re-instantiating the v1 rejected shape.**
  `crates/dst/tests/tikv_await_commit_interleaving.rs:110-186` instantiates
  `RedbMetadataStore::in_memory()` (`:112`) and races two writers through
  `write::commit_overwrite` (`:140`) → `metadata::commit_chunk_map`
  (`crates/core/src/metadata.rs:299-317`) → the **generic** `store.commit`. That is the
  identical store, identical helper, and identical await window (intent + write_fragments
  between reading `prior` and committing) already exercised by
  `crates/dst/tests/concurrency.rs:35-94` (`exactly_one_concurrent_writer_wins`). The only
  behavioural delta is 4 writers → 2 and an added `conflicted == 1` assert (which, with
  `committed == 1` and two writers, is arithmetically implied). **A production regression in
  `TikvMetadataStore::commit` — the `get_for_update`(`crates/metadata-tikv/src/lib.rs:560`)
  → `txn.commit().await`(`:597`) window the brief names as the defect surface — cannot flip
  this seed, because the seed never constructs a `TikvMetadataStore`.** The docstring itself
  concedes it (`tikv_await_commit_interleaving.rs:54-56`: "What redb cannot exhibit — and
  this seed therefore does NOT claim to prove — is TiKV's await-inside-`commit()` percolator
  window"). Concrete failing case for the claim: delete the `get_for_update` re-check at
  `metadata-tikv/src/lib.rs:555-574` and the seed stays green. This is v1's "decorative DST
  seed that re-proved redb's atomicity."

- **NEEDS-HUMAN — C4-verify's "red without the fix, green with it" is unverifiable from the
  supplied artifacts and, given the seed above, is at best redundant with `concurrency.rs`
  and at worst a compile/file-absence flip (the iter-1/v3 forbidden shape, iter-7 must-fix 5).**
  `check-gates.json:42-48` marks C4-verify PASS via `./engine/scripts/run-verify.sh`, which
  does **not** exist in the target worktree, so the perturbation that produced "red" cannot be
  inspected. Because the sole at-Check flippable artifact drives redb, the only production
  code whose perturbation could flip it is the **shared** `commit_chunk_map` /
  `RedbMetadataStore::commit` — but that same perturbation flips `concurrency.rs` identically,
  so the new seed adds zero incremental red→green over an already-committed test. A human must
  confirm the red was a *behavioural* perturbation of production code that **this seed catches
  and `concurrency.rs` does not**; otherwise C4-verify is redundant or non-behavioural.

## The fix (find the input that breaks it)

- **NEEDS-HUMAN — Even accepting the ratified Option-B posture, the seed fails its own
  narrowed Option-B mandate.** `brief.md:86-89` requires the seed-as-coverage-artifact to
  assert "the `concurrency.rs` synchronous-commit rationale is unsound; here is a
  newly-reachable interleaving." The seed exhibits **no** newly-reachable interleaving: its
  await window is byte-for-byte the `concurrency.rs` window, and it runs over redb, where the
  "no await inside commit" rationale (`concurrency.rs:3-4`) is *true*. It therefore cannot
  demonstrate that rationale is unsound — it re-confirms it. The docstring's "under the
  interleaving the `concurrency.rs` rationale declares impossible"
  (`tikv_await_commit_interleaving.rs:60-61`) is the reverse of what the code does.

- The Tier-1 live scenario (`crates/metadata-tikv/tests/tier1_metadata_consistency.rs`) and
  Tier-2 leg are `#[ignore]`d and endpoint-gated, so they execute **only** off-Check in the
  privileged Tier job — none of their partition/heal/oracle behaviour is exercised at Check.
  The pure `wyrd_testkit` oracles (`partition_took_effect`, `heal_is_complete`,
  `converged_exactly_once`, `consistency_passes`, `partition_materialized`,
  `crates/testkit/src/lib.rs:889-995`) and the xtask dispatch tests are genuine
  non-tautological unit coverage — **I attempted to refute these as tautologies and could
  not**: each asserts a hand-computed expectation independent of the function body
  (`testkit/src/lib.rs:1012-1145`). They are, however, *arithmetic about* a partition, not
  evidence *of* one; their green says nothing about whether the live leg's `SymmetricPartition`
  actually isolates a node — that remains an off-Check, human-confirmed claim.

## The verdict (where the reviewer may have rationalized)

- **NEEDS-HUMAN — The docstring's independence claim is the specific unwarranted verdict.**
  `tikv_await_commit_interleaving.rs:36-38` ("a real production regression in the commit-point
  re-check … produces a real lost update this seed catches. This is the independence the six
  rejected iterations lacked") asserts teeth against production code the seed does not invoke.
  Iter-7 must-fix 4 offered two clean exits: bind the seed to the real commit path so a
  production regression flips it, **or** label it honestly as pure coverage with no correctness
  weight. The patch does neither: it binds to redb (not the swap under test) while still
  asserting independence/teeth. A reviewer crediting this docstring as "the honest red→green
  the six iterations lacked" has been fooled by prose, not code.

## Attempted-but-could-not-refute

- The pure quorum/consistency oracles and the `metadata_tier_dispatch` routing tests are
  non-tautological and flippable at Check; I could not reduce them to `concurrency.rs`-style
  redundancy or boolean vacuity.
- The invariant "no `crates/metadata-tikv/src` and no `crates/traits` edit" holds in the diff
  (only `metadata-tikv/Cargo.toml` gains a `wyrd-testkit` dev-dep; `traits/src/lib.rs`
  untouched) — I could not find a stealth trait/`src` change.
