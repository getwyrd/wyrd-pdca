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
- C4 diff coverage: changed lines executed by the patch's tests: pass — diff coverage 92.1% — 325 of 353 instrumentable changed lines executed (floor 80%); 353 of 935 changed lines were instru
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): fail — 68 mutants tested in 6m: 4 missed, 42 caught, 22 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): pass — review-branch: 0 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_1007/review-
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 29.76s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Review of #1007’s multipart Complete fence, validation and budget-preserving release: the requested scenarios pass, with test-coverage work and explicit sign-off decisions remaining.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The fence-only boundary and answer-versus-error invariant are concrete and independently checkable against the state table and rejected-request rule; `target/docs/design/proposals/draft/0016-multipart-commit-protocol.md:976`, `brief.md:169`. |
| C2 Reproduction (red pre-fix) | N/A | This is a new API absent from the baseline; reverting production code prevents the retained test from compiling, so it establishes no behavioral red for the historical wedge; `reviewer-red.log:2`, `target/crates/core/tests/multipart_complete.rs:44`. |
| C3 Change | PASS | The change stays within fencing and release, preserves other publishers’ ownership, and updates the living architecture; no production caller or publication was added; `target/crates/core/src/multipart/complete.rs:523`, `target/crates/core/src/multipart/complete.rs:768`, `target/docs/design/architecture/05-building-block-view.md:248`. |
| C4 Verification (red→green) | NEEDS-HUMAN | Accept or reject the declared net-new verification exception — the independent green has 16 passing tests but the reverted leg only fails compilation; the full local CI also encounters a missing target-copy gitlink, while the frozen CI evidence passed; `reviewer-green.log:23`, `reviewer-red.log:2`, `gate-logs/C4-verify.log:24`, `reviewer-target-caveat.json:1`. |
| C5 Causal adequacy | NEEDS-HUMAN [impl] | Add discriminators for a nonzero prior attempt budget and a frozen set crossing a page boundary — zero-only assertions let a wrong budget accessor survive, and continuation never executes; `target/crates/core/tests/multipart_complete.rs:846`, `target/crates/core/src/multipart/complete.rs:733`, `gate-logs/C5-mutants.log:13`, `gate-logs/C4-diff-cov.log:97`. |
| T1 Structure | PASS | Store access stays behind MetadataStore, session transitions remain crate-private, and the new module composes the existing answer table without changing dependency direction; `target/crates/core/src/multipart/complete.rs:501`, `target/crates/core/src/multipart/complete.rs:626`, `target/crates/core/src/multipart.rs:2626`. |
| T2 Shape | PASS | State changes retain the existing record identity and exact-byte CAS discipline; malformed stored records remain errors and request ordering is explicit; `target/crates/core/src/multipart.rs:2643`, `target/crates/core/src/multipart/complete.rs:522`, `target/crates/core/src/multipart/complete.rs:639`, `target/crates/core/tests/multipart_complete.rs:609`. |
| T3 Runtime | PASS | Retry counts and ordered page progress bound work under the store’s termination contract; timestamps must belong to the session’s clock and unknown commits stop further work; `target/crates/core/src/multipart/complete.rs:526`, `target/crates/core/src/multipart/complete.rs:571`, `target/crates/core/src/multipart/complete.rs:715`, `target/crates/traits/src/lib.rs:1334`. |
| T4 Contribution | PASS | Affected-path history and closed-work checks confirm the cited predecessors and #636’s discontinued scope; living docs are current, the recorded batch review reports zero findings, and the contribution-artifact audit is N/A until publish; `reviewer-prior-art.json:2`, `reviewer-prior-art.json:460`, `gate-logs/T4-batch-review.log:10`, `gate-logs/T4-contribution.log:10`. |
| T5 Judgment | NEEDS-HUMAN | Confirm the temporary public held-fence API before the publication consumer exists — this is the brief’s explicit API-boundary decision, and dropping the value leaves recovery to the later reaper; `brief.md:188`, `target/crates/core/src/multipart/complete.rs:180`. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether this fence-only slice is fit to accept as the foundation for the publication slices — the evidence exercises a real in-memory store but intentionally proves neither publication nor an S3 user workflow; `brief.md:99`, `target/crates/core/src/multipart/complete.rs:449`, `reviewer-green.log:23`. |

