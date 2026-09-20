# Result — issue 808 / staged-drain-status

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect: drain status tells an operator a server may be wiped while a live upload's bytes
  are on it. `reconciliation_status` answers `Satisfied` for a server holding only staged
  bytes, because its `genuinely_holds` test reads committed placements alone
  (`crates/custodian/src/desired_state.rs:181-196`) — the F6 trace. The sharper form is an
  in-flight part with no `part:` record yet (`0016:827`). The class that answers this already
  exists and is unread here: `StagedSet::protects` (`crates/custodian/src/gc.rs:705`). Second,
  0016 requires a rebalance pass over a draining server holding only staged fragments to plan
  no move and rewrite no `part:` record while drain status answers `Pending` for that same
  server (`0016:881`; `plan_evacuations`, `crates/custodian/src/rebalance.rs:257`). Nothing
  asserts that today. #803 left this to #664 by name: `deferred: #663, #664` at `gc.rs:669` and
  `crates/custodian/tests/staged_protection.rs:2160`.
- Success criterion: the NEW file `crates/custodian/tests/staged_drain_status.rs` passes
  over in-memory doubles, with records seeded as raw JSON the base decoders accept (the shapes
  in `crates/core/tests/multipart_session_records.rs:81-145`). Legs:
  **(A) Drain counts an in-flight part as held.** Server `S` holds **only** an owned `sidx:`
  fragment, and `desired:dserver:<S>` is set: `reconciliation_status(S)` is `Pending`. On the
  base it is `Satisfied` — the red.
  **(B) Drain counts a committed part as held**, as its own case: `S` holds only a committed
  `part:` fragment, and the answer is `Pending`. An implementation counting only one class
  passes one of A and B and fails the other (`0016:883`).
  **(C) Drain still finishes when the uploads live elsewhere.** Staged fragments sit on servers
  0–2, and server 3 is draining and holds none of them and no committed reference:
  `reconciliation_status(3)` is `Satisfied`. Iteration 1's `*server != dserver` mutant survived
  every other leg; this case kills it. It is green on the base too — a guard.
  **(D) Rebalance and drain agree, and rebalance leaves staged bytes alone (`0016:881`).** For a
  draining server holding **only** staged fragments, a rebalance pass writes no fragment
  anywhere and rewrites no `part:` record, **and** `reconciliation_status` is `Pending`. The red
  comes from the `Pending` half. State in `build-notes.md` which `Reconciled` the pass returns
  there, and why it does not tell an operator the drain is done.
  **(E) An unreadable or untrusted staged record never yields `Satisfied`.** A staged record the
  query cannot read blocks every drain; one it can read but not trust blocks them the way a
  committed map with an untrustworthy placement does (mirror `StagedSet::protection`,
  `gc.rs:690`).
  **(F) `cargo xtask ci` green.**
