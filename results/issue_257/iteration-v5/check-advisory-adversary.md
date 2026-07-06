# Adversarial review — issue 257 / m4.6-tier1-scenario-tier2

Advisory only; never gates. Attacked the at-Check red→green (the DST seed + the pure
xtask/testkit unit logic). Grounded on target source at `$PDCA_TARGET`.

## The at-Check flip rests on a self-authored toggle, not a defect

- **NEEDS-HUMAN — the seed's only flippable oracle is a property of a fixture this patch
  authored, toggled by a constructor argument.** The load-bearing red→green is
  `interleavings_observed >= 1` (`crates/dst/tests/tikv_await_commit_interleaving.rs:244-249`).
  The "red under the uncorrected assumption" is produced by passing `false` to
  `SimTikvMetadataStore::with_await_inside_commit` (`crates/dst/src/sim_tikv.rs:168`), a
  parameter added in this same patch — not a pre-existing bug in `wyrd_core`,
  `metadata-redb`, or the `concurrency.rs` harness. Nothing in production code fails
  before the patch and passes after; the "flip" is a boolean the fixture reads about
  itself. This is close to the compile-level / self-referential flip shape that got v3
  rejected ("the on-Check green was a compile-level flip"). Whether a toggle-produced red
  inside brand-new test-only code satisfies "genuine flippable red→green" is the human's
  call. (The `run-verify.sh` "red without the fix" almost certainly reverts the whole new
  file — a file-absence red — which I could not inspect; worth confirming the red is
  behavioural, not "the test didn't exist yet.")

- **`assert_backend_equivalence` is vacuous — it can never fail for any reachable input.**
  `crates/dst/tests/tikv_await_commit_interleaving.rs:217-233` (called at :256) compares
  two runs of the **same** `SimTikvMetadataStore` CAS: await-on vs await-off. Both modes
  re-check preconditions against *current* state at the commit point
  (`crates/dst/src/sim_tikv.rs:238`) — there is **no** mode that "trusts the prewrite
  check," so the lost-update the doc-comment claims to guard ("a store that skipped the
  commit-point re-check would let a second stale writer commit") is not representable.
  Unlike `await_inside_commit`, there is no toggle for prewrite-trust. Both runs
  deterministically yield `committed_writers == 1` / `final_version == 2`, so the two
  `assert_eq!`s always hold. Success-criterion §1.1 item 3 ("No lost update — the swap is
  observationally equivalent") is therefore decorative: it proves nothing the code could
  violate.

- **The "contract survived" assertions test the model, not the production path, and are
  near-definitional.** `assert_contract_survived`
  (`crates/dst/tests/tikv_await_commit_interleaving.rs:178-215`) can fail only if the
  hand-written in-memory `SimTikvMetadataStore` CAS is itself buggy — and that store is
  authored in this patch. The real `metadata-tikv/src` CAS is out of scope and never
  exercised by the seed. `final_version == PRIOR_VERSION + committed_writers` (:210) is
  satisfied by the store's own coupled bookkeeping (generation and version bumped together
  inside the one guarded block), so it re-derives the exactly-one-winner atomicity DST
  already owns — the very thing the Invariant says the seed must not rest on. The seed
  *demonstrates* an interleaving; it does not *test* that any production abstraction
  matches TiKV.

## The "fault-effect oracle" the brief touts is unwired on the live path

- **`metadata_leg_passes` / `MetadataLegVerdict` are dead relative to every live leg.**
  `xtask/src/metadata_faults.rs:110` and `:136` are referenced **only** by
  `xtask/tests/metadata_faults_orchestration.rs` (confirmed by grep: no other consumer).
  No live tier test and no `run_metadata_tier1` path ever constructs a `MetadataLegVerdict`
  from real observations. The unit tests set the five struct fields by hand and assert
  `a && b && c && d && e` — i.e. they test boolean AND, a tautology in the iter-1 sense
  ("asserts the same literal the function returns"). The *actual* Invariant-B enforcement
  on the live path comes from `wyrd_testkit::partition_materialized`
  (`crates/testkit/src/lib.rs:415`) and `MetadataQuorumPlan::is_valid_minority_fault`
  (`:465`), which the consistency test does wire in
  (`crates/metadata-tikv/tests/tier1_metadata_consistency.rs`). So the oracle the brief
  advertises as the Check-time flippable fault-effect guard is not the oracle doing the
  work; `metadata_leg_passes` is redundant scaffolding that could be deleted with no live
  behaviour change.

- **`metadata_consistency_route`'s `true` branch is dead outside the test.**
  `xtask/src/metadata_faults.rs:73` — the only production caller passes `false`
  (`xtask/src/faults.rs:592`); `true` is reached solely by the unit test at
  `metadata_faults_orchestration.rs:47`. The dispatch test thus guards a
  constant-returning function against a body edit only — marginal, and borderline the
  same "returns a literal the test asserts" shape flagged as the iter-1 defect. (It does
  mirror the accepted `jepsen_dispatch` pattern, so this is a weak point, not a blocker.)

## Attacked and could not refute

- The `testkit` quorum arithmetic (`crates/testkit/src/lib.rs`
  `retains_quorum`/`is_valid_minority_fault`) is sound across the boundaries I probed
  (replicas 2/3/4/5, partitioned 0..=replicas, saturating_sub guards underflow); no
  off-by-one found.
- `partition_materialized` (`:415`) is a genuine independent before/after oracle, not a
  tautology, and it *is* wired into the live consistency leg — the strongest artifact here.
- `no_interleaving_reachable_under_synchronous_commit` correctly stays `0`: with the await
  removed there is no `.await` between the generation snapshot and the commit-point lock,
  so madsim (single-threaded, yields only at awaits) cannot interleave. That red demo holds.

## Net

The privileged legs are pre-declared off-Check (C2/C4 sign-off items) — not attacked as
"unbuilt." The refutable weakness is in the **at-Check** evidence: the binding flip is a
fixture toggling a property of itself, its "no lost update"/"backend equivalence"
assertions cannot fail, and the advertised fault-effect oracle (`metadata_leg_passes`) is
unwired from any real observation. The seed clears the iter-1 bar (real mid-commit
interleaving now materialises) but the *bindingness* still leans on constructs the patch
authored to pass. Human should adjudicate whether that is the honest red→green the four
prior iterations lacked, or a more sophisticated restatement of the same hollow flip.
