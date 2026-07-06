# Build notes — issue 257 (iteration 10), M4.6 real-commit-over-madsim-tikv

**Withheld from the reviewer.** Rationale for the human at sign-off.

## Live layer (report at build, per Success criterion)

**Option B is LIVE and RATIFIED — not re-opened.** `madsim-tikv-client` genuinely does not
exist (no release tracks `tikv-client = "0.4"`), confirmed across iterations 7–9 and accepted
as a posture by the human. The real ADR-0015-on-TiKV correctness proof therefore lives
**off-Check** in the privileged Tier-1/Tier-2 job; the at-Check bar is the pure oracles
(testkit quorum/consistency arithmetic + xtask dispatch) plus the DST seed **labelled as pure
redb coverage with no TiKV correctness weight** (iteration-8 exit (b), ratified). This
iteration does **not** touch that posture.

## What this iteration changes (iteration-9 carry-forward — the ONLY open defect)

Iteration 9 was rejected on **one confirmed gate gap**, not the posture. Verbatim directive:

> Add a `cargo check -p wyrd-metadata-tikv --features tikv --tests` step to `run_ci` in
> xtask/src/main.rs so the `#[cfg(feature = "tikv")]` scenario code … is compiled and
> type-checked by the whole-tree gate.

The root cause: `run_ci` builds/tests `--workspace` with **default** features (tikv OFF), and
`--all-targets` widens *target kinds*, not the *feature set*. So the load-bearing live-scenario
bodies (`SymmetricPartition`, its `Drop` heal, the PD-side fault-effect oracle, and the
`partition_took_effect` / `heal_is_complete` / `consistency_passes` consumption in
`crates/metadata-tikv/tests/tier1_metadata_consistency.rs`) — all behind
`#[cfg(feature = "tikv")]` — were compiled by **no** step of the gate. A type error inside them
left `cargo xtask ci` green, so a live-leg regression flipped no Check artifact, and iter-7
must-fix-2 ("oracles wired in, not dead code") was only nominally satisfied.

### The three edits (scope-guarded exactly to the directive)

1. **`xtask/src/main.rs:819` — new pure `feature_gated_checks()`** returning the
   `cargo check -p wyrd-metadata-tikv --features tikv --tests` argv as data, and
   **`xtask/src/main.rs:859`** — `run_ci` iterates it (`for check in feature_gated_checks()`)
   right after the `--workspace` test step. Returned as data (not inlined) so the wiring is
   unit-testable.
2. **`xtask/src/main.rs:1092` — new unit test `ci_type_checks_feature_gated_metadata_scenario`**
   asserting the metadata-tikv `--features tikv --tests` check is present in
   `feature_gated_checks()`. Removing the step (the single source `run_ci` iterates) flips it
   red — a regression guard, red→green shown below.
