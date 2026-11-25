# core: Complete fence — validation, release and a budget a client's mistakes never spend (636.5a)

> First of the five slices #658 (636.5) was split into on 2026-10-30. Later slices publish
> flat (636.5b) and segmented (636.5c) objects, recover from unknown outcomes (636.5d) and
> enforce the value ceiling (636.5e). Do reads ONLY this file. The design is normative in
> proposal 0016, `docs/design/proposals/draft/0016-multipart-commit-protocol.md`: read
> **decision 3** (`0016:895-1036`, the verb × state table at `:973-979`, the rejected-Complete
> rule `:994-1000`), and the **Complete fence** and **Fence release** rows of the batch inventory
> (`0016:661`, `:664`). Citations were verified against `getwyrd/wyrd` at `4bac538`; re-resolve
> them on your base.

- **Slug:** multipart-complete-fence
- **Track:** alpha
- **Kind:** enhancement (design proposal)
- **Defect:** Nothing fences a multipart session for publication. The discontinued #636 patch
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
- **Success criterion:** A NEW test file `crates/core/tests/multipart_complete.rs`, over
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
- **Falsifiability:** RED is reachable in-process on the plain `cargo test -p wyrd-core` target.
  Every red leg is forced by a wrapper step, not by timing: D2's re-plant before the fence, D4's
  refused releases, G0's unknown outcomes and failed read, and H's bucket removal. (D) is the
  #636 patch's reproduced defect.
- **Invariant to restore:** **C-1** (`docs/principles.md` §5 C-1 and its §6 row *storage
  lifecycle / reclamation*), for the fence phase. No Complete that returns an **answer** leaves
  **its own** fence in `Completing`. A Complete that meets **another** publisher's `Completing`
  fence leaves it untouched. A fence is owned only when a fence commit of its own answered a
  definite success. Only an **error** may leave its own fence in place: the in-doubt error, or the
  stopped error with the fence standing. Within S3's part-number range, no client error can leave
  an `Open` session unable to fence again. **Sources:** the maintainer's rule 2026-07-25 (via
  principles.md §5); decision 3's rejected-Complete rule (`0016:994-1000`); X58 (`0016:2577`,
  `:1479`). SELF-TEST: it binds the fence, the release and the budget together, so guarding one
  function cannot satisfy it.
- **Repo + branch target:** getwyrd/wyrd @ main
- **Depends on:**
- **Conflicts with:**
- **Ordering note:** No sibling dependency: this is the first slice. No external dependency is
  needed either. The tests plant part records through helpers already on `main`, and nothing here
  stages or publishes, so it can build before #927 and #653 land. Add `927` after `--accept` if
  the human wants this slice to wait for #927's record shapes. child-2 to child-5 build on this.
- **Scope:** in `crates/core/src/multipart/complete.rs` (a new module beside `abort.rs`):
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
- **Repro instruction:** n/a (new functionality). The wedge (D) guards against is in the
  discontinued #636 patch (`/home/eddie/wyrd/wyrd-pdca/results/issue_636/patch.diff`).
- **Difficulty:** high
- **Do model:** opus
- **External dependencies:** `typos`
- **Test file:** `crates/core/tests/multipart_complete.rs`, a **NEW** file. child-2, child-3 and
  child-4 append to it.
- **Verification posture:** (a) net-new. The module does not exist on the reverted base, so
  C4-verify reports **UNVERIFIABLE** (exit 77), a pre-declared sign-off item.
  **Demonstrated reds (required, recorded in `build-notes.md`).** Each is a temporary edit that
  makes the named leg fail:
  - a rejected request counted against `attempts` → (D1) fails;
  - pre-fence validation kept, with `attempts` not restored on a post-fence rejection → (D2)
    fails;
  - the session left `Completing` after a rejection → D's Abort leg fails;
  - a request meeting another publisher's `Completing` fence releases it → (E) fails;
  - a release that fails definitely answered with its refusal, or reported as in doubt → (D4)
    fails;
  - an unknown initial fence treated as success → (G0-fence) fails.
