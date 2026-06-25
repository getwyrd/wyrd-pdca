# Brief — issue 195 / tier1-disk-fault-harness

> The Plan artifact (docs 02 §PLAN). Human-authored. Do reads ONLY this file.
> The field labels below are parsed by the driver — keep the `- **Label:** value`
> shape. The success criterion is the load-bearing field: it is the sentence
> Check tests "did this work" against.

- **Slug:** tier1-disk-fault-harness
- **Defect:** M3.8 (#146) shipped only the Tier-0 DST campaign. The Tier-1 legs of the
  M3 verification campaign (proposal 0005 §13.2, `0005:405-408`) — disk-fault injection
  via device-mapper `dm-flakey`/`dm-error` and a Jepsen consistency run over the repair
  path — are **not built**. What exists in `xtask/src/faults.rs` is inert dispatch
  scaffolding: `run_disk_faults` / `run_jepsen` gate on `WYRD_TIER1=1` and then `sh -c`
  an **externally-supplied** `WYRD_TIER1_DISK_CMD` / `WYRD_TIER1_JEPSEN_CMD` that does
  not exist anywhere in the repo. The only thing unit-tested is the opt-in gating
  decision (`plan`); there is no in-repo harness that actually sets up a faulted block
  device, drives scrub / checksum-verification + reconstruction over it, and asserts the
  redundancy outcome. The Tier-1 coverage proposal 0005 promises is therefore absent.
- **Success criterion:** Real in-repo Tier-1 disk-fault harness code exists and is
  exercised by tests, replacing the `WYRD_TIER1_DISK_CMD` external-command bypass.
  BINDING and demonstrable by C4-verify at Check: (a) `xtask` contains a Tier-1
  disk-fault harness module whose orchestration logic (device-mapper table planning, the
  fault scenario steps, and the post-repair redundancy/no-read-error assertion) is
  implemented **in-repo**, not delegated to an unset env-var command; (b) that harness
  logic is covered by unit tests that run inside `cargo xtask ci` and fail if the harness
  is stubbed out (born-at-tier coverage — see Verification posture); (c) `./engine/xtask.sh
  ci` still exits 0 and the privileged Tier-1 path remains **excluded** from the
  unprivileged container-free `ci` gate (ADR-0016). The actual privileged execution
  (root + `dmsetup`, real block faults) is confirmed green off-Check by the new
  privileged CI job (see Verification posture) — it is supplementary evidence, NOT the
  Check-gating condition. The component names ("device-mapper table planning", "redundancy
  assertion") are ILLUSTRATIVE of the harness's parts; BINDING is that real in-repo
  harness code exists and is test-exercised, not external-command scaffolding.
- **Invariant to restore:** The M3 verification campaign's Tier-1 leg is honoured by
  **real, in-repo, test-exercised harness code**, not by an opt-in shell-out to an
  absent external command. Stated over the category (a deferred/off-Check tier): a tier
  that is "deferred" means its *green is observed off-Check*, NOT that the deliverable is
  unbuilt — the harness itself must exist and be exercised by something at Check (unit
  tests over its logic). Source: proposal 0005 §13.2 / §"DST and tests" (`0005:405-408`,
  the Tier-1 disk-fault + Jepsen mandate) and the crate touch-point `0005:437` ("xtask —
  Tier-1 disk-fault (dm-flakey/dm-error) + Jepsen runners"); ADR-0009 (DST is the
  correctness authority and every real-world discovery is promoted back as a seeded
  regression); `templates/brief.md.tpl` Verification-posture forcing function ("deferred
  ≠ unbuilt — the #146 Tier-1/2 gap"). This is new-feature/infrastructure work, not a
  behavioural bug fix, so minimalism does not govern it (principles.md §1.3): the target
  is a Tier-1 harness that actually runs the scrub/checksum + reconstruction path against
  real block-layer faults, not the smallest diff to `faults.rs`.
- **Repo + branch target:** getwyrd/wyrd @ main   (no maintenance branches today; INTEGRATION §2)
- **Onto branch:**
- **Depends on:**
- **Depends on (merged):**
- **Conflicts with:** 196
- **Stacks on:**
- **Ordering note:** 195 (Tier-1) and 196 (Tier-2) both edit `xtask/src/faults.rs` (the
  `run_disk_faults`/`run_jepsen` vs `run_kill_reconstruct` runners), the `xtask/src/main.rs`
  subcommand dispatch, and add a privileged off-Check CI workflow. They have no build-on
  dependency, but they collide on those shared files, so they must never run in the same
  concurrent wave. 195 is sequenced first (proposal 0005 §13.4 tier order Tier-0→1→2);
  196 carries the reciprocal `Depends on (merged): 195` so its Do builds on the merged
  `faults.rs`/`main.rs` instead of colliding at merge.
- **Surfaces:** data   (xtask/CI tooling + custodian repair path; no frontend)
- **Scope:** Build the Tier-1 disk-fault harness as **real in-repo Rust**, mirroring the
  existing Tier-2 *container* precedent (xtask `run_integration` + the `#[ignore]`d
  `crates/chunkstore-grpc/tests/tier2_integration.rs`): (1) a `#[ignore]`d
  integration-test scenario that opens a real `FsChunkStore` (`crates/chunkstore-fs`,
  `FsChunkStore::open`) on a device-mapper `dm-flakey`/`dm-error`-faulted backing device
  and drives the **production** custodian path — `custodian::reconcile_step` /
  `scrub::reconcile` / `reconstruction::reconcile`
  (`crates/custodian/src/{reconciliation,scrub,reconstruction}.rs`), the same fenced
  control point the Tier-0 DST campaign drives — asserting the campaign outcome (faulted
  chunks driven back to full redundancy with **no read errors during repair**); (2)
  xtask's `run_disk_faults` orchestrating the dm-device setup and invoking that scenario,
  **replacing** the `WYRD_TIER1_DISK_CMD` external-command shell-out; (3) host-independent
  orchestration logic (the dm-table plan, the redundancy/no-read-error assertion helpers)
  unit-tested inside `cargo xtask ci`; (4) a **privileged** off-Check CI job (root +
  device-mapper / `dmsetup`) that runs the Tier-1 suite green, kept out of the
  unprivileged container-free `cargo xtask ci` (ADR-0016). The harness MUST drive the
  REAL production scrub/reconstruction path, never a parallel reimplementation (ADR-0009:
  a real-world discovery promotes back as a permanent seeded regression). **Jepsen leg —
  scope — RESOLVED:** the issue title also names a Jepsen consistency run over the repair
  path; a full Jepsen harness is a separate Clojure/`lein` ecosystem, so it has been
  **split out into its own tracked issue, getwyrd/wyrd#250** ("M3.11 — Tier-1 Jepsen
  consistency harness", proposal 0005 §13.2). This bundle's BINDING deliverable is the
  dm-flakey/dm-error disk-fault harness only. / out of scope: Tier-2 single-node
  kill-and-reconstruct (#196); the Jepsen consistency harness (now #250); any change to
  production custodian /
  reconstruction behaviour (the harness exercises existing behaviour, it does not alter
  it); Tier-3 multi-region hardware (M5+, proposal 0005 §"non-goals"); editing the
  accepted proposal 0005 or any ADR (immutable, INTEGRATION §2 — author a superseding ADR
  if a decision must change, do not edit in place).
- **Repro instruction:** On `main`, read `../wyrd/xtask/src/faults.rs`: `run_disk_faults`
  and `run_jepsen` contain no harness — they `execute(...)` an env-supplied
  `WYRD_TIER1_DISK_CMD`/`WYRD_TIER1_JEPSEN_CMD`, and `grep -rn "WYRD_TIER1_DISK_CMD" ../wyrd`
  shows the command is never defined in-repo. The `#[cfg(test)]` module covers only the
  `plan` gating decision, never a fault scenario. Compare the Tier-2 *container*
  precedent `run_integration` in `xtask/src/main.rs`, which DOES carry a real in-repo
  harness (`compose_up`/`run_integration_test`/`finish_integration`) plus unit tests over
  its host-independent logic — Tier-1 must reach that bar.
- **Test file:** TWO-part, mirroring the verified Tier-2 container precedent. (a)
  SCENARIO — a new `#[ignore]`d integration test carrying the disk-fault scenario (e.g.
  `crates/custodian/tests/tier1_disk_faults.rs` or under `crates/chunkstore-fs/tests/` —
  Do's call), attributed exactly like `tier2_integration.rs`
  (`#[ignore = "Tier-1: needs root + device-mapper — run via cargo xtask disk-faults"]`).
  `cargo xtask ci`'s `cargo test --workspace` (`xtask/src/main.rs:413`) **compiles and
  type-checks** this file at Check (its `#[ignore]`d body runs only in the privileged
  job), so the harness is real, API-bound Rust — not an env-var shell string. (b)
  ORCHESTRATION — xtask `#[cfg(test)]` unit tests over `run_disk_faults`'s
  host-independent logic (dm-table plan / assertion helpers), in `faults.rs` or a new
  `disk_faults.rs` sibling, running inside `cargo xtask ci`. The born-at-tier flippable
  coverage is (b) (red when the helper is stubbed, green when implemented); (a) is the
  off-Check scenario whose green the privileged job confirms.
- **Verification posture:** DEFERRED/off-Check, NET-NEW (forcing function — the #146
  Tier-1 gap; the case `templates/brief.md.tpl` calls out explicitly). What is BUILT AND
  exercised at Check: (i) the `#[ignore]`d scenario harness is **real Rust compiled and
  type-checked by `ci`'s `cargo test --workspace`** — it calls the production
  `FsChunkStore`/`reconcile_step`/`scrub::reconcile`/`reconstruction::reconcile` APIs, so
  a regression that reduced it to a stub (or the old shell-string dispatch) would fail to
  compile; compilation alone proves it is not inert dispatch scaffolding; (ii) the xtask
  orchestration logic (`run_disk_faults`'s dm-table plan / assertion helpers) is
  unit-tested inside `cargo xtask ci` — this is the flippable born-at-tier coverage.
  "Red" for (ii) is criterion-ABSENCE plus a *demonstrated* red: Do must capture a
  demonstrated red (temporarily stub an orchestration helper/assertion and show the unit
  test fails) proving the new seam is load-bearing, not resting red on non-existence.
  DEFERRED to off-Check: the real privileged run against `dm-flakey`/`dm-error` needs root
  + `dmsetup`, so the `#[ignore]`d scenario body cannot go green in the unprivileged
  container-free Check worktree (ADR-0016); its green is confirmed by the new **privileged
  Tier-1 CI job** (root + device-mapper), opted in via `WYRD_TIER1=1`, modelled on
  `integration-nightly.yml`, whose maintainer is Eduard Ralph (INTEGRATION §10). FORCING
  FUNCTION satisfied: the deferred deliverable is itself BUILT (the in-repo harness,
  compiled at Check) and exercised by unit tests — NOT inert dispatch scaffolding. If a
  leg (the Jepsen leg, or even the disk-fault scenario) cannot be functionally
  implemented in this slice, it is a SEPARATE work item, not a deferred-verification
  line — say so rather than ship an empty runner.
- **Production reach:** N/A as a production seam — this slice builds verification
  tooling. But the harness MUST traverse the REAL production custodian path
  (`reconcile_step` → `scrub::reconcile` / `reconstruction::reconcile` over a real
  `FsChunkStore`), the same fenced control point the Tier-0 campaign drives; it must NOT
  be a parallel reimplementation of repair, or it verifies nothing (ADR-0009).
- **Citations expected:** Do must cite path:line on `main` (getwyrd/wyrd) for every
  change — `xtask/src/faults.rs` (`run_disk_faults`), `xtask/src/main.rs` dispatch
  (`disk-faults` arm ~line 41), the new `#[ignore]`d scenario test, the new privileged CI
  workflow under `.github/workflows/`, and any `crates/custodian` / `crates/chunkstore-fs`
  / `crates/testkit` helper touched. Cite the production entry points exercised
  (`crates/custodian/src/reconciliation.rs::reconcile_step` and `scrub`/`reconstruction`).
- **Prior-art check (triage cycles):** Searched by affected file path across merged
  history and open/closed PRs. `xtask/src/faults.rs` was introduced by #146 (PR #194,
  commit 2516b68, "test(dst): add the Tier-0 custodian property campaign") which
  explicitly added the **deferred** Tier-1/Tier-2 runners "which skip cleanly unless
  explicitly opted in" — i.e. the scaffolding this brief replaces; no later PR has
  implemented the harness body. A repo-wide search for fault-harness tests
  (`dm-flakey`/`dm-error`/`disk-fault`/`tier1`) finds only `crates/dst/tests/custodian.rs`
  (the Tier-0 *simulated* campaign) and `crates/chunkstore-grpc/tests/tier2_integration.rs`
  (the M2 container e2e) — no real Tier-1 disk-fault harness exists. The Tier-2 *container*
  tier (`run_integration` + the `#[ignore]`d `tier2_integration.rs`, M2/proposal 0004,
  `integration-nightly.yml`) is the in-repo precedent to mirror. Not a duplicate.
- **Disposition hint:** likely-fix

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a
draft PR MAY happen during the cycle (useful for CI feedback). The PR MUST NOT be
marked ready before sign-off accepts.

## Iteration 1 — carry-forward (from the previous attempt)
- Sign-off rationale: Why rejected (C5 causal adequacy): the scenario does not drive the REAL reconstruction path over the real block-layer fault (brief:77-78). scrub + reconstruction run over `healthy_view`, which strips the victim from the fleet BEFORE the reconstruction pass (tier1_disk_faults.rs:209-211), so `inject_disk_fault()` is causally inert for repair — delete it and the reconstruction half passes identically. The fault is load-bearing only for the two `read_object` assertions. So "faulted chunk driven back to full redundancy" is demonstrated as a normal survivor-only rebuild over an absent server, not over a real disk fault — it adds nothing over the Tier-0 in-memory campaign. What to change next (Do): keep the victim IN the reconstruction fleet view (do not pre-exclude it via `healthy_view`) so the fault drives loss classification through the production read in `reconstruction::assess`. This exercises the branch a real-block-layer Tier-1 harness exists to flush. Heads-up — likely exposes a real production divergence, which is the point: - read path tolerates EIO: read.rs:188-213 admits only `if let Ok(Some(_))`, so an Err fragment is read around (reconstructs from k survivors). Good. - reconstruction path does NOT: assess reads each placed server with `store.get_fragment(frag).await?` (reconstruction.rs:247) — the `?` PROPAGATES an Err; only Ok(None)/checksum-fail become a `missing` shard. - FsChunkStore::get_fragment (chunkstore-fs/src/lib.rs:240-241) maps only NotFound -> Ok(None); a dm-error device returns EIO -> Err. => leaving the victim in will likely make reconstruction propagate the EIO and abort (reconcile_step(...).expect(...) panics). The real fix is to make reconstruction treat a non-NotFound get_fragment error as a missing shard (read-around), mirroring the read path — then the scenario proves reconstruction-over-real-fault instead of side-stepping it. (Note: this edits production reconstruction behaviour, which the current brief lists out of scope — widen scope, or split the fix into its own issue and have the harness assert the corrected behaviour.) Keep (do not churn): it correctly drives the real `reconcile_step` (not a parallel reimpl); the read-during-repair assertion and the single version-conditional commit checks are sound. Confidence: read all three source paths directly; "dm-error returns EIO not NotFound" is inference but ~certain.
- Failing gate: C4 per-fix red->green: this patch's test red pre-fix, green post-fix (advisory) — run-verify.sh: FAIL — the test PASSES without the fix, so it does not catch the bug (no red).
- Full previous attempt preserved in `iteration-v1/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 2 — carry-forward (from the previous attempt)
- Sign-off rationale: issue_195 — Check sign-off, iteration 2. Disposition: iterate-plan (re-scope the brief). Decision (human): the production reconstruction behaviour change at crates/custodian/src/reconstruction.rs:258 (`?` -> `.ok().flatten()`) is split OUT of #195 into its own issue, getwyrd/wyrd#251. Re-scope #195 to the Tier-1 disk-fault HARNESS ALONE; drop the production reconstruction edit from this bundle's patch. Carry-forward for the next Plan: - #195 brief Scope already forbids "any change to production custodian / reconstruction behaviour" (brief.md:85-87). Honour that: #195 ships the harness only. The reconstruction read-around fix (and the over-broad-swallow correctness nuance — `.ok().flatten()` swallows EVERY get_fragment error, not just block-layer EIO; narrow it) belong to #251. - TENSION to resolve, do not paper over: iteration-2's Check-running red->green test (reconstruction_read_fault.rs) is flippable ONLY because of the production read-around at reconstruction.rs:258. Remove that edit and the test loses its in-scope Check-time production seam — which is exactly iteration-1's "adds nothing over Tier-0" objection (brief.md:165). Plan must decide how a harness-only #195 keeps a genuine born-at-tier, Check-running red->green without depending on the #251 production change: e.g. sequence #251 first and have #195's harness assert the corrected behaviour once it lands, or find an in-scope flippable seam in the harness orchestration itself. - Still-open for whoever owns the harness bundle (T5, carry to the re-scoped brief): the real dm-error device test and the only scrub::reconcile coverage are #[ignore]d / off-Check; their green rests on the privileged CI job with no maintainer-run evidence in the artifacts. Confirm that meets the "deferred != unbuilt" bar and that the privileged run is green.
- Full previous attempt preserved in `iteration-v2/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).