3. **The two now-false compile-at-Check claims corrected** (the directive's "seed docstring +
   brief line"): the tier docstrings previously claimed `cargo test --workspace` "compiles and
   type-checks" the scenario — **false** (default features). Rewritten to state the truth: the
   default build compiles only the skeleton, and `cargo xtask ci` type-checks the real body via
   the dedicated feature-check step. `crates/metadata-tikv/tests/tier1_metadata_consistency.rs:44-54`
   and `crates/metadata-tikv/tests/tier2_metadata_io.rs:14-21`. The seed docstring
   (`crates/dst/tests/tikv_await_commit_interleaving.rs:70`) makes only a `--cfg madsim` compile
   claim, which is accurate — left unchanged. brief.md:230's "scenario tests compile in the
   whole-tree gate" is made **true** by edit (1), not by editing the brief.

Everything else in the patch is the accepted iteration-9 body, carried forward unchanged (the
testkit oracles, xtask dispatch, tier1/tier2 scenario rework from must-fixes 1–3, the deploy
compose, the seed-as-coverage labelling).

## Red→green evidence (run through the project's cargo toolchain, in `$PDCA_WORKTREE`)

**A. The behavioural gap the directive names (the real defect).** With a temporary type error
injected inside the `#[cfg(feature = "tikv")]` body of `tier1_metadata_consistency.rs`
(`let _type_error: u32 = "perturbation";` in `SymmetricPartition::rules`):

| command | before fix behaviour | result |
|---|---|---|
| `cargo check -p wyrd-metadata-tikv --tests` (default features — what `--workspace` runs) | should MISS it | **exit 0 (green)** — the gap |
| `cargo check -p wyrd-metadata-tikv --features tikv --tests` (the new gate step) | should CATCH it | **exit 101 (red)** — `E0308 expected u32, found &str` |

So the default build the gate ran was blind to a type error that the new step catches; `cargo()`
propagates the non-zero exit as `Err`, so `run_ci` now returns red. Perturbation reverted.

**B. The committed wiring guard (red→green).** `cargo test -p xtask --bins
ci_type_checks_feature_gated_metadata_scenario`:
- **Green** on the tree (1 passed).
- Emptying `feature_gated_checks()` to `vec![]` (simulating a removed gate step) → **FAILED**
  (panic at the assertion). Restored → green again.

**C. No regressions.** `cargo fmt --all -- --check` clean (gate honesty — v6's only gate
failure was fmt); `cargo clippy -p xtask --all-targets` clean; `cargo check -p
wyrd-metadata-tikv --features tikv --tests` green on the tree (the feature body type-checks);
`cargo test -p wyrd-testkit` green (18 passed — the pure oracles survive).

## Why this shape (and what I rejected)

- **Why a `cargo check` step, not making run_ci build `--features tikv` for the whole
  workspace.** A blanket `--workspace --features tikv` would pull the pre-1.0 `tikv-client`
  tree into the default gate build/clippy/test for every crate and risk the container-free /
  offline invariant (`crates/metadata-tikv/Cargo.toml:11-19`: tikv is OFF by default precisely
  to keep `cargo xtask ci` from touching that tree). A single scoped `cargo check -p
  wyrd-metadata-tikv --features tikv --tests` compiles exactly the feature body that was
  invisible, and `check` (not `build`/`test`) is the minimum that type-checks it without
  linking or running — the `#[ignore]`d scenario still needs a real cluster to *run*. This is
  the directive verbatim.
- **Why not a heavier behavioural at-Check flip of the live scenario.** The directive's scope
  guard is explicit: this is a CI/gate change plus fixing two false claims; it "does NOT
  re-litigate Option B, exit (b), the seed's off-Check labelling, or the off-Check
  Tier-1/Tier-2 legs." Driving the real scenario at Check is impossible (no cluster, no
  `madsim-tikv-client` — the ratified Option-B fact), so the honest at-Check evidence is that
  the gate now *compiles* it and a regression flips red.
- **Why the wiring test isn't a tautology.** It does not assert the literal the function
  returns; it asserts the **property** the gate needs (a `check` invocation targeting
  `wyrd-metadata-tikv` with `--features tikv` and `--tests`), and its red is a genuine
  regression signal (delete the step → red), demonstrated above. It is a wiring guard, not the
  binding correctness oracle (which is Option-B / off-Check).

## Invariants held

- `crates/traits/src/lib.rs` — **untouched** (byte-for-byte).
- `crates/metadata-tikv/src/**` — **untouched**. Only `crates/metadata-tikv/Cargo.toml`
  (a dev-dependency, from iter-9) and the test files change.
- The at-Check binding correctness posture is unchanged (Option B); no self-authored sim, no
  patch-authored mode flag re-introduced.
- Commit-ready: `cargo fmt --all` clean over every touched file.

## Cited facts

- Gate structure & the gap: `xtask/src/main.rs:832` (`run_ci`), the pre-fix
  `--workspace`/`--all-targets` steps, and `xtask/src/main.rs:978` (`fn cargo`, propagates a
  non-zero exit as `Err`).
- The feature is off by default: `crates/metadata-tikv/Cargo.toml:11-20`.
- The feature-gated live bodies the fix compiles: `tier1_metadata_consistency.rs` `run()` +
  `SymmetricPartition` (`#[cfg(feature = "tikv")]`), and `tier2_metadata_io.rs` `run()`.
- Real commit under test (off-Check, unchanged): `crates/metadata-tikv/src/lib.rs:540+`.
- Precedent for a scoped feature build in xtask: `run_tikv_conformance` already builds
  `--features tikv` against the throwaway TiKV (`xtask/src/main.rs` tikv-conformance path).

## NEEDS-HUMAN carried forward (unchanged; not this iteration's call)

Option-B posture / `madsim-tikv-client` non-existence (ratified); the off-Check binding
Tier-1/Tier-2 legs confirmed only by the privileged CI/eval Tier job; the metadata-nemesis ADR
question (architecture board); the #365 static-endpoints reduced bar. This iteration re-opens
none of them.
