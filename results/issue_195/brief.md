# Brief — issue 195 / tier1-disk-fault-harness

> The Plan artifact (docs 02 §PLAN). Human-authored. Do reads ONLY this file.
> The field labels below are parsed by the driver — keep the `- **Label:** value`
> shape. The success criterion is the load-bearing field: it is the sentence
> Check tests "did this work" against.

- **Slug:** tier1-disk-fault-harness
- **Defect:** M3.8 (#146) shipped only the Tier-0 DST campaign. The Tier-1 disk-fault leg
  of the M3 verification campaign (proposal 0005 §13.2, `0005:405-408`) — disk-fault
  injection via device-mapper `dm-flakey`/`dm-error` exercising the custodian
  repair/reconstruction path — is **not built**. On `main` (verified 4150ca5)
  `xtask/src/faults.rs:114` `run_disk_faults` is inert dispatch scaffolding: it gates on
  `WYRD_TIER1=1` and then `execute(...)` an externally-supplied `WYRD_TIER1_DISK_CMD`
  (`faults.rs:121`) that is **defined nowhere in the repo**; the only `#[cfg(test)]`
  coverage is the opt-in gating decision (`execute_*`, `faults.rs:185-197`). No in-repo
  harness sets up a faulted block device, drives the production scrub/checksum-verify +
  reconstruction path over it, and asserts the redundancy outcome. The Tier-1 coverage
  proposal 0005 promises is therefore absent.
- **Success criterion:** Real in-repo Tier-1 disk-fault **harness** code exists and is
  test-exercised, replacing the `WYRD_TIER1_DISK_CMD` external-command bypass. BINDING and
  demonstrable by C4-verify at Check: (a) `xtask` contains a Tier-1 disk-fault harness
  module whose host-independent orchestration logic — the device-mapper table plan, the
  fault-scenario steps, and the post-repair campaign verdict (`chunk → full redundancy`
  AND `read_errors == 0`) — is implemented **in-repo**, with `faults.rs::run_disk_faults`
  now invoking it instead of shelling out to the unset env-var command; (b) that
  orchestration logic is covered by `#[cfg(test)]` unit tests that run inside
  `cargo xtask ci` and go **red when the verdict/plan helper is stubbed** (born-at-tier
  flippable coverage — Do must demonstrate the red per Verification posture); (c) a
  `#[ignore]`d real-device scenario test exists that calls the **production**
  `FsChunkStore`/`reconcile_step`/`scrub::reconcile`/`reconstruction::reconcile` APIs, so
  `cargo xtask ci`'s `cargo test --workspace` compiles and type-checks it (the Check-time
  guard against reverting to inert dispatch); (d) `./engine/xtask.sh ci` still exits 0 and
  the privileged Tier-1 path remains **excluded** from the unprivileged container-free `ci`
  gate (ADR-0016). The real privileged execution (root + `dmsetup`, real `dm-error` faults,
  chunks driven back to full redundancy with no read errors during repair) is confirmed
  green **off-Check** by the new privileged Tier-1 CI job — supplementary evidence, NOT the
  Check-gating condition, and green only once #251 has merged (see Depends-on-merged /
  Verification posture). The component names ("dm table plan", "campaign verdict") are
  ILLUSTRATIVE of the harness's parts; BINDING is that real in-repo harness code exists and
  is test-exercised at Check, not external-command scaffolding.
- **Invariant to restore:** The M3 verification campaign's Tier-1 disk-fault leg is honoured
  by **real, in-repo, test-exercised harness code**, not by an opt-in shell-out to an absent
  external command. Stated over the category (a deferred/off-Check verification tier): a
  tier that is "deferred" means its *green is observed off-Check*, NOT that the deliverable
  is unbuilt — the harness itself must exist and be exercised by something at Check (the
  xtask unit tests over its orchestration logic). Source: proposal 0005 §13.2 / §"DST and
  tests" (`0005:405-408`, the Tier-1 disk-fault mandate) and the crate touch-point
  `0005:437` ("xtask — Tier-1 disk-fault (dm-flakey/dm-error) + Jepsen runners"); ADR-0009
  (DST/real-world discovery is the correctness authority — the harness must drive the REAL
  production repair path, never a parallel reimplementation); ADR-0016 (the unprivileged
  `cargo xtask ci` gate stays container-free, so the privileged tier is excluded); and
  `templates/brief.md.tpl`'s Verification-posture forcing function ("deferred ≠ unbuilt — the
  #146 Tier-1/2 gap"). This is new-feature/infrastructure work, not a behavioural bug fix,
  so minimalism does not govern it (principles.md §1.3): the target is a Tier-1 harness that
  actually runs the production scrub/reconstruction path against real block-layer faults,
  not the smallest diff to `faults.rs`.
