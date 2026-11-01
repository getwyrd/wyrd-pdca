# Result — issue 1007 / multipart-complete-fence

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect: Nothing fences a multipart session for publication. The discontinued #636 patch
  did, and it wedged the session: three malformed Completes made the fourth, correct one answer
  `CompleteAttemptsExhausted`, with the session still `Open` and every part durable. Abort and a
  full re-upload were the only way out. This slice adds the fence phase of
  `CompleteMultipartUpload` as `wyrd-core` functions over `MetadataStore`:
  - answer every non-`Open` state from `complete_answer` (`crates/core/src/multipart.rs:4782`);
  - fence `Open@E → Completing@E+1`, charging `attempts`;
  - validate the named-part list against the frozen part set and the bucket;
  - on a rejection, release the fence (`Completing@E+1 → Open@E+2`) with `attempts` restored,
    since a fence that wrote no segment minted nothing the cap bounds;
  - when the release cannot land, stop with an error that says the fence still stands, never an
    answer.

  A valid request leaves the caller holding the fence, as a value the next slice consumes to
  publish. Nothing here publishes.
- Success criterion: A NEW test file `crates/core/tests/multipart_complete.rs`, over
  `RedbMetadataStore::in_memory()` and a test-local store wrapper that logs every batch and,
  before a chosen commit, can:
  - answer `Conflict` without applying the batch;
  - answer `CommitUnknownResult` (applied or not, with either `may_still_commit` value);
  - answer a definite `Err`;
  - write a key first;
  - fail a read.

  Build the wrapper so later slices can extend it, for example with a hold step. Sessions are
  made by #894's Create. Parts are planted as canonical `part:` / `psum:` records through the key
  and record helpers on the base (`part_key` `multipart.rs:1321`, `psum_key` `:1336`), with no
  staging. Every leg is bounded by a watchdog. It shows all of the following.
  **(E) Answers by state** (`0016:976`). Against a `Completed` record planted with a
  publication, an identical request answers `AlreadyCompleted` with the **recorded** publication
  (same ETag, inode, version), and a different list answers `Refused(NoSuchUpload)`. Against a
  `Completing` session (another publisher's fence), it answers `Refused(OperationAborted)` and
  leaves that session record and its `seg:` range byte-for-byte unchanged (`0016:953-958`). Each
  of these sends zero commits.
  **(D1) Typos.** `MAX_COMPLETE_ATTEMPTS` (`multipart.rs:5200`) requests in a row with invalid
  lists (an absent part, a digest mismatch, part numbers out of order) each answer
  `Refused(InvalidPart { .. })`. After each one, the session is `Open` and `attempts` equals its
  value before that request.
  **(D2) A part replaced between the first read and the fence.** `MAX_COMPLETE_ATTEMPTS` requests
  in a row. Each names a list that is valid against the part set when first read. The wrapper
  re-plants one named part (new digest) just before the fence commits. Each request fences, fails
  against the frozen set, releases, and answers `Refused(InvalidPart { .. })`, with the session
  `Open` and `attempts` restored.
  After D1 and D2, a request with a valid list **fences**: it returns the held fence at
  `Completing@E'+1` with `attempts` one higher, not `CompleteAttemptsExhausted`. After a rejected
  request, an Abort answers `Fenced` (`0016:1033`).
  **(D4) A fence it owns and cannot release.** Set up as D2, so the fence lands with a **definite**
  success and the frozen list is invalid. The wrapper answers every release commit `Conflict`
  without applying it and without changing the session (the TiKV lock case,
  `crates/core/src/multipart/abort.rs:245-247`). No unknown outcome is injected. The request
  returns the **stopped** error:
  - it names the upload and the epoch of the fence it still holds (`E+1`);
  - it says the fence still stands;
  - its cause says the release rounds ran out;
  - it carries no `CommitUnknownResult`.

  The session is `Completing@E+1`, byte-identical to the fence's bytes. The wrapper counts at most
  the stated bound of release commits. A second run answers the release with a definite store
  error: the same stopped error, with that store error as its cause.
  **(H) Bucket deleted** (`0016:1035`). With the bucket record removed while the session is
  open, the request answers `Refused(NoSuchBucket)`, and the session is `Open` again with
  `attempts` restored.
  **(G0-fence) Unknown outcomes stop with the in-doubt error.** The wrapper answers
  `CommitUnknownResult` on the initial fence (once applied, once not) and on a release. Each time
  the request returns the in-doubt error carrying that outcome and its `may_still_commit`. It never
  returns a `CompleteOutcome`, and it sends no further commit. In a further run, the first read of
  `mpu:<id>` fails with a store error, and the request returns an error, never an answer.
- Repo + branch target: getwyrd/wyrd @ main
- Scope: in `crates/core/src/multipart/complete.rs` (a new module beside `abort.rs`):
  - the request type (named `(part, digest)` list, target key);
  - the entry read and the answers by state;
  - the fence commit with the attempts budget;
  - the bounded read of the frozen `psum:` / `part:` ranges and the validation (named parts
    present with the named digest, numbers strictly ascending, bucket present, recording the
    `bucket:` marker bytes read);
  - the held-fence value returned for a valid request (carrying the frozen set, the epoch and
    the marker bytes for the next slice);
  - the release (`Completing@E+1 → Open@E+2`, `attempts` restored, bounded rounds);
  - the error type with its two arms, **in doubt** and **stopped**;
  - the crate-private session transitions it needs (modelled on `SessionRecord::open`,
    `multipart.rs:2360`).

  Out of scope:
  - assembly, any publication, any flip or any `retire:` obligation (child-2,
    child-3);
  - the release's `retire:records:{seg}` obligation and the charging rule for fences that wrote
    segments (child-3);
  - any re-read or resume after an unknown outcome (child-4);
  - the value-ceiling check (child-5);
  - the reaper (#625), the drain (#659), the S3 verb (#670), DST (#660);
  - any edit to 0016 or an ADR, `Cargo.lock` or `deny*.toml`.

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: new-feature
- Confidence: medium
- Recommendation: (set by Do)

## 3. Correctness (Check — chain)
- C1 Spec: none — brief.md
- C2 Reproduction (red pre-fix): none — (no gate configured)
- C3 Change: none — patch.diff
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): pass — xtask ci: all checks passed
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: unverifiable —                why this slice has no isolable red (the cargo output is above).
- C4 diff coverage: changed lines executed by the patch's tests: pass — diff coverage 98.9% — 373 of 377 instrumentable changed lines executed (floor 80%); 377 of 1028 changed lines were instr
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): pass — 70 mutants tested in 7m: 46 caught, 24 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): pass — review-branch: 0 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_1007/review-
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 28.21s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

