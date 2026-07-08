# Result — issue 406 / elle-register-listappend-models-and-workload-recorder

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect / goal: (gap / need) Wyrd has **no** externally-recognizable consistency-checker
- Success criterion: In `cargo xtask ci` (the unprivileged, container-free gate):
- Repo + branch target: getwyrd/wyrd @ feat/m4-production-metadata-backend
- Scope (one logical fix) / out of scope: Build the checker **models** (rw-register primary; list-append/set secondary),

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

Issue 406 implements the ADR-0041 mutable-metadata consistency-checker substrate: register/list-append models, session checks, recorder/serialization, and an in-process gateway workload.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The scoped decision is to build Check-exercised pure Rust models/workload while leaving the live Elle/JVM verdict off-Check; the target exposes that split in the model module and verdict seam at `/home/eddie/wyrd/wyrd.pdca-wt-l0/crates/testkit/src/consistency.rs:300` and `/home/eddie/wyrd/wyrd.pdca-wt-l0/crates/testkit/src/consistency.rs:1078`. |
| C2 Reproduction (red pre-fix) | NEEDS-HUMAN | The decision owed is whether the asserted red pre-fix evidence is acceptable: `check-gates.json` cites `./engine/scripts/run-verify.sh`, but that helper is not present in the target for me to rerun, so I only confirmed the crafted rejection tests exist at `/home/eddie/wyrd/wyrd.pdca-wt-l0/crates/testkit/tests/consistency_models.rs:109`. |
| C3 Change | PASS | The patch changes the relevant surface by adding committed-write-gated register provenance and winner counting, which is the issue-406 correctness risk, at `/home/eddie/wyrd/wyrd.pdca-wt-l0/crates/testkit/src/consistency.rs:328` and `/home/eddie/wyrd/wyrd.pdca-wt-l0/crates/testkit/src/consistency.rs:343`. |
| C4 Verification (red→green) | NEEDS-HUMAN | The focused target suite is green (`cargo test -p wyrd-testkit --test consistency_models`: 18 passed), but full `cargo xtask ci` hit a host loopback-bind denial in `crates/chunkstore-grpc/tests/list_delete.rs:55` and the red→green helper was unavailable, so the merge-gate claim remains provisional. |
| C5 Causal adequacy | PASS | The prior false-accept class is directly covered: failed-write values are excluded from the provenance domain and the regression asserts rejection at `/home/eddie/wyrd/wyrd.pdca-wt-l0/crates/testkit/src/consistency.rs:330` and `/home/eddie/wyrd/wyrd.pdca-wt-l0/crates/testkit/tests/consistency_models.rs:109`. |
| T1 Structure | PASS | The decision boundary stays in testkit as a pure checker/recorder module exported from the existing crate, avoiding gateway or metadata-store production changes, at `/home/eddie/wyrd/wyrd.pdca-wt-l0/crates/testkit/src/lib.rs:17`. |
| T2 Shape | PASS | The API shape covers the briefed artifacts: register ops, namespace ops including rename, session checks, recorder methods, EDN serialization, and dispatch, with rename recordability grounded at `/home/eddie/wyrd/wyrd.pdca-wt-l0/crates/testkit/src/consistency.rs:597` and `/home/eddie/wyrd/wyrd.pdca-wt-l0/crates/testkit/tests/consistency_models.rs:290`. |
| T3 Runtime | PASS | The runtime risk specific to this patch is exercised by the in-process gateway workload and passed locally; it asserts model acceptance, session checks, namespace checks, overlap, and version bump at `/home/eddie/wyrd/wyrd.pdca-wt-l0/crates/testkit/tests/consistency_models.rs:622` and `/home/eddie/wyrd/wyrd.pdca-wt-l0/crates/testkit/tests/consistency_models.rs:635`. |
| T4 Contribution | PASS | The prior-art decision is that this is not duplicating existing repair-path or metadata-tier scenarios: tree search found older Jepsen/TiKV consistency surfaces, while the new affected files introduce the first `check_register`/`HistoryRecorder` mutable-register substrate at `/home/eddie/wyrd/wyrd.pdca-wt-l0/crates/testkit/src/consistency.rs:320` and `/home/eddie/wyrd/wyrd.pdca-wt-l0/crates/testkit/src/consistency.rs:868`. |
| T5 Judgment | NEEDS-HUMAN | The decision owed is the brief's sequencing question: proceed with the in-process core before #405's networked observable, or hold this slice until #405 lands; the workload explicitly scopes rename and wire driving to later surfaces at `/home/eddie/wyrd/wyrd.pdca-wt-l0/crates/testkit/tests/consistency_models.rs:538`. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Human sign-off must decide whether the produced EDN plus deferred privileged Elle/JVM leg is fit for the externally-recognizable checker goal, because Check only verifies serialization/dispatch and Rust models at `/home/eddie/wyrd/wyrd.pdca-wt-l0/crates/testkit/tests/consistency_models.rs:357`. |