- **Repo + branch target:** getwyrd/wyrd @ main   (no maintenance branches today; INTEGRATION §2)
- **Onto branch:**
- **Depends on:**
- **Depends on (merged):** 251
- **Conflicts with:** 196
- **Stacks on:**
- **Ordering note:** Depends-on-merged 251 because the harness's headline outcome —
  "faulted chunk driven back to full redundancy with no read errors during repair" (the
  issue's definition-of-done bullet 3) — CANNOT pass, even off-Check on the privileged job,
  while `crates/custodian/src/reconstruction.rs:246` still aborts on a block-layer `EIO`
  (`store.get_fragment(frag).await?` propagates the error; a `dm-error` device returns `EIO`,
  not `NotFound`, so `reconcile_step` returns `Err` and the campaign panics). #251 ships the
  (narrowed) reconstruction read-around as its own production change with its own flippable
  regression test; #195 builds on the MERGED result, driving the corrected production path —
  this also keeps the production-behaviour edit OUT of #195's patch entirely (the iteration-2
  human decision; avoids the C5 scope/correctness contradiction that rejected iteration 2).
  Conflicts-with 196 (Tier-2) because both edit `xtask/src/faults.rs` (the
  `run_disk_faults`/`run_jepsen` vs `run_kill_reconstruct` runners), the `xtask/src/main.rs`
  subcommand dispatch, and add a privileged off-Check CI workflow — no build-on dependency,
  but they collide on those shared files, so never co-schedule them in one concurrent wave.
  Tier order is Tier-0→1→2 (proposal 0005 §13.4); 196 carries the reciprocal
  `Depends on (merged): 195`.