No implementation defect found in #1007's multipart Complete fence, validation and budget-restoring release; the declared verification exception and public API fitness remain human decisions.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The scoped fence-only contract and revised best-effort release policy have explicit, testable outcomes; publication and production wiring remain separate work (`brief.md:25`, `brief.md:201`; `crates/core/src/multipart/complete.rs:485`). |
| C2 Reproduction (red pre-fix) | N/A | This is a new API: keeping the tests while stashing production changes fails on the missing module before any test executes, so it establishes no behavioral red (`crates/core/tests/multipart_complete.rs:50`; `review-red.log:2`). |
| C3 Change | PASS | The five affected files stay within the fence slice, preserve existing record identity, and update the living architecture; reverse-apply checking confirms the target matches the bundle (`crates/core/src/multipart.rs:2626`, `crates/core/src/multipart.rs:2655`, `docs/design/architecture/05-building-block-view.md:248`). |
| C4 Verification (red→green) | NEEDS-HUMAN | Accept or reject the declared new-API exception to behavioral red→green evidence — the independent stashed leg stops at compilation, while restoration passes all 35 tests (`brief.md:133`; `review-red.log:2`; `review-green-restored.log:42`). |
| C5 Causal adequacy | PASS | Repeated post-fence rejection restores the exact prior budget and leaves Abort usable; another publisher's fence remains untouched, and unknown outcomes stop further commits. This addresses the wedge rather than masking it with a capability probe (`crates/core/tests/multipart_complete.rs:932`, `crates/core/tests/multipart_complete.rs:1045`, `crates/core/tests/multipart_complete.rs:1525`, `crates/core/src/multipart/complete.rs:614`). |
| T1 Structure | PASS | The implementation uses the narrow MetadataStore seam, shared state answers and fingerprint validation; the owned fence cannot be constructed or cloned by callers (`crates/core/src/multipart/complete.rs:93`, `crates/core/src/multipart/complete.rs:190`, `crates/core/src/multipart/complete.rs:672`). |
| T2 Shape | PASS | Exact-byte preconditions protect both transitions, malformed summaries surface as errors, and the error vocabulary distinguishes released, standing and unknown fences (`crates/core/src/multipart/complete.rs:316`, `crates/core/src/multipart/complete.rs:600`, `crates/core/src/multipart/complete.rs:785`, `crates/core/src/multipart/complete.rs:859`). |
| T3 Runtime | PASS | Commit rounds and ordered page traversal are bounded; backend deadlines supply per-operation termination, and the request's clock must match the session. Independent real-redb tests complete under watchdogs (`crates/core/src/multipart/complete.rs:528`, `crates/core/src/multipart/complete.rs:569`, `crates/traits/src/lib.rs:1334`; `review-green-restored.log:42`). |
| T4 Contribution | N/A | Contribution artifacts are deliberately absent at Check; the substantive audit must rerun at publish, so the deferred row needs no human clearance (`gate-logs/T4-contribution.log:10`). Other T4 evidence and the path-based prior-art check are assessed below. |
| T5 Judgment | PASS | The carry-forward's nonzero budgets, second-page races, malformed pages, fault-release outcomes and truthful error wording are exercised; mutation evidence reports 46 caught and 24 unviable, with none missed. The explicit DST deferral remains settled (`crates/core/tests/multipart_complete.rs:1525`, `crates/core/tests/multipart_complete.rs:1633`, `crates/core/tests/multipart_complete.rs:1683`, `crates/core/tests/multipart_complete.rs:1811`, `crates/core/src/multipart/complete.rs:480`; `gate-logs/C5-mutants.log:13`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Confirm that exposing the public held-fence API is suitable for this first slice — tests exercise it, but publication and the S3 caller arrive later, so end-user completion is not demonstrated here (`brief.md:188`; `crates/core/src/multipart/complete.rs:190`, `crates/core/src/multipart/complete.rs:485`). |