- **Production reach:** (a) exercised over a real store in-process; (b) the S3 verb (#670 under
  #508) is the eventual caller, after child-2 to child-5; (c) this slice **must not** add a
  production caller; (d) no existing production path changes.
- **Citations expected:** Do cites `path:line` on the target branch for every change.
  - Peer for the error arms and bounded rounds: #895's Abort, `abort.rs` (`AbortError`, `Stop`,
    `ABORT_ROUNDS`, `:100-160`, `:239-282`).
  - Records: `SessionState` (`multipart.rs:2173`), `PublishTarget` (`:2119`), `complete_answer`
    (`:4782`), `complete_fingerprint` (`:4423`), `InvalidPart` (`:4445`),
    `MAX_COMPLETE_ATTEMPTS` (`:5200`).
  - Peer for the bucket marker read: `require_bucket`, `crates/core/src/multipart/create.rs:700`.
- **Prior-art check (triage cycles):** the parent's. Merged: the answer table and digests
  (PR #799), the nonce on every session (PR #847), Create (#908), Abort (#909). Rejected: #636's
  single-slice attempt. The test file does not exist.
- **Disposition hint:** new-feature

## Design

**Fence** (`0016:661`). Read `mpu:<id>`, and answer from `complete_answer` for every state but
`Open`. On `Open@E`, commit `require(mpu == Open@E)` and put the session at `Completing@E+1`, with
`fenced_at_millis`, `segments_written: 0`, `publish_target {parent, name, epoch: E+1}` and
`attempts + 1`. At the budget, refuse `CompleteAttemptsExhausted` with zero commits.
Pre-validating against the part set read before fencing is allowed as an optimization, never as
the budget's protection (D2).

**Validate** against the frozen set (`0016:993-1000`). A rejection releases the fence: one batch
`require(mpu == Completing@E+1)` → `Open@E+2` with `attempts` restored to its pre-fence value.
Releases are bounded (Do picks the bound and cites it, as `ABORT_ROUNDS` bounds Abort). A
`Conflict` with the session unchanged spends a round. A session found moved is answered from its
new state.

**The stopped arm**, modelled on `AbortError::Stopped` (`abort.rs:109-117`, causes `:139-159`),
means no batch is in doubt. It names the upload, the fence's epoch, the cause, and **whether the
fence still stands**. child-2 uses the "released" case for a flip refused for good (J).

**The in-doubt arm**, in the shape of `AbortError::InDoubt` (`abort.rs:121-135`). In this slice,
any `CommitUnknownResult` returns it at once. child-4 adds the re-reads that settle some of them.

**Docs currency** (`AGENTS.md:154-157`). Add the fence to the multipart paragraphs of
`docs/design/architecture/05-building-block-view.md` (`:244-248`). Say that a rejected request
releases the fence without spending the budget, and that nothing publishes yet.

## Open questions

- **For sign-off:** the fence phase is exposed as `pub` functions with a held-fence value, so the
  integration test can drive it before the verb exists. child-2 composes them into the verb, and
  may narrow their visibility then. Confirm or redirect.

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a draft PR MAY
happen during the cycle. The PR MUST NOT be marked ready before sign-off accepts.

## Iteration 1 — carry-forward (from the previous attempt)
- Sign-off rationale: Human override of the size backstop (112 KB vs 100 KB): do NOT split; rebuild this slice. No production result was found wrong; the tests could not tell correct code from several broken versions, and one policy is changed. 1. Policy change (decided at sign-off): a store fault while validating behind the fence (complete.rs:607-611) now tries ONE best-effort release, with the same exact-bytes check and the release record already minted with the fence (attempts restored to the pre-fence value). Release lands -> error that says the fence was taken down (reuse NotPlaced or add a Released FenceState), session Open, attempts restored. Release fails -> today's Stands error. Release CommitUnknownResult -> InDoubt (Release). Conflict -> the existing moved() path. Rewrite the module doc at complete.rs:60-63 to match; a crash behind the fence still costs W_completing (0016:956-962). 2. Nonzero budget: run D2, H and one held fence from a planted Open@E with attempts at 1 and at MAX_COMPLETE_ATTEMPTS-1. Assert release restores that exact count, the fence charges attempts+1 (not a constant), and HeldFence::attempts_before() reports it (kills the attempts_before -> 0 survivor, and fence_released(0) / constant-charge mutants). 3. Pre-fence check: in D1 and H's first leg, assert zero commits and byte-identical session bytes (a typo must not fence; complete.rs:550). 4. Paging: plant >=129 part/psum pairs, name a part on the second page, change that tail part before the fence and assert rejection + release. Give the wrapper a scripted scan_page returning a duplicate or out-of-range key, killing the complete.rs:717/:718 survivors. 5. Store fault behind the fence: make the wrapper able to fail a psum: scan_page (not only get) after the fence commits; assert the outcome of item 1 for each release result, and that Stands is never reported as NotPlaced (complete.rs:609 mutant). 6. Wording: a release Conflict followed by a failed re-read (complete.rs:772-777) must not claim the fence "still stands"; report it as unknown (reword, or add an Unknown FenceState) and test it. 7. Duplicate check: keep the ordering error from complete_fingerprint / canonical_named_parts (multipart.rs:4398) instead of re-checking at complete.rs:639; keep state-first answers. 8. Remaining C5 survivors (Display mutant included): kill or justify each, aiming for zero missed.
- Sign-off session carry-forward (captured live, before §9 flattened it):
  Human override of the size backstop (112 KB vs 100 KB): do NOT split; rebuild this slice. No production result was found wrong; the tests could not tell correct code from several broken versions, and one policy is changed.
  1. Policy change (decided at sign-off): a store fault while validating behind the fence (complete.rs:607-611) now tries ONE best-effort release, with the same exact-bytes check and the release record already minted with the fence (attempts restored to the pre-fence value). Release lands -> error that says the fence was taken down (reuse NotPlaced or add a Released FenceState), session Open, attempts restored. Release fails -> today's Stands error. Release CommitUnknownResult -> InDoubt (Release). Conflict -> the existing moved() path. Rewrite the module doc at complete.rs:60-63 to match; a crash behind the fence still costs W_completing (0016:956-962).
  2. Nonzero budget: run D2, H and one held fence from a planted Open@E with attempts at 1 and at MAX_COMPLETE_ATTEMPTS-1. Assert release restores that exact count, the fence charges attempts+1 (not a constant), and HeldFence::attempts_before() reports it (kills the attempts_before -> 0 survivor, and fence_released(0) / constant-charge mutants).
  3. Pre-fence check: in D1 and H's first leg, assert zero commits and byte-identical session bytes (a typo must not fence; complete.rs:550).
  4. Paging: plant >=129 part/psum pairs, name a part on the second page, change that tail part before the fence and assert rejection + release. Give the wrapper a scripted scan_page returning a duplicate or out-of-range key, killing the complete.rs:717/:718 survivors.
  5. Store fault behind the fence: make the wrapper able to fail a psum: scan_page (not only get) after the fence commits; assert the outcome of item 1 for each release result, and that Stands is never reported as NotPlaced (complete.rs:609 mutant).
  6. Wording: a release Conflict followed by a failed re-read (complete.rs:772-777) must not claim the fence "still stands"; report it as unknown (reword, or add an Unknown FenceState) and test it.
  7. Duplicate check: keep the ordering error from complete_fingerprint / canonical_named_parts (multipart.rs:4398) instead of re-checking at complete.rs:639; keep state-first answers.
  8. Remaining C5 survivors (Display mutant included): kill or justify each, aiming for zero missed.
- Failing gate: C5 surviving mutants on the bundle diff (cargo mutants --in-diff) (advisory) — 68 mutants tested in 6m: 4 missed, 42 caught, 22 unviable
- Full previous attempt preserved in `iteration-v1/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 2 — carry-forward (from the previous attempt)
- Sign-off rationale: Human decision at sign-off: rebuild this slice to close the test gaps and doc mismatches below. Do not split it. No production result was found wrong, so keep the fence, validation and release logic as they are unless a new test proves otherwise. Test gaps. Each new leg must fail under the edit named: 1. complete.rs:123: add a leg where a release loses one Conflict and then lands, and pin the release bound to a number written in the test, not the constant. RELEASE_ROUNDS = 1 must fail. This is the TiKV lock case. 2. complete.rs:677: add an "another target" leg with a different parent. Dropping the session.parent() != request.parent check must fail. 3. complete.rs:584: add a leg where the fence loses to a session moved to a later Open with a different attempts, the retry fences, and the request is then rejected. Restoring attempts from the first read must fail. 4. complete.rs:722-725 and :736: test that the bucket is checked first and that the first bad part is the one reported, using a list with two bad parts and a deleted bucket combined with a bad part. Alternatively, drop that promise from the doc. Doc mismatches: 5. complete.rs:59-60, :523 and docs/design/architecture/05-building-block-view.md:248 say a store fault behind the fence is an error "whatever happens next", but moved() (complete.rs:915-920) can return an answer on that path. Keep the behaviour and correct the wording. 6. complete.rs:73 says each error keeps the fault as its cause, but the in-doubt arm (complete.rs:882-885) drops it. Keep the scan fault visible to an operator, or correct the doc. 7. abort.rs:53-54 says a client sending invalid Completes can move the session under an Abort, but complete.rs:30-32 answers those with no commit. Only a list that passes the first check and is rejected behind the fence can move it. Correct the doc. Not part of this rebuild: the deleted-and-recreated bucket question is still open with the human. Do not add a persisted session field for it here.
- Sign-off session carry-forward (captured live, before §9 flattened it):
  Human decision at sign-off: rebuild this slice to close the test gaps and doc mismatches below. Do not split it. No production result was found wrong, so keep the fence, validation and release logic as they are unless a new test proves otherwise.

  Test gaps. Each new leg must fail under the edit named:
  1. complete.rs:123: add a leg where a release loses one Conflict and then lands, and pin the release bound to a number written in the test, not the constant. RELEASE_ROUNDS = 1 must fail. This is the TiKV lock case.
  2. complete.rs:677: add an "another target" leg with a different parent. Dropping the session.parent() != request.parent check must fail.
  3. complete.rs:584: add a leg where the fence loses to a session moved to a later Open with a different attempts, the retry fences, and the request is then rejected. Restoring attempts from the first read must fail.
  4. complete.rs:722-725 and :736: test that the bucket is checked first and that the first bad part is the one reported, using a list with two bad parts and a deleted bucket combined with a bad part. Alternatively, drop that promise from the doc.

  Doc mismatches:
  5. complete.rs:59-60, :523 and docs/design/architecture/05-building-block-view.md:248 say a store fault behind the fence is an error "whatever happens next", but moved() (complete.rs:915-920) can return an answer on that path. Keep the behaviour and correct the wording.
  6. complete.rs:73 says each error keeps the fault as its cause, but the in-doubt arm (complete.rs:882-885) drops it. Keep the scan fault visible to an operator, or correct the doc.
  7. abort.rs:53-54 says a client sending invalid Completes can move the session under an Abort, but complete.rs:30-32 answers those with no commit. Only a list that passes the first check and is rejected behind the fence can move it. Correct the doc.

  Not part of this rebuild: the deleted-and-recreated bucket question is still open with the human. Do not add a persisted session field for it here.
- Full previous attempt preserved in `iteration-v2/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).