- Repo + branch target: getwyrd/wyrd @ main
- Scope: `reconciliation_status` reads the existing `StagedSet` and counts both staged
  classes as held; rebalance is confirmed disjoint from the staged set (expected: a test and a
  comment, no behaviour change — if `plan_evacuations` needs a real change, say why in
  `build-notes.md`); #664's half of the two `deferred:` markers is discharged, leaving #663's;
  the drain-status sentence in `docs/design/architecture/06-runtime-view.md:78` is corrected.
  / out of scope: anything in `restore.rs`, `crates/core/src/multipart.rs` or
  `crates/server/src/cli.rs` (child-2, child-3); rebuilding the staged class (#803, merged);
  `scrub.rs` and `reconstruction.rs` (#663); the mark codec (#804);
  `crates/dst/tests/custodian.rs`; evacuating committed segmented objects (#653/#722); any edit
  to 0016 or an ADR.

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: likely-fix
- Confidence: medium
- Recommendation: (set by Do)

## 3. Correctness (Check — chain)
- C1 Spec: none — brief.md
- C2 Reproduction (red pre-fix): none — (no gate configured)
- C3 Change: none — patch.diff
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): fail — xtask: `cargo test --workspace --exclude wyrd-dst` failed with exit status: 101
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass — run-verify.sh: PASS — red without the fix, green with it (10 test(s) ran red).
- C4 diff coverage: changed lines executed by the patch's tests: pass — diff coverage 100.0% — 22 of 22 instrumentable changed lines executed (floor 80%); 22 of 139 changed lines were instrume
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): pass — 6 mutants tested in 25s: 4 caught, 2 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): pass — review-branch: 0 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_808/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 13.24s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Reviewing the staged-drain-status fix—count both multipart staged classes before certifying a drain while keeping rebalance staged-blind: no patch defect was found, but clean aggregate-CI evidence and final fitness remain human decisions.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The acceptance contract separately decides both staged classes, the unaffected-server guard, rebalance disjointness, fail-closed damaged records, and CI against the governing consumer split (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:765`). |
| C2 Reproduction (red pre-fix) | PASS | An independent pre-fix run kept the negative guard green but produced 9 assertion failures, including the in-flight, committed-part, and rebalance/status legs at `crates/custodian/tests/staged_drain_status.rs:548`, `:571`, and `:664`. |
| C3 Change | PASS | The scoped status path now evaluates the existing staged class alongside committed references and blocks on incomplete or untrusted records (`crates/custodian/src/desired_state.rs:222`, `:270`, `:298`), while rebalance receives no behavior change (`crates/custodian/src/rebalance.rs:242`). |
| C4 Verification (red→green) | NEEDS-HUMAN | Decide whether to rerun or waive clean aggregate CI—focused red→green, all custodian tests, and both implicated server binaries pass independently, but frozen CI first failed an unrelated health test and its confirmation timed out (`gate-logs/C4-ci.log:2272`, `:2299`), leaving criterion F unproven. |
| C5 Causal adequacy | PASS | The fix removes the committed-only cause by reading the source staged class before committed references, so publication cannot disappear between classes (`crates/custodian/src/desired_state.rs:211`, `crates/custodian/tests/staged_drain_status.rs:916`); no capability probe or symptom guard was added. |
| T1 Structure | PASS | Status logic remains in `desired_state`, reuses `gc::staged_fragments`, and the required new integration test and living runtime documentation occupy their established locations (`crates/custodian/src/desired_state.rs:222`, `docs/design/architecture/06-runtime-view.md:80`). |
| T2 Shape | PASS | No API or persisted-record shape is added; the result preserves existing status precedence and deterministically sorts/deduplicates blockers from both classes (`crates/custodian/src/desired_state.rs:230`, `:270`, `:301`). |
| T3 Runtime | PASS | Staged metadata is still walked in bounded pages and store faults fail closed rather than certify from a partial picture (`crates/custodian/src/gc.rs:1023`, `:1032`, `crates/custodian/src/desired_state.rs:216`). |
| T4 Contribution | N/A | Contribution artifacts are absent by design and the frozen evidence assigns their substantive check to publish (`gate-logs/T4-contribution.log:10`); an independent affected-path audit across all 334 repository PRs found no open duplicate. |
| T5 Judgment | PASS | Deep review found no unhandled standing-rubric defect; affected-path prior art consists of merged precursors plus the older closed-unmerged broad PR #647, not competing live work, and the focused mutant gate caught every viable mutation (`gate-logs/C5-mutants.log:13`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether production operator safety is adequately demonstrated by seeded in-memory records before client session creation exists—this determines whether the staged-drain semantics and their staged-population read cost are fit for deployment. |


## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C4 Verification (red→green) — Decide whether to rerun or waive clean aggregate CI—focused red→green, all custodian tests, and both implicated server binaries pass independently, but frozen CI first failed an unrelated health test and its confirmation timed out (`gate-logs/C4-ci.log:2272`, `:2299`), leaving criterion F unproven.
- [ ] Validation — fitness-to-purpose — Decide whether production operator safety is adequately demonstrated by seeded in-memory records before client session creation exists—this determines whether the staged-drain semantics and their staged-population read cost are fit for deployment.
- [ ] C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance) FAILED (gating) — xtask: `cargo test --workspace --exclude wyrd-dst` failed with exit status: 101

## 7. Proven / not proven
- Proven by which oracle: gates overall = fail (stub oracles).
- Unproven / needs manual run: anything flagged in §6.

## 8. Ready-to-ship attachments
- patch.diff
- tracker-comment.md     (ALWAYS, every tracker item)
- build-notes.md         (builder rationale — for the human, not the reviewer)

## 9. Check sign-off                     ← human completes Check here
- Disposition confirmed / overridden:
- Outcome: iterated-to-Do
- Iteration delta (if iterating): Auto-iterate (round 1): rebuilding for the implementation-level findings — C4 Verification (red→green) — Decide whether to rerun or waive clean aggregate CI—focused red→green, all custodian tests, and both implicated server binaries pass independently, but frozen CI first failed an unrelated health test and its confirmation timed out (`gate-logs/C4-ci.log:2272`, `:2299`), leaving criterion F unproven.; C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance) FAILED (gating) — xtask: `cargo test --workspace --exclude wyrd-dst` failed with exit status: 101.
- By / date: auto-iterate / 2026-09-15

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