- **Surfaces:** data   (xtask/CI tooling + custodian repair path; no frontend)
- **Scope:** Build the Tier-1 disk-fault harness as **real in-repo Rust**, mirroring the
  existing Tier-2 *container* precedent (xtask `run_integration` + the `#[ignore]`d
  `crates/chunkstore-grpc/tests/tier2_integration.rs`): (1) a `#[ignore]`d integration-test
  scenario that opens a real `FsChunkStore` (`crates/chunkstore-fs`, `FsChunkStore::open`) on
  a device-mapper `dm-flakey`/`dm-error`-faulted backing device and drives the **production**
  custodian path — `custodian::reconcile_step` / `scrub::reconcile` /
  `reconstruction::reconcile`
  (`crates/custodian/src/{reconciliation,scrub,reconstruction}.rs`), the same fenced control
  point the Tier-0 DST campaign drives — asserting the campaign outcome (faulted chunks
  driven back to full redundancy with **no read errors during repair**); (2) xtask's
  `run_disk_faults` orchestrating the dm-device setup and invoking that scenario,
  **replacing** the `WYRD_TIER1_DISK_CMD` external-command shell-out; (3) host-independent
  orchestration logic — the dm-table plan AND the campaign verdict (covering BOTH the scrub
  and the reconstruction legs of the verdict, not reconstruction alone) — unit-tested inside
  `cargo xtask ci`; (4) a **privileged** off-Check CI job (root + device-mapper / `dmsetup`)
  that runs the Tier-1 suite green, kept out of the unprivileged container-free
  `cargo xtask ci` (ADR-0016), modelled on `integration-nightly.yml`. The harness MUST drive
  the REAL production scrub/reconstruction path, never a parallel reimplementation (ADR-0009).
  / out of scope: **any change to production custodian / reconstruction / scrub behaviour** —
  the reconstruction read-around fix (`reconstruction.rs:246` `?` → narrowed read-around) and
  its over-broad-swallow nuance are **#251's** deliverable; this bundle ships the harness only
  and builds on #251 already merged, so #195's `patch.diff` MUST NOT edit
  `crates/custodian/src/{reconstruction,scrub,reconciliation}.rs` production code. Also out of
  scope: the Tier-1 **Jepsen** consistency harness (split to **#250**); Tier-2 single-node
  kill-and-reconstruct (#196); Tier-3 multi-region hardware (M5+, proposal 0005 §non-goals);
  editing the accepted proposal 0005 or any ADR (immutable, INTEGRATION §2 — author a
  superseding ADR if a decision must change, never edit in place).
- **Repro instruction:** On `main` (4150ca5), read `../wyrd/xtask/src/faults.rs`:
  `run_disk_faults` (line 114) contains no harness — it `execute(...)`s an env-supplied
  `WYRD_TIER1_DISK_CMD` (line 121), and `grep -rn "WYRD_TIER1_DISK_CMD" ../wyrd` shows the
  command is defined nowhere in-repo. The `#[cfg(test)]` module covers only the `execute`
  gating decision (`faults.rs:185-197`), never a fault scenario. Compare the Tier-2
  *container* precedent `run_integration` in `xtask/src/main.rs`, which DOES carry a real
  in-repo harness (`compose_up`/`run_integration_test`/`finish_integration`) plus unit tests
  over its host-independent logic — Tier-1 must reach that bar.
- **Test file:** TWO-part, mirroring the verified Tier-2 container precedent. (a)
  ORCHESTRATION (the Check-running flippable seam) — xtask `#[cfg(test)]` unit tests over
  `run_disk_faults`'s host-independent logic (dm-table plan + campaign verdict for BOTH the
  scrub and reconstruction legs), in `faults.rs` or a new `disk_faults.rs` sibling, running
  inside `cargo xtask ci`. This is the born-at-tier flippable coverage: red when the
  verdict/plan helper is stubbed, green when implemented. (b) SCENARIO — a new `#[ignore]`d
  integration test carrying the real-device disk-fault scenario (e.g.
  `crates/custodian/tests/tier1_disk_faults.rs` or under `crates/chunkstore-fs/tests/` — Do's
  call), attributed exactly like `tier2_integration.rs`
  (`#[ignore = "Tier-1: needs root + device-mapper — run via cargo xtask disk-faults"]`).
  `cargo xtask ci`'s `cargo test --workspace` (`xtask/src/main.rs:~413`) **compiles and
  type-checks** this file at Check (its body runs only in the privileged job), so the harness
  is real, API-bound Rust — not an env-var shell string.
- **Verification posture:** DEFERRED/off-Check, NET-NEW (forcing function — the #146 Tier-1
  gap; the case `templates/brief.md.tpl` calls out explicitly). What is BUILT AND exercised at
  Check: (i) the xtask orchestration logic (dm-table plan + campaign verdict, both scrub and
  reconstruction legs) is unit-tested inside `cargo xtask ci` — this is the flippable
  born-at-tier coverage. "Red" is criterion-ABSENCE plus a *demonstrated* red: Do MUST capture
  a demonstrated red (temporarily stub the verdict/plan helper and show the unit test fails)
  proving the seam is load-bearing, not resting red on non-existence. (ii) the `#[ignore]`d
  scenario is real Rust **compiled and type-checked by `ci`'s `cargo test --workspace`** — it
  calls the production `FsChunkStore`/`reconcile_step`/`scrub::reconcile`/`reconstruction::reconcile`
  APIs, so a regression reducing it to a stub (or the old shell-string dispatch) fails to
  compile; compilation alone proves it is not inert dispatch scaffolding. DEFERRED to
  off-Check: the real privileged run against `dm-flakey`/`dm-error` needs root + `dmsetup`, so
  the `#[ignore]`d scenario body cannot go green in the unprivileged container-free Check
  worktree (ADR-0016); its green is confirmed by the new **privileged Tier-1 CI job** (root +
  device-mapper, opted in via `WYRD_TIER1=1`), whose maintainer-confirmed green run is owned by
  Eduard Ralph (INTEGRATION §10). That privileged run is green ONLY once #251 has merged (the
  read-around landing — see Depends-on-merged); if #251 is not yet merged the privileged job
  stays red and #195 must wait, which is exactly why this bundle is `Depends on (merged): 251`.
  FORCING FUNCTION satisfied: the deferred deliverable (the harness) is itself BUILT — compiled
  at Check and exercised by the xtask unit tests — NOT inert dispatch scaffolding; only the
  real-device *run* is off-Check. NOTE: the in-process EIO stand-in unit test that proves the
  reconstruction read-around is functionally correct belongs to **#251** (its acceptance names
  it), NOT this bundle — #195 asserts the harness orchestration + drives the corrected
  production path it builds on; it does not re-test #251's production behaviour change.