Source citations resolve against `$PDCA_TARGET` (`target/`); brief and evidence citations resolve in this review directory. The target was readable, compiled successfully, and passed `git apply --reverse --check ../patch.diff`. The production-only stash was popped, the original five-file patch restored, and no reviewer implementation changes remain.

Independent verification supports the scoped behavior. `cargo test --offline --locked -p wyrd-core --test multipart_complete` passed 35/35 before and after the stashed production leg. The stashed leg retained the tests and failed with E0432, matching the frozen gate's UNVERIFIABLE result rather than proving a behavioral failure (`review-red.log:2`; `gate-logs/C4-verify.log:16`). `typos`, `python3 docs/publishing/tools/lint_docs.py`, `cargo fmt --all -- --check`, and `git diff --check` also passed. The brief's external dependency, `typos`, was present and exercised. Searches found no production call to `fence_complete`; its executable callers are confined to the new integration test (`crates/core/tests/multipart_complete.rs:533`).

The remaining gate verdicts rest on their frozen captured output, not an independent rerun of the instance-scoped wrappers:

- C4-ci reports workspace formatting, clippy, build/tests, dependency checks, conformance and DST success, ending with “all checks passed” (`gate-logs/C4-ci.log:4535`). Its prose checks ran, including `typos` and docs rendering (`gate-logs/C4-ci.log:11`).
- C4-diff-cov reports 373/377 instrumentable changed lines covered (98.9%); 651 changed production lines were unscored, not counted as covered (`gate-logs/C4-diff-cov.log:78`).
- C5-mutants reports a passing unmutated baseline, 46 caught mutants and 24 unviable mutants; unviable mutants are not successful behavioral kills (`gate-logs/C5-mutants.log:11`).
- T4-batch-review reports zero blocking findings; this report supplies the independent code review (`gate-logs/T4-batch-review.log:10`). The TiKV log shows both requested feature builds completing; it demonstrates compilation, not a live TiKV deployment (`gate-logs/host-tikv.log:1`, `gate-logs/host-tikv.log:211`).
- T4-contribution is N/A at Check and remains owed at publish (`gate-logs/T4-contribution.log:10`).

