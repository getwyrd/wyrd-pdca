# Result — issue 408 / m4-checked-consistency-run-elle-report

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect / goal: one opt-in command runs the checked register + directory workload against the
- Success criterion: the run pipeline exists end-to-end and refuses to overstate
- Repo + branch target: getwyrd/wyrd @ main
- Scope (one logical fix) / out of scope: (1) the Elle-EDN **format-contract fix at its source**: amend

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

Review of issue #408: add an opt-in, non-vacuous FDB consistency run whose real Elle verdicts produce a public credibility report.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | FAIL | The acceptance boundary requires both the disjoint Wyrd-checked DELETE pool and a committed first witnessed-run report, but the live workload constructs only overwrite and directory-create pools (`crates/server/tests/consistency_run_fdb.rs:351`) and no #408 report exists under `docs/design/reviews/`; accepting would omit promised workload coverage and the public artifact. |
| C2 Reproduction (red pre-fix) | NEEDS-HUMAN | Decide whether the recorded revert-based red leg is acceptable without an independent reviewer rerun — the review target is read-only, so I could run green but could not stash/revert the fix to reproduce red; this matters because the asserted causal flip remains provisional. |
| C3 Change | FAIL | The implementation must include the separately checked DELETE traffic and committed witnessed result, yet `drive_pools` returns only the Elle register history, directory creates, and universe (`crates/server/tests/consistency_run_fdb.rs:326`), while runtime reports are written only to `target/consistency-run/report.md` (`xtask/src/consistency_run_runner.rs:385`); the delivered change is incomplete against scope. |
| C4 Verification (red→green) | NEEDS-HUMAN | Decide whether to sign off after running the privileged leg and reviewer-blocked socket test — the 25 orchestration tests passed, but the server suite stopped at 12/13 because loopback bind was denied at `crates/server/tests/consistency_workload.rs:654`, and no independent red leg or live Docker/FDB/Elle green was reproduced. |
| C5 Causal adequacy | FAIL | The credibility goal depends on exercising all declared workload pools and publishing the witnessed verdict; omitting DELETE traffic at `crates/server/tests/consistency_run_fdb.rs:351` and retaining the report only as transient output at `xtask/src/consistency_run_runner.rs:385` leaves the stated cause-to-artifact chain incomplete. |
| T1 Structure | PASS | The host-independent decision core is default-compiled and the privileged I/O remains isolated behind the opt-in runner (`xtask/src/consistency_run.rs:38`), preserving the intended dependency boundary. |
| T2 Shape | FAIL | The human must require the three-pool topology before acceptance — the actual live shape has one PUT writer, one GET reader, and directory creators only (`crates/server/tests/consistency_run_fdb.rs:351`), so it cannot produce the brief's Wyrd-side DELETE evidence. |
| T3 Runtime | NEEDS-HUMAN | Run `WYRD_TIER1=1 WYRD_ELLE_CLI_JAR=<elle-cli-0.1.9.jar> cargo xtask consistency-run` on the privileged 3-node FDB topology and confirm both models return `Pass`, materialization/concurrency are non-vacuous, and teardown completes; Docker, FDB linkage/topology, nemesis privileges, and real live histories were not exercised here, so runtime fitness is undischarged (`xtask/src/consistency_run_runner.rs:408`). |
| T4 Contribution | NEEDS-HUMAN | Decide whether closed/rejected work contains a superseding #408 implementation — affected-path merged history locally shows only #479 (`3859d41`) for the established serializer path, but closed/rejected PR state could not be mechanically established; duplicate contribution risk therefore remains. |
| T5 Judgment | NEEDS-HUMAN | Decide whether the partial artifact is acceptable despite missing promised DELETE coverage and witnessed report — those omissions materially weaken the externally recognizable consistency claim even though the pure orchestration/parser design is coherent. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether a real, non-vacuous two-model Elle pass under a materialized fault is credible for the project claim — inspect and commit the generated five-field report after the privileged command above, because curated fixtures and code-read tests cannot establish live fitness. |

### Advisory — adversary

# check-advisory-adversary.md — issue 408 (m4-checked-consistency-run-elle-report, v3)

Skeptic's pass. Evidence attacks were re-run for real in this sandbox (cargo + java + the
pinned elle-cli 0.1.9 jar are all present on this host); findings below are grounded on the
target source at `/home/eddie/development/wyrd/wyrd.pdca-wt`.

## Refutation attempts that FAILED (the evidence held)