- **Production reach:** N/A as a production seam — this slice builds verification tooling and
  edits no production code (the reconstruction read-around is #251's, already merged when #195
  runs). The harness MUST traverse the REAL production custodian path (`reconcile_step` →
  `scrub::reconcile` / `reconstruction::reconcile` over a real `FsChunkStore`), the same fenced
  control point the Tier-0 campaign drives; it must NOT be a parallel reimplementation of
  repair, or it verifies nothing (ADR-0009). The harness's *value over Tier-0* is the real
  block-device privileged scenario (off-Check) the simulated in-memory campaign cannot reach —
  not an in-scope production edit.
- **Citations expected:** Do must cite path:line on `main` (getwyrd/wyrd, atop merged #251)
  for every change — `xtask/src/faults.rs` (`run_disk_faults`, line 114/121),
  `xtask/src/main.rs` dispatch (`disk-faults` arm), the new orchestration module + its
  `#[cfg(test)]` tests, the new `#[ignore]`d scenario test, the new privileged CI workflow
  under `.github/workflows/`, and any `crates/custodian` / `crates/chunkstore-fs` /
  `crates/testkit` *test/helper* touched (NOT production custodian source). Cite the production
  entry points the harness exercises (`crates/custodian/src/reconciliation.rs::reconcile_step`
  and `scrub`/`reconstruction`) — exercised, not edited.
- **Prior-art check (triage cycles):** Searched by affected file path across merged history and
  open/closed PRs. `xtask/src/faults.rs` was introduced by #146 (PR #194, commit 2516b68,
  "test(dst): add the Tier-0 custodian property campaign"), which explicitly added the
  **deferred** Tier-1/Tier-2 runners "which skip cleanly unless explicitly opted in" — i.e. the
  scaffolding this brief replaces; no later PR has implemented the harness body. A repo-wide
  search for fault-harness tests (`dm-flakey`/`dm-error`/`disk-fault`/`tier1`) finds only
  `crates/dst/tests/custodian.rs` (the Tier-0 *simulated* campaign) and
  `crates/chunkstore-grpc/tests/tier2_integration.rs` (the M2 container e2e) — no real Tier-1
  disk-fault harness exists. The Tier-2 *container* tier (`run_integration` + the `#[ignore]`d
  `tier2_integration.rs`, M2/proposal 0004, `integration-nightly.yml`) is the in-repo precedent
  to mirror. This bundle's own prior attempts are preserved in `iteration-v1/` and
  `iteration-v2/` (the iteration-2 patch folded a production reconstruction edit into the
  harness and was rejected at sign-off — that edit is now #251). Not a duplicate.
- **Disposition hint:** likely-fix

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a
draft PR MAY happen during the cycle (useful for CI feedback). The PR MUST NOT be
marked ready before sign-off accepts.

## Iteration 1 — carry-forward (from the previous attempt)
- Sign-off rationale (C5 causal adequacy): the scenario did not drive the REAL reconstruction
  path over the real block-layer fault — scrub + reconstruction ran over a `healthy_view` that
  stripped the victim from the fleet BEFORE the reconstruction pass, so `inject_disk_fault()`
  was causally inert for repair (delete it and the reconstruction half passed identically). The
  fault was load-bearing only for the read assertions, so "faulted chunk driven back to full
  redundancy" was demonstrated as a normal survivor-only rebuild over an absent server — adding
  nothing over the Tier-0 in-memory campaign.
- Failing gate: C4 per-fix red→green — the test PASSED without the fix (no red), so it did not
  catch the bug.
- Full attempt preserved in `iteration-v1/`.

## Iteration 2 — carry-forward (from the previous attempt)
- Sign-off rationale: Disposition iterate-plan (re-scope). Human decision: the production
  reconstruction behaviour change at `crates/custodian/src/reconstruction.rs:258`
  (`?` → `.ok().flatten()`) is split OUT of #195 into its own issue, **getwyrd/wyrd#251**.
  Re-scope #195 to the Tier-1 disk-fault HARNESS ALONE; drop the production reconstruction edit.
  The over-broad-swallow nuance (`.ok().flatten()` swallows EVERY `get_fragment` error, not just
  block-layer EIO — narrow it) belongs to #251.
- TENSION the human handed to Plan: iteration-2's Check-running red→green
  (`reconstruction_read_fault.rs`) was flippable ONLY because of the production read-around at
  `reconstruction.rs:258`; remove it and #195 loses its in-scope Check seam (iteration-1's "adds
  nothing over Tier-0" objection). **Resolved in THIS brief:** `Depends on (merged): 251` — #195
  builds on the merged read-around; #195's own Check-time flippable seam is the harness
  ORCHESTRATION logic (dm-table plan + campaign verdict, unit-tested in xtask), independent of
  production reconstruction; the real-fault green is deferred off-Check to the privileged job,
  which is green only once #251 has merged. The in-process EIO stand-in proving the read-around
  is #251's test, not #195's.
- Still-open (T5), now addressed by this brief: the scrub leg must be covered by the
  Check-running orchestration verdict tests (not only the `#[ignore]`d body); the real
  `dm-error` device green is deferred off-Check with maintainer-confirmed green required, which
  meets the "deferred ≠ unbuilt" bar because the harness itself is built and exercised at Check.
- Full attempt preserved in `iteration-v2/`.
- Address the above; do NOT re-attempt the rejected approach unchanged (no production custodian
  edit in #195's patch). Satisfy the brief's Success criterion (the end result).

## Iteration 3 — carry-forward (from the previous attempt)
- Sign-off rationale: Rejected: the named test does not go red without the fix (C4-verify FAIL), and no primary advisory review exists (Claude reviewer leaf failed to run). The codex cross-vendor review independently confirms the harness validates a shadow implementation. Next attempt must produce a real advisory review AND address all four codex concerns: 1. xtask/src/disk_faults.rs:120 — device-table and verdict logic live entirely in #[cfg(test)]; the real scenario (tier1_disk_faults.rs:215) rebuilds its own tables and never calls the verdict helpers, so unit tests validate a shadow implementation and stay green when the runtime harness is removed/broken (matches the C4-verify no-red result). Move the shared plan/verdict types into normal code and have the scenario consume them — and make the test red pre-fix. 2. tier1_disk_faults.rs:300 — scrub leg corrupts the fragment with a direct std::fs::write; the only runtime dm transitions are linear setup + dm-error at line 375. The advertised dm-flakey phase exists only in the test-only helper, so the privileged campaign never exercises the required flakey block-layer fault. 3. tier1_disk_faults.rs:369 — cache eviction is best-effort and failure is accepted; get_fragment can be served from page cache after switching to dm-error, so the scenario passes without observing any block-layer EIO (the #251 read-around it must prove). Make eviction/proof of EIO mandatory before accepting the reconstruction verdict. 4. tier1_disk_faults.rs:33 — references a dedicated tier1-disk-faults.yml privileged workflow that does not exist under .github/workflows and the patch adds none; the ignored real-device test has no in-repo CI execution path, so the off-Check green cannot be produced automatically.
- Failing gate: C4 per-fix red->green: this patch's test red pre-fix, green post-fix (advisory) — run-verify.sh: FAIL — the test PASSES without the fix, so it does not catch the bug (no red).
- Full previous attempt preserved in `iteration-v3/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).
