# Result — issue 407 / m4-metadata-nemesis-partition-skew-pause

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect / goal: the metadata Tier-1 scenario can be driven under a composable **nemesis** with
- Success criterion: the nemesis exposes three leg kinds (partition / clock-skew /
- Repo + branch target: getwyrd/wyrd @ main
- Scope (one logical fix) / out of scope: the three-leg nemesis seam + its materialization oracles + the pure

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: new-feature
- Confidence: medium
- Recommendation: (set by Do)

## 3. Correctness (Check — chain)
- C1 Spec: none — brief.md
- C2 Reproduction (red pre-fix): none — (no gate configured)
- C3 Change: none — patch.diff
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): pass — xtask ci: all checks passed
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass — run-verify.sh: PASS — red without the fix, green with it.
- C5 Causal adequacy: none — reviewer + human sign-off

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 Contribution: none — (no gate configured)
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Review of issue #407: add a reusable three-leg metadata nemesis (partition, clock skew, and process pause) with materialization oracles and Tier-1 FDB orchestration.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | FAIL | The promised runnable xtask dispatch is absent: the module only returns leg values/argv and even documents a nonexistent `run_metadata_nemesis`, so maintainers cannot invoke the campaign through the specified seam (`xtask/src/nemesis.rs:52`). |
| C2 Reproduction (red pre-fix) | NEEDS-HUMAN | Accept the recorded red only if the gate host's stash run is trusted — this reviewer could not remove the staged patch because the linked-worktree Git index is read-only, although both added test binaries are genuinely new. |
| C3 Change | FAIL | The pause leg restores service inside materialization confirmation, before `drive_leg` invokes its workload, so the advertised workload-under-pause functionality is not implemented (`crates/metadata-fault-conformance/src/nemesis.rs:585`). |
| C4 Verification (red→green) | NEEDS-HUMAN | Decide whether the gate evidence is sufficient despite independent-run limits — the 3 xtask and 5 oracle tests passed green, but red could not be recreated due the read-only Git index and the asserted `./engine/xtask.sh`/`run-verify.sh` scripts are absent from the target. |
| C5 Causal adequacy | FAIL | A green oracle cannot establish fault exposure when confirmation itself unpauses at line 595 and the workload runs only later at line 276; the pause campaign therefore tests recovery-state service rather than service during the fault (`crates/metadata-fault-conformance/src/nemesis.rs:595`). |
| T1 Structure | PASS | The lifecycle/oracle API is placed in the importable conformance crate and exported through its library boundary, preserving the intended dependency direction (`crates/metadata-fault-conformance/src/lib.rs:65`). |
| T2 Shape | FAIL | The orchestration surface is helper-only and has no caller outside its tests, so the human-facing xtask campaign shape promised by the brief is unreachable (`xtask/src/nemesis.rs:58`). |
| T3 Runtime | FAIL | A pause run cannot exercise the callback while the process is paused because `confirm_materialized` executes `docker unpause` before returning to `drive_leg` (`crates/metadata-fault-conformance/src/nemesis.rs:595`). |
| T4 Contribution | NEEDS-HUMAN | Decide whether prior art is fully cleared — affected-path merged history shows #442/#257-era work, but closed/rejected work could not be mechanically queried, so uniqueness beyond local Git history remains unsettled. |
| T5 Judgment | FAIL | The contribution should not be treated as fit for sign-off until the pause lifecycle encloses the workload and the three-leg campaign has an actual xtask entry point (`crates/metadata-fdb/tests/tier1_metadata_nemesis.rs:105`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Witness all three opt-in legs on the required privileged ≥3-process FDB topology with Docker, libfaketime, FDB tooling, and the iptables sidecar — Check exercised only pure curated logic, so real fault materialization and recovery remain unvalidated (`crates/metadata-fdb/tests/tier1_metadata_nemesis.rs:36`). |

### Advisory — adversary

# check-advisory-adversary.md — issue #407 (m4-metadata-nemesis-partition-skew-pause)

Adversarial pass. Target: `$PDCA_TARGET` = `/home/eddie/development/wyrd/wyrd.pdca-wt` (patch applied). I re-ran both
named Check tests green (`cargo test -p wyrd-metadata-fault-conformance --test nemesis_oracles` → 5 passed;
`cargo test -p xtask --test nemesis_orchestration` → 3 passed); the red leg is a genuine compile-red (both test files
import the reverted `…::nemesis` modules — `crates/metadata-fault-conformance/tests/nemesis_oracles.rs:12-15`,
`xtask/tests/nemesis_orchestration.rs:18-20` — no parallel re-implementation). The evidence itself holds for the pinned
Check-core scope. The findings below are where the fix, its live legs, and one reviewer-adjacent claim break.

- NEEDS-HUMAN [impl] — **The live clock-skew leg cannot materialize with its own defaults — service/override/probe
  triple-mismatch.** `deploy/fdb-multi-replica/docker-compose.faketime.yml:30` hardcodes the override to service
  `fdb1`, but `crates/metadata-fdb/tests/tier1_metadata_nemesis.rs:117` defaults `WYRD_TIER1_SKEW_SERVICE` to
  `"fdb2"`, and :118-121 probes `all.last()`'s container (= fdb2, per the runner's netns-map order
  `xtask/src/fdb_faults.rs:39-43`) *independently of `service`*. Concrete failing case: default env →
  `docker compose -f base -f faketime up -d --force-recreate fdb2` recreates fdb2 with **no** `LD_PRELOAD`/`FAKETIME`
  (the override declares only `fdb1`) → the `date +%s` probe reads a true clock → offset ≈ 0 < floor 60 →
  `SkewEvidence::materialized()` false (`crates/metadata-fault-conformance/src/nemesis.rs:194`) → every live skew run
  fails inconclusive. Setting `WYRD_TIER1_SKEW_SERVICE=fdb1` instead skews fdb1 while still probing fdb2. The comment
  at :115 ("Skew a NON-master node") is also unenforced — nothing checks `all.last()` isn't the master.