The implementation concern is test discrimination, not an observed wrong production result. The fence and release use exact session bytes, restore the saved attempt count on rejection, preserve another publisher’s fence, and return typed stopped/in-doubt errors in the exercised scenarios. No capability probe or runtime fallback papers over a load-time cause. The seeded DST work is explicitly deferred to #660 at `target/crates/core/src/multipart/complete.rs:444`; that settled deferral is not raised again.

- **Preserve already-spent attempts in the handoff test.** Every successful held-fence assertion starts from attempts = 0; the cap test starts at the cap and never returns a held fence. Consequently, replacing `HeldFence::attempts_before()` with constant zero passes the mutation gate (`gate-logs/C5-mutants.log:13`). Seed an Open session with attempts = 1 or MAX_COMPLETE_ATTEMPTS - 1, force D2’s post-fence rejection, assert that release preserves that nonzero count, then fence successfully and assert both the saved count and the incremented session count. This protects the budget value the next slice consumes (`target/crates/core/src/multipart/complete.rs:217`, `target/crates/core/tests/multipart_complete.rs:838`).
- **Exercise continuation with real part records.** The fixtures plant at most three parts (`target/crates/core/tests/multipart_complete.rs:638`), below the 128-record page size (`target/crates/core/src/multipart/complete.rs:112`). The frozen coverage log explicitly misses continuation at line 733 (`gate-logs/C4-diff-cov.log:97`). Use at least 129 canonical part/summary pairs and name a part on the second page; assert the complete frozen set and the rejection/release when that tail part changes before fencing. This makes a prematurely terminated scan observable. The two surviving ordering/range mutants (`gate-logs/C5-mutants.log:15`) additionally expose absent malformed-page tests; those defend against a broken backend contract, not a demonstrated failure of conforming redb. The surviving Display mutant is lower priority because tests already assert the typed doubtful-commit identity (`target/crates/core/tests/multipart_complete.rs:561`).

Independent verification used the harness-owned target only. I stashed the production/doc changes while retaining the new test, ran the test on the reverted tree, restored the stash, and ran it again. The reverted run exits 101 on the missing module; the restored run passes all 16 tests. Reverse-apply checking succeeds and the two added files match patch.diff exactly after restoration. No patch changes were made.

The independent `cargo xtask ci` run completed with exit 1. Formatting, clippy, build, docs rendering/link audit, typos, conformance, static-state/dependency-direction guards and DST passed. Workspace tests stopped at the advisory-db index test, and dependency auditing could not prepare its pin; these are the two failures in `reviewer-ci.log`, both caused by the same target-copy omission.

The full local CI has a target-state caveat: its synthetic Git index omits `.github/advisory-db`, so the index test and advisory-pin preparation fail. Both `git ls-files -s` and `git ls-tree HEAD` return no entry. The actual base commit 376b106248d2ac2e816c5d258609e04de09a425f carries the submodule at 40f5be6fb12ed93fc591bf4ab23b7d793b3f2239, independently checked through the repository API (`reviewer-target-caveat.json:1`). The patch does not touch that path. This is not a patch verification defect; the frozen run actually fetched the pin and ran the dependency checks (`gate-logs/C4-ci.log:3863`).

Prior-art checking used all five affected paths: history returned 17 commits for multipart.rs, one for abort.rs, none for the two new files, and 43 for the architecture document. I also searched closed multipart/Complete PRs, checked the changed-file lists of all 33 results against those paths, and read closed-as-not-planned #636 and its rejection/split comments. The matches include merged #799, #847, #908 and #909. The rejected work was discontinued for excessive scope and unrelated reclamation regressions; this slice introduces no retirement routing. The captured evidence is `reviewer-prior-art.json`.

All seven frozen gate logs are present and readable. Their dispositions, kept distinct from independent reruns, are:

