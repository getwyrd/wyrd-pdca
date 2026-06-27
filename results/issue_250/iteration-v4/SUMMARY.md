# Result — issue 250 / tier1-jepsen-consistency-harness

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect / goal: The Tier-1 **Jepsen** consistency leg of proposal 0005 §13.2 (`0005:408`) was
- Success criterion: **DECISION (the human/maintainer chose Option A): build the genuine
- Repo + branch target: getwyrd/wyrd @ main   (per INTEGRATION §2 — Wyrd targets `main`;
- Scope (one logical fix) / out of scope: Build the Tier-1 Jepsen consistency leg as the genuine Jepsen framework

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: likely-fix
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

# Check review — issue 250 / tier1-jepsen-consistency-harness (iteration 4)

Advisory, artifact-only, decorrelated. Inputs: patch.diff, brief.md, check-gates.json
(build-notes.md withheld). Citations grounded on the post-patch target at
`$PDCA_TARGET=/home/eddie/wyrd/wyrd.pdca-wt` (readable, patch applied) and on patch.diff.

## Verdict table

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | All three BINDING parts present: dispatch rewire (`xtask/src/faults.rs:178-226`), in-repo Jepsen+Elle harness (`jepsen/`), privileged `.github/workflows/tier1-jepsen.yml`. Spec matches the #146 "deferred≠unbuilt" defect. |
| C2 Reproduction (red pre-fix) | PASS | No CI gate (`check-gates.json` row C2 "none"); C4-verify confirmed red→green. Red is born-at-tier *compile-absence* (`xtask/tests/jepsen_orchestration.rs` won't resolve `xtask::jepsen` without the patch), not a behavioral flip — accepted per brief Verification-posture (ii). |
| C3 Change | PASS | `run_jepsen` now matches `plan()` and dispatches `Plan::Run → run_jepsen_harness()` (`lein run test` in `jepsen/`), replacing `execute(...,"WYRD_TIER1_JEPSEN_CMD")`; grounded at faults.rs:178-226, matches diff exactly. |
| C4 Verification (red→green) | PASS | Gating `C4-ci` PASS (`xtask ci` green) and `C4-verify` PASS (red without fix, green with). Caveat, not a defect: the gate exercises ONLY the Rust dispatch rewire; the harness substance is off-Check by design (ADR-0016, Clojure). Target is current — no stale-target blocker. |
| C5 Causal adequacy | NEEDS-HUMAN | Decision owed: does the harness drive the *production* repair path (brief success-criterion 2)? Reconstruction (`reconcile_step`) is production, but its trigger is a **test-only** `detect_and_enqueue_missing` (jepsen_custodian_step.rs) — because the production read path does NOT enqueue repair for *missing* fragments, only present-but-corrupt (`crates/core/src/read.rs:189,209-210`). Maintainer must judge whether synthesizing the repair obligation out-of-band still counts as exercising the production trigger. Symptom-guard smell-test: the `tool_available("lein")`/`WYRD_TIER1` gate is by-design dispatch gating (sibling pattern), not a capability probe papering a load-time side effect — does not fire. |
| T1 Structure | PASS | Files placed by convention: `jepsen/` project, `xtask/src/jepsen.rs` + `pub mod jepsen`, born-at-tier test `xtask/tests/jepsen_orchestration.rs`, workflow under `.github/workflows/`. |
| T2 Shape | PASS | Mirrors the merged sibling shape (`run_disk_faults`→in-repo scenario; #195/#196); `tier1-jepsen.yml` modelled on the disk-faults/kill-reconstruct workflows, own non-colliding 02:00 cron. |
| T3 Runtime | NEEDS-HUMAN | Decision owed: the Jepsen/Elle suite cannot run at Check (no cluster; deferred to `tier1-jepsen.yml`). Whether it runs green AND non-vacuously is observable only in a live `workflow_dispatch` run — the brief requires a **demonstrated red** (planted anomaly caught over a non-vacuous history). Maintainer must confirm that artifact before trusting the leg. |
| T4 Contribution | NEEDS-HUMAN | Decision owed (pre-declared, INTEGRATION §4 / brief ordering-note): net-new non-Cargo toolchain (JVM+Clojure+Leiningen+Jepsen+Elle, `jepsen/project.clj`) is outside `deny.toml`/cargo-deny. Maintainer weighs whether a short ADR recording the non-Rust test-toolchain decision is warranted (proposal 0005 accepts "Jepsen" in principle; this is the *how*). |
| T5 Judgment | NEEDS-HUMAN | Decision owed — the recurring iteration-1/2/3 "vacuous on first run" class: the `:r` op reconstructs its observed list from the client-side `slot-writes` atom sorted by `:seq` (jepsen.clj `slot-history`/`invoke!`), querying Wyrd only to confirm each *immutable* per-seq key reads back its own value. The list ORDER is imposed by the client, never observed from Wyrd's stored state, so Elle's list-append checker may have no Wyrd-induced interleaving to find anomalies in — `:concurrency 5` adds processes but not a contended register. Maintainer must confirm the workload yields a genuinely non-vacuous Elle history. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decision owed at sign-off: does this harness, on its first live `tier1-jepsen.yml` run, actually assert consistency over the production repair/reconstruction path under partitions+crashes (ADR-0015; commit-point-atomic repair, `0005:277`,`0005:385-389`)? All three prior rejections landed here. Confirm via an actual dispatch run with a demonstrated planted-anomaly catch; validate against clean upstream `main` (fork-discipline §4), not drifted branches. |

## Notes (advisory, non-gating)

- Iteration-3 carry-forward items are *structurally* addressed: `:concurrency 5` (jepsen.clj `wyrd-test`), per-slot `seq` used as appended value (`alloc-seq!`, not `rand-int`). Whether `:concurrency 5` over distinct immutable keys produces real interleaving for Elle is the open T5 concern above — structurally present, substantively unconfirmed (off-Check).
- Iteration-2 items addressed: partition nemesis added (`partition-nemesis` via `docker network disconnect/connect`, composed with crash nemesis through `nemesis/compose`); custodian step now asserts `Reconciled::Changed` (not bare `Ok`); failed/partial `wyrd get` now throws → recorded `:fail`, not `:ok`.
- Iteration-1 items addressed: no `wyrd ls` (CLI confirms only put/get/d-server/demo at `crates/server/src/cli.rs:57-60`; all harness flags `--key/--data-dir/--durability/--endpoints` exist `cli.rs:82-83`); ephemeral ports resolved via `docker compose port --index` (workflow); checker self-test uses `(true?/false? (:valid? ...))` boolean (`checker_test.clj`).
- Prior-art check: brief documents a by-file-path search (faults.rs history `0b5fea3`/`02983aa`; no `jepsen/` or `tier1-jepsen.yml` on origin/main pre-patch). Consistent with the post-patch target; not a duplicate — siblings are the pattern precedent. No mechanical contradiction found.
- Minor (off-Check, non-blocking): `wyrd-checker` uses `(:store _test "/tmp/jepsen-store")` as the Elle output dir — jepsen `:store` is typically a map, not a path string; may mis-locate the Elle report directory but does not affect the validity verdict. Worth a glance at the live run.

### Advisory — codex

- jepsen/src/wyrd/jepsen.clj:317 — A wrong-value read is thrown as `"data integrity violation"` and then caught by the broad `catch` at `jepsen/src/wyrd/jepsen.clj:341`, which returns the operation as `:fail`. That masks the strongest consistency failure as an availability failure that Elle may ignore; corrupt or stale bytes should make the run fail, or be recorded as a successful bad observation for the checker.
- jepsen/src/wyrd/jepsen.clj:188 — The read model sorts observed values by the preallocated `:seq`, while appends allocate that seq before `wyrd put` and only record success after the external put returns at `jepsen/src/wyrd/jepsen.clj:288`. With `:concurrency 5`, two same-slot appends can commit in a different order than seq allocation, so the harness can synthesize list orders Elle flags even when Wyrd is linearizable. The list order needs to come from the actual completed append order/linearization point, or the workload should use a model that does not invent ordering from client-side allocation.
- jepsen/src/wyrd/jepsen.clj:453 — `docker network disconnect` failures are only logged and the nemesis still returns an `:info` partitioned operation at `jepsen/src/wyrd/jepsen.clj:490`. If the Docker network name is wrong or the command fails, the required partition fault is silently absent and the run can pass without exercising partitions; nonzero Docker exits should fail the nemesis operation/run.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C5 Causal adequacy — Decision owed: does the harness drive the *production* repair path (brief success-criterion 2)? Reconstruction (`reconcile_step`) is production, but its trigger is a **test-only** `detect_and_enqueue_missing` (jepsen_custodian_step.rs) — because the production read path does NOT enqueue repair for *missing* fragments, only present-but-corrupt (`crates/core/src/read.rs:189,209-210`). Maintainer must judge whether synthesizing the repair obligation out-of-band still counts as exercising the production trigger. Symptom-guard smell-test: the `tool_available("lein")`/`WYRD_TIER1` gate is by-design dispatch gating (sibling pattern), not a capability probe papering a load-time side effect — does not fire.
- [ ] T3 Runtime — Decision owed: the Jepsen/Elle suite cannot run at Check (no cluster; deferred to `tier1-jepsen.yml`). Whether it runs green AND non-vacuously is observable only in a live `workflow_dispatch` run — the brief requires a **demonstrated red** (planted anomaly caught over a non-vacuous history). Maintainer must confirm that artifact before trusting the leg.
- [ ] T4 Contribution — Decision owed (pre-declared, INTEGRATION §4 / brief ordering-note): net-new non-Cargo toolchain (JVM+Clojure+Leiningen+Jepsen+Elle, `jepsen/project.clj`) is outside `deny.toml`/cargo-deny. Maintainer weighs whether a short ADR recording the non-Rust test-toolchain decision is warranted (proposal 0005 accepts "Jepsen" in principle; this is the *how*).
- [ ] T5 Judgment — Decision owed — the recurring iteration-1/2/3 "vacuous on first run" class: the `:r` op reconstructs its observed list from the client-side `slot-writes` atom sorted by `:seq` (jepsen.clj `slot-history`/`invoke!`), querying Wyrd only to confirm each *immutable* per-seq key reads back its own value. The list ORDER is imposed by the client, never observed from Wyrd's stored state, so Elle's list-append checker may have no Wyrd-induced interleaving to find anomalies in — `:concurrency 5` adds processes but not a contended register. Maintainer must confirm the workload yields a genuinely non-vacuous Elle history.
- [ ] Validation — fitness-to-purpose — Decision owed at sign-off: does this harness, on its first live `tier1-jepsen.yml` run, actually assert consistency over the production repair/reconstruction path under partitions+crashes (ADR-0015; commit-point-atomic repair, `0005:277`,`0005:385-389`)? All three prior rejections landed here. Confirm via an actual dispatch run with a demonstrated planted-anomaly catch; validate against clean upstream `main` (fork-discipline §4), not drifted branches.

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
- Iteration delta (if iterating): Rejected: the dispatch rewire is sound, but the Jepsen/Elle harness substance — the actual deliverable — has structural defects flagged by both reviewers and is unverified. Keep the brief; fix the harness at Do level. Resolve the code advisories: 1. jepsen.clj:317 / :341 — a wrong-value read throws "data integrity violation" and the broad catch records it as :fail, masking the strongest consistency failure (corrupt/stale bytes) as a mere availability failure Elle ignores. Corrupt/stale reads must fail the run (or be recorded as a successful bad observation the checker sees), never be swallowed as :fail. 2. jepsen.clj:188 / :884 (T5, the recurring iter-1/2/3 "vacuous history" class) — the :r op rebuilds list order from the client-side slot-writes atom sorted by :seq, never from Wyrd's stored state. The order is invented client-side, so Elle has no Wyrd-induced interleaving to check (vacuous) AND can synthesize orders it flags even when Wyrd is linearizable (false positive). List order must come from the actual completed/linearized append order observed from Wyrd, not client-side seq allocation. 3. jepsen.clj:453 / :490 — docker network disconnect failures are only logged; the nemesis still returns :info partitioned. A wrong network name then means the required partition fault is silently absent and the run passes without partitioning. Nonzero docker exits must fail the nemesis op / run. Also still open (not blocking the Do fix, but carry forward): C5 — reconstruction is driven by the test-only detect_and_enqueue_missing because the production read path does not enqueue repair for missing (only present-but-corrupt) fragments; confirm this still counts as exercising the production trigger. T3/Validation — the leg has never been run; needs a live tier1-jepsen.yml dispatch with a demonstrated planted-anomaly catch over a non-vacuous history, validated against clean upstream main. T4 — non-Cargo toolchain (JVM/Clojure/Lein/Jepsen/Elle) may warrant a short ADR.
- By / date: Eduard Ralph / 2026-06-27

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