- NEEDS-HUMAN [impl] — **The pause leg's live sampling is exactly the "single probe" its own contract forbids.**
  `ProcessPauseLeg::confirm_materialized` samples `served_during` once, immediately after `docker pause`
  (`crates/metadata-fault-conformance/src/nemesis.rs:588`), with no settle window — but a survivor's
  `fdbcli status json` keeps reporting the frozen target live until FDB's failure detector times out (seconds).
  Concrete failing case: pause bites, probe lands at t+~1-2s while the survivor still lists the target →
  `served_during=true` → `PauseEvidence::materialized()` false → the live leg is near-deterministically inconclusive.
  Contrast the same file's `PartitionLeg::confirm_materialized`, which polls the flip for 45s (nemesis.rs:470), and
  the peer `peers_still_see_target_live_after` window poll
  (`crates/metadata-fdb/tests/tier1_metadata_consistency.rs:279-288`). The `nemesis_oracles` test named
  "…not_a_single_probe" pins the *arithmetic*, not the *sampling*, so Check stays green over this.

- NEEDS-HUMAN [impl] — **`drive_leg` leaks fault state on every non-happy path, despite quoting "Invariant B forbids
  leaked fault state".** (a) `leg.apply()?` (`crates/metadata-fault-conformance/src/nemesis.rs:264`) returns without
  healing: `PartitionLeg::apply` (nemesis.rs:455-464) inserts 4 iptables DROP rules one at a time, so a failure at
  rule 3 leaks 2 rules into the cluster netns with no removal, no Drop guard — the peer `MasterIsolation` has exactly
  this guard (`Drop` retry of residue, `crates/metadata-fdb/tests/tier1_metadata_consistency.rs:309-316`) and it was
  not mirrored. (b) The skew leg applies its fault in `plan()` (nemesis.rs:706 `recreate(true)`), so a plan failure
  after the recreate (container never exec-able, :733) errors out of `drive_leg` **before** the heal path exists,
  leaving a permanently skewed node. (c) A panicking workload — and the shipped workload panics by design via
  `expect`/`assert_eq!` (`crates/metadata-fdb/tests/tier1_metadata_nemesis.rs:1170-1196` region, `cluster_still_serves`)
  — unwinds past `workload()` (nemesis.rs:276) with no `catch_unwind`, skipping `heal()` entirely: a failed
  partition-leg assertion leaves the cluster cut for every subsequent leg. Since `drive_leg` is the seam #408 consumes
  (no compose-down teardown wraps it there), this is not covered by the xtask runner's unconditional teardown either.