| Gate | Verdict | Evidence |
|------|---------|----------|
| C4-ci | PASS | The captured gate ends “all checks passed,” including dependency checks and DST; `gate-logs/C4-ci.log:4513`. The independent run finishes with two failures from the missing target-copy gitlink, described above; `reviewer-ci.log:3971`. |
| C4-verify | NEEDS-HUMAN | The log and independent rerun agree: 16 green tests, but no executed pre-fix red; `gate-logs/C4-verify.log:15`, `reviewer-red.log:2`. |
| C4-diff-cov | PASS | 325/353 instrumentable changed lines executed (92.1%); 582 other changed production lines are unscored, not claimed covered; `gate-logs/C4-diff-cov.log:106`. |
| C5-mutants | FAIL | 68 tested: 42 caught, 22 unviable, four missed; the actionable gaps and lower-priority survivors are assessed above; `gate-logs/C5-mutants.log:13`. |
| T4-batch-review | PASS | The captured invocation reports zero blocking, recorded-rejected or noise-dropped findings; `gate-logs/T4-batch-review.log:10`. |
| T4-contribution | N/A | The artifacts are absent by design at Check; the substantive contribution audit must rerun at publish; `gate-logs/T4-contribution.log:10`. |
| host-tikv | PASS | Both feature-enabled clippy invocations also pass independently (`reviewer-tikv.log`); this is compile evidence, not a live TiKV run; `gate-logs/host-tikv.log:8`, `gate-logs/host-tikv.log:211`. |

The declared external dependency, typos, was exercised in both frozen and independent CI (`gate-logs/C4-ci.log:11`, `reviewer-ci.log:2`). The prescribed real-store test dependency was also exercised; no shim or code-read substitutes for its compile/test evidence. Publication, the S3 caller and #660’s DST coverage remain the named later slices, not undisclosed dependencies of this fence-only test.

<!-- pdca:leaf-complete -->

### Advisory — adversary

# Adversarial review — #1007 (Complete fence, 636.5a)

**Evidence I re-ran.** C4-verify's UNVERIFIABLE was declared ahead of time: the module is new, so the test cannot compile against the base. That is not a refutation. I copied the target to scratch and ran `cargo test -p wyrd-core --test multipart_complete`: 16/16 green. Then I applied five small changes to the production code by hand. **All five stayed green.** The suite exercises the real production path (`fence_complete` over `RedbMetadataStore`), but it does not tell apart several behaviours the brief names. The 92.1% diff coverage overstates the evidence: the uncovered lines are the error and edge branches (`complete.rs:545`, `:551`, `:607-611`, `:719-723`, `:732-733`, `:811-824`).

## Findings

- NEEDS-HUMAN [impl] — **A release that resets `attempts` to 0 passes every test.** I changed `crates/core/src/multipart/complete.rs:541` from `fenced.fence_released(opened.attempts())` to `fenced.fence_released(0)`: 16/16 green. I also made the fence charge a constant 1 instead of `attempts + 1` (`crates/core/src/multipart.rs:2633`): still 16/16 green. The cause is that every test that reaches a fence starts at `attempts == 0` (`crates/core/tests/multipart_complete.rs:432`, `:840`). So the "attempt given back" checks (`:625-629`, `:587-591`) compare 0 with 0. Concrete failing case: a session at `attempts = 2`, whose two earlier fences wrote segments and were rolled back, hits a D2-style rejection. The mutant writes `attempts: 0`, which refills the budget that X58 (`0016:2577`) relies on to bound `seg:` epochs. The same blind spot explains the C5 survivor `HeldFence::attempts_before -> 0` (`complete.rs:218`). Fix: run D2 and H (and one held fence) from a planted `Open@E` with `attempts` at 1 or 2.

- NEEDS-HUMAN [impl] — **The pre-fence check is untested, though the module doc promises what it does.** I changed `complete.rs:550` so a pre-fence rejection falls through to the fence: 16/16 green. The doc says an already-invalid request is answered "without fencing (no commit, and no in-flight `UploadPart` loses the session bytes it is fenced on)" (`complete.rs:30-32`). H's own doc says "removed before the request, nothing is fenced" (`multipart_complete.rs:973`). But H's first leg only checks state and `attempts` (`:990-992`), not the epoch or the commit count. D1 throws its commit log away (`:810`). This matters beyond speed. Without the check, every typo fences, and staging stops any in-flight UploadPart whose renewal sees the session out of `Open` (`crates/core/src/multipart/stage.rs:122`). Fix: in D1 and in H's first leg, assert zero commits and unchanged session bytes.

