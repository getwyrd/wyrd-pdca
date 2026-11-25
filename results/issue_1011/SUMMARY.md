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
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass — run-verify.sh: PASS — red without the fix, green with it (9 test(s) ran red).
- C4 diff coverage: changed lines executed by the patch's tests: fail — diff coverage not measured — patch.diff does not apply on origin/main
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): pass — 16 mutants tested in 72s: 9 caught, 7 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): pass — review-branch: 0 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_1011/review-
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 13.62s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

The #1011 patch correctly refuses multipart Complete part sets above the stored-value ceiling; the refusal policy and fitness-to-purpose remain human decisions.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | I1–I4 define observable size boundaries, zero-commit rejection, abortability, and release after a concurrent addition; the policy choice is explicitly reserved for sign-off (brief.md:20, brief.md:97). |
| C2 Reproduction (red pre-fix) | PASS | Independently stashing only the production fix leaves compiling tests with four semantic failures: oversize obligations produce a stopped publication error instead of the required refusal (reviewer-red.log:192, reviewer-red.log:217). |
| C3 Change | PASS | The four affected files implement the scoped refusal and its tests, with the required living-architecture update; reverse-application and whitespace checks pass on the supplied target (target/crates/core/src/multipart/complete.rs:1138, target/docs/design/architecture/05-building-block-view.md:252, reviewer-reproduction.log:20). |
| C4 Verification (red→green) | PASS | Restoring the fix makes all nine focal tests pass; all 85 existing Complete tests also pass independently; full-CI and diff-coverage caveats are detailed below (reviewer-green-pinned.log:17, reviewer-ci.log:998). |
| C5 Causal adequacy | PASS | Checking the actual obligation values at both validation points prevents the oversized flip, including an unnamed set already too large and one enlarged before fencing; the same obligations reach publication (target/crates/core/src/multipart/complete.rs:961, target/crates/core/src/multipart/complete.rs:1011, target/crates/core/src/multipart/complete.rs:1192, target/crates/core/src/multipart/complete.rs:1836). |
| T1 Structure | PASS | The checked obligations remain owned by the held fence and use the existing MetadataStore seam; the change introduces no clock source, shared global, backend coupling, or new transaction path (target/crates/core/src/multipart/complete.rs:410, target/crates/core/src/multipart/complete.rs:1154). |
| T2 Shape | PASS | Measurement and storage share the existing payload encoder, preserving serialized identity and the inclusive 100,000-byte boundary; the existing refusal vocabulary and architecture documentation describe the new outcome (target/crates/core/src/multipart.rs:4382, target/crates/core/src/multipart.rs:4950, target/crates/core/tests/multipart_complete_value_ceiling.rs:562). |
| T3 Runtime | PASS | Client rejection preserves session usability: existing oversize sets send zero commits with identical session bytes, while raced overflow releases to Open@2 with attempts restored; the regressions execute these outcomes (target/crates/core/tests/multipart_complete_value_ceiling.rs:513, target/crates/core/tests/multipart_complete_value_ceiling.rs:638, target/crates/core/tests/multipart_complete_value_ceiling.rs:691). |
| T4 Contribution | PASS | The independent audit covers merged history and closed/rejected work by all four affected paths; the prior missing audit is discharged; contribution-artifact inspection is N/A until the mandatory publish re-run (reviewer-prior-art.md:7, reviewer-prior-art.md:12, reviewer-prior-art.md:16, gate-logs/T4-contribution.log:10). |
| T5 Judgment | NEEDS-HUMAN | Choose refusal at Complete using SessionTooLarge versus preventing out-of-range part admission at UploadPart — this changes the core client contract and the brief says the human has not ratified that choice (brief.md:97, target/crates/core/src/multipart.rs:4950). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Accept the demonstrated core-only behavior as sufficient for this slice — tests establish boundary handling and recoverability, while end-user S3 behavior remains outside this patch until #670 supplies the production caller (brief.md:83, target/crates/core/src/multipart/complete.rs:292, reviewer-green-pinned.log:17). |