### Advisory — adversary

# Adversarial review — issue 406 (elle-register-listappend-models-and-workload-recorder)

Advisory only; I never gate. Toolchain **was** available (cargo 1.96.0), so I re-ran the
red→green and attacked the models with crafted histories against a byte-for-byte copy of the
shipped `crates/testkit/src/consistency.rs` (pure-std; no target files modified).

## Evidence re-run — attempted to refute, could NOT

- **Green half holds.** `cargo test -p wyrd-testkit --test consistency_models` → 18/18 pass in
  the target.
- **The headline red is a genuine model-weakening red, not a compile-error red** (the exact
  concern iteration-1 raised). I built the mutant the test's own doc-comment names — dropping
  `&& c.ok` from the provenance loop at `crates/testkit/src/consistency.rs:330` only — and it
  **compiles and false-accepts** the failed-write-value history (`check_register` → `Ok(())`
  where `register_rejects_a_read_of_a_failed_writes_value` asserts `Err(TornRead)`). So the
  `check-gates.json` C4-verify claim ("red without the fix, green with it") is sound for that
  test, and the iteration-1 provenance fix (`consistency.rs:328-335`) genuinely closes the
  previously-rejected false-accept.
- **The workload test is not flaky:** 40/40 clean runs of
  `workload_against_the_in_process_gateway_yields_a_nonvacuous_checkable_history`. The
  real-time-proxy interval modeling (observe() sandwiched between the recorder's invoke/ok
  lock sections) is sound against spurious version-regression, and the barrier makes the
  non-vacuity asserts (`concurrency >= 2`, `version >= 2`) robust.

I could not refute the specific defect the previous cycle rejected, nor the two-winners /
rename / session crafted reds — those flips are all real behavioral reds on real inputs.

## The fix — concrete inputs that break it (new false-accepts)