- NEEDS-HUMAN [impl] — **Reading parts past the first page is never tested.** I made `read_part_set` return after the first page (`complete.rs:730`): 16/16 green. A page holds 128 parts (`PART_PAGE_LIMIT` = `SEGMENT_PAGE_LIMIT`, `crates/core/src/metadata.rs:2867`) and every test plants 3. So any real upload with more than 128 parts runs the cursor step (`complete.rs:733`) with no test. The C5 survivors at `complete.rs:717` (`>`→`>=`) and `:718` (`||`→`&&`) are the check the doc calls what makes the walk end (`complete.rs:490-492`, `:694-695`). No test feeds it a bad page, because the wrapper passes `scan_page` straight through (`multipart_complete.rs:361-368`). Fix: plant 129 or more parts (cheap on in-memory redb), and give the wrapper a scripted `scan_page` that returns a duplicate or out-of-range key.

- NEEDS-HUMAN [impl] — **A store fault behind the fence is untested, and it is one of the two cases where C-1 lets an error leave the fence standing.** Changing `complete.rs:609` from `FenceState::Stands { .. }` to `FenceState::NotPlaced` passes all tests (diff-cov also flags `:607-611` as missed). That mutant tells the caller "no fence of this complete landed" while `Completing@E+1` is in the store. That is the false statement child-4 and the S3 verb would act on. The brief asked the wrapper to be able to "fail a read". `fail_reads_under` only affects `get` (`multipart_complete.rs:345-355`), so it cannot fail a `psum:` page. Fix: fail a `psum:` `scan_page` (or the bucket `get`) after the fence commits, and assert `Stopped { fence: Stands { epoch: 1 }, cause: Store(..) }` with the session byte-identical to the fence.

- NEEDS-HUMAN [human] — **A transient store fault behind the fence spends the budget and holds the session in `Completing`, even though nothing was minted.** `complete.rs:606-611` (reason given at `:59-62`) does not try a release when validation behind the fence fails with a store error. Concrete case: a valid list, then one transient fault on the post-fence `psum:` read. The session stays `Completing@E+1` with `attempts + 1`. The client gets `409 OperationAborted` until the reaper rolls back after `W_completing`, and the rollback keeps the charge (X58, `0016:2577`; `multipart.rs:5334-5340`). Three such faults leave the session answering `CompleteAttemptsExhausted` with no `seg:` epoch ever minted. That is the #636 wedge, reached through store faults instead of typos. The brief's invariant covers only *client* errors, so the patch meets the brief as written. But the brief's own reason for restoring `attempts` ("a fence that wrote no segment minted nothing the cap bounds") applies here too. A best-effort release costs one commit, and if it fails the result is the same `Stands` error, so "a release may fail with it" is no reason not to try. A human should decide: accept this cost, or release (best effort) on validation faults.

- NEEDS-HUMAN [impl] — (low) **A release `Conflict` followed by a failed re-read claims the fence "still stands" when, on most backends, it is gone.** `complete.rs:772-776` maps that read fault to `FenceState::Stands`, and the error message says "still stands" (`:390-393`). On redb and FDB, a `Conflict` on the release's exact-bytes check means `mpu:` no longer holds the fence's bytes (reaper rollback, or a teardown). It is safe, because every later batch checks the exact session bytes first. But the stopped arm exists to say *whether* the fence stands, and here that is unknown. Either reword `Stands`'s message for this path, or add an "unknown" state.

## Attempted, could not refute

- **Unknown outcomes.** The check for an unknown commit outcome matches what its sibling modules do (`abort.rs:343`, `create.rs:464`). Nothing is sent after one, and both fence and release are tested with both `may_still_commit` values.
- **Another publisher's fence.** It gets zero commits, and the test compares its session bytes and its `seg:` range byte-for-byte (`multipart_complete.rs:705-721`).
- **D4.** The test checks the exact release-round bound and that the definite-error release carries its cause.
- **Epoch overflow.** The release record is built before the fence commit, so a fence that could not be released is never placed (`complete.rs:535-546`).
- **Decoder rules.** The fenced record meets every rule the decoder enforces (`multipart.rs:2736-2749`).
- **Bucket and target.** The request's bucket is never checked against the session's object name. That is the same caller contract Create has (`create.rs:49-57`), not something this diff added.
- **Abort rounds.** The `ABORT_ROUNDS` doc change (`abort.rs:129-139`) admits that a stream of rejected Completes can use up an Abort's rounds. That needs a part re-upload racing each Complete, since typos are answered before any fence, and it ends in an error, not a wrong answer. Not raised.