No builder-fixable defect remains in this review. The previously missing unnamed-set case now asserts both zero commits and byte-identical session state; it fails without the fix and passes with it (target/crates/core/tests/multipart_complete_value_ceiling.rs:676, reviewer-red.log:198, reviewer-green-pinned.log:8). The C5 capability-probe smell-test does not trigger: this is validation of input against a fixed storage limit, with no optional-capability probe or load-time fallback.

| Gate | Disposition | Evidence |
|------|-------------|----------|
| C4-verify | PASS | Independent Rust 1.96.0 run: four failures before the fix, nine passes after restoration; agrees with the frozen log (reviewer-reproduction.log:3, reviewer-reproduction.log:19, gate-logs/C4-verify.log:56). |
| C4-ci | PASS on frozen evidence; local checkout caveat | Frozen full CI passes, including the actual RustSec pin and dependency wall; the local re-run lacks the synthetic checkout's advisory-db gitlink, failing its index assertion and pin audit (gate-logs/C4-ci.log:4227, gate-logs/C4-ci.log:4871, reviewer-ci.log:3687, reviewer-ci.log:3705). |
| C4-diff-cov | N/A — unmeasured | The frozen failure reports that the patch does not apply to origin/main, so it provides no coverage measurement; this is a stacked-base caveat, not evidence of a patch defect (gate-logs/C4-diff-cov.log:10). |
| C5-mutants | PASS | Independent run with capped lints matches the frozen result: 16 mutants, nine caught, seven invalid Rust mutations, zero survivors (reviewer-mutants-capped.log:3, gate-logs/C5-mutants.log:13). |
| T4-batch-review | PASS on frozen evidence | The supplied aggregate log reports zero blocking, rejected, or noise findings; this report independently reviews the code and does not infer a prior-art audit from that aggregate (gate-logs/T4-batch-review.log:10). |
| T4-contribution | N/A | PR-description artifacts are intentionally drafted after Check; the substantive contribution audit must run at publish (gate-logs/T4-contribution.log:10). |
| host-tikv | PASS | Both requested feature-enabled clippy commands pass independently, matching the frozen compilation evidence (reviewer-tikv.log:202, gate-logs/host-tikv.log:211). |

The local full-CI caveat belongs to the supplied checkout: `git ls-files -s .github/advisory-db` returns no entry, and the failing assertion requires that entry (target/xtask/tests/advisory_db.rs:65). The run finished with exactly two failed checks, both caused by that missing gitlink; the simulation suite passed (reviewer-ci.log:4332, reviewer-ci.log:4252). No advisory-db path is changed by this patch. The frozen run actually exercised the dependency wall, so this is not an undischarged dependency of the fix. The brief's external tool, `typos`, also ran independently; docs lint, rendering/link audit, formatting, clippy, and the relevant tests ran locally (reviewer-ci.log:2, reviewer-ci.log:10, reviewer-ci.log:21, reviewer-ci.log:998).

Prior art is recorded in reviewer-prior-art.md and reviewer-prior-art.json. GitHub returned 19, 1, 0, and 49 main-history commits for the four affected paths; all 19 closed/unmerged PR file lists were checked, with no overlaps. Closed-issue searches also found the merged Create/fence precedents and the rejected broad #636 predecessor. This resolves the audit gap named in brief.md:116 without inspecting another local checkout.

All source citations above resolve in the provided `$PDCA_TARGET` copy. The production fix was restored after the red run, and `git apply --reverse --check ../patch.diff` confirms that the supplied patch remains intact (reviewer-reproduction.log:20).

<!-- pdca:leaf-complete -->

### Advisory — adversary

# Adversarial review — #1011 (Complete refuses part sets past the value ceiling)