- NEEDS-HUMAN — **Nothing can run the nemesis legs: the dispatch was built but never wired, and the in-tree docs claim
  otherwise.** `FDB_TIER1_LEGS` (`xtask/src/fdb_faults.rs:52-56`) still lists only the three #442 legs; no xtask
  command consumes `xtask::nemesis::{metadata_nemesis_legs, nemesis_scenario_args}` — their only caller is the
  orchestration test itself. Yet the new test binary's `#[ignore]` strings and module doc say the legs "run only
  under … `cargo xtask fdb-metadata-tier1`" (`crates/metadata-fdb/tests/tier1_metadata_nemesis.rs:16,37,47,57,164`) —
  false on this tree. The brief's ordering note anticipated this patch touching `xtask/src/main.rs`; it doesn't, and
  the sign-off open question ("one witnessed local `WYRD_TIER1=1` run of the three legs") is unsatisfiable via xtask
  as landed — only a hand-built `cargo test --features fdb …` incantation with 6+ env vars can run them. Human call:
  is runner wiring in-scope for #407 (the brief's "runner-argument building" suggests it feeds *something*), or
  legitimately deferred to #408/#409? If deferred, the doc strings still need the [impl] fix.

- NEEDS-HUMAN [impl] — **The rename-safety claim is false: `--exact` on a missing function exits 0.** The doc at
  `crates/metadata-fdb/tests/tier1_metadata_nemesis.rs:21-23` claims "the runner selects each with `--exact`, so
  renaming one here without updating that dispatch would fail the leg". Verified on target:
  `cargo test -p wyrd-metadata-fdb --test tier1_metadata_nemesis -- --ignored --exact no_such_leg_fn` → "0 passed …
  ok", exit 0. So a renamed scenario fn (or a stale `scenario_fn` name, `xtask/src/nemesis.rs:44-49`) silently turns
  a leg into a green no-op; nothing pins the xtask names to the actual `#[test]` names. Fix: the (future) runner must
  assert exactly one test ran, or a Check test must pin the correspondence.

- NEEDS-HUMAN [impl] — **The brief's "lifecycle + oracle arithmetic" Check claim is only half-delivered: `drive_leg`'s
  two #442 gates are exercised by no test.** `nemesis_oracles.rs` covers the evidence arithmetic, enum and parse
  helpers only; no test drives `drive_leg` with a mock `NemesisLeg`. Deleting the inconclusive bail
  (`crates/metadata-fault-conformance/src/nemesis.rs:266-274`) or the `heal_is_complete` check (:279-285) flips
  nothing red at Check — the central "un-materialized fault FAILS, never passes silently" rule (success criterion,
  brief line 22-27; module doc nemesis.rs:15-23) is itself unguarded. A ~30-line mock-leg test closes this.

- **Minor (no adjudication needed):** `survivor_status_json` (`crates/metadata-fault-conformance/src/nemesis.rs:339-347`)
  drops the `--timeout 10` that the peer `support::status_json` passes to `fdbcli`
  (`crates/metadata-fdb/tests/support/mod.rs:55`); a wedged survivor probe can stall the 45s/60s poll loops well past
  their nominal windows.

