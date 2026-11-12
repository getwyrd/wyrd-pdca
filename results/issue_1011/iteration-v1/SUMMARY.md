# Result — issue 1011 / multipart-complete-value-ceiling

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect: Complete's flip writes each part set inside its own obligation value: the
  published parts' `retire:records:` and the unnamed parts' `retire:bytes:`. Part numbers can be
  spread past S3's range (core's format allows up to 999,999, `multipart.rs:939`). Then a set can
  encode past `MAX_VALUE_BYTES`, and the store refuses that flip for good (FoundationDB `2103
  value_too_large`). After child-2, such a Complete fences, is refused at the flip, releases and
  returns the stopped error (child-2's J), every time. The session can never publish, only be
  aborted. The `all` wildcard is reserved for teardown (`multipart.rs:3174-3177`).
- Success criterion: A NEW test file `crates/core/tests/multipart_complete_value_ceiling.rs`.
  It runs over a wrapper that refuses any value past `MAX_VALUE_BYTES` (as
  `crates/custodian/tests/restore_completing_fence.rs:473-486` does). Sessions are made by #894's
  Create, and parts are planted as canonical records.
  **(I1)** Parts 1 to 10,000, Complete naming the odd ones: it publishes, and both `{parts}`
  obligations fit (about 58.9 KB each).
  **(I2)** Parts 1, 3, …, 19,999 (10,000 parts), Complete naming all of them (about 129 KB
  encoded): Complete refuses `Refused(SessionTooLarge { bytes, limit })` with **zero commits**.
  The session stays `Open` with `attempts` unchanged, and an Abort then fences it.
  **(I3) The boundary is the stored value, not the bare set.**
  - Parts 1, 3, …, 15,869 (7,935 parts), Complete naming all: the set encodes to 99,981 bytes
    and the `{"parts":…}` value to 99,991, so it publishes. The installed obligation's stored
    length is at most `MAX_VALUE_BYTES`.
  - Parts 1, 3, …, 15,871 (7,936 parts): the set is 99,995 bytes but the value is 100,005, so
    Complete refuses `SessionTooLarge` with zero commits.

  If the real payload carries more than `parts`, move the pair so it still straddles the ceiling,
  and record the numbers in `build-notes.md`.
  **(I4) A part added after the first read.** A set fits when Complete first reads it. The
  wrapper plants one more unnamed part just before the fence commits, so the unnamed parts'
  `retire:bytes:` value crosses the ceiling. Complete fences, refuses `SessionTooLarge` against the
  frozen set, and releases (`Open@E+2`, `attempts` restored).
- Repo + branch target: getwyrd/wyrd @ main
- Scope: 
  - the encoded-value check, before fencing and again against the frozen set, through
    `RetireObligation`;
  - the `SessionTooLarge` doc (`multipart.rs:4515-4526`), extended to say Complete answers it;
  - the new test file.

  Out of scope:
  - any change to `UploadPart` or the part-number range;
  - any new `Refusal` variant;
  - other children's legs;
  - 0016 and ADRs, `Cargo.lock`, `deny*.toml`.

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: new-feature
- Confidence: medium
- Recommendation: (set by Do)

## 3. Correctness (Check — chain)
- C1 Spec: none — brief.md
- C2 Reproduction (red pre-fix): none — (no gate configured)
- C3 Change: none — patch.diff
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): pass — xtask ci: all checks passed
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass — run-verify.sh: PASS — red without the fix, green with it (8 test(s) ran red).
- C4 diff coverage: changed lines executed by the patch's tests: fail — diff coverage not measured — patch.diff does not apply on origin/main
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): pass — 16 mutants tested in 72s: 9 caught, 7 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): pass — review-branch: 0 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_1011/review-
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 13.47s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

