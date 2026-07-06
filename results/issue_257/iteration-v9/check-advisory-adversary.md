# Adversarial review — issue 257 (iteration 9), advisory only

Posture note: iter-8 ratified Option-B and pre-authorized exit (b) for the flagship seed
(pure coverage + honest relabel). I did not re-litigate the posture; I attacked whether the
patch's *evidence* and the reviewer's *claims* actually hold under (b).

## Findings

- **NEEDS-HUMAN — The whole live-scenario body is `#[cfg(feature = "tikv")]`, which
  `cargo xtask ci` never compiles — so the "compiles + type-checks in the whole-tree gate"
  claim is false for the code that matters.** `run_ci` builds/tests `--workspace`
  with **no** `--features tikv` (`xtask/src/main.rs:805-819`: `clippy`/`build --all-targets`
  and `test --workspace`, all default-features), and `tikv` is off by default
  (`crates/metadata-tikv/Cargo.toml: default = []`). `--all-targets` selects target *kinds*,
  not features, so every `#[cfg(feature = "tikv")]` item in
  `crates/metadata-tikv/tests/tier1_metadata_consistency.rs:79` onward — `SymmetricPartition`,
  `apply`/`heal`, the PD oracle `pd_store_state`, and the *entire* consumption of
  `partition_took_effect` / `heal_is_complete` / `consistency_passes` /
  `converged_exactly_once` — is excluded from the Check gate; only the
  `#[cfg(not(feature="tikv"))]` stub (`:511`) is compiled. The docstring's claim that
  `cargo test --workspace` "still **compiles and type-checks** it"
  (`tier1_metadata_consistency.rs:45-46`) and the brief's "scenario tests compile in the
  whole-tree gate" (brief.md:230) are therefore **unwarranted**. Concrete failing case: swap
  the argument order in the heal check to `heal_is_complete(&healed, &p.applied_rules(), ...)`
  (`:392`), or introduce any type error inside `SymmetricPartition` — `cargo xtask ci` stays
  **green**. The reviewer's C4-ci pass does not cover this code.

- **NEEDS-HUMAN — iter-7 must-fix-2 ("wire the pure oracles into the scenario, not dead
  code") is only *nominally* satisfied: at the Check boundary the oracles are still consumed
  by nothing but their own unit tests.** Their sole real consumer
  (`tier1_metadata_consistency.rs:127-137`, `:388-392`) sits behind `--features tikv`, which
  Check never builds (see above). So a regression that stopped calling
  `partition_took_effect` in the live leg, or re-pointed it, would flip **no** Check artifact
  — the exact "computed/wired, never applied at Check" shape the earlier iterations were
  rejected for, relocated one `cfg` deep.

- **NEEDS-HUMAN — The C4-verify "red without the fix, green with it" cannot be a behavioural
  flip against production commit code, because this patch adds no at-Check production
  behavioural surface.** The only Check-reachable code the patch adds is *pure arithmetic*
  (`crates/testkit/src/lib.rs:931-1033`) and *pure dispatch* (`xtask/src/metadata_faults.rs`);
  the redb seed explicitly disclaims correctness weight (below); the TiKV commit path
  (`crates/metadata-tikv/src/lib.rs:540-601`) is untouched and its scenario is off-Check.
  A red produced by deleting/mutating a just-added pure function is either a **compile-flip**
  (the v3 / iter-7 must-fix-5 rejection, brief.md:290) or proves only that the quorum/version
  arithmetic matches its own hand-computed table — **not** that the redb→TiKV swap upholds
  ADR-0015. run-verify.sh / build-notes.md are not in my inputs, so a human must confirm the
  recorded red was a genuine *assertion* failure and not the recurring compile-flip.

- **The flagship deliverable named by the brief does not exist at Check.** The slice's
  identity (brief.md:42-56, 194) is "a DST seed exercising the await-inside-commit
  interleaving **against the real `metadata-tikv` commit code**." Under (b) the seed
  (`crates/dst/tests/tikv_await_commit_interleaving.rs:26-32,58-71`) concedes it drives
  **redb only**, carries **no** correctness weight, exhibits **no** newly-reachable
  interleaving, and pushes the determinism-gap assertion off-Check. This is *allowed* by
  iter-8, so it is not a refutation — but the human at sign-off should confirm that "redb
  coverage + off-Check live legs + arithmetic" is accepted as "the end result the Success
  criterion names," since the on-Check behavioural proof the brief keeps demanding is, by
  construction, absent.

- **The redb seed's incremental value over `concurrency.rs` is thin and near-forced.** The
  seed (`tikv_await_commit_interleaving.rs:124`) is a 2-writer clone of
  `crates/dst/tests/concurrency.rs:35` whose only new assertion is `conflicted == 1`
  (`:179-185`). Given the seed already asserts `committed == 1` and `.unwrap()`s each outcome
  (so an `Err` panics rather than counting), and `CommitOutcome` has exactly two variants
  (`crates/traits/src/lib.rs:355`), `conflicted == 1` is arithmetically forced by
  `committed == 1` — it adds almost nothing a mutation could independently flip. Honest as
  coverage, but the reviewer should not credit it as meaningful new signal.

- **`partition_materialized` is inert in the live leg (redundant, not wrong).**
  `tier1_metadata_consistency.rs:136` calls
  `partition_materialized(p.total_replicas, p.isolated)` where the operands come from
  xtask-hardcoded env `WYRD_TIER1_REPLICAS=3` / `WYRD_TIER1_ISOLATED=1`
  (`xtask/src/faults.rs:1437-1438`), i.e. always `(3,1)` → always `true`. So the leg's entire
  fault-effect gate reduces to `partition_took_effect(connected_before, connected_during)`
  fed by a hand-rolled HTTP/1.0 + whitespace-strip + substring parse of PD's `/stores`
  (`pd_store_state`, `:465-489`). This fails *safe* (a parse miss → `connected_before=false`
  → leg fails), so it is not a false-green vector, but the reviewer should not read
  `partition_materialized`'s presence as an independent live check — it is a compile-time
  constant here.

## Could not refute
- Attempted to break the testkit pure oracles (`quorum`, `partition_outcome`,
  `converged_exactly_once`, `consistency_passes`, `heal_is_complete`,
  `partition_took_effect`) as tautologies — could not; each unit test asserts a
  hand-computed expectation distinct from the returned literal, and a boundary mutation
  (`total/2` vs `total/2+1`; `+1` vs `+2`) flips them red. These are genuine.
- Attempted to find a *false-green* path in the off-Check tier1 leg (a no-op partition that
  still passes) — could not; the fault-effect gate and heal checks fail safe. The residual
  risk is that none of that code is compiled at Check (finding 1), not that it passes
  vacuously when it does run.