Prior art was checked by all five affected paths against merged history at the supplied base, then against the exact changed-file lists of 38 closed multipart PRs. The prerequisite work in #799, #847, #908 and #909 is merged; neither new file exists in that base's history. The rejected #636 disposition confirms the monolithic attempt was discontinued and split; this slice does not reintroduce its publication/reclamation routing. Search scope and results are recorded in `review-prior-art.md:7` and `review-prior-art.md:15`, with raw API evidence in `review-prior-art.json`. The outside-checkout archived patch was not accessed.

<!-- pdca:leaf-complete -->

### Advisory — adversary

# Adversarial review: #1007, Complete fence

**Result:** partly refuted. The fence, the release and the budget held up against every attack below. A deleted and re-created bucket gets past the bucket check, and several behaviours have no test that fails when they break. All reruns used a scratch copy of the target, and nothing was changed at `$PDCA_TARGET`.

## Findings

- NEEDS-HUMAN [impl] — `crates/core/src/multipart/complete.rs:732` never compares the bucket marker read before the fence with the one read behind it. Failing case, run in scratch: the wrapper re-puts `bucket:photos` with different bytes just before the fence commits, and `fence_complete` returns `Held` carrying the new marker. Create treats that same change as `NoSuchBucket`, because an immutable marker that changed belongs to a bucket deleted and created again (`crates/core/src/multipart/create.rs:226-228`, `:408-410`). Fix: keep the first marker, reject a mismatch behind the fence, release, and add this test leg.
- NEEDS-HUMAN [human] — `crates/core/src/multipart/complete.rs:732` has the same gap over the session's whole life, and the fix above does not close it. Failing case, run in scratch: Create under marker M, re-put the marker as M2, then send a valid Complete, and the answer is `Held` with M2. The session record stores no marker, and at Alpha the target is root plus `<bucket>/<key>` (`crates/core/src/multipart/create.rs:53-55`), so the target check at `complete.rs:677` cannot tell the two buckets apart. The next slices would then publish an upload from the deleted bucket into the new one. `0016:1035` only states the plain delete case. The human must decide whether a session records its marker (a new persisted field) or bucket deletion (#511) handles open sessions. The `// deferred: #511` marker at `create.rs:701` covers only the deletion fence key, so it does not settle this.
- NEEDS-HUMAN [human] — `crates/core/src/multipart/complete.rs:36-40` and `:735` read only `psum:`, but the brief's Scope asks for "the frozen `psum:` / `part:` ranges" (brief line 107). Failing case, run in scratch: delete `part:` for named part 2 and leave its `psum:`, and the request returns `Held`. The part commit writes both records in one batch (`0016:660`), so this needs an already-torn store, and the module doc gives a sound reason (chunk lists can reach tens of megabytes). Still, the rubric's "absent entries" class applies and the choice departs from the brief's text. Please confirm it, and make sure the next slice errors on a missing `part:` record rather than skipping it.
- NEEDS-HUMAN [impl] — `crates/core/src/multipart/complete.rs:123`: setting `RELEASE_ROUNDS = 1` passes all 35 tests. D4 compares against the constant by name, and no test has a release lose a `Conflict` and then land. The shipped code does handle this: my scratch test (conflict once, then land) passed with 3 commits and the session `Open`. Nothing stops a later edit from removing the retry, and that would leave a fence standing on a single TiKV lock conflict. Add that leg, and pin the bound to a literal.
- NEEDS-HUMAN [impl] — `crates/core/src/multipart/complete.rs:677`: dropping the `session.parent() != request.parent` half passes all 35 tests. The only "another target" leg changes `object` (`crates/core/tests/multipart_complete.rs:893-899`). Add a leg with a different `parent`.
- NEEDS-HUMAN [impl] — `crates/core/src/multipart/complete.rs:584`: restoring the `attempts` from the first read, rather than the read the winning fence was built on, passes all 35 tests. The shipped code is right, because `opened` is rebound on each round. The untested case is: the fence loses to a session moved to a later `Open` with a different `attempts`, the retry fences, and the request is then rejected. A regression here would hand back an attempt a rival had been charged. The nonzero-budget tests (`multipart_complete.rs:1525-1590`) never retry.
- NEEDS-HUMAN [impl] — `crates/core/src/multipart/complete.rs:722-725` and `:736`: the doc promises the bucket is checked first and the first bad part is the one reported, but neither has a test. Walking the list in reverse, or reading the marker after the parts, both pass all 35 tests. The failing inputs are a list with two bad parts, and a deleted bucket combined with a bad part. This matters little, because 0016 does not fix either order. Test it or drop the promise.
- NEEDS-HUMAN [impl] — The docs do not match the code in three places:
  - `crates/core/src/multipart/complete.rs:59-60`, `:523` and `docs/design/architecture/05-building-block-view.md:248` say a store fault behind the fence is an error "whatever happens next". `moved()` at `complete.rs:915-920` returns an answer on that path, and `a_fault_behind_the_fence_whose_release_finds_the_session_moved` expects it. The answer itself follows the sign-off's "Conflict -> the existing moved() path", so only the wording is wrong. `complete.rs:70` already describes it correctly.
  - `complete.rs:73` says each error keeps the fault as its cause, but the in-doubt arm at `complete.rs:882-885` drops it, so an operator never sees the scan fault that started the release.
  - `crates/core/src/multipart/abort.rs:53-54` says a client sending invalid Completes can move the session under an Abort, but `complete.rs:30-32` answers those with no commit. Only a list that passes the first check and is rejected behind the fence can move it.
