# Build notes — issue 257 (iteration 12), M4.6 real-commit-over-madsim-tikv

**Withheld from the reviewer.** Rationale for the human at sign-off.

## Scope of this iteration (iteration-11 carry-forward — ONE narrow fix)

The iteration-11 sign-off **ratified the whole bundle except one thing** and was explicit
that this is *"a harness-implementation defect only … no brief/plan change needed."* The
defect: the live Tier-1 fault-effect oracle keyed off PD's **administrative `state_name`**,
which stays `"Up"` through a short partition (it only flips to `Down` after
`max-store-down-time`, PD default ~30 min), so in the scenario's ~45 s window
`partition_took_effect()` was **always `false`** → `fault_materialized = false` → the binding
Tier-1 leg could **never** pass, regardless of a real cut (empirically confirmed in the
privileged run: `ConsistencySignals { read_after_commit: true, converged_once: true,
fault_materialized: false }`). The iptables partition itself was ruled sound; **only the
oracle field was wrong.**

**Ratified, NOT re-opened (kept byte-for-byte from iteration 11):** the Option-B posture
(`madsim-tikv-client` does not exist; the ADR-0015-on-TiKV correctness proof lives off-Check
in the privileged Tier-1/Tier-2 job); the exit-(b) seed relabelling
(`crates/dst/tests/tikv_await_commit_interleaving.rs` is pure redb coverage, **no** TiKV
correctness weight); the pure testkit quorum/consistency oracles; the xtask metadata dispatch
(`xtask/src/metadata_faults.rs`, `xtask/tests/metadata_faults_orchestration.rs`); the two
iter-10 fixes (toolchain-gated feature check at `xtask/src/main.rs:846,857,888,898` +
the non-tautological `run_ci` guard test at `xtask/src/main.rs:1124`); the `deploy/tikv-multi-replica`
compose; the symmetric bidirectional partition + verified heal (must-fixes 1/3). Invariants
hold: **no** `crates/metadata-tikv/src` edit, **no** `crates/traits` edit (byte-for-byte).

## The fix — heartbeat-freshness oracle, extracted so it is at-Check-flippable

The iteration-11 directive: *"Replace the `state_name` oracle with a transient-liveness
signal: read the target store's `last_heartbeat` … and assert it goes STALE … Do NOT rely on
`state_name`."* A partitioned voter stops heartbeating PD, so PD's `last_heartbeat` for that
store **freezes** and its age grows past a few store-heartbeat intervals (PD default 10 s)
within seconds — the transient signal that *does* move inside the window.

Rather than bury the new decision logic inside the `#[cfg(feature = "tikv")]` live scenario
(where no at-Check gate compiles it, the recurring "a live-leg regression flips nothing at
Check" failure), I **extracted the two pure pieces into `wyrd-testkit`** (import-light, no
TiKV deps, compiled and run by the default `cargo xtask ci`):

- `wyrd_testkit::parse_store_last_heartbeat(stores_json, target_ip) -> Option<i128>`
  (`crates/testkit/src/lib.rs:466`) — the **field selection**: parse the target store's
  `last_heartbeat` nanos out of PD's `/pd/api/v1/stores` body.
- `wyrd_testkit::heartbeat_is_fresh(last_heartbeat_nanos, now_nanos, max_staleness) -> bool`
  (`crates/testkit/src/lib.rs:499`) — the **staleness arithmetic**: `age = now - last`
  (clamped for clock skew), fresh iff `age < max_staleness`.

The live scenario **calls these very functions** (`crates/metadata-tikv/tests/tier1_metadata_consistency.rs:405`
`pd_sees_target_live` → `heartbeat_is_fresh`; `:495` `pd_store_last_heartbeat` → the HTTP GET
delegates its parse to `parse_store_last_heartbeat`). So the at-Check unit tests drive the
**same production oracle logic** the live leg uses — not a copy. A regression in the field
selection or the threshold flips **both** the unit test and the live leg.

`state_name` is now referenced **nowhere** in the oracle path (grep-clean); the old
`pd_store_state`/`pd_sees_target_up`/`pd_still_sees_target_up_after`/`wait_pd_sees_target_up`
were replaced wholesale by the `*_live` heartbeat variants
(`tier1_metadata_consistency.rs:404-433,494-516`). Staleness threshold is
`WYRD_TIER1_HEARTBEAT_STALE_SECS` (default 20 s ≈ two PD heartbeat intervals),
`tier1_metadata_consistency.rs:297`.

## Red→green evidence (project cargo toolchain, in `$PDCA_WORKTREE`)

At-Check, behavioural (not a compile-flip, not a self-restated literal):

- **GREEN on the tree:** `cargo test -p wyrd-testkit --lib` → 20 passed, incl.
  `last_heartbeat_is_parsed_for_the_addressed_store` and
  `heartbeat_freshness_is_an_age_threshold_not_state_name` (`crates/testkit/src/lib.rs:753,772`).
- **RED on the iter-11 defect shape:** perturb `heartbeat_is_fresh` to treat every store live
  regardless of age (`… || true` — the `state_name`-never-flips behaviour the defect had, and
  still *uses* `max_staleness` so it is **not** an unused-variable compile error) ⇒
  `heartbeat_freshness_is_an_age_threshold_not_state_name` **FAILS**. Reverted → green.
- A parse regression (reading the wrong store's field, or `state_name` instead of
  `last_heartbeat`) flips `last_heartbeat_is_parsed_for_the_addressed_store` — hand-computed
  expectations (`Some(222)` / `Some(111)` / `None`), not the literal the function returns.