The model documents itself (check_register doc, `consistency.rs:311` and the fn header) as
proving "no torn **or stale** read". The stale-read guarantee is delivered **only** by the
Pass-3 real-time version-regression loop (`consistency.rs:412-431`), and Pass-2 value
provenance (`consistency.rs:379`) keys a read's value to the **key**, never to its
`(key, version)`. That leaves detectable stale reads that the model **accepts** — the same
false-accept class that got iteration-1 rejected ("a checker whose whole value is its
correctness MUST detect this"). All three below were run through the shipped model and
returned `Ok(())`:

- **NEEDS-HUMAN — stale read on the contended path is invisible (`consistency.rs:412-419`).**
  `[w(k,10)→ok v1; w(k,20)→ok **version=None**; r(k,10)@v1]` with the read beginning strictly
  after the v2 write completed → **accepted**. The read returns the superseded value after a
  committed overwrite (a textbook stale read / non-linearizable), but because the winning
  write recorded `version=None` it is excluded from Pass 3's observation set
  (`if let Some(ver) = c.version`), so no regression fires. This is not academic: the
  production workload's `put_with_retry` returns `None` exactly under contention
  (`crates/testkit/tests/consistency_models.rs:454-456`), and the barriered 3-way HOT
  overwrite makes `version=None` write-oks the **common** case in the very history fed to
  `check_register`. So on the path criterion (d) most stresses, the model's stale-read
  detection is disabled — a buggy gateway that served a stale contended read would still be
  **passed**. That undercuts the load-bearing claim that "the register model then passes" is
  evidence of anything.

- **NEEDS-HUMAN — provenance is per-key, not per-(key,version) (`consistency.rs:379`).**
  `[w(k,10)→ok v1; w(k,20)→ok v2; r(k,10)@v3]` → **accepted**. Value 10 was superseded at v2
  and version 3 was never committed, but the read passes because 10 was written *somewhere*
  for k and no committed write pinned `(k,3)`. A crafted stale read the brief's criterion (a)
  says the register model must reject.

- **NEEDS-HUMAN — a vanished committed value (lost write) is not detected
  (`consistency.rs:377`).** `[w(k,10)→ok v1; r(k)→absent]` → **accepted**. The register model
  has no delete op, so a key that reads absent after a committed write is a lost write /
  stale read; Pass 2 treats an absent read as "observes nothing" and skips it, and Pass 3
  ignores versionless observations, so nothing flags the disappearance.

- **NEEDS-HUMAN — exactly-one-writer-wins is also disabled on the contended path
  (`consistency.rs:344-346`).** Pass 1 only counts a write when it carries `Some(ver)`.
  `[w(k,10)→ok None; w(k,20)→ok None; …]` (two winners, both `version=None`) → **accepted**.
  Same root cause as the stale-read gap: the exact `version=None` writes the workload emits
  under contention are the ones on which two-winners detection cannot fire, so the "real
  overwrites at the commit point" the workload advertises are the least-checked ops.

## The verdict — where the reviewer may have rationalized

- **NEEDS-HUMAN — the "no stale read" guarantee in the `check_register` doc
  (`consistency.rs:311` / fn header) is stronger than the implementation delivers.** The
  crafted red tests cover *one* torn read, *one* version regression, and two two-winners
  variants — a real green — but by the iteration-1 standard (one false-accept was sufficient
  to reject), the four false-accepts above are the same defect class. A human must decide
  whether they are acceptable incompleteness for a net-new conservative checker **or** a
  repeat of the rejected fault. The strongest of the four (contended-path stale read /
  two-winners) is not mere crafted-history incompleteness — it degrades detection on the
  production path the slice exists to exercise, so criterion (d)'s "the register model passes
  the produced history" is weak positive evidence: on the contended ops, passing is nearly
  guaranteed regardless of correctness.

## Scope note

All findings are on the code this diff adds (`crates/testkit/src/consistency.rs`,
`crates/testkit/tests/consistency_models.rs`). No pre-existing debt cited. The rename-branch,
session-teeth, and identical-value two-winners remarks from iteration-1 are genuinely
addressed by the patch (recorder rename API + tests at
`consistency_models.rs:1461`; occurrence-count two-winners at `consistency.rs:343-359`).

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C2 Reproduction (red pre-fix) — The decision owed is whether the asserted red pre-fix evidence is acceptable: `check-gates.json` cites `./engine/scripts/run-verify.sh`, but that helper is not present in the target for me to rerun, so I only confirmed the crafted rejection tests exist at `/home/eddie/wyrd/wyrd.pdca-wt-l0/crates/testkit/tests/consistency_models.rs:109`.
- [ ] C4 Verification (red→green) — The focused target suite is green (`cargo test -p wyrd-testkit --test consistency_models`: 18 passed), but full `cargo xtask ci` hit a host loopback-bind denial in `crates/chunkstore-grpc/tests/list_delete.rs:55` and the red→green helper was unavailable, so the merge-gate claim remains provisional.
- [ ] T5 Judgment — The decision owed is the brief's sequencing question: proceed with the in-process core before #405's networked observable, or hold this slice until #405 lands; the workload explicitly scopes rename and wire driving to later surfaces at `/home/eddie/wyrd/wyrd.pdca-wt-l0/crates/testkit/tests/consistency_models.rs:538`.
- [ ] Validation — fitness-to-purpose — Human sign-off must decide whether the produced EDN plus deferred privileged Elle/JVM leg is fit for the externally-recognizable checker goal, because Check only verifies serialization/dispatch and Rust models at `/home/eddie/wyrd/wyrd.pdca-wt-l0/crates/testkit/tests/consistency_models.rs:357`.

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
- Iteration delta (if iterating): Rejected: the register model still FALSE-ACCEPTS on the CONTENDED path — the same false-accept class that sank iteration-1, now recurring where it matters most. Root cause: contended writes record version=None, and every load-bearing check is gated on Some(version), so on exactly the contended ops the model's detection is switched off: - stale read after a committed overwrite is accepted — Pass-3 version-regression excludes version=None writes (consistency.rs:412-419); - provenance is per-key, not per-(key,version), so a superseded value reads clean (consistency.rs:379); - a vanished committed value / lost write is not detected — absent read is skipped (consistency.rs:377); - two-winners-at-one-commit-point is not counted when both writes are version=None (consistency.rs:344-346). NOT acceptable incompleteness: version=None is the COMMON case in the very workload fed to check_register, so on criterion (d)'s contended ops "the register model passes" is near-guaranteed regardless of correctness — the checker's whole value is that it detects these, and here it cannot. What to change (same brief, no re-plan): make the checker fire stale-read / lost-write / two-winners detection on version=None writes too. Preferred: capture the REAL commit version even under contention (thread the committed inode version back through put_with_retry instead of dropping it to None) so Pass-1/Pass-3 observe every committed write; alternatively, have the model conservatively REJECT unresolvable version=None overwrites rather than silently skip them. Add a crafted flippable red for each of the four cases above so the regression is locked in. Do NOT re-run the previous approach unchanged: the builder is already at the top escalation rung (opus-xhigh = opus + max thinking budget), so there is no stronger model to escalate to — this pointed carry-forward is the whole leverage. Do NOT relitigate the accepted deferred-Elle / off-CI verdict split; that scope is fine and was not the reason for rejection.
- By / date: Eduard Ralph / 2026-07-07

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