**Verdict: I could not refute the fix.** I re-ran the red→green myself, ran four mutants that each drop one guard, measured diff coverage by hand (the gate never ran it), and looked for a value the flip writes that the check misses. Nothing broke. Details:

## The evidence (re-run, not taken on trust)

- `crates/core/tests/multipart_complete_value_ceiling.rs:503,580,630,676` — **red→green reproduced.** On a scratch copy of `$PDCA_TARGET`: 9/9 pass with the fix. With `crates/core/src/multipart.rs` and `crates/core/src/multipart/complete.rs` reverted and the test kept, the same 4 legs fail as in `gate-logs/C4-verify.log`: I2, I3-past, I4 and I4-already-past. All four fail with the committer's `Publication(ValueOverCeiling { phase: Flip, … })` after a fence and release, not with `SessionTooLarge`. The test drives the production `complete_upload` over a real `RedbMetadataStore`. The wrapper only adds a ceiling refusal and a one-shot plant (`:288`), so this is not a copy of production logic.
- `crates/core/src/multipart/complete.rs:961,1011,1140,1194` — **the brief's three "demonstrated reds" hold, plus one more.** I built each mutant and ran the file:
  - check only before the fence → I4 fails;
  - check only after the fence → I2, I3-past and I4-already-past fail;
  - after-fence check measures only the published set → I4 fails;
  - check measures the bare set (value − 10 bytes) → I3-past fails, and so do I2 and both I4 legs.
  The round-1 gap (no zero-commit leg for the unnamed set) is closed by `:676`.
- The tests are not circular. Sizes are literal constants. Stored obligations are compared byte-for-byte with a spelling of the range encoding written in the test (`bare_set`), not with the codec's output. The refusals assert the exact `bytes`, the zero-commit legs assert an empty send log, and the session bytes are compared before and after.
- I4's race happens in production too. The part commit only *requires* `mpu:` and never puts it (`crates/core/src/multipart/commit.rs:1199`). So a part can land between Complete's first read and its fence without the fence conflicting, which is the window the plant hook at `:630` uses.

## The fix (inputs I tried)

- `crates/core/src/multipart/complete.rs:1837,1848` — the flip installs `fence.retired`, the same obligations validation measured. There is no second computation that could drift from the one checked. The pre-fence check uses `placed.epoch()` and `HeldFence::epoch()` is `placed.session.epoch()` (`:429-431`), so keys and values match. Only the key depends on the epoch anyway; the value is `{"parts":…}`.
- `crates/core/src/multipart/complete.rs:1195` vs `crates/core/src/staged_publication.rs:750` — both use `> MAX_VALUE_BYTES`, so no off-by-one between this check and the committer's. The at-ceiling leg (`:562`, exactly 100,000 bytes) pins it.
- The flip's other values can't cross the ceiling either. The superseded-prior `{generation}` obligation wraps a flat map capped at `MAX_ROOT_VALUE_BYTES` = 50,000 (`crates/core/src/metadata.rs:602`, `multipart.rs:5385-5397`) or a segment-group reference. The flip batch is far inside the 5 MB `MAX_BATCH_BYTES` envelope even with two obligations near 100 KB. Dropping the `// deferred: #1011` marker on `Stop::Publication` is therefore justified.
- The "widest set inside S3's range is 58,909 bytes" claim (`multipart.rs:3342-3346`, the complete.rs module doc) is correct. With 5,000 isolated runs, the i-th run can only sit at 2i−1 or 2i, so taking every even number is the maximum. Fewer, longer runs cost more slots per byte.
- After the fence, nothing can add a part: every part commit requires `Open@E`, which the fence replaced. So the frozen set the second check measures is the set the flip installs.
- Not raised: Complete has no DST suite (only `crates/core/tests/multipart_complete*.rs` call it). That predates this diff. The after-fence `SessionTooLarge` reuses the existing rejection→release path, so this is not a new destructive path under the rubric's *Test fidelity* rule.

## The gate verdicts