#1011: Review multipart Complete's refusal of retirement values above the store ceiling, preserving valid publication and returning rejected sessions to Open without spending an attempt.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | I1–I4 specify observable publication, exact stored-value boundaries, zero pre-fence commits and restored attempts; the outstanding refusal-policy choice is identified separately below (`brief.md:20`, `brief.md:97`). |
| C2 Reproduction (red pre-fix) | PASS | With production changes stashed and the new test retained, Rust 1.96.0 compiled the suite and reproduced the three expected I2/I3/I4 assertion failures: oversized flips returned `Stopped` (`reviewer-red.log:189`, `reviewer-red.log:209`). |
| C3 Change | PASS | The change stays within Complete's capacity validation, obligation encoding, regression tests and required living documentation; it preserves UploadPart's range and existing refusal variants (`crates/core/src/multipart/complete.rs:1138`, `crates/core/src/multipart.rs:4950`, `docs/design/architecture/05-building-block-view.md:252`). |
| C4 Verification (red→green) | PASS | Restoring the patch gives 8/8 passes on the same pinned toolchain, with 85 existing Complete and 28 obligation tests also passing; diff coverage remains unmeasured because its base differs (`reviewer-green-pinned.log:15`, `reviewer-regressions.log:92`, `reviewer-regressions.log:126`, `gate-logs/C4-diff-cov.log:10`). |
| C5 Causal adequacy | PASS | The tests distinguish stored-value overflow from bare-set size and detect growth after the initial read; both validations use the obligations later published, addressing the demonstrated cause without a capability probe (`crates/core/src/multipart/complete.rs:961`, `crates/core/src/multipart/complete.rs:1011`, `crates/core/src/multipart/complete.rs:1192`, `crates/core/tests/multipart_complete_value_ceiling.rs:628`). |
| T1 Structure | PASS | The MetadataStore seam and fence ownership remain intact; carrying the validated obligations into the flip prevents measurement/publication drift (`crates/core/src/multipart/complete.rs:419`, `crates/core/src/multipart/complete.rs:1836`). |
| T2 Shape | PASS | The persisted representation keeps its canonical encoder and optional-field behavior; tests compare actual stored bytes and admit the exact 100,000-byte boundary (`crates/core/src/multipart.rs:4382`, `crates/core/tests/multipart_complete_value_ceiling.rs:400`, `crates/core/tests/multipart_complete_value_ceiling.rs:560`). |
| T3 Runtime | PASS | The injected concurrent part produces only the fence and release, ending Open@2 with attempts restored and no publication; I2 remains abortable and I1 publishes both bounded obligations (`crates/core/tests/multipart_complete_value_ceiling.rs:480`, `crates/core/tests/multipart_complete_value_ceiling.rs:510`, `crates/core/tests/multipart_complete_value_ceiling.rs:634`). |
| T4 Contribution | PASS | Merged history and closed/rejected work were checked by affected path, and the living architecture documents the new refusal; the contribution-artifact audit is N/A until its mandatory publish rerun (`reviewer-prior-art.json:3`, `reviewer-prior-art.json:506`, `docs/design/architecture/05-building-block-view.md:252`, `gate-logs/T4-contribution.log:10`). |
| T5 Judgment | NEEDS-HUMAN | Choose rejection at Complete versus restricting part numbers at UploadPart — the brief leaves this policy unapproved, and it determines whether clients can accumulate a session that cannot publish (`brief.md:97`, `crates/core/src/multipart.rs:4950`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Accept the demonstrated core behavior as fit for this slice — the real-store tests exercise publication and recovery, while S3 integration remains absent pending #670 (`brief.md:83`, `crates/core/src/multipart/complete.rs:292`). |

No builder-fixable defect was found. The human decisions concern the refusal policy and fitness of the functionality implemented in core. Source citations resolve under `$PDCA_TARGET` (`./target`); brief, gate-log and reviewer-evidence citations resolve in this review directory. The disposable target was usable, and its original tracked diff and new-test hash were unchanged after the stash/pop rerun.

Independent verification used `cargo +1.96.0 test -p wyrd-core --test multipart_complete_value_ceiling --offline`, with only the production/documentation changes stashed for the red run. The failures were I2, I3's oversized stored value, and I4's concurrent addition. Restoring the changes passed all eight tests. The existing `multipart_complete` and `multipart_retire_obligation` suites passed 113 additional tests. `cargo clippy --offline -p wyrd-core --tests`, `cargo fmt --all -- --check`, `typos`, the docs linter and `git diff --check` also passed. The brief's external dependency, `typos`, was present and exercised (`brief.md:76`).

Mutation verification independently reproduced the frozen result: **9 caught, 7 unviable, 0 survivors** across 16 mutants (`reviewer-mutants-capped.log:20`, `gate-logs/C5-mutants.log:13`). The rerun used `cargo mutants --in-diff ../patch.diff -p wyrd-core --test-package wyrd-core --cap-lints true -C=--offline --cargo-test-arg=--test --cargo-test-arg=multipart_complete_value_ceiling`. Capping lints made removal of the size check fail at runtime (`reviewer-mutants-capped.log:12`); the initial uncapped run had classified that mutation as unviable because its constant import became unused.

The remaining frozen gate evidence supports these limited conclusions:

- **C4-ci / host-tikv: PASS from captured execution.** The full workspace gate records spelling, docs rendering, formatting, clippy, build, tests, dependency checks and DST, ending successfully (`gate-logs/C4-ci.log:11`, `gate-logs/C4-ci.log:4870`). Both TiKV feature compilations finish successfully (`gate-logs/host-tikv.log:112`, `gate-logs/host-tikv.log:211`). These full gate invocations were assessed from their logs; the independent reruns are listed above.
- **C4-diff-cov: base-state caveat; coverage unknown.** Its recorded failure is inability to apply the stacked patch to `origin/main`, with no coverage measurement (`gate-logs/C4-diff-cov.log:10`). The prerequisite is explicit (`brief.md:56`), and the supplied target compiled and passed. This is not a C4 patch defect or evidence of uncovered lines.
- **T4-batch-review: PASS as recorded.** Its log reports zero blocking findings; it supplies only the aggregate review result (`gate-logs/T4-batch-review.log:10`). This review independently examined the implementation and tests.
- **T4-contribution: N/A.** The artifacts are drafted after Check; the substantive audit must rerun at publish (`gate-logs/T4-contribution.log:10`). No human clearance is needed for this deferral.

The prior-art check queried `main` history separately for all four affected paths and searched closed PRs by those paths, supplemented by closed multipart work. It confirmed merged Create and Complete-fence peers [#908](https://github.com/getwyrd/wyrd/pull/908) and [#1032](https://github.com/getwyrd/wyrd/pull/1032), inspected the abandoned [#636](https://github.com/getwyrd/wyrd/issues/636) and the sparse-value finding in [#842](https://github.com/getwyrd/wyrd/issues/842), and found no competing landed value-ceiling fix. Query results and verified PR file lists are preserved in `reviewer-prior-art.json:3`. Settled deferrals were not reopened. No `INTEGRATION.md` was supplied or found in the target.

<!-- pdca:leaf-complete -->

### Advisory — adversary

# Adversarial review — #1011 (Complete refuses part sets past the value ceiling)

**Verdict: I could not refute the fix.** The check measures the same bytes the flip writes, it runs both before and after the fence, and the tests go red for the right reasons. What I found is one test gap and three problems with the gate evidence. None of them shows the code is wrong.

## Findings

- NEEDS-HUMAN [impl] — **No test pins the zero-commit refusal for the unnamed set.** `crates/core/src/multipart/complete.rs:1140` checks both obligations before the fence, but every zero-commit leg (I2 at `crates/core/tests/multipart_complete_value_ceiling.rs:501`, I3-past at `:578`) overflows the *published* `retire:records:` set. The only leg that overflows the *unnamed* `retire:bytes:` set is I4 (`:628`), and that one overflows after the fence. I ran a mutant where the check before the fence measures only the published set (the check after the fence still measures both). All 8 tests still passed. Concrete failing case for that mutant: parts {2} ∪ {1, 3, …, 15,871}, a Complete naming only [2], no part planted. Correct code refuses `SessionTooLarge { bytes: 100_005 }` with zero commits. The mutant fences, refuses after the fence, and releases: 2 commits and the session ends at `Open@2`. That breaks the brief's invariant that a Complete outside the range "refuses before it writes anything". Fix: add that leg and assert `take_sent().is_empty()` and that the session bytes are unchanged, as I3-past does.

- NEEDS-HUMAN [human] — **Diff coverage was never measured, and all green evidence is on a base that is not `origin/main`.** `gate-logs/C4-diff-cov.log:10`: "patch.diff does not apply on origin/main — the bundle is stale". `check-gates.json` still says `overall: pass` because that row does not gate. C4-ci, C4-verify and C5 all ran on the pre-fix base `b74afe3`. The brief says this bundle conflicts with #1010 around the fence (`complete.rs:920-1036`, the code this patch edits). A human should decide which case this is: a stack whose parents (#1008–#1010) have not landed yet (expected), or a base that has gone stale. If it is stale, rebase and re-run the C4 gates before sign-off.

- NEEDS-HUMAN [human] — **The gating review looks too fast to have run.** `gate-logs/T4-batch-review.log:10` reports "0 blocking, 0 recorded-rejected, 0 noise-dropped" after 19.03 s, for three model passes over an 80 KB diff. Zero findings of any kind, not even noise to drop, in 19 s suggests a cached or empty run. Someone should open `review-batch.md` and confirm the three passes actually reviewed this diff before treating T4 as green.

- Gate summary is wrong, but the evidence holds (no action on the fix): `gate-logs/C4-verify.log:51` says "8 test(s) ran red", while `:48` shows 3 failed and 5 passed on the reverted base. The 3 that failed are the ones the brief predicts (I2, I3-past, I4). The 5 that passed are controls (the wrapper probe, I1, I3-inside, I3-at, I4-control), which should pass on the base. Only the count in the summary line is wrong.

## What I tried that did not break it

- **Re-ran the proof.** I built a scratch copy of `$PDCA_TARGET` and ran `cargo test -p wyrd-core --test multipart_complete_value_ceiling`: 8/8 pass. The red leg in `C4-verify.log:20-48` fails on the production path: the base's committer refuses the flip (`Publication(ValueOverCeiling { phase: Flip, … })`, `crates/core/src/staged_publication.rs:747-757`) instead of answering `SessionTooLarge`. That is the defect the brief describes. The test is not checking a copy of production code.
- **The brief's three demonstrated reds, reproduced by mutation:**
  - check only before the fence → I4 fails;
  - check only after the fence → I2 and I3-past fail on "zero commits";
  - bare set measured (value length − 10) → I2, I3-past and I4 fail;
  - published set only → I4 fails.

  The SELF-TEST claim holds. C5 also caught the `>`→`>=` boundary mutant, through the at-ceiling leg (`:560`).
- **Measure vs. write drift.** `past_ceiling` (`complete.rs:1194`) and the flip (`complete.rs:1848`) both call `RetireObligation::value()` (`crates/core/src/multipart.rs:4382-4384`), and the flip installs the exact obligations that validation measured (`HeldFence.retired`, `complete.rs:1024`, `:1836-1837`). The check cannot drift from the stored bytes.
- **Epoch / key drift.** Both validations pass `placed.epoch()` (`complete.rs:961`, `:1011`). That is E+1, the same value as `HeldFence::epoch()` (`:429-431`, `:503-506`), which the removed `FlipPlan` code used. I1 checks the obligation keys at epoch 1.
- **A false refusal before the fence.** While a session is `Open` its frozen set only grows (a part number is replaced, never removed), and the named list is fixed. So a set already too large at the first read cannot fit later. The early refusal is sound.
- **Retry loop.** After a refusal behind the fence, a retry sees the larger set before the fence and is refused with zero commits, so it does not fence/release forever. The attempt is given back (I4 checks this).
- **Other values in the flip.** The root inode is checked by the committer (`staged_publication.rs:910-916`). The `Completed` record is bounded at Create (#894). The two part-set values (≤100 KB each) are far inside the 5 MB batch envelope (`multipart.rs:5490`, `:5507`). The patch adds no new way to cross a limit.
- **"Widest set inside 1–10,000 is 58,909 bytes" (`multipart.rs:3341-3346`).** The most runs possible is 5,000 non-adjacent singletons, which means all odds or all evens. The evens have wider digits and include 10,000. The claim holds, and the value is far under the ceiling either way.
- **Rubric surfaces.** No clock read, `static`, crate root or persisted encoding is added (`value()` is the same `metadata::encode`). The living architecture doc and the `SessionTooLarge` doc are updated in the same diff (docs currency).

<!-- pdca:leaf-complete -->

### Advisory — code-review

- No findings on either advisory lens: no introduced correctness bugs or actionable reuse, simplification, or efficiency issues found. Full-value sizing and reuse of the validated obligations are consistent (`crates/core/src/multipart/complete.rs:1138`, `crates/core/src/multipart/complete.rs:1836`); the tests exercise the exact ceiling and insertion before the fence commits (`crates/core/tests/multipart_complete_value_ceiling.rs:560`, `crates/core/tests/multipart_complete_value_ceiling.rs:628`).

Validation used the frozen gate evidence: CI and all eight regression tests passed; reverting the production changes caused the three expected failures. Diff coverage was not measured because the patch did not apply to the gate's base. No builds were rerun or target files changed.

<!-- pdca:leaf-complete -->

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] T5 Judgment — Choose rejection at Complete versus restricting part numbers at UploadPart — the brief leaves this policy unapproved, and it determines whether clients can accumulate a session that cannot publish (`brief.md:97`, `crates/core/src/multipart.rs:4950`).
- [ ] Validation — fitness-to-purpose — Accept the demonstrated core behavior as fit for this slice — the real-store tests exercise publication and recovery, while S3 integration remains absent pending #670 (`brief.md:83`, `crates/core/src/multipart/complete.rs:292`).
- [ ] **No test pins the zero-commit refusal for the unnamed set.** `crates/core/src/multipart/complete.rs:1140` checks both obligations before the fence, but every zero-commit leg (I2 at `crates/core/tests/multipart_complete_value_ceiling.rs:501`, I3-past at `:578`) overflows the *published* `retire:records:` set. The only leg that overflows the *unnamed* `retire:bytes:` set is I4 (`:628`), and that one overflows after the fence. I ran a mutant where the check before the fence measures only the published set (the check after the fence still measures both). All 8 tests still passed. Concrete failing case for that mutant: parts {2} ∪ {1, 3, …, 15,871}, a Complete naming only [2], no part planted. Correct code refuses `SessionTooLarge { bytes: 100_005 }` with zero commits. The mutant fences, refuses after the fence, and releases: 2 commits and the session ends at `Open@2`. That breaks the brief's invariant that a Complete outside the range "refuses before it writes anything". Fix: add that leg and assert `take_sent().is_empty()` and that the session bytes are unchanged, as I3-past does.
- [ ] **Diff coverage was never measured, and all green evidence is on a base that is not `origin/main`.** `gate-logs/C4-diff-cov.log:10`: "patch.diff does not apply on origin/main — the bundle is stale". `check-gates.json` still says `overall: pass` because that row does not gate. C4-ci, C4-verify and C5 all ran on the pre-fix base `b74afe3`. The brief says this bundle conflicts with #1010 around the fence (`complete.rs:920-1036`, the code this patch edits). A human should decide which case this is: a stack whose parents (#1008–#1010) have not landed yet (expected), or a base that has gone stale. If it is stale, rebase and re-run the C4 gates before sign-off.
- [ ] **The gating review looks too fast to have run.** `gate-logs/T4-batch-review.log:10` reports "0 blocking, 0 recorded-rejected, 0 noise-dropped" after 19.03 s, for three model passes over an 80 KB diff. Zero findings of any kind, not even noise to drop, in 19 s suggests a cached or empty run. Someone should open `review-batch.md` and confirm the three passes actually reviewed this diff before treating T4 as green.

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
- Iteration delta (if iterating): Auto-iterate (round 1): rebuilding for the implementation-level findings — **No test pins the zero-commit refusal for the unnamed set.** `crates/core/src/multipart/complete.rs:1140` checks both obligations before the fence, but every zero-commit leg (I2 at `crates/core/tests/multipart_complete_value_ceiling.rs:501`, I3-past at `:578`) overflows the *published* `retire:records:` set. The only leg that overflows the *unnamed* `retire:bytes:` set is I4 (`:628`), and that one overflows after the fence. I ran a mutant where the check before the fence measures only the published set (the check after the fence still measures both). All 8 tests still passed. Concrete failing case for that mutant: parts {2} ∪ {1, 3, …, 15,871}, a Complete naming only [2], no part planted. Correct code refuses `SessionTooLarge { bytes: 100_005 }` with zero commits. The mutant fences, refuses after the fence, and releases: 2 commits and the session ends at `Open@2`. That breaks the brief's invariant that a Complete outside the range "refuses before it writes anything". Fix: add that leg and assert `take_sent().is_empty()` and that the session bytes are unchanged, as I3-past does.. Deferred, not addressed here: 3 finding(s) that need human judgment, held in deferred-findings.json for the human at handover — not build work
- By / date: auto-iterate / 2026-11-10

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