- NEEDS-HUMAN [human] — Two green rows in `check-gates.json` prove less than they appear to:
  - `C5-mutants` reports 46 caught and none missed, yet it missed the gaps above, because it did not try these edits (changed constants, a removed `||` operand, reordered reads). Read it as "no missed mutants among those tried", which does not meet carry-forward item 8's aim on its own.
  - `T4-batch-review` reports 0 blocking, 0 recorded-rejected and 0 noise-dropped in 35.76 s, for three codex passes over a 154 KB diff (`gate-logs/T4-batch-review.log`). Not even one noise-dropped item is surprisingly clean. `review-batch.md` is outside my inputs, so I can't say the passes were empty. Open it at sign-off before relying on this row.

## Evidence

- **Red→green:** `C4-verify` is UNVERIFIABLE (exit 77) because the module is new and the test can't compile without it, as the brief declared up front. That is not a refutation. The baseline here was 35 passed on the production code.
- **The test path:** the tests call the real `fence_complete` over a real redb store, through a wrapper that forwards to it. I found no parallel copy of the production logic.

The brief's required reds, reproduced with temporary edits (all tests failed as they should):

| Temporary edit | Failing test |
|---|---|
| attempts not restored | `a_rejected_request_restores_the_attempts_it_read` and others |
| session left `Completing` after a rejection | same group |
| another publisher's `Completing` fence treated as fenceable | `e_another_publishers_completing_fence_is_refused_and_left_untouched` |
| definite release failure answered with its refusal | `d4_a_fence_it_owns_and_cannot_release_stops_with_the_fence_standing` |
| definite release failure reported as in doubt | same D4 test |
| unknown fence treated as success | `g0_an_unknown_fence_outcome_returns_the_in_doubt_error_and_nothing_more` |

## Attempted, could not refute

- **Lost `require`:** removing it from the fence or the release fails 12 and 11 tests.
- **Other broken versions, all caught:** a wrong stamp, wrong held bytes, a constant charge, a wrong marker, a hard-coded `may_still_commit`, and the paging checks.
- **C-1 on every return path:** I found no answer that leaves this Complete's own fence standing. A moved session is never overwritten, because each release requires the fence's exact bytes and epochs only rise.
- **`scan_page` contract:** using the last key as the cursor and treating a repeated or lower key as a fault both fit clauses 2-4 of `crates/traits/src/lib.rs`. Part keys are fixed width, so byte order matches number order.
- **`downcast_ref::<CommitUnknownResult>` without walking the source chain:** this is the documented contract (`crates/traits/src/lib.rs:216`), and Abort and Create do the same.
- **Epoch and attempts ceilings:** these stop before any write, because the release record is built with the fence.
- **Not raised:** Tier-0 DST coverage is deferred to #660 by the marker at `complete.rs:480`, so that is settled. The value ceiling (child-5) and recovery after an unknown outcome (child-4) are out of scope.

<!-- pdca:leaf-complete -->

### Advisory — code-review