- `gate-logs/C4-diff-cov.log` — **the failing advisory gate hides no gap.** It failed for the third round because `patch.diff` does not apply on `origin/main`, which is expected for a slice stacked on #1008. I measured it myself with `cargo llvm-cov` over `multipart_complete_value_ceiling` + `multipart_complete`: all **55/55** instrumented added production lines run (3 in `multipart.rs`, 52 in `complete.rs`). This should not send the build back to Do again.
- `check-gates.json` C4-verify row — the summary "9 test(s) ran red" is wrong. The log shows 4 failed and 5 passed in the red run; the 5 are controls (I1, I3-inside, I3-at, I4-control, the wrapper probe) that are meant to pass on both sides. This is a harness reporting slip, not a patch defect. The red→green itself is sound.

## For sign-off (input to the open question already in the brief)

- NEEDS-HUMAN [human] — `crates/core/src/multipart.rs:4941` — the reused `SessionTooLarge` describes "a session that does not fit", but for Complete the refusal depends on the **list**, not the session. Concrete case: parts {2} ∪ {1, 3, …, 15,871}. Naming only [2] is refused `SessionTooLarge { bytes: 100_005 }` (the I4-already-past leg, `:676`). Naming all of them publishes: {1,2,3} merges into one run, the published set is 99,999 bytes, and no part is left unnamed. So the S3 error #670 maps this to must not tell the client the upload can only be aborted. This bears on the brief's still-open choice between refusing at Complete with the reused variant and refusing out-of-range part numbers at `UploadPart`. It is not a builder fix (a new `Refusal` variant is out of scope).
<!-- pdca:leaf-complete -->

### Advisory — code-review

- No introduced correctness defects found. Both validation passes check the stored obligation values, and publication uses the obligations validated against the frozen set (`crates/core/src/multipart/complete.rs:961`, `crates/core/src/multipart/complete.rs:1011`, `crates/core/src/multipart/complete.rs:1192`, `crates/core/src/multipart/complete.rs:1848`).
- No actionable reuse, simplification, or efficiency findings. Part-set construction is centralized in `PartRetirement`, and measurement and writing share the obligation encoder (`crates/core/src/multipart/complete.rs:1165`, `crates/core/src/multipart.rs:4382`).

Validation reviewed from frozen gate evidence; tests were not rerun. All nine new tests passed; reverting production code produced four behavioral failures. Mutation testing reported nine caught and seven unviable mutants, with none surviving. The tests cover the exact ceiling, both zero-commit refusal paths, and the concurrent-addition release (`crates/core/tests/multipart_complete_value_ceiling.rs:562`, `crates/core/tests/multipart_complete_value_ceiling.rs:580`, `crates/core/tests/multipart_complete_value_ceiling.rs:630`, `crates/core/tests/multipart_complete_value_ceiling.rs:676`). Diff coverage was not measured because the gate could not apply the patch to `origin/main`.

