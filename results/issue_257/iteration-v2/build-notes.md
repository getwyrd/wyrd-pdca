# Build notes — issue 257 / m4.6-tier1-jepsen-tier2 (iteration 2)

Target branch: `getwyrd/wyrd @ feat/m4-production-metadata-backend` (worktree
`$PDCA_WORKTREE = /home/eddie/wyrd/wyrd.pdca-wt-l0`). Planning artifact: accepted
proposal **0015** §"DST and tests", §"Crate touch-points", §"Suggested PR sequence" item 6.

## What changed since iteration 1 (the carry-forward, point by point)

The v1 patch was rejected on C4-verify (the Check-observable red→green **did not
reproduce** — the flippable passed without the fix) plus three adversarial findings. The
root cause was structural: **v1 put its flippable unit tests INSIDE modified production
files** (`#[cfg(test)] mod tests` in `xtask/src/faults.rs` and `crates/testkit/src/lib.rs`).
`engine/scripts/run-verify.sh` reverts modified production files for the RED phase and keeps
only the *added* `*/tests/*.rs` files — so v1's flippables were reverted along with the fix
and never went red. Every fix below moves the load-bearing assertion into an **added test
file** whose *compilation or oracle* depends on the production change.

### 1. `meta_dispatch` routing test was a tautology → now a filesystem-resolved oracle
- The dispatch decision is extracted to a **new pure module** `xtask/src/meta_dispatch.rs`
  (`MetaTier` / `MetaDispatch` / `meta_dispatch`), exposed via the xtask **library target**
  (`pub mod meta_dispatch` in `xtask/src/lib.rs:18`) exactly like the precedent
  `xtask/tests/disk_faults_orchestration.rs` ↔ `xtask::disk_faults`.
- The flippable is the **added** `xtask/tests/meta_dispatch_orchestration.rs`. Its oracle is
  **not** a restatement of the returned literals: `MetaDispatch` now carries `manifest_dir`,
  and the test **resolves the route against the real workspace** — the routed `package` must
  equal the crate's actual `Cargo.toml` `name`, and the routed `--test <name>` must correspond
  to an **existing** `tests/<name>.rs` file (`meta_dispatch_orchestration.rs:61-98`). A typo in
  `package`/`test` matched by a typo in the expectation can no longer pass: the filesystem, not
  a mirrored literal, is the oracle. That is precisely "verify routing resolves to a real,
  runnable target."
- **RED proof:** with `xtask/src/meta_dispatch.rs` removed and `pub mod meta_dispatch`
  reverted, `use xtask::meta_dispatch::…` fails to resolve → the target fails to compile → RED.