<!-- pdca:leaf-complete -->

### Advisory — code-review

No introduced runtime correctness bug identified. Two low-priority advisory findings:

- NEEDS-HUMAN [impl] — **Test fidelity:** `crates/core/tests/multipart_complete.rs:775` starts the refund scenario with zero attempts, and `crates/core/tests/multipart_complete.rs:846` only checks `attempts_before() == 0`. Thus resetting the budget to zero instead of restoring its previous value would pass these assertions; frozen C5 evidence also shows the `attempts_before() -> 0` mutant survived. Add a case starting at `MAX_COMPLETE_ATTEMPTS - 1`, force a post-fence rejection, and assert that release preserves that nonzero count and the subsequent held fence reports it. This protects against accidentally refunding earlier charged attempts.
- NEEDS-HUMAN [impl] — **Reuse:** `crates/core/src/multipart/complete.rs:510` discards the ordering error from `complete_fingerprint`, then `crates/core/src/multipart/complete.rs:639` implements the same check again, invoked on every Open fence round at `crates/core/src/multipart/complete.rs:628`. The existing `canonical_named_parts` helper at `crates/core/src/multipart.rs:4398`, already used by fingerprinting, supplies the offending part number for duplicates and descending order. Retaining and mapping that validation result would remove the duplicate rule and repeated list traversal while preserving state-first answers.