<!-- pdca:leaf-complete -->

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [x] T5 Judgment — Choose refusal at Complete using SessionTooLarge versus preventing out-of-range part admission at UploadPart — this changes the core client contract and the brief says the human has not ratified that choice (brief.md:97, target/crates/core/src/multipart.rs:4950).
- [x] Validation — fitness-to-purpose — Accept the demonstrated core-only behavior as sufficient for this slice — tests establish boundary handling and recoverability, while end-user S3 behavior remains outside this patch until #670 supplies the production caller (brief.md:83, target/crates/core/src/multipart/complete.rs:292, reviewer-green-pinned.log:17).
- [x] `crates/core/src/multipart.rs:4941` — the reused `SessionTooLarge` describes "a session that does not fit", but for Complete the refusal depends on the **list**, not the session. Concrete case: parts {2} ∪ {1, 3, …, 15,871}. Naming only [2] is refused `SessionTooLarge { bytes: 100_005 }` (the I4-already-past leg, `:676`). Naming all of them publishes: {1,2,3} merges into one run, the published set is 99,999 bytes, and no part is left unnamed. So the S3 error #670 maps this to must not tell the client the upload can only be aborted. This bears on the brief's still-open choice between refusing at Complete with the reused variant and refusing out-of-range part numbers at `UploadPart`. It is not a builder fix (a new `Refusal` variant is out of scope).
- [x] T5 Judgment — Choose rejection at Complete versus restricting part numbers at UploadPart — the brief leaves this policy unapproved, and it determines whether clients can accumulate a session that cannot publish (`brief.md:97`, `crates/core/src/multipart.rs:4950`).
- [x] **Diff coverage was never measured, and all green evidence is on a base that is not `origin/main`.** `gate-logs/C4-diff-cov.log:10`: "patch.diff does not apply on origin/main — the bundle is stale". `check-gates.json` still says `overall: pass` because that row does not gate. C4-ci, C4-verify and C5 all ran on the pre-fix base `b74afe3`. The brief says this bundle conflicts with #1010 around the fence (`complete.rs:920-1036`, the code this patch edits). A human should decide which case this is: a stack whose parents (#1008–#1010) have not landed yet (expected), or a base that has gone stale. If it is stale, rebase and re-run the C4 gates before sign-off.
- [x] **The gating review looks too fast to have run.** `gate-logs/T4-batch-review.log:10` reports "0 blocking, 0 recorded-rejected, 0 noise-dropped" after 19.03 s, for three model passes over an 80 KB diff. Zero findings of any kind, not even noise to drop, in 19 s suggests a cached or empty run. Someone should open `review-batch.md` and confirm the three passes actually reviewed this diff before treating T4 as green.
- [x] **All the evidence was gathered on a base that is not `main`.** C4-diff-cov failed again with "patch.diff does not apply on origin/main" (`gate-logs/C4-diff-cov.log`). Iteration 1 failed the same way, and the rebuild did not change it. So C4-ci, C4-verify, C5 and my own runs were all done against `b74afe3`, and diff coverage was never measured. The brief lists this child as conflicting with #1010 around the fence code (`crates/core/src/multipart/complete.rs:960-1011`), so a rebase could touch exactly the lines this fix depends on. A human should refresh the base, or accept the gap, before sign-off. Sending it back to Do as-is is likely to reproduce the same failure.
- [x] **The T4-batch-review row looks too thin for a gating pass.** It claims 3 review passes over a 1,163-line diff with "0 blocking, 0 recorded-rejected, 0 noise-dropped" in 14.0 s (`gate-logs/T4-batch-review.log`). That is fast for three model reviews. `review-batch.md` is not among my inputs, so I can't tell whether the passes actually ran on this bundle or reused an earlier result. Worth a look before treating the row as a pass. I am not claiming any defect in the patch here.

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
- By / date: Eduard Ralph / 2026-11-10

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
- Object size ceiling (#1011 sign-off): the two-level chunk map caps one object at 512 × 158 chunks ≈ 79 GiB at 1 MiB chunks, far below S3's 5 TiB. Evaluate a third map level (root → segment index → segments → chunks): one extra read only for objects past ~79 GiB, ceiling ~24–40 TiB at 1 MiB chunks. File as an issue.
- `SessionTooLarge` at Complete misleads native-client users (#1011 sign-off): the refusal depends on which parts are named, not the whole session, and a different named set may publish. Consider a dedicated refusal or clearer docs; #670's S3 mapping must not imply abort is the only exit.
- Core multipart docs and tests from #1011 frame the value-ceiling case in S3 terms ("past S3's range"); core is protocol-neutral, so rephrase as format range vs. storage value limit.
- Core's `MAX_PARTS_PER_SESSION` default (10,000, `multipart.rs:5430`) is S3's number living in core; decide whether core's part-count cap should be chosen for Wyrd's own reasons (capacity budget) rather than copied from S3.