### 2. The decorative DST "promoted regression" → removed; replaced by a committed seed registry
- v1's `crates/dst/tests/tikv_surfaced_regressions.rs` re-proved redb's own atomicity
  (`AwaitingStore::commit` yielded *before* an inner redb commit that has **no `.await`**, so
  under madsim's single-threaded executor no racer interleaved mid-commit). The adversary was
  right: it modelled nothing and could never go red, violating the invariant "a real
  environment is never used to test correctness the simulation already covers."
- The carry-forward offered two paths: model the real interleaving so it goes red without the
  fix, **or** remove the decorative seed. A genuine red→green DST regression is **impossible in
  this slice**: the behaviour is only observable through the commit protocol / conflict
  detection in the metadata backend, which the brief invariant **forbids touching** ("does not
  touch `traits`, `core`, `custodian`, or the metadata backend logic"). A red→green whose green
  requires editing the backend belongs to the backend's slice.
- So the decorative test is **deleted** and the discovery is committed as a **documented
  known-gap seed registry** `crates/dst/tests/tikv_surfaced_seeds.md` (data, not a vacuous
  test — cargo ignores non-`.rs` under `tests/`). `SEED-0001` pins seed `17`, names the
  await-inside-commit hypothesis (proposal 0015 §"Pinning the trait"), and states exactly what
  the slice-7 / #258 harness must replay. This satisfies "the seed is committed" (DoD) without
  a test that re-proves Tier-0 atomicity. **Provenance is NEEDS-HUMAN** per the brief's Known
  NEEDS-HUMAN #5 (documented known-gap vs a live-cluster discovery — the live Tier-1 job that
  would surface a real one is off-Check).

### 3. The Jepsen leg "applied no nemesis" → the runner now actually injects it
- v1 computed fault node indices, exported them in `WYRD_TIER1_NEMESIS_NODES`, and the test
  merely *logged* them — it could pass with no fault applied.
- Now the runner **genuinely injects** a partition nemesis: `run_meta_jepsen_with_nemesis`
  (`xtask/src/faults.rs`) draws a reproducible, quorum-safe plan through the `wyrd-testkit`
  seam (`jepsen_nemesis_services(seed)`), maps each faulted node to its real
  `deploy/small-multi-node` PD service (`pd0`/`pd1`/`pd2`), and on a background thread
  **`docker compose pause`s those PD services** (the partition), holds a fault window, then
  `unpause`s them (the heal) — concurrent with the scenario's CAS/read load. `docker compose
  pause` is a real container-level process-pause needing no root/`tc`.
- The Jepsen test (`crates/metadata-tikv/tests/tier1_jepsen_metadata.rs`) now **refuses to run
  without a nemesis plan present** (`nemesis_nodes()` → `panic!` if `WYRD_TIER1_NEMESIS_NODES`
  is absent/empty). A Jepsen leg that applied no fault now fails loudly rather than passing
  vacuously. An empty plan is also rejected in the runner itself.
- **On-Check testability of the wiring:** the pure node→service mapping and the quorum-safe
  reproducibility are unit-tested in `meta_dispatch_orchestration.rs:114-155` with a real
  oracle (`survivors*2 > n`, `same seed → same plan`, and each service must be **declared in
  the deploy compose**). The *live* nemesis red→green stays off-Check (needs docker + the #256
  cluster) — declared, not hidden.

## Changes (path:line on `feat/m4-production-metadata-backend`)

- **`xtask/src/meta_dispatch.rs`** (new) — pure dispatch + nemesis-plan core (no docker/TiKV/
  `crate::` helpers), exposed via the lib target so it is unit-tested at Check.
- **`xtask/src/lib.rs:18`** — `pub mod meta_dispatch;`.
- **`xtask/src/faults.rs`** — the three privileged runners (`run_meta_integration` /
  `run_meta_jepsen` / `run_meta_tier2`), the real nemesis injection, and the compose helpers.
- **`xtask/src/main.rs`** — `mod meta_dispatch;` + the `meta-integration`/`meta-jepsen`/
  `meta-tier2` subcommands, usage, and module doc.
- **`xtask/tests/meta_dispatch_orchestration.rs`** (new) — the Check-observable dispatch/seam
  flippable (filesystem-resolved routing oracle + nemesis-plan oracle).
- **`crates/testkit/src/lib.rs`** — the real-TiKV `MetaFault` / `SeededMetaFaults` seam
  (kept from v1 — the adversary certified its `survivors*2 > n` oracle genuine; added
  `faulted_nodes()`). The seam unit tests moved to an **added** file:
- **`crates/testkit/tests/meta_fault_seam.rs`** (new) — the seam flippable (compile-RED when
  the seam is reverted; independent `survivors*2 > n` majority oracle).
- **`crates/metadata-tikv/tests/{tier1_metadata_integration,tier1_jepsen_metadata,tier2_metadata_io}.rs`**
  (new) — the metadata-swap tier targets; `#[ignore]` + clean-skip without
  `WYRD_TIKV_PD_ENDPOINTS`, real body only under `--features tikv` (headless-safe; the gate
  stays green with no TiKV).
- **`crates/dst/tests/tikv_surfaced_seeds.md`** (new) — the committed compounding-loop seed
  registry (replaces v1's decorative DST test).
- **`xtask/Cargo.toml`** + **`Cargo.lock`** — the `wyrd-testkit` dep the nemesis seam needs.

## Invariants held

- **Trait untouched:** `crates/traits/src/lib.rs` `MetadataStore` byte-for-byte unmodified.
- **DST keeps correctness authority:** no atomicity is re-proved against TiKV; the decorative
  redb re-proof is removed.
- **Single-zone only; static endpoints:** the nemesis faults a strict PD minority; the client
  dials the published static PD endpoints (`23791..=23793`) per the Deployment-prerequisite note.
- **Gate honesty:** `cargo xtask ci` stays green with no TiKV / no privileged fault injection —
  the tier targets skip cleanly.

## Red→green proof (the failing gate, now passing)

Run through the project's own gate `engine/scripts/run-verify.sh` (base
`origin/feat/m4-production-metadata-backend`):

```
run-verify.sh: GREEN — cargo test … (fix applied)      → all pass / tier tests skip
run-verify.sh: RED   — (production reverted, test kept) → E0432: no `SeededMetaFaults`
                                                          in wyrd_testkit → compile fail
run-verify.sh: PASS  — red without the fix, green with it.
```

Gate-parity in the worktree: `cargo fmt --all -- --check` clean; `cargo clippy -p xtask
-p wyrd-testkit -p wyrd-metadata-tikv --all-targets` no warnings (`-D warnings`);
`cargo build --workspace --all-targets` Finished; `cargo machete` no unused deps.

## Rejected alternatives (with cost)

- **Keep the flippable inline in `faults.rs`/`lib.rs` (v1).** Rejected — `run-verify.sh`
  reverts modified production files in the RED phase, so an inline `#[cfg(test)]` test is
  reverted with the fix and never goes red. Concrete cost: it is exactly why C4-verify FAILED
  in iteration 1. An **added** `tests/*.rs` file is the only shape the gate can isolate a RED
  against.
- **Assert the dispatch literals directly (v1's tautology).** Rejected — a matching typo in
  literal + expectation passes green (adversary finding #1). The filesystem-resolved oracle
  (`Cargo.toml` name + `tests/<name>.rs` existence, ≈35 lines) cannot be fooled that way.
- **Model the await-inside-commit interleaving as a genuine red→green DST test.** Rejected —
  its green requires the commit-protocol/conflict-detection code in the metadata backend,
  which the brief invariant forbids this slice from touching. Cost of doing it here: a diff into
  `crates/metadata-tikv/src` (forbidden surface) — a boundary violation, not a size argument.
  Committed as a documented seed for #258 instead.

## NEEDS-HUMAN (pre-declared)

1. **Privileged-off-Check live green (C2/C4).** The live Tier-1 integration + Jepsen + Tier-2
   green (docker + the #256 cluster; `WYRD_TIER1=1`/`WYRD_TIER2=1`) is confirmed only by the
   privileged CI/eval Tier job — not in the Check worktree. The Check-observable red→green is
   the dispatch + seam unit tests above.
2. **#256 dependency.** The metadata Tier-1 legs need the `deploy/small-multi-node` cluster
   (on the base per `git log`, PR #428). The live job's staging is the human's to confirm.
3. **#365 / L5-discovery reduced bar.** Static endpoints until #365; human confirms the
   reduced-bar posture is acceptable.
4. **Compounding-loop seed provenance.** `crates/dst/tests/tikv_surfaced_seeds.md` `SEED-0001`
   is a documented **known-gap** hypothesis (proposal 0015 §"Pinning the trait"), not yet a
   live-cluster discovery — the live Tier-1 job that would surface one is off-Check. Human
   judges whether the committed known-gap seed satisfies the DoD bullet (brief Known
   NEEDS-HUMAN #5). It feeds #258 either way.
5. **Jepsen tooling shape.** Confirmed in-repo (the metadata Jepsen leg is an in-repo Rust
   scenario driven by `cargo xtask meta-jepsen`, mirroring the post-#250 chunkstore Jepsen
   route), with the nemesis applied by the runner via `docker compose pause`. Noted per the
   brief's item.
