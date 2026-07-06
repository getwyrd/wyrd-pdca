# Build notes — issue 257 (iteration 11), M4.6 real-commit-over-madsim-tikv

**Withheld from the reviewer.** Rationale for the human at sign-off.

## Scope of this iteration (iteration-10 carry-forward — TWO narrow fixes, nothing else)

The iteration-10 sign-off ratified everything from iteration 9 and rejected **narrowly the
one piece of new-in-iter-10 code** (the feature-gated compile step and its guard). Verbatim
directive: *"Fix exactly these two, change nothing else."* This iteration does exactly that.

**Not re-opened / carried forward unchanged (ratified):** Option B (no `madsim-tikv-client`
exists; the ADR-0015-on-TiKV correctness proof lives off-Check in the privileged
Tier-1/Tier-2 job); exit-(b) seed relabelling (`crates/dst/tests/tikv_await_commit_interleaving.rs`
is pure redb coverage with **no** TiKV correctness weight); the pure testkit quorum/consistency
oracles; the xtask metadata dispatch; the tier1/tier2 scenario rework (must-fixes 1–3); the
`deploy/tikv-multi-replica` compose. The invariants hold: **no** `crates/metadata-tikv/src`
edit, **no** `crates/traits` edit (byte-for-byte).

## Defect 1 — gate-honesty regression (the feature-gated step ran unconditionally)

Iteration 10 wired `feature_gated_checks()` **unconditionally** into `run_ci`
(`for check in feature_gated_checks() { cargo(&check)?; }`). That runs
`cargo check -p wyrd-metadata-tikv --features tikv --tests`, which compiles the pre-1.0
grpcio-bearing `tikv-client` tree on **every** `cargo xtask ci` — contradicting the documented
invariant that a laptop/worktree with no TiKV toolchain "never compiles or audits this tree and
stays green" (`crates/metadata-tikv/Cargo.toml`: `tikv` is off by default precisely for this).
The recorded C4-ci pass came from a toolchain-complete box and masked it.

**Fix (directive: "gate it, don't drop it").** Introduced `tikv_toolchain_available()`
(`xtask/src/main.rs:846`) — `WYRD_TIKV_TOOLCHAIN` present ⇒ true, absent ⇒ false — and made
the feature-gated step conditional on it inside `run_ci_steps` (`xtask/src/main.rs:887`). The
privileged Tier CI/eval job (which already owns the live Tier-1/Tier-2 legs and has the TiKV
toolchain) sets `WYRD_TIKV_TOOLCHAIN=1` to opt the type-check in; the default offline gate never
touches the tree. Iter-9's type-check intent is preserved (the step still runs where the
toolchain exists), the no-TiKV-CI invariant is restored.

## Defect 2 — tautology guard (the test restated the constant, never exercised the wiring)

Iteration 10's `ci_type_checks_feature_gated_metadata_scenario` asserted only that
`feature_gated_checks()` contained its own hard-coded literal. It never called `run_ci`, so
deleting `run_ci`'s wiring loop left it green — the exact "assert the literal the function
returns" shape the early iterations were rejected for.

**Fix.** Extracted `run_ci_steps(tikv_toolchain, exec)` (`xtask/src/main.rs:857`) — the **sole
cargo-step source `run_ci` iterates** (`run_ci` at `:897` is now `run_ci_steps(tikv_toolchain_available(),
&mut |args| cargo(args))?` + the non-cargo helpers). `exec` is injected, so the unit test
(`xtask/src/main.rs:1128`) drives `run_ci`'s **real** wiring with a **recording closure** — no
`cargo` spawned — and asserts on the argv it actually invokes:

- `tikv_toolchain = true`  ⇒ the invocations **must include** the metadata feature check
  (the type-check is wired).
- `tikv_toolchain = false` ⇒ they **must NOT** include it (gate honesty — the no-TiKV invariant).

This exercises `run_ci`'s wiring, not a constant, and asserts **both** the presence and the gate.