<!-- pdca:leaf-complete -->

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C4 Verification (red→green) — Accept or reject the declared net-new verification exception — the independent green has 16 passing tests but the reverted leg only fails compilation; the full local CI also encounters a missing target-copy gitlink, while the frozen CI evidence passed; `reviewer-green.log:23`, `reviewer-red.log:2`, `gate-logs/C4-verify.log:24`, `reviewer-target-caveat.json:1`.
- [ ] C5 Causal adequacy — Add discriminators for a nonzero prior attempt budget and a frozen set crossing a page boundary — zero-only assertions let a wrong budget accessor survive, and continuation never executes; `target/crates/core/tests/multipart_complete.rs:846`, `target/crates/core/src/multipart/complete.rs:733`, `gate-logs/C5-mutants.log:13`, `gate-logs/C4-diff-cov.log:97`.
- [ ] T5 Judgment — Confirm the temporary public held-fence API before the publication consumer exists — this is the brief’s explicit API-boundary decision, and dropping the value leaves recovery to the later reaper; `brief.md:188`, `target/crates/core/src/multipart/complete.rs:180`.
- [ ] Validation — fitness-to-purpose — Decide whether this fence-only slice is fit to accept as the foundation for the publication slices — the evidence exercises a real in-memory store but intentionally proves neither publication nor an S3 user workflow; `brief.md:99`, `target/crates/core/src/multipart/complete.rs:449`, `reviewer-green.log:23`.
- [ ] C4-verify — The log and independent rerun agree: 16 green tests, but no executed pre-fix red; `gate-logs/C4-verify.log:15`, `reviewer-red.log:2`.
- [ ] **A release that resets `attempts` to 0 passes every test.** I changed `crates/core/src/multipart/complete.rs:541` from `fenced.fence_released(opened.attempts())` to `fenced.fence_released(0)`: 16/16 green. I also made the fence charge a constant 1 instead of `attempts + 1` (`crates/core/src/multipart.rs:2633`): still 16/16 green. The cause is that every test that reaches a fence starts at `attempts == 0` (`crates/core/tests/multipart_complete.rs:432`, `:840`). So the "attempt given back" checks (`:625-629`, `:587-591`) compare 0 with 0. Concrete failing case: a session at `attempts = 2`, whose two earlier fences wrote segments and were rolled back, hits a D2-style rejection. The mutant writes `attempts: 0`, which refills the budget that X58 (`0016:2577`) relies on to bound `seg:` epochs. The same blind spot explains the C5 survivor `HeldFence::attempts_before -> 0` (`complete.rs:218`). Fix: run D2 and H (and one held fence) from a planted `Open@E` with `attempts` at 1 or 2.
- [ ] **The pre-fence check is untested, though the module doc promises what it does.** I changed `complete.rs:550` so a pre-fence rejection falls through to the fence: 16/16 green. The doc says an already-invalid request is answered "without fencing (no commit, and no in-flight `UploadPart` loses the session bytes it is fenced on)" (`complete.rs:30-32`). H's own doc says "removed before the request, nothing is fenced" (`multipart_complete.rs:973`). But H's first leg only checks state and `attempts` (`:990-992`), not the epoch or the commit count. D1 throws its commit log away (`:810`). This matters beyond speed. Without the check, every typo fences, and staging stops any in-flight UploadPart whose renewal sees the session out of `Open` (`crates/core/src/multipart/stage.rs:122`). Fix: in D1 and in H's first leg, assert zero commits and unchanged session bytes.
- [ ] **Reading parts past the first page is never tested.** I made `read_part_set` return after the first page (`complete.rs:730`): 16/16 green. A page holds 128 parts (`PART_PAGE_LIMIT` = `SEGMENT_PAGE_LIMIT`, `crates/core/src/metadata.rs:2867`) and every test plants 3. So any real upload with more than 128 parts runs the cursor step (`complete.rs:733`) with no test. The C5 survivors at `complete.rs:717` (`>`→`>=`) and `:718` (`||`→`&&`) are the check the doc calls what makes the walk end (`complete.rs:490-492`, `:694-695`). No test feeds it a bad page, because the wrapper passes `scan_page` straight through (`multipart_complete.rs:361-368`). Fix: plant 129 or more parts (cheap on in-memory redb), and give the wrapper a scripted `scan_page` that returns a duplicate or out-of-range key.
- [ ] **A store fault behind the fence is untested, and it is one of the two cases where C-1 lets an error leave the fence standing.** Changing `complete.rs:609` from `FenceState::Stands { .. }` to `FenceState::NotPlaced` passes all tests (diff-cov also flags `:607-611` as missed). That mutant tells the caller "no fence of this complete landed" while `Completing@E+1` is in the store. That is the false statement child-4 and the S3 verb would act on. The brief asked the wrapper to be able to "fail a read". `fail_reads_under` only affects `get` (`multipart_complete.rs:345-355`), so it cannot fail a `psum:` page. Fix: fail a `psum:` `scan_page` (or the bucket `get`) after the fence commits, and assert `Stopped { fence: Stands { epoch: 1 }, cause: Store(..) }` with the session byte-identical to the fence.
- [ ] **A transient store fault behind the fence spends the budget and holds the session in `Completing`, even though nothing was minted.** `complete.rs:606-611` (reason given at `:59-62`) does not try a release when validation behind the fence fails with a store error. Concrete case: a valid list, then one transient fault on the post-fence `psum:` read. The session stays `Completing@E+1` with `attempts + 1`. The client gets `409 OperationAborted` until the reaper rolls back after `W_completing`, and the rollback keeps the charge (X58, `0016:2577`; `multipart.rs:5334-5340`). Three such faults leave the session answering `CompleteAttemptsExhausted` with no `seg:` epoch ever minted. That is the #636 wedge, reached through store faults instead of typos. The brief's invariant covers only *client* errors, so the patch meets the brief as written. But the brief's own reason for restoring `attempts` ("a fence that wrote no segment minted nothing the cap bounds") applies here too. A best-effort release costs one commit, and if it fails the result is the same `Stands` error, so "a release may fail with it" is no reason not to try. A human should decide: accept this cost, or release (best effort) on validation faults.
- [ ] (low) **A release `Conflict` followed by a failed re-read claims the fence "still stands" when, on most backends, it is gone.** `complete.rs:772-776` maps that read fault to `FenceState::Stands`, and the error message says "still stands" (`:390-393`). On redb and FDB, a `Conflict` on the release's exact-bytes check means `mpu:` no longer holds the fence's bytes (reaper rollback, or a teardown). It is safe, because every later batch checks the exact session bytes first. But the stopped arm exists to say *whether* the fence stands, and here that is unknown. Either reword `Stands`'s message for this path, or add an "unknown" state.
- [ ] **Test fidelity:** `crates/core/tests/multipart_complete.rs:775` starts the refund scenario with zero attempts, and `crates/core/tests/multipart_complete.rs:846` only checks `attempts_before() == 0`. Thus resetting the budget to zero instead of restoring its previous value would pass these assertions; frozen C5 evidence also shows the `attempts_before() -> 0` mutant survived. Add a case starting at `MAX_COMPLETE_ATTEMPTS - 1`, force a post-fence rejection, and assert that release preserves that nonzero count and the subsequent held fence reports it. This protects against accidentally refunding earlier charged attempts.
- [ ] **Reuse:** `crates/core/src/multipart/complete.rs:510` discards the ordering error from `complete_fingerprint`, then `crates/core/src/multipart/complete.rs:639` implements the same check again, invoked on every Open fence round at `crates/core/src/multipart/complete.rs:628`. The existing `canonical_named_parts` helper at `crates/core/src/multipart.rs:4398`, already used by fingerprinting, supplies the offending part number for duplicates and descending order. Retaining and mapping that validation result would remove the duplicate rule and repeated list traversal while preserving state-first answers.
- [ ] C4 per-fix red->green: this patch's test red pre-fix, green post-fix unverifiable —                why this slice has no isolable red (the cargo output is above).
- [ ] size backstop — this slice is behaving oversized: patch is 109 KB (threshold 100 KB). Recommend answering `iterate-plan` at sign-off and authoring the split in the re-plan (`pdca split`), rather than `iterate-do`: a slice that is too big yields implementation-shaped findings every round, and splitting authors briefs, which is Plan's beat.

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
- Iteration delta (if iterating): Human override of the size backstop (112 KB vs 100 KB): do NOT split; rebuild this slice. No production result was found wrong; the tests could not tell correct code from several broken versions, and one policy is changed. 1. Policy change (decided at sign-off): a store fault while validating behind the fence (complete.rs:607-611) now tries ONE best-effort release, with the same exact-bytes check and the release record already minted with the fence (attempts restored to the pre-fence value). Release lands -> error that says the fence was taken down (reuse NotPlaced or add a Released FenceState), session Open, attempts restored. Release fails -> today's Stands error. Release CommitUnknownResult -> InDoubt (Release). Conflict -> the existing moved() path. Rewrite the module doc at complete.rs:60-63 to match; a crash behind the fence still costs W_completing (0016:956-962). 2. Nonzero budget: run D2, H and one held fence from a planted Open@E with attempts at 1 and at MAX_COMPLETE_ATTEMPTS-1. Assert release restores that exact count, the fence charges attempts+1 (not a constant), and HeldFence::attempts_before() reports it (kills the attempts_before -> 0 survivor, and fence_released(0) / constant-charge mutants). 3. Pre-fence check: in D1 and H's first leg, assert zero commits and byte-identical session bytes (a typo must not fence; complete.rs:550). 4. Paging: plant >=129 part/psum pairs, name a part on the second page, change that tail part before the fence and assert rejection + release. Give the wrapper a scripted scan_page returning a duplicate or out-of-range key, killing the complete.rs:717/:718 survivors. 5. Store fault behind the fence: make the wrapper able to fail a psum: scan_page (not only get) after the fence commits; assert the outcome of item 1 for each release result, and that Stands is never reported as NotPlaced (complete.rs:609 mutant). 6. Wording: a release Conflict followed by a failed re-read (complete.rs:772-777) must not claim the fence "still stands"; report it as unknown (reword, or add an Unknown FenceState) and test it. 7. Duplicate check: keep the ordering error from complete_fingerprint / canonical_named_parts (multipart.rs:4398) instead of re-checking at complete.rs:639; keep state-first answers. 8. Remaining C5 survivors (Display mutant included): kill or justify each, aiming for zero missed.
- By / date: Eduard Ralph / 2026-10-30

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