- NEEDS-HUMAN [impl] — `crates/core/src/multipart/complete.rs:582`: **Refresh the fence timestamp immediately before submission.** `request.fenced_at_millis` is fixed before the initial session read, the paged prevalidation, and every conflict retry. A slow precheck or several retries can therefore consume `W_completing` before the fence even lands, making a newly acquired fence immediately eligible for rollback under decision 3 (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:1005`). The caller cannot refresh this immutable request between the function's internal awaits. Sample the session's clock through an injected seam after prevalidation, immediately before each fence submission. Add a controlled-clock regression that advances time during prevalidation and a conflict retry and checks the persisted stamp.

- NEEDS-HUMAN [impl] — `crates/core/src/multipart/complete.rs:596`: **Check the retry budget before another full validation.** When all `FENCE_ROUNDS` commits conflict and the session remains `Open`, the loop calls `validate` `FENCE_ROUNDS + 1` times before returning `FenceRoundsSpent`. The last validation cannot lead to a commit, yet reads the bucket and scans/decodes the entire part set—79 extra page requests for 10,000 parts. Move the exhaustion check ahead of `validate`, retaining the state-first answers, and extend the conflict-exhaustion test to assert the scan count.

Reviewed against the read-only target and frozen gate evidence; no builds or tests were rerun. These findings are advisory.
<!-- pdca:leaf-complete -->

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C4 Verification (red→green) — Accept or reject the declared new-API exception to behavioral red→green evidence — the independent stashed leg stops at compilation, while restoration passes all 35 tests (`brief.md:133`; `review-red.log:2`; `review-green-restored.log:42`).
- [ ] Validation — fitness-to-purpose — Confirm that exposing the public held-fence API is suitable for this first slice — tests exercise it, but publication and the S3 caller arrive later, so end-user completion is not demonstrated here (`brief.md:188`; `crates/core/src/multipart/complete.rs:190`, `crates/core/src/multipart/complete.rs:485`).
- [ ] `crates/core/src/multipart/complete.rs:732` never compares the bucket marker read before the fence with the one read behind it. Failing case, run in scratch: the wrapper re-puts `bucket:photos` with different bytes just before the fence commits, and `fence_complete` returns `Held` carrying the new marker. Create treats that same change as `NoSuchBucket`, because an immutable marker that changed belongs to a bucket deleted and created again (`crates/core/src/multipart/create.rs:226-228`, `:408-410`). Fix: keep the first marker, reject a mismatch behind the fence, release, and add this test leg.
- [ ] `crates/core/src/multipart/complete.rs:732` has the same gap over the session's whole life, and the fix above does not close it. Failing case, run in scratch: Create under marker M, re-put the marker as M2, then send a valid Complete, and the answer is `Held` with M2. The session record stores no marker, and at Alpha the target is root plus `<bucket>/<key>` (`crates/core/src/multipart/create.rs:53-55`), so the target check at `complete.rs:677` cannot tell the two buckets apart. The next slices would then publish an upload from the deleted bucket into the new one. `0016:1035` only states the plain delete case. The human must decide whether a session records its marker (a new persisted field) or bucket deletion (#511) handles open sessions. The `// deferred: #511` marker at `create.rs:701` covers only the deletion fence key, so it does not settle this.
- [ ] `crates/core/src/multipart/complete.rs:36-40` and `:735` read only `psum:`, but the brief's Scope asks for "the frozen `psum:` / `part:` ranges" (brief line 107). Failing case, run in scratch: delete `part:` for named part 2 and leave its `psum:`, and the request returns `Held`. The part commit writes both records in one batch (`0016:660`), so this needs an already-torn store, and the module doc gives a sound reason (chunk lists can reach tens of megabytes). Still, the rubric's "absent entries" class applies and the choice departs from the brief's text. Please confirm it, and make sure the next slice errors on a missing `part:` record rather than skipping it.
- [ ] `crates/core/src/multipart/complete.rs:123`: setting `RELEASE_ROUNDS = 1` passes all 35 tests. D4 compares against the constant by name, and no test has a release lose a `Conflict` and then land. The shipped code does handle this: my scratch test (conflict once, then land) passed with 3 commits and the session `Open`. Nothing stops a later edit from removing the retry, and that would leave a fence standing on a single TiKV lock conflict. Add that leg, and pin the bound to a literal.
- [ ] `crates/core/src/multipart/complete.rs:677`: dropping the `session.parent() != request.parent` half passes all 35 tests. The only "another target" leg changes `object` (`crates/core/tests/multipart_complete.rs:893-899`). Add a leg with a different `parent`.
- [ ] `crates/core/src/multipart/complete.rs:584`: restoring the `attempts` from the first read, rather than the read the winning fence was built on, passes all 35 tests. The shipped code is right, because `opened` is rebound on each round. The untested case is: the fence loses to a session moved to a later `Open` with a different `attempts`, the retry fences, and the request is then rejected. A regression here would hand back an attempt a rival had been charged. The nonzero-budget tests (`multipart_complete.rs:1525-1590`) never retry.
- [ ] `crates/core/src/multipart/complete.rs:722-725` and `:736`: the doc promises the bucket is checked first and the first bad part is the one reported, but neither has a test. Walking the list in reverse, or reading the marker after the parts, both pass all 35 tests. The failing inputs are a list with two bad parts, and a deleted bucket combined with a bad part. This matters little, because 0016 does not fix either order. Test it or drop the promise.
- [ ] The docs do not match the code in three places:
- [ ] Two green rows in `check-gates.json` prove less than they appear to:
- [ ] `crates/core/src/multipart/complete.rs:582`: **Refresh the fence timestamp immediately before submission.** `request.fenced_at_millis` is fixed before the initial session read, the paged prevalidation, and every conflict retry. A slow precheck or several retries can therefore consume `W_completing` before the fence even lands, making a newly acquired fence immediately eligible for rollback under decision 3 (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:1005`). The caller cannot refresh this immutable request between the function's internal awaits. Sample the session's clock through an injected seam after prevalidation, immediately before each fence submission. Add a controlled-clock regression that advances time during prevalidation and a conflict retry and checks the persisted stamp.
- [ ] `crates/core/src/multipart/complete.rs:596`: **Check the retry budget before another full validation.** When all `FENCE_ROUNDS` commits conflict and the session remains `Open`, the loop calls `validate` `FENCE_ROUNDS + 1` times before returning `FenceRoundsSpent`. The last validation cannot lead to a commit, yet reads the bucket and scans/decodes the entire part set—79 extra page requests for 10,000 parts. Move the exhaustion check ahead of `validate`, retaining the state-first answers, and extend the conflict-exhaustion test to assert the scan count.
- [ ] C4 per-fix red->green: this patch's test red pre-fix, green post-fix unverifiable —                why this slice has no isolable red (the cargo output is above).
- [ ] size backstop — this slice is behaving oversized: patch is 151 KB (threshold 100 KB). Recommend answering `iterate-plan` at sign-off and authoring the split in the re-plan (`pdca split`), rather than `iterate-do`: a slice that is too big yields implementation-shaped findings every round, and splitting authors briefs, which is Plan's beat.

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
- Iteration delta (if iterating): Human decision at sign-off: rebuild this slice to close the test gaps and doc mismatches below. Do not split it. No production result was found wrong, so keep the fence, validation and release logic as they are unless a new test proves otherwise. Test gaps. Each new leg must fail under the edit named: 1. complete.rs:123: add a leg where a release loses one Conflict and then lands, and pin the release bound to a number written in the test, not the constant. RELEASE_ROUNDS = 1 must fail. This is the TiKV lock case. 2. complete.rs:677: add an "another target" leg with a different parent. Dropping the session.parent() != request.parent check must fail. 3. complete.rs:584: add a leg where the fence loses to a session moved to a later Open with a different attempts, the retry fences, and the request is then rejected. Restoring attempts from the first read must fail. 4. complete.rs:722-725 and :736: test that the bucket is checked first and that the first bad part is the one reported, using a list with two bad parts and a deleted bucket combined with a bad part. Alternatively, drop that promise from the doc. Doc mismatches: 5. complete.rs:59-60, :523 and docs/design/architecture/05-building-block-view.md:248 say a store fault behind the fence is an error "whatever happens next", but moved() (complete.rs:915-920) can return an answer on that path. Keep the behaviour and correct the wording. 6. complete.rs:73 says each error keeps the fault as its cause, but the in-doubt arm (complete.rs:882-885) drops it. Keep the scan fault visible to an operator, or correct the doc. 7. abort.rs:53-54 says a client sending invalid Completes can move the session under an Abort, but complete.rs:30-32 answers those with no commit. Only a list that passes the first check and is rejected behind the fence can move it. Correct the doc. Not part of this rebuild: the deleted-and-recreated bucket question is still open with the human. Do not add a persisted session field for it here.
- By / date: Eduard Ralph / 2026-10-30

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
