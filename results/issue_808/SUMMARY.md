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
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): pass — xtask ci: all checks passed
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass — run-verify.sh: PASS — red without the fix, green with it (7 test(s) ran red).
- C4 diff coverage: changed lines executed by the patch's tests: pass — diff coverage 100.0% — 22 of 22 instrumentable changed lines executed (floor 80%); 22 of 161 changed lines were instrume
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): pass — 6 mutants tested in 24s: 4 caught, 2 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): pass — review-branch: 0 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_808/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 13.26s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Task under review: prevent drain status from certifying a server while live multipart-upload bytes are staged on it, without making rebalance move or rewrite those staged bytes.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The required safety boundary is decidable: both committed-part and in-flight `sidx:` placements hold a drain, while rebalance deliberately excludes staged bytes (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:765`, `docs/design/proposals/draft/0016-multipart-commit-protocol.md:826`). |
| C2 Reproduction (red pre-fix) | PASS | The stashed base compiled and independently failed 6 of 7 tests by assertion, including separate in-flight and committed-part cases; the frozen evidence records the same red (`gate-logs/C4-verify.log:15`, `gate-logs/C4-verify.log:80`). |
| C3 Change | PASS | Drain certification now reads staged before committed references, checks both placement classes, and fails closed on unreadable or untrusted staged records without changing rebalance behavior (`crates/custodian/src/desired_state.rs:225`, `crates/custodian/src/desired_state.rs:277`, `crates/custodian/src/rebalance.rs:231`). |
| C4 Verification (red→green) | PASS | Restoring the patch independently made all 7 focused tests pass; frozen evidence also shows 100% coverage of 22 instrumentable changed lines and the complete aggregate CI green (`gate-logs/C4-verify.log:10`, `gate-logs/C4-diff-cov.log:46`, `gate-logs/C4-ci.log:3601`). |
| C5 Causal adequacy | PASS | The committed-only cause is removed by reusing the staged set rather than adding a capability probe or symptom guard; holder/non-holder, both staged sources, rebalance disjointness, and damaged-record cases are exercised, with no surviving viable mutant (`crates/custodian/tests/staged_drain_status.rs:423`, `crates/custodian/tests/staged_drain_status.rs:537`, `crates/custodian/tests/staged_drain_status.rs:596`, `gate-logs/C5-mutants.log:13`). |
| T1 Structure | PASS | The change preserves dependency direction by consuming the existing crate-local `StagedSet` and its bounded reader instead of rebuilding the class or adding a dependency (`crates/custodian/src/gc.rs:901`, `crates/custodian/src/gc.rs:1038`, `crates/custodian/src/desired_state.rs:229`). |
| T2 Shape | PASS | Existing status variants retain their public shape while their safety semantics and the living runtime architecture are updated; no persisted field, API operation, or crate surface is added (`crates/custodian/src/desired_state.rs:83`, `docs/design/architecture/06-runtime-view.md:80`). |
| T3 Runtime | PASS | Source-before-destination reading preserves both staging handoffs, paged ranges remain bounded, and the production rebalance/status entry points are exercised together (`crates/custodian/src/gc.rs:1012`, `crates/custodian/tests/staged_drain_status.rs:648`). |
| T4 Contribution | N/A | Contribution artifacts are absent by design and their substantive audit reruns at publish; affected-path history found no open overlap and the frozen multi-pass review found zero blocking findings (`gate-logs/T4-contribution.log:10`, `gate-logs/T4-batch-review.log:10`). |
| T5 Judgment | PASS | No implementation-level gap remains: the code and tests preserve the staged/committed separation, fail closed on incomplete evidence, and distinguish a server with no staged bytes from actual holders (`crates/custodian/src/desired_state.rs:236`, `crates/custodian/src/desired_state.rs:303`, `crates/custodian/tests/staged_drain_status.rs:528`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether operators can safely act on the split contract where rebalance reports its own pass `Satisfied` but drain status stays `Pending` until uploads publish, abort, or are reaped — tests prove the state transition, but only sign-off can accept that operational workflow (`crates/custodian/src/rebalance.rs:244`, `crates/custodian/tests/staged_drain_status.rs:654`). |

Host caveat: my local aggregate `cargo xtask ci` rerun passed spelling, docs rendering/link audit, guards, formatting, clippy, build, and workspace tests, then stopped at `cargo deny check` because the sandbox exposes `/home/eddie/.cargo/advisory-dbs/db.lock` read-only; the frozen gate ran that same aggregate in its writable gate environment and completed green (`gate-logs/C4-ci.log:3002`, `gate-logs/C4-ci.log:3601`).


## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [x] Validation — fitness-to-purpose — Decide whether operators can safely act on the split contract where rebalance reports its own pass `Satisfied` but drain status stays `Pending` until uploads publish, abort, or are reaped — tests prove the state transition, but only sign-off can accept that operational workflow (`crates/custodian/src/rebalance.rs:244`, `crates/custodian/tests/staged_drain_status.rs:654`).

## 7. Proven / not proven
- Proven by which oracle: gates overall = pass (stub oracles).
- Unproven / needs manual run: anything flagged in §6.

## 8. Ready-to-ship attachments
- patch.diff
- tracker-comment.md     (ALWAYS, every tracker item)
- build-notes.md         (builder rationale — for the human, not the reviewer)

## 9. Check sign-off                     ← human completes Check here
- Disposition confirmed / overridden:
- Outcome: merged-wider
- Iteration delta (if iterating):
- By / date: Eduard Ralph / 2026-09-15

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
