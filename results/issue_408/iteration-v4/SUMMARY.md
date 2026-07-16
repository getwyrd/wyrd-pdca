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

Review of issue #408: add an opt-in, non-vacuous FoundationDB consistency run that checks register and directory histories with Elle and publishes the witnessed report.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The acceptance boundary is explicit: checker vocabulary, non-vacuity, live dependencies, and the required committed witnessed report are independently stated in `brief.md:14`. |
| C2 Reproduction (red pre-fix) | NEEDS-HUMAN | The red-leg decision remains provisional — the referenced `engine/scripts/run-verify.sh` is not present in the target, so I could not independently reproduce the asserted pre-fix failure; the human must accept the gate's recorded red result or rerun it in the driver environment. |
| C3 Change | FAIL | Readiness requires the first live report to be committed, but `patch.diff` contains no `docs/design/reviews/` file; the patch therefore does not deliver the public credibility artifact required by `brief.md:28`. |
| C4 Verification (red→green) | NEEDS-HUMAN | The focused patched test passed 37/37 and real elle-cli returned good→`true`, bad→`false` for both models, but the full `cargo xtask ci` rerun stopped on sandbox-denied loopback bind in `crates/chunkstore-grpc/tests/list_delete.rs:55` and the red-leg script is unavailable; the human must rerun both legs on a host permitting sockets. |
| C5 Causal adequacy | FAIL | The human need not choose between symptom and root cause here: an unknown final probe is dropped from `present`, thereby asserting definite absence to Elle; the run must instead become inconclusive (or retry to a determinate result) when `membership(record.status)` is `Unknown` at `crates/server/tests/consistency_run_fdb.rs:604`. |
| T1 Structure | PASS | The pure decision seam remains default-compiled while privileged execution stays in the runner/live test, keeping FDB and JVM out of the xtask test graph (`xtask/src/consistency_run.rs:38`). |
| T2 Shape | PASS | Typed nemesis evidence, concurrency, outcome counts, delete-check results, member mapping, and the final read cross a deny-unknown-fields summary contract (`xtask/src/consistency_run.rs:213`). |
| T3 Runtime | FAIL | A transport/torn-read status maps to `Membership::Unknown`, yet the live sweep emits every non-`Present` member as absent in the complete set value; this can fabricate a directory violation or verdict (`crates/server/tests/consistency_run_fdb.rs:599`). |
| T4 Contribution | NEEDS-HUMAN | Merged-history inspection by affected path found prior serializer work (#479), but closed/rejected PR and remote-branch prior art could not be mechanically queried here; the human must confirm no superseding #408 contribution before accepting this broad patch. |
| T5 Judgment | NEEDS-HUMAN | The human must decide whether a credibility feature is reviewable before its promised live three-node run and committed dual-`true` report exist — without that artifact, the central operational claim remains unevidenced despite green pure tests (`xtask/src/consistency_run_runner.rs:458`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | The human must confirm a witnessed, materialized-fault run produces a credible public artifact with both real model verdicts `true` — this determines whether the feature satisfies #329 rather than merely implementing orchestration. |

### Advisory — adversary

# check-advisory-adversary.md — issue #408 (m4-checked-consistency-run-elle-report, v4)

Adversarial pass. I re-ran the green legs at `$PDCA_TARGET` (37/37 orchestration tests,
13/13 + 11/11 server-side consistency tests), re-ran the REAL elle-cli 0.1.9 jar over every
committed fixture, and executed the production #406 checks against a delete-pool-shaped
history in a scratch harness. Two findings are execution-verified refutations, both in the
live scenario file — the one file **no Check gate exercises** (it compiles only under
`--features fdb`; the C4-verify red→green covers only the xtask pure core).

- NEEDS-HUMAN [impl] — **The new delete pool fabricates violations on a correct system; the
  witnessed run will near-certainly FAIL with a false "real violation".**
  `crates/server/tests/consistency_run_fdb.rs:497-509` gives each process a disjoint
  *version band* (p0: `1..`, p1: `1_000_000..`) on the **shared** `DELETE_POOL_KEY`, but all
  three #406 checks judging the pool assume per-key version tags are monotone in *commit*
  order: `reads_are_monotone` compares raw version numbers across processes
  (`crates/server/src/consistency_workload.rs:512-530`), and the RYW arm flags any read
  below the session's own write with no cross-process-overwrite waiver
  (`consistency_workload.rs:260` — only 404s are waived, `:271-282`). Concrete failing case,
  **verified by running the production checks**: the linearizable history
  `p0 PUT v=1; p1 PUT v=1_000_001; p1 GET→1_000_001; p0 PUT v=2 (newer commit, smaller tag);
  p0 GET→2; p1 GET→2` returns `false` from ALL THREE of `reads_monotone_per_key`,
  `session_monotonic_reads`, `session_read_your_writes`. With both processes interleaving 60
  PUT/GET/DELETE/GET rounds on one key inside the fault window, a cross-band read pair is
  essentially guaranteed, and the runner escalates it to "checked consistency run FAILED —
  ... a real violation observed on the live cluster"
  (`xtask/src/consistency_run_runner.rs:537-545`). This inverts INV-1 (a fabricated
  violation instead of fabricated certainty) and blocks the brief's acceptance artifact (the
  witnessed `true`-verdict report). Fix by construction: per-process disjoint *keys* inside
  the delete pool (single writer per key keeps tag order = commit order), not shared-key
  disjoint bands. No Check-time test covers the two-writer banded shape, which is why C4 is
  green anyway.

- NEEDS-HUMAN [impl] — **The composed final read silently omits Unknown-probed members —
  fabricating a "lost element" `false` from Elle on a correct run.**
  `crates/server/tests/consistency_run_fdb.rs:599-608`: a post-heal probe whose status is
  neither 200 nor 404 (5xx/timeout ⇒ `Membership::Unknown`) simply drops that member from
  the composed `:read` set. In the `set` model an acknowledged `:add` missing from the final
  read is a lost element — **verified against the real jar**: the committed
  `directory-history-known-bad.edn` is exactly this shape and returns `false`. Aggravating:
  Design §2 requires the sweep "after heal + quiesce", but `consistency_run_fdb.rs:209` runs
  `compose_final_read` immediately after `drive_leg` returns — no quiesce — so a transient
  probe error while FDB is still recovering from the partition is plausible. An Unknown
  probe must re-probe, abort, or degrade the composed read to `:info`; silent omission from
  a definite `:ok` read is the same INV-1 fabrication the scenario's own comment (`:603`)
  claims to avoid, just in the absence direction.

- NEEDS-HUMAN [impl] — **`deny_unknown_fields` does not cover the nested seam objects,
  contradicting the "seam fails loudly" claim.** `xtask/src/consistency_run.rs:136-152`
  (`NemesisEvidence`) and `:159-169` (`OutcomeCounts`) lack `#[serde(deny_unknown_fields)]`
  (serde does not propagate it from `RunSummary`), so a field the scenario adds inside
  `nemesis` or any outcomes object is still silently dropped — the exact
  `member_id_map`-style loss the doc at `consistency_run.rs:220-226` declares closed. The
  orchestration test pins only a top-level unknown field
  (`xtask/tests/consistency_run_orchestration.rs:337-352`).

- `RUN_STAGES`/`run_plan` is a mirror, not the production path: the impure runner never
  consults it (`run_consistency_check`'s control flow at
  `xtask/src/consistency_run_runner.rs:499-564` is hand-sequenced), so
  `run_plan_carries_bring_up_through_report_in_order` would stay green if the runner dropped
  a real stage. Mild tautology (shared with the `metadata_faults` peer pattern) — the
  reviewer's "run-orchestration plan exercised red→green" claim is true only of the
  constant, not the orchestration.

- Minor overstatement: `directory_ops = creates.len() + universe.len()`
  (`crates/server/tests/consistency_run_fdb.rs:217`) counts the post-heal sweep probes as
  history ops, but they enter the EDN only as ONE composed read — the report's "history
  size" field overstates the checked directory history (~2x), against the Success
  criterion's "refuses to overstate itself".

Attempted and could NOT refute: (a) the golden fixtures' authenticity — I re-ran
`java -jar elle-cli-0.1.9-standalone.jar` over all five committed EDN fixtures and got
byte-identical verdict lines to the committed capture files (`true`/exit 0, `false`/exit 1,
`:unknown`/exit 0), so the "REAL elle-cli-accepted samples" claim holds; (b) the
three-valued verdict parser (token-keyed, `:unknown`+exit-0 → inconclusive, `true`+non-zero
→ inconclusive) — consistent with observed real outputs; (c) the C4-verify red — the
`xtask::consistency_run` module is absent on `origin/main`, so the kept test genuinely
fails pre-fix; (d) the v3 sign-off items are all structurally addressed (OpFailed returns
the op's OWN record, errored ops recorded as `:info`, checker version + member-id map cross
the seam into the report). The gates' green is real for what it measures — but note for the
verdict: `check-gates.json`'s all-pass says nothing about `consistency_run_fdb.rs`, where
both execution-verified defects live; per the v3 sign-off rationale the live leg must not
be attempted until they are fixed.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C2 Reproduction (red pre-fix) — The red-leg decision remains provisional — the referenced `engine/scripts/run-verify.sh` is not present in the target, so I could not independently reproduce the asserted pre-fix failure; the human must accept the gate's recorded red result or rerun it in the driver environment.
- [ ] C4 Verification (red→green) — The focused patched test passed 37/37 and real elle-cli returned good→`true`, bad→`false` for both models, but the full `cargo xtask ci` rerun stopped on sandbox-denied loopback bind in `crates/chunkstore-grpc/tests/list_delete.rs:55` and the red-leg script is unavailable; the human must rerun both legs on a host permitting sockets.
- [ ] T4 Contribution — Merged-history inspection by affected path found prior serializer work (#479), but closed/rejected PR and remote-branch prior art could not be mechanically queried here; the human must confirm no superseding #408 contribution before accepting this broad patch.
- [ ] T5 Judgment — The human must decide whether a credibility feature is reviewable before its promised live three-node run and committed dual-`true` report exist — without that artifact, the central operational claim remains unevidenced despite green pure tests (`xtask/src/consistency_run_runner.rs:458`).
- [ ] Validation — fitness-to-purpose — The human must confirm a witnessed, materialized-fault run produces a credible public artifact with both real model verdicts `true` — this determines whether the feature satisfies #329 rather than merely implementing orchestration.
- [ ] **The new delete pool fabricates violations on a correct system; the
- [ ] **The composed final read silently omits Unknown-probed members —
- [ ] **`deny_unknown_fields` does not cover the nested seam objects,
- [ ] external dependency: unzip — the off-Check runner reads the elle-cli version out of the jar (META-INF/maven/elle-cli/elle-cli/pom.properties) via `unzip -p`, because elle-cli 0.1.9 has NO --version flag (verified on this host: it prints `Unknown option: "--version"` and exits 0). Plan's External-dependencies list names docker/libfdb_c/fdb-headers/java/elle-cli but not unzip. It IS present on this host (/usr/bin/unzip), so nothing was blocked and no evidence is missing — but the witnessed live run now hard-fails at preflight without it, so it belongs in the registered set rather than being discovered by a maintainer mid-run.

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
- Iteration delta (if iterating): Rejected on the adversary review's two execution-verified defects in the live scenario file (crates/server/tests/consistency_run_fdb.rs — the one file no Check gate exercises); the pure core, serializer vocabulary, and golden fixtures are confirmed against the real elle-cli and should be kept as-is. Do NOT attempt the witnessed live run until these are fixed: 1. Delete pool fabricates violations on a correct system: per-process disjoint version BANDS on the shared DELETE_POOL_KEY break the commit-order-monotone assumption of all three #406 checks (verified by running the production checks). Fix by construction: per-process disjoint KEYS in the delete pool (single writer per key), not shared-key bands. 2. Composed final read silently omits Unknown-probed members, fabricating a "lost element" false from Elle (verified against the real jar). An Unknown probe must re-probe, abort, or degrade the composed read to :info — never silent omission. Also add the Design §2 quiesce before compose_final_read (currently runs immediately after drive_leg). Secondary, fix alongside: 3. Add #[serde(deny_unknown_fields)] to the nested seam objects (NemesisEvidence, OutcomeCounts) — serde does not propagate it from RunSummary; pin with a nested-unknown-field test, not only the top-level one. 4. Register unzip in the external-dependencies list (runner preflight reads the elle-cli version via `unzip -p`; elle-cli 0.1.9 has no --version flag). 5. Add a Check-time test covering the two-writer banded/delete-pool history shape so defect (1) cannot regress silently, and fix the ~2x directory_ops overstatement (sweep probes counted as history ops though they enter the EDN as one composed read).
- By / date: Eduard Ralph / 2026-07-16

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