**Attempted and could not refute:** the red→green evidence for the two named test files (re-ran green; red is a real
compile-red against the production modules, not a mirror copy); the C4 `xtask ci` pass; the oracle arithmetic itself
(tried boundary cases — `floor_secs=0` guarded at nemesis.rs:194, `unsigned_abs` handles `i64::MIN`, crash-vs-partition
and crash-vs-pause confusions are rejected by `target_running_during`/`inspected_paused_during`); the two mirrored
`NemesisLegKind` enums are per-brief (xtask zero-dep constraint), not a defect. The Check-core is sound; every finding
above lives in the live-leg half the gates never execute — which is precisely where a confirmatory review would
rationalize "deferred green" into "presumed green".

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C2 Reproduction (red pre-fix) — Accept the recorded red only if the gate host's stash run is trusted — this reviewer could not remove the staged patch because the linked-worktree Git index is read-only, although both added test binaries are genuinely new.
- [ ] C4 Verification (red→green) — Decide whether the gate evidence is sufficient despite independent-run limits — the 3 xtask and 5 oracle tests passed green, but red could not be recreated due the read-only Git index and the asserted `./engine/xtask.sh`/`run-verify.sh` scripts are absent from the target.
- [ ] T4 Contribution — Decide whether prior art is fully cleared — affected-path merged history shows #442/#257-era work, but closed/rejected work could not be mechanically queried, so uniqueness beyond local Git history remains unsettled.
- [ ] Validation — fitness-to-purpose — Witness all three opt-in legs on the required privileged ≥3-process FDB topology with Docker, libfaketime, FDB tooling, and the iptables sidecar — Check exercised only pure curated logic, so real fault materialization and recovery remain unvalidated (`crates/metadata-fdb/tests/tier1_metadata_nemesis.rs:36`).
- [ ] **The live clock-skew leg cannot materialize with its own defaults — service/override/probe
- [ ] **The pause leg's live sampling is exactly the "single probe" its own contract forbids.**
- [ ] **`drive_leg` leaks fault state on every non-happy path, despite quoting "Invariant B forbids
- [ ] **Nothing can run the nemesis legs: the dispatch was built but never wired, and the in-tree docs claim
- [ ] **The rename-safety claim is false: `--exact` on a missing function exits 0.** The doc at
- [ ] **The brief's "lifecycle + oracle arithmetic" Check claim is only half-delivered: `drive_leg`'s
- [ ] external dependency: fdb-toolchain (libfdb_c + fdb headers) — blocks type-checking the

## 7. Proven / not proven
- Proven by which oracle: gates overall = pass (stub oracles).
- Unproven / needs manual run: anything flagged in §6.

## 8. Ready-to-ship attachments
- patch.diff
- tracker-comment.md     (ALWAYS, every tracker item)
- build-notes.md         (builder rationale — for the human, not the reviewer)

## 9. Check sign-off                     ← human completes Check here
- Disposition confirmed / overridden:
- Outcome: iterated-to-Do
- Iteration delta (if iterating): Rejected on reviewer + adversary feedback: the Check-core (oracle arithmetic, enumeration, red→green) is sound, but the live-leg half is defective. Fix in the next Do attempt: 1. Pause lifecycle must ENCLOSE the workload: `confirm_materialized` currently runs `docker unpause` (nemesis.rs:595) before `drive_leg` invokes the workload (:276), so nothing runs under the pause. Also replace the single post-pause probe with a settle-window poll (mirror PartitionLeg's 45s poll at nemesis.rs:470) — one immediate `served_during` sample is near-deterministically inconclusive. 2. Skew leg default triple-mismatch: compose override hardcodes fdb1 (docker-compose.faketime.yml:30), test defaults WYRD_TIER1_SKEW_SERVICE=fdb2, and the probe reads all.last()'s container independently of `service` — default runs can never materialize. Make service/override/probe agree (probe the skewed service), and enforce or drop the "non-master node" comment. 3. Wire an actual runnable entry point: no xtask command consumes `xtask::nemesis::*`; docs reference a nonexistent `run_metadata_nemesis` and falsely claim the legs run under `cargo xtask fdb-metadata-tier1`. Either wire the dispatch (brief anticipated xtask/src/main.rs) or, if runner wiring is deferred to #408/#409, correct every doc string — but the brief's sign-off open question (witnessed WYRD_TIER1=1 run of all three legs) must be satisfiable. 4. `drive_leg` must not leak fault state on non-happy paths: heal on `apply()` failure (partial iptables rules — mirror MasterIsolation's Drop guard, tier1_metadata_consistency.rs:309-316); don't apply the skew fault in `plan()` (plan failure leaves a permanently skewed node); catch_unwind around the workload so a panicking workload (the shipped one panics by design) still heals. 5. Guard the central rule with tests: add a mock-NemesisLeg test driving `drive_leg` so deleting the inconclusive bail (nemesis.rs:266-274) or the `heal_is_complete` check (:279-285) goes red — "un-materialized fault FAILS, never passes silently" is currently unguarded. Also fix or pin the false `--exact` rename-safety claim (a missing test fn exits 0 → silent green no-op). Minor: `survivor_status_json` should pass `--timeout 10` to fdbcli like support::status_json does.
- By / date: Eduard Ralph / 2026-07-15

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