- **Fixture authenticity** — fed all five committed EDN fixtures through the REAL
  `elle-cli-0.1.9-standalone.jar` on this host: `register-history-known-good.edn` → `true`
  (exit 0), `known-bad` → `false` (exit 1), `directory-history-known-good.edn` → `true`,
  `known-bad` → `false`, `rejected-vocabulary.edn` → `:unknown` (exit 0). Every claim in the
  brief's Design §3/§5 reproduced byte-for-byte, including the captured checker-output
  format (`<file> \t <token>`, Clojure `println`'s space-tab-space) in
  `xtask/tests/fixtures/consistency-run/checker-output-*.txt`. The "REAL samples" claim is
  genuine, not hand-written.
- **Green leg** — `cargo test -p xtask --test consistency_run_orchestration`: 25/25 pass.
- **Red leg** — `origin/main` (`e0e39c1`) lacks `xtask/src/consistency_run.rs` and the test
  file entirely (both added by this patch), so the kept test cannot compile without the fix;
  the C4-verify red is real, not a vacuum.
- **Parser attacks** — `:unknown`-with-exit-0, `true`-with-nonzero-exit, empty/garbage
  output (`xtask/src/consistency_run.rs:1704-1730` region) all resolve
  inconclusive/never-pass; `self_check_matches` demands a genuine `false` for known-bad, so
  an `:unknown` cannot masquerade as a caught violation. Could not refute.
- **Leg-name mismatch** — `xtask::nemesis::NemesisLegKind::as_str` (`xtask/src/nemesis.rs:67`)
  emits `network-partition`/`clock-skew`/`process-pause`, all accepted by the scenario's
  match arms (`crates/server/tests/consistency_run_fdb.rs:156-186`). No mismatch.

## Findings

- NEEDS-HUMAN [impl] — **The Wyrd-checked register delete pool is missing.** Design §2 says
  "the scenario runs three pools" and scope item (3) names the "Wyrd-checked delete pool"
  (PUT/GET/DELETE on a disjoint key set, judged by the landed session/resurrection checks,
  counted in the summary). `drive_pools` (`crates/server/tests/consistency_run_fdb.rs:326`)
  drives only two: the overwrite pool and the directory create pool; no DELETE traffic runs
  anywhere, none of the #406 Wyrd-side checks are invoked in the live run, and the summary
  (`:225`) carries no delete-pool counts. Tellingly, the orchestration test's own fixture
  claims the missing pool exists: `xtask/tests/consistency_run_orchestration.rs:145`
  describes "register delete pool (Wyrd)" in its attesting workload string — a pool the
  scenario never runs. The brief's DELETE-resurrection/lost-write coverage silently vanished.

- NEEDS-HUMAN [impl] — **Stale-status fabrication in the creator and the final-read sweep —
  a false `false` on a correct run.** `ObservableS3Client::put`/`get` return `Err` WITHOUT
  recording an op (`crates/server/src/consistency_observable.rs:161-170,179-186` — the `?`
  fires before `history.ops.push`). The scenario then reads
  `c.history().ops().last().map(|op| op.status).unwrap_or(0)`
  (`crates/server/tests/consistency_run_fdb.rs:437-439` and `:478-479`) — so a
  transport-errored create inherits the PREVIOUS create's status (e.g. 200) and is
  serialized as a determinate `:ok` `:add` of an element that may never have been created.
  Concrete failing case: mid-window connection error on create *i*>0 after a successful
  create → EDN says `:ok`, the post-heal sweep doesn't find the element → the Elle set
  checker returns `false` on a correct history — exactly the INV-1 "fabricated determinate
  completion" class this brief exists to bury (the `unwrap_or(0)` synthetic-indeterminate
  guard only covers the empty-history first op). The same pattern at `:478-479` marks a
  member Present off the previous member's 200.

- NEEDS-HUMAN [impl] — **Errored register ops are dropped, not `:info`.**
  `let _ = c.put(...)` / `let _ = c.get(...)`
  (`crates/server/tests/consistency_run_fdb.rs:360,371`) discard `io::Error`, and the op
  never enters the history at all. A PUT whose response was lost after the request was sent
  may have committed; omitting it (instead of recording an indeterminate `:info`, the
  synthetic-0 convention #406's own module doc reserves for this) means a later read can
  observe a version with **no corresponding write in the EDN** → rw-register `false` on a
  correct run. INV-1 says indeterminate → `:info`, never a definite outcome — silent
  omission fabricates the definite outcome "never happened".

- NEEDS-HUMAN [impl] — **Report/fixture conformance vs the brief.** (a) Design §6 requires
  the report carry "checker + **version** + jar SHA-256" and the "**member-id map**":
  `write_report` (`xtask/src/consistency_run_runner.rs:361-380`) records only the jar
  sha256 (no elle-cli version string), and the member-id map never crosses the seam —
  `RunSummary` (`xtask/src/consistency_run.rs`, struct at the "run summary" section) has no
  `member_id_map` field, so the scenario's emitted map (`consistency_run_fdb.rs:217-220`)
  is silently dropped from the report. (b) The Verification posture names a captured
  "stack-trace error" checker output among the golden fixtures; only pass/fail/unknown are
  committed under `xtask/tests/fixtures/consistency-run/` — the error shape is exercised
  only by an inline `"garbage"` literal, not a real capture.

- NEEDS-HUMAN — **The acceptance artifact is absent and the live half is unverifiable
  here.** No witnessed run's report exists under `docs/design/reviews/` in this diff (the
  brief's §6 ready-mark precondition), and the live leg (cluster bring-up, `drive_leg`, the
  two real `true` verdicts) cannot be exercised in this sandbox — that half of the verdict
  is provisional and is the human's at sign-off. Note the specific rationalization risk in
  the gate row set: C4-ci/C4-verify all-green covers **none** of
  `crates/server/tests/consistency_run_fdb.rs`'s runtime behavior — the two INV-1 defects
  above live exactly in that Check-blind file, and would first surface as a spurious
  `false`/lost-element verdict in the witnessed run itself.