Both expectations are **independent** (hand-computed constants), so the tautology shapes the
early iterations were rejected for cannot recur here.

Gate honesty:
- `cargo fmt --all -- --check` → exit 0.
- `cargo check --workspace --all-targets` (default, tikv OFF) → clean.
- `cargo check -p wyrd-metadata-tikv --features tikv --tests` → **exit 0** — the edited
  `#[cfg(feature = "tikv")]` scenario body (the `heartbeat_stale` field, the `*_live` oracle
  methods, and the `wyrd_testkit::{parse_store_last_heartbeat, heartbeat_is_fresh}` calls)
  type-checks. This is exactly the command the toolchain-gated `feature_gated_checks()`
  (`xtask/src/main.rs:828`) runs under `WYRD_TIKV_TOOLCHAIN`, so iter-11 item 5's type-check
  confirmation holds with the fix in place.
- `cargo xtask ci` (via `./engine/xtask.sh ci`, tikv toolchain OFF) → **all checks passed**;
  the no-TiKV/offline invariant is preserved (the feature check stays gated off, the pre-1.0
  `tikv-client` tree is not compiled by the default gate).

## Off-Check (privileged Tier job) — what the fix now makes possible, and what a human must confirm

The correctness of the oracle *logic* is validated at Check in **both directions** by the pure
unit tests (fresh 5 s < 20 s → live; stale 30 s > 20 s → not-live). The **live end-to-end**
both-directions validation the iteration-11 directive asks for — a real bidirectional cut
setting `fault_materialized = true` (leg passes) **and** a no-op negative control (skip
iptables) still classifying as no-op (leg fails), with logs captured — can only run on the
privileged Docker host with the ≥3-replica cluster (`deploy/tikv-multi-replica`), gated by
`WYRD_TIER1=1` / `WYRD_TIKV_PD_ENDPOINTS` / `WYRD_TIER1_ISOLATED_IP`. That remains a
**NEEDS-HUMAN** confirmation for the named Tier-job owner at sign-off (items 3(a)/(b)). The
mechanism is now correct by construction: with the heartbeat oracle, a real cut makes PD's
`last_heartbeat` for the isolated store go stale within the window (so
`partition_took_effect(true, false)` → materialized), while a no-op leaves it fresh (so
`partition_took_effect(true, true)` → not materialized) — the two directions the pure oracle
already encodes and the unit tests already pin.

## Why this shape (and what I rejected)

- **Why extract into `testkit`, not just fix the field in the gated scenario.** The minimal
  textual fix (swap `"state_name"` → `"last_heartbeat"` inline in the `#[cfg(feature="tikv")]`
  body) would satisfy the letter of the directive but leaves the new decision logic behind a
  feature the default gate never compiles — precisely the iter-9 gap ("the oracles' sole real
  consumer sits behind a feature Check never builds, so a live-leg regression flips no Check
  artifact"). Extracting the ~15 lines of pure parse+arithmetic into `testkit` (where the
  sibling oracles `partition_took_effect`/`heal_is_complete` already live) costs two small
  functions + two unit tests and makes the fault-effect field selection **flippable at Check**
  for the first time. The live scenario still drives it (it calls the extracted functions), so
  this is not a parallel re-implementation.
- **Why `last_heartbeat` and not pd-ctl's derived `Disconnected` status.** The directive
  offered either. `last_heartbeat` needs no extra binary on the host and reuses the existing
  dependency-free raw-HTTP GET to `/pd/api/v1/stores` (`tier1_metadata_consistency.rs:494`),
  so it adds **zero** crate/host deps — a smaller change than shelling out to pd-ctl.
- **Why not touch the ratified body.** iteration 11 ratified everything else; the toolchain
  gating, the guard test, the symmetric partition, the heal, the seed labelling, and the pure
  quorum oracles all held under adversarial probing across v5–v11. Re-opening any of them
  risks regressing evidence that already survived review.

## Invariants held

- `crates/traits/src/lib.rs` — untouched (byte-for-byte).
- `crates/metadata-tikv/src/**` — untouched. Only `Cargo.toml` (dev-dependency, from the
  earlier iterations) and test files change; no `src` edit. The trait is *driven*, never edited.
- The at-Check binding-correctness posture is unchanged (Option B, off-Check); no self-authored
  sim, no patch-authored mode flag reintroduced. This iteration only corrects the off-Check
  fault-effect oracle and adds its at-Check-flippable pure coverage.
- Commit-ready: `cargo fmt --all` clean over every touched file; `cargo xtask ci` green.

## NEEDS-HUMAN carried forward (unchanged; not this iteration's call)

Option-B posture / `madsim-tikv-client` non-existence (ratified); the off-Check binding
Tier-1/Tier-2 legs — now with a **correct** heartbeat fault-effect oracle — confirmed only by
the privileged CI/eval Tier job running both directions live and capturing logs (names the
confirmer at sign-off; iter-11 items 3(a)/(b)); iter-11 item 5 (a `WYRD_TIKV_TOOLCHAIN=1`
`cargo xtask ci` run type-checks the `#[cfg(feature = "tikv")]` bodies — the underlying
`cargo check … --features tikv --tests` is confirmed exit-0 here); the metadata-nemesis ADR
question (architecture board); the #365 static-endpoints reduced bar. This iteration re-opens
none of them.