## Red→green evidence (project cargo toolchain, in `$PDCA_WORKTREE`)

Green on the tree: `cargo test -p xtask --bin xtask ci_type_checks_feature_gated_metadata_scenario`
→ 1 passed. Both behavioural failure modes the test must catch flip it red (perturb → red →
revert → green):

| perturbation | which case flips | result |
|---|---|---|
| make the step **unconditional** (drop the `if tikv_toolchain` gate) | `without_toolchain` | **FAILED** — "default no-TiKV `cargo xtask ci` must not compile the tikv feature tree" |
| **empty** `feature_gated_checks()` (step dropped from the wiring) | `with_toolchain` | **FAILED** — "run_ci must invoke `cargo check … --features tikv --tests`" |
| **fully unwire** the loop from `run_ci_steps` | (compile) | **compile error** — `feature_gated_checks` dead code under `-D warnings`; cannot pass silently |

So neither a lost gate (regression to the iter-10 defect) nor a lost step can pass green — the
tautology is gone.

**No regressions.** `cargo fmt --all -- --check` exit 0 (gate honesty — v6's only gate failure
was fmt; verified exit code directly, not through a pipe); `cargo clippy -p xtask --all-targets`
clean; `cargo test -p xtask --bin xtask` → 17 passed.

## Two now-stale docstrings corrected (kept honest under the gate)

The tier docstrings claimed `cargo xtask ci` type-checks the `#[cfg(feature = "tikv")]` bodies
in the whole-tree gate. That is now true only in the privileged Tier job (where
`WYRD_TIKV_TOOLCHAIN` is set); the default offline gate skips it. Rewrote both to say so:
`crates/metadata-tikv/tests/tier1_metadata_consistency.rs:48-56` and
`crates/metadata-tikv/tests/tier2_metadata_io.rs:16-20`. The doc comment on
`feature_gated_checks` (`xtask/src/main.rs:797`) and `run_ci_steps` (`:850`) document the gate.

## Why this shape (and what I rejected)

- **Why an env-gate, not dropping the step.** The directive is explicit: "Do not drop the step;
  gate it." Dropping it re-opens iter-9's confirmed gap (a type error in the live scenario slips
  the gate). An env-gate (`WYRD_TIKV_TOOLCHAIN`) keeps the type-check where the toolchain exists
  and restores the offline invariant where it doesn't — a 3-line predicate + one `if`.
- **Why an injected executor, not a heavier `run_ci` rewrite.** The non-cargo phases
  (`cargo_machete_check`, `run_conformance`, `run_dst`, …) genuinely spawn processes and are out
  of this fix's scope. The cargo-step block (fmt→test→feature-checks) is a contiguous prefix of
  `run_ci`; extracting just it behind an injected `exec` makes the wiring testable with the
  minimum surface — `run_ci` stays a thin `run_ci_steps(...)? + helpers`.
- **Why not re-touch the ratified body.** §6 items 1,2,3,5 and the posture-part of 4 were
  ratified; touching them would re-open settled questions and risk regressing oracles that held
  under adversarial probing across v5–v10.

## Invariants held

- `crates/traits/src/lib.rs` — untouched (byte-for-byte).
- `crates/metadata-tikv/src/**` — untouched. Only `Cargo.toml` (dev-dependency, from iter-9) and
  test files / docstrings change; no `src` edit.
- The at-Check binding correctness posture is unchanged (Option B, off-Check); no self-authored
  sim, no patch-authored mode flag reintroduced.
- Commit-ready: `cargo fmt --all` clean over every touched file; `cargo clippy -p xtask
  --all-targets` clean.

## NEEDS-HUMAN carried forward (unchanged; not this iteration's call)

Option-B posture / `madsim-tikv-client` non-existence (ratified); the off-Check binding
Tier-1/Tier-2 legs confirmed only by the privileged CI/eval Tier job (names the confirmer at
sign-off); the metadata-nemesis ADR question (architecture board); the #365 static-endpoints
reduced bar. This iteration re-opens none of them.