## Verdict

The pure xtask core (parser, gate, invocation, report renderer) and the serializer
vocabulary survived every attack I could mount, including re-running the real checker over
the committed fixtures. The live scenario file did not: two concrete
correct-history-comes-back-`false` cases (stale-status inheritance; dropped errored ops) and
one whole missing pool — all builder-iterable before the witnessed run is attempted.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C2 Reproduction (red pre-fix) — Decide whether the recorded revert-based red leg is acceptable without an independent reviewer rerun — the review target is read-only, so I could run green but could not stash/revert the fix to reproduce red; this matters because the asserted causal flip remains provisional.
- [ ] C4 Verification (red→green) — Decide whether to sign off after running the privileged leg and reviewer-blocked socket test — the 25 orchestration tests passed, but the server suite stopped at 12/13 because loopback bind was denied at `crates/server/tests/consistency_workload.rs:654`, and no independent red leg or live Docker/FDB/Elle green was reproduced.
- [ ] T3 Runtime — Run `WYRD_TIER1=1 WYRD_ELLE_CLI_JAR=<elle-cli-0.1.9.jar> cargo xtask consistency-run` on the privileged 3-node FDB topology and confirm both models return `Pass`, materialization/concurrency are non-vacuous, and teardown completes; Docker, FDB linkage/topology, nemesis privileges, and real live histories were not exercised here, so runtime fitness is undischarged (`xtask/src/consistency_run_runner.rs:408`).
- [ ] T4 Contribution — Decide whether closed/rejected work contains a superseding #408 implementation — affected-path merged history locally shows only #479 (`3859d41`) for the established serializer path, but closed/rejected PR state could not be mechanically established; duplicate contribution risk therefore remains.
- [ ] T5 Judgment — Decide whether the partial artifact is acceptable despite missing promised DELETE coverage and witnessed report — those omissions materially weaken the externally recognizable consistency claim even though the pure orchestration/parser design is coherent.
- [ ] Validation — fitness-to-purpose — Decide whether a real, non-vacuous two-model Elle pass under a materialized fault is credible for the project claim — inspect and commit the generated five-field report after the privileged command above, because curated fixtures and code-read tests cannot establish live fitness.
- [ ] **The Wyrd-checked register delete pool is missing.** Design §2 says
- [ ] **Stale-status fabrication in the creator and the final-read sweep —
- [ ] **Errored register ops are dropped, not `:info`.**
- [ ] **Report/fixture conformance vs the brief.** (a) Design §6 requires
- [ ] **The acceptance artifact is absent and the live half is unverifiable

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
- Iteration delta (if iterating): Human decision: the pure xtask core (parser, gate, report renderer, fixtures) held under adversarial re-verification and stays; the live scenario file must be fixed before the witnessed run is attempted. Builder must address, within the existing brief: 1. Missing Wyrd-checked DELETE pool — `drive_pools` (crates/server/tests/consistency_run_fdb.rs:326,351) drives only the overwrite and directory-create pools; add the disjoint PUT/GET/DELETE pool judged by the #406 Wyrd-side checks, with its counts in the run summary. 2. Stale-status fabrication — errored `put`/`get` inherit the PREVIOUS op's status via `history().ops().last() ... unwrap_or(0)` (consistency_run_fdb.rs:437-439, 478-479); a transport error must never serialize as a definite `:ok` (INV-1). 3. Dropped errored register ops — `let _ = c.put(...)` / `let _ = c.get(...)` (consistency_run_fdb.rs:360,371) silently omit indeterminate ops from the history; record them as `:info` per the synthetic-0 convention, never omit. 4. Report conformance — report must carry the elle-cli version string (not just jar sha256) and the member-id map must cross the RunSummary seam into the report (xtask/src/consistency_run_runner.rs:361-380); the map is currently emitted by the scenario but dropped. The privileged witnessed run (WYRD_TIER1=1 cargo xtask consistency-run on the live FDB topology) and the committed report under docs/design/reviews/ — the brief's acceptance artifact — will be produced in the NEXT iteration, after these fixes land; do not attempt the live leg before they do.
- By / date: Eduard Ralph / 2026-07-16

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
