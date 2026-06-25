# Brief — issue 196 / tier2-kill-reconstruct-harness

> The Plan artifact (docs 02 §PLAN). Human-authored. Do reads ONLY this file.
> The field labels below are parsed by the driver — keep the `- **Label:** value`
> shape. The success criterion is the load-bearing field: it is the sentence
> Check tests "did this work" against.

- **Slug:** tier2-kill-reconstruct-harness
- **Defect:** M3.8 (#146) shipped only the Tier-0 DST campaign. The Tier-2 leg of the
  M3 verification campaign (proposal 0005 §13.2, `0005:409-411`) — on a single real node,
  kill a real D server and watch real reconstruction over real NVMe/fsync — is **not
  built**. What exists in `xtask/src/faults.rs` is inert dispatch scaffolding:
  `run_kill_reconstruct` gates on `WYRD_TIER2=1` and then `sh -c` an
  **externally-supplied** `WYRD_TIER2_CMD` that does not exist anywhere in the repo. The
  only thing unit-tested is the opt-in gating decision (`plan`); there is no in-repo
  harness that stands up a live node, kills a D server, drives reconstruction, and
  asserts the durability outcome. The Tier-2 coverage proposal 0005 promises is absent.
- **Success criterion:** Real in-repo Tier-2 kill-and-reconstruct harness code exists and
  is exercised by tests, replacing the `WYRD_TIER2_CMD` external-command bypass. BINDING
  and demonstrable by C4-verify at Check: (a) `xtask` contains a Tier-2 harness module
  whose orchestration logic (node/container bring-up plan, the D-server kill step, the
  post-kill reconstruction wait, and the durability assertion — affected chunks return to
  full redundancy in **distinct failure domains**, and a crash mid-repair leaves
  **collectable garbage, never corruption**) is implemented **in-repo**, not delegated to
  an unset env-var command; (b) that harness logic is covered by unit tests that run
  inside `cargo xtask ci` and fail if the harness is stubbed out (born-at-tier coverage —
  see Verification posture); (c) `./engine/xtask.sh ci` still exits 0 and the privileged
  Tier-2 path remains **excluded** from the unprivileged container-free `ci` gate
  (ADR-0016). The actual privileged execution (real node, NVMe/fsync, docker) is
  confirmed green off-Check by the new privileged CI job (see Verification posture) — it
  is supplementary evidence, NOT the Check-gating condition. The component names are
  ILLUSTRATIVE of the harness's parts; BINDING is that real in-repo harness code exists
  and is test-exercised, not external-command scaffolding.
- **Invariant to restore:** The M3 verification campaign's Tier-2 leg is honoured by
  **real, in-repo, test-exercised harness code**, not by an opt-in shell-out to an
  absent external command. Stated over the category (a deferred/off-Check tier): a tier
  that is "deferred" means its *green is observed off-Check*, NOT that the deliverable is
  unbuilt — the harness itself must exist and be exercised by something at Check (unit
  tests over its logic). Source: proposal 0005 §13.2 / §"DST and tests" (`0005:409-411`,
  the Tier-2 single-node kill-and-reconstruct mandate) and the crate touch-point
  `0005:437-438` ("xtask — … the Tier-2 kill-and-reconstruct integration"); ADR-0009
  (DST is the correctness authority; a real-world bug-finding run is promoted back as a
  permanent seeded regression); `templates/brief.md.tpl` Verification-posture forcing
  function ("deferred ≠ unbuilt — the #146 Tier-1/2 gap"). This is
  new-feature/infrastructure work, not a behavioural bug fix, so minimalism does not
  govern it (principles.md §1.3): the target is a Tier-2 harness that actually kills a
  live D server and verifies real reconstruction, not the smallest diff to `faults.rs`.
- **Repo + branch target:** getwyrd/wyrd @ main   (no maintenance branches today; INTEGRATION §2)
- **Onto branch:**
- **Depends on:**
- **Depends on (merged):** 195
- **Conflicts with:** 195
- **Stacks on:**
- **Ordering note:** 196 (Tier-2) and 195 (Tier-1) both edit `xtask/src/faults.rs` (the
  `run_kill_reconstruct` vs `run_disk_faults`/`run_jepsen` runners), the
  `xtask/src/main.rs` subcommand dispatch, and add a privileged off-Check CI workflow.
  No build-on dependency, but they edit the same files, so this bundle is held until
  195's PR is **merged** (`Depends on (merged): 195`) so Do builds on the merged
  `faults.rs`/`main.rs` instead of colliding at merge. 195 is first per proposal 0005
  §13.4 tier order (Tier-0→1→2). `Conflicts with: 195` is the reciprocal of 195's entry —
  belt-and-braces so they never co-schedule in one wave even if the merge gate is relaxed.
- **Surfaces:** data   (xtask/CI tooling + custodian repair path; no frontend)
- **Scope:** Build the Tier-2 single-node kill-and-reconstruct harness as **real in-repo
  Rust**, reusing the existing Tier-2 *container* precedent rather than re-inventing
  cluster bring-up — `crates/chunkstore-grpc/tests/docker-compose.yml`, the `d-server`
  role of the single `wyrd` binary (`crates/server`), and xtask `run_integration`'s
  compose-up/endpoint-resolve/teardown plumbing (`compose_up`, `resolve_endpoints`,
  `finish_integration`, `finalize_panic_safe`): (1) a new `#[ignore]`d integration-test
  scenario (e.g. `crates/chunkstore-grpc/tests/tier2_kill_reconstruct.rs`, sibling to
  `tier2_integration.rs`) that, against the live containerized cluster, kills a D server,
  drives the **production** custodian reconstruction path
  (`custodian::reconcile_step` / `reconstruction::reconcile`,
  `crates/custodian/src/{reconciliation,reconstruction}.rs`), and asserts the campaign
  outcome (the killed node's affected chunks return to full redundancy in **distinct
  failure domains**, and a crash mid-repair leaves **collectable garbage, never
  corruption**); (2) xtask's `run_kill_reconstruct` orchestrating bring-up + kill +
  invoking that scenario, **replacing** the `WYRD_TIER2_CMD` external-command shell-out;
  (3) host-independent orchestration logic (kill-victim selection, the redundancy /
  distinct-domain / garbage-not-corruption assertion helpers) unit-tested inside
  `cargo xtask ci`; (4) a **privileged** off-Check CI job (real node — NVMe/fsync, docker)
  that runs it green, kept out of the unprivileged container-free `cargo xtask ci`
  (ADR-0016). The harness MUST drive the REAL production reconstruction path, never a
  parallel reimplementation (ADR-0009: a real-world discovery promotes back as a permanent
  seeded regression). / out of scope: Tier-1 disk-fault / Jepsen harness (#195); any
  change to production custodian / reconstruction behaviour (the harness exercises
  existing behaviour, it does not alter it); Tier-3 multi-region hardware (M5+); editing
  the accepted proposal 0005 or any ADR (immutable, INTEGRATION §2 — supersede, don't edit
  in place).
- **Repro instruction:** On `main`, read `../wyrd/xtask/src/faults.rs`:
  `run_kill_reconstruct` contains no harness — it `execute(...)` an env-supplied
  `WYRD_TIER2_CMD`, and `grep -rn "WYRD_TIER2_CMD" ../wyrd` shows the command is never
  defined in-repo. The `#[cfg(test)]` module covers only the `plan` gating decision,
  never a kill/reconstruct scenario. Compare the Tier-2 *container* integration
  `run_integration` in `xtask/src/main.rs`, which DOES carry a real in-repo harness
  (`compose_up`/`run_integration_test`/`finish_integration` + `finalize_panic_safe`) with
  unit tests over its host-independent logic — the kill-and-reconstruct harness must
  reach that bar.
- **Test file:** TWO-part, mirroring the verified Tier-2 container precedent. (a)
  SCENARIO — a new `#[ignore]`d integration test carrying the kill-and-reconstruct
  scenario (e.g. `crates/chunkstore-grpc/tests/tier2_kill_reconstruct.rs`, sibling to
  `tier2_integration.rs`), attributed like its sibling
  (`#[ignore = "Tier-2: needs real containerized D servers — run via cargo xtask kill-reconstruct"]`).
  `cargo xtask ci`'s `cargo test --workspace` (`xtask/src/main.rs:413`) **compiles and
  type-checks** this file at Check (its `#[ignore]`d body runs only in the privileged
  job), so the harness is real, API-bound Rust — not an env-var shell string. (b)
  ORCHESTRATION — xtask `#[cfg(test)]` unit tests over `run_kill_reconstruct`'s
  host-independent logic (kill-victim selection / redundancy / distinct-domain /
  garbage-not-corruption assertion helpers), in `faults.rs` or a new `kill_reconstruct.rs`
  sibling, running inside `cargo xtask ci`. The born-at-tier flippable coverage is (b)
  (red when a helper is stubbed, green when implemented); (a) is the off-Check scenario
  whose green the privileged job confirms.
- **Verification posture:** DEFERRED/off-Check, NET-NEW (forcing function — the #146
  Tier-2 gap; the case `templates/brief.md.tpl` calls out explicitly). What is BUILT AND
  exercised at Check: (i) the `#[ignore]`d scenario harness is **real Rust compiled and
  type-checked by `ci`'s `cargo test --workspace`** — it calls the production
  `reconcile_step`/`reconstruction::reconcile` APIs and the gRPC chunkstore, so a
  regression that reduced it to a stub (or the old shell-string dispatch) would fail to
  compile; compilation alone proves it is not inert dispatch scaffolding; (ii) the xtask
  orchestration logic (`run_kill_reconstruct`'s kill-victim selection / assertion helpers)
  is unit-tested inside `cargo xtask ci` — the flippable born-at-tier coverage. "Red" for
  (ii) is criterion-ABSENCE plus a *demonstrated* red: Do must capture a demonstrated red
  (temporarily stub a helper/assertion and show the unit test fails) proving the new seam
  is load-bearing, not resting red on non-existence. DEFERRED to off-Check: the real run
  against a live node (NVMe/fsync, docker) cannot go green in the unprivileged
  container-free Check worktree (ADR-0016); its green is confirmed by the new **privileged
  Tier-2 CI job** (real node, docker), opted in via `WYRD_TIER2=1`, modelled on
  `integration-nightly.yml`, whose maintainer is Eduard Ralph (INTEGRATION §10). FORCING
  FUNCTION satisfied: the deferred deliverable is itself BUILT (the in-repo harness,
  compiled at Check) and exercised by unit tests — NOT inert dispatch scaffolding. If the
  live-node scenario cannot be functionally implemented in this slice, it is a SEPARATE
  work item, not a deferred-verification line — say so rather than ship an empty runner.
- **Production reach:** N/A as a production seam — this slice builds verification tooling.
  But the harness MUST traverse the REAL production reconstruction path (`reconcile_step`
  → `reconstruction::reconcile` over the live gRPC chunkstore cluster), the same fenced
  control point the Tier-0 campaign drives; it must NOT be a parallel reimplementation of
  repair, or it verifies nothing (ADR-0009).
- **Citations expected:** Do must cite path:line on `main` (getwyrd/wyrd) for every
  change — `xtask/src/faults.rs` (`run_kill_reconstruct`), `xtask/src/main.rs` dispatch
  (`kill-reconstruct` arm ~line 43), the new `#[ignore]`d scenario test, any reuse of
  `crates/chunkstore-grpc/tests/docker-compose.yml` / xtask compose plumbing, the new
  privileged CI workflow under `.github/workflows/`, and any `crates/custodian` /
  `crates/server` helper touched. Cite the production entry point exercised
  (`crates/custodian/src/reconciliation.rs::reconcile_step` and `reconstruction`).
- **Prior-art check (triage cycles):** Searched by affected file path across merged
  history and open/closed PRs. `xtask/src/faults.rs`'s `run_kill_reconstruct` was
  introduced by #146 (PR #194, commit 2516b68) as a **deferred** runner that "skips
  cleanly unless explicitly opted in" — the scaffolding this brief replaces; no later PR
  has implemented the harness body. A repo-wide search for kill-and-reconstruct fault
  tests finds only `crates/dst/tests/custodian.rs` (the Tier-0 *simulated* campaign,
  including the D-server-kill seam) and `crates/chunkstore-grpc/tests/tier2_integration.rs`
  (the M2 container write/read e2e — NOT a kill/reconstruct scenario). The Tier-2 container
  integration (`run_integration`, M2/proposal 0004, `integration-nightly.yml`, the
  `docker-compose.yml` cluster) is the in-repo precedent to mirror/reuse for cluster
  bring-up. Not a duplicate. Overlaps #195 on shared xtask files — handled via the
  scheduling fields above.
- **Disposition hint:** likely-fix

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a
draft PR MAY happen during the cycle (useful for CI feedback). The PR MUST NOT be
marked ready before sign-off accepts.

## Iteration 1 — carry-forward (from the previous attempt)
- Sign-off rationale: Rejected on T4 (born-at-tier coverage must be correct, and one helper is not). What to change next (issue_196): 1. `assert_garbage_not_corruption` (xtask/src/kill_reconstruct.rs:109) is logically inverted. After a crash before the version-conditional commit the inode is FULLY OLD, so the victim IS still in the committed placement. The helper must PASS when committed_placement_has_victim == true (not false), matching the live scenario test (tier2_kill_reconstruct.rs:537) and the DST property (crates/dst/tests/custodian.rs:617). Update the helper AND its unit test (kill_reconstruct.rs:1043) together — both currently encode the same inversion, so the green is vacuous. 2. Resolve the orphaned-helper architecture: the three #[cfg(test)] assert_* helpers are unreachable from the real harness (separate crate) and merely duplicate the scenario test's inline asserts. Either wire them into the real scenario/harness path so the born-at-tier coverage is load-bearing, or drop the dead duplication — do not keep both. (select_victim_index / victim_container_name are genuinely wired and fine.) 3. While in here, fix the non-gating broken intra-doc link at faults.rs:149 (crate::kill_reconstruct_test::tier2_kill_reconstruct resolves to nothing; the scenario lives in a different crate; would fail cargo doc under broken_intra_doc_links = "deny"). T5 / Validation (privileged WYRD_TIER2=1 green, fitness-to-purpose) were not reached this pass — re-evaluate after the T4 fix lands.
- Full previous attempt preserved in `iteration-v1/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 2 — carry-forward (from the previous attempt)
- Sign-off rationale: Rejected on the codex advisory: broken intra-doc links in the new xtask/src/kill_reconstruct.rs module doc. It references the three assertion helpers as rustdoc links — [`assert_garbage_not_corruption`], [`assert_redundancy_outcome`], [`assert_distinct_domains`] (patch ~lines 1033-1037) — but those helpers were re-homed into the chunkstore-grpc test crate (crates/chunkstore-grpc/tests/tier2_kill_reconstruct.rs), so they are unresolvable from xtask. Root Cargo.toml:170 sets rustdoc::broken_intra_doc_links = "deny", so `cargo doc -p xtask` would error. This is the SAME defect class iteration 1 was rejected on (the faults.rs broken link); the re-home fixed the orphan but left dangling references behind. Why the gate stayed green: `cargo xtask ci` (fmt/clippy/build/test/deny/ conformance/dst) runs no `cargo doc` step and no workflow runs cargo doc, so the rustdoc lint is never exercised — C4-ci pass does not clear this. What to change: convert those three bracketed intra-doc links to plain code spans (drop the square brackets: `assert_garbage_not_corruption` etc.), or otherwise make them resolvable. Re-scan the patch for any other cross-crate `[`...`]` doc references introduced by the re-home before rebuilding. Still-open §6 items carried forward (not the rejection cause, but unresolved): - T5 fidelity: in-memory MemMeta/CrashMeta seam vs. proposal 0005 §13.2 "real NVMe/fsync" Tier-2 mandate — the author must ratify the reinterpretation. - Validation: confirm the privileged WYRD_TIER2=1 job runs the scenario green on a real node, that CrashMeta's single-positive-precondition crash model matches the production reconstruction commit sequence, and that the gate base contains merged #195.
- Full previous attempt preserved in `iteration-v2/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).
