# Result — issue 658 / multipart-complete

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect: Parts can be staged (#657) but nothing publishes them. This slice adds
  `CompleteMultipartUpload` as a `wyrd-core` function over `MetadataStore`, answering in
  `CompleteOutcome` (`crates/core/src/multipart.rs:4624`) and `Refusal` (`:4487`): fence the
  session `Open@E → Completing@E+1`, validate the named-part list against the frozen part set,
  assemble the chunk list, publish it through #653's committer (flat at most `MAX_MAP_CHUNKS`
  chunks, segmented above: the first real producer of `ChunkMap::Segmented`), and flip the root
  in one batch with the session's move to `Completed` (recording the ETag and the request
  fingerprint) and the retirement obligations for the parts. A rejected request releases the
  fence, and **a client's mistakes never use up the session's Complete budget**.
- Success criterion: A NEW test file `crates/core/tests/multipart_complete.rs`, over
  `RedbMetadataStore::in_memory()`, a test-local placement-aware chunk store, and a test-local
  store wrapper that can inject an outcome or a rival write before a chosen commit. Sessions are
  made by #894's Create and parts by #657's staging. It shows all of the following.
  **(A) Publish and read back.** Parts staged **out of order**, at least one **non-final** part
  whose length is not a whole multiple of the chunk size, then Complete naming all of them, answers
  `Published`. `read_object` (`crates/core/src/read.rs:510`) returns the parts' bytes
  concatenated, byte-identical. The inode's ETag and the session's recorded `Completion.etag` both
  equal `multipart_etag` over the named `(part, digest)` list (`multipart.rs:4392`); the session
  decodes as `Completed` with `complete_fingerprint` equal to `complete_fingerprint` of that list
  (`:4423`).
  **(B) Segmented.** With a chunk size small enough that the parts total more than
  `MAX_MAP_CHUNKS` chunks (`multipart.rs:4959`), the published root is `ChunkMap::Segmented` and
  names the group `(session segment_nonce, E)` where `E` is the Completing epoch; the read is still
  byte-identical.
  **(C) Subset Complete** (`0016:1034`). Stage parts 1, 2, 3; Complete naming 1 and 3. The object
  is parts 1 + 3. The flip installed `retire:records:` naming **only** parts 1 and 3 and
  `retire:bytes:` naming part 2 (its bytes and its records); no chunk of part 1 or 3 appears in any
  `retire:bytes:` obligation. Every obligation decodes under `decode_retire_obligation`
  (`multipart.rs:3891`).
  **(D) Typo, then correct.** (D1) `MAX_COMPLETE_ATTEMPTS` (`multipart.rs:5200`) Completes in a
  row with invalid lists (an absent part, a digest mismatch, part numbers out of order) each
  answer `Refused(InvalidPart { .. })` and leave the session `Open`. (D2) **A part replaced
  between the first read and the fence.** `MAX_COMPLETE_ATTEMPTS` Completes in a row, each naming a
  list that is valid against the part set when Complete first reads it, while the wrapper
  re-uploads one named part (new digest) just before the fence commits. Each fences, fails against
  the frozen set, releases, and answers `Refused(InvalidPart { .. })`. After each rejected Complete
  in D1 and D2 the session is `Open` and its `attempts` equals its value before that Complete. A
  following Complete with a valid list answers `Published`, not `CompleteAttemptsExhausted`. After
  a rejected Complete, an Abort answers `Fenced` (`0016:1033`).
  (D3) **A fence that wrote segments stays charged, and the cap stops the next one** (the other
  half of the budget rule; the cap is what bounds a session's segment generations,
  `multipart.rs:5194-5200`). On a fresh session whose parts total more than `MAX_MAP_CHUNKS`
  chunks (as in B), the wrapper lets every segment batch land and answers every flip batch
  `Conflict` without applying it. Run `MAX_COMPLETE_ATTEMPTS` (3) valid Completes in a row. Each
  fences at a new epoch `E_i`, writes its `seg:` records durably, loses `R_PUBLISH` flips,
  releases and answers `Refused(OperationAborted)`. After each one the session is `Open`, its
  `attempts` is one higher than before that Complete (charged, not restored), and a
  `retire:records:` obligation naming that attempt's group `(segment_nonce, E_i)` exists and
  decodes under `decode_retire_obligation`. The next valid Complete answers
  `Refused(CompleteAttemptsExhausted)` with **zero commits**: no fence, and no `seg:` record of a
  new epoch. An implementation that restores `attempts` on every release passes D1 and D2 and
  fails here.
  (D4) **A fence it owns and cannot release** (added 2026-10-22). Set up as D2, so the fence
  lands with a **definite** success and the frozen list is invalid. The wrapper then answers
  every release commit `Conflict` without applying it and without changing the session — the
  conflict no read explains, which TiKV gives when a locking read meets another writer's lock
  (`crates/core/src/multipart/abort.rs:245-247`). No unknown outcome is injected anywhere.
  Complete returns the **stopped** error (see Design § *The stopped error*): it names the upload
  and the epoch of the fence it still holds (`E+1`), its cause says the release rounds ran out,
  and it carries no `CommitUnknownResult`. The session is `Completing@E+1`, byte-identical to the
  fence's bytes; no `seg:` record and no flip was sent; the wrapper counts at most the stated
  bound of release commits. A second run answers the release with a definite store error (an
  `Err` that is not a `CommitUnknownResult`): the same stopped error, with that store error as
  its cause.
  **(E) Answers by state** (`0016:976`). An identical retry against the `Completed` tombstone
  answers `AlreadyCompleted` with the **recorded** publication (same ETag, inode, version); a
  retry naming a different list answers `Refused(NoSuchUpload)`; a Complete against a
  `Completing` session (another publisher's fence) answers `Refused(OperationAborted)` and leaves
  that session record and its `seg:` range byte-for-byte unchanged (`0016:953-958`: only the
  gateway that owns the fence resumes it).
  **(F) The version comes from the re-read prior** (`0016:1025`). Key `K` exists at version `v`;
  the wrapper lands an ordinary overwrite of `K` (version `v+1`) between the fence and the flip.
  Complete retries the flip and publishes `v+2`. When every flip attempt loses (`R_PUBLISH`,
  `multipart.rs:5192`), Complete releases the fence, answers `Refused(OperationAborted)`, and the
  session is `Open`, never left `Completing`. Publishing over an existing generation **that has
  at least one chunk** installs a `retire:bytes:` obligation naming that generation (`0016:663`).
  (F2) **An empty prior owes nothing** (added 2026-10-22). Key `K` exists as a zero-byte object
  (an ordinary write of empty data: an empty plan, committed with an empty chunk map,
  `crates/core/src/write.rs:166-168`, `:277-281`). Complete publishes over it at the next
  version, answers `Published`, installs **no** generation obligation for the empty prior, and
  every obligation it does install decodes under `decode_retire_obligation`; the read returns the
  parts' bytes. A flat generation obligation with no chunks is one the decoder refuses
  (`RetireObligationOwesNothing`, `multipart.rs:3272-3277` via `:3427`), so the rule is "a
  generation with chunks", never "the key existed".
  **(G) Unknown outcomes** (`0016:942-964`, `:1028`). (G1) The wrapper applies the flip batch and
  answers `CommitUnknownResult`. Complete answers `Published` once; the inode version advanced
  once and exactly one set of obligations exists. (G2) **A held flip.** The wrapper holds the
  flip (`may_still_commit: true`) and answers **every** later commit (flip retries and any fence
  release) `Conflict` without applying it, so the session reads `Completing@E+1` unchanged until
  the rounds run out (the shape of `crates/core/tests/multipart_abort.rs:961-980`). Complete returns the in-doubt error
  (see Design), not `Published` and not a refusal. Released after Complete returned, the held
  flip lands: the object is published once, with one set of obligations. (G3) **A failed
  re-read.** After an unknown flip outcome, the wrapper fails the re-read of `mpu:<id>`: Complete
  returns the in-doubt error, never an answer. (G4) **An unknown segment write.** A segment batch
  answers `CommitUnknownResult { may_still_commit: false }`, in one run applied and in another
  not. Complete
  re-reads the session, resumes from its `segments_written` cursor at the same epoch, writes no
  `seg:` record twice, and publishes once. (G5) **An unknown INITIAL fence never proves
  ownership.** With the clock fixed, Complete A (naming parts 1 and 2) and Complete B (naming 1
  and 3) both read `Open@E` and build byte-identical fences (same `fenced_at_millis`, `attempts`
  and target; `Completing` records no request identity, `multipart.rs:2186-2194`). Run 1: the
  wrapper holds A's fence, lets B's commit (B gets a definite success), then answers A's
  `CommitUnknownResult { may_still_commit: false }` unapplied. Run 2: the wrapper applies A's
  fence and answers it `CommitUnknownResult { may_still_commit: false }`, then B's fence
  conflicts. In both runs A's re-read finds `Completing@E+1` with exactly the bytes it proposed,
  and A returns the in-doubt error: it writes no `seg:` record and sends no flip. In run 1 B
  publishes parts 1 + 3, once. In run 2 B answers `Refused(OperationAborted)`, nothing is
  published, and the session stays `Completing@E+1` unchanged for the reaper's rollback (#625).
  An implementation that treats the matching re-read as its own fence publishes twice in run 1.
  **(H) Bucket deleted** (`0016:1035`). With the bucket record removed while the session is open,
  Complete answers `Refused(NoSuchBucket)`, no object is bound, and the session is `Open` again.
  (H2) **Removed after validation, just before the flip** (added 2026-10-22). H alone is caught by
  the validation read, so it passes even with no bucket guard on the flip. Here the wrapper
  deletes the `bucket:` marker after Complete's validation (and, in the segmented run, after the
  last segment batch lands), immediately before the flip commit. The flip pins the marker's
  exact bytes (the `require_bucket` form, `crates/core/src/multipart/create.rs:698-705`; the
  root-flip row's bucket-existence pair, `0016:663`), so it conflicts. Complete answers
  `Refused(NoSuchBucket)` — not `OperationAborted` and not a retried flip; no dirent or inode
  exists for the key; the session is `Open@E+2`. Run it flat and segmented (parts past
  `MAX_MAP_CHUNKS` chunks, as in B): in the segmented run the release installed a
  `retire:records:` obligation naming the attempt's group `(segment_nonce, E+1)`, which decodes,
  and `attempts` stays charged; in the flat run `attempts` is restored. A flip without the bucket
  precondition publishes here, so this leg goes red.
  **(I) Sparse part sets and the value ceiling.** Over a wrapper that refuses any value past
  `MAX_VALUE_BYTES` (`crates/core/src/metadata.rs:549`), as
  `crates/custodian/tests/restore_completing_fence.rs:473-486` does, with parts planted as
  canonical records (staging 10,000 parts is not needed). (I1) Parts 1 to 10,000, Complete naming
  the odd ones: it publishes, and both `{parts}` obligations fit (about 58.9 KB each; S3's
  part-number range 1 to 10,000 cannot do worse). (I2) Parts 1, 3, …, 19,999 (10,000 parts),
  Complete naming all of them, so the published set encodes to about 129 KB: Complete refuses `Refused(SessionTooLarge { bytes, limit })` with zero commits,
  the session stays `Open` with `attempts` unchanged, and an Abort then fences it (`{session,
  all}` fits). (I3) **The boundary is the stored value, not the bare set.** Parts 1, 3, …,
  15,869 (7,935 parts), Complete naming all: the set encodes to 99,981 bytes and the
  `{"parts":…}` obligation value to 99,991, so it publishes. Parts 1, 3, …, 15,871 (7,936 parts):
  the set is 99,995 bytes, under the ceiling, but the obligation value is 100,005, over it, so
  Complete refuses `SessionTooLarge` with zero commits. (Sizes are compact JSON of the range
  encoding, `metadata.rs:1976-1977`, `multipart.rs:3184-3193`; they assume the published-parts
  obligation's payload carries only `parts`. The publishing run asserts the installed
  obligation's stored length is at most `MAX_VALUE_BYTES`; if the real payload carries more,
  move the pair so it still straddles the ceiling and record the numbers in `build-notes.md`.)
- Repo + branch target: getwyrd/wyrd @ main
- Scope: the Complete verb as a store-I/O function in `wyrd-core`: the fence (with the
  attempts budget), validation against the frozen part set, assembly of the ordered chunk list,
  publication through #653's committer with the session's flip contribution, the fence release on
  a rejected request or an exhausted publish, the same-gateway recovery after an unknown outcome
  and the bounded in-doubt error, the value-ceiling check on the flip's part sets (with the
  `SessionTooLarge` doc extended), the session-record transitions it needs (crate-private, like
  `SessionRecord::open`), the
  `retire:` rows it installs (through `RetireObligation`), and the docs sentence. / out of scope:
  - draining any obligation, the terminal delete, and the routing of `unlink` / `PutObject`
    supersede through `retire:bytes:` (#659);
  - the reaper's rollback of a stale `Completing` session and the `W_session` ceiling (#625); the
    operator abort (#633);
  - minting the segment-group nonce: Create already mints it and stores it on every session
    record (`multipart.rs:2271-2287`, #847). Complete only reads it (`attempt_segment_group`,
    `:2503`). Do not add a nonce field or generator;
  - the gateway / S3 `CompleteMultipartUpload` verb and any production caller (#670 under #508);
  - seeded DST races (#660);
  - any edit to proposal 0016 or to any ADR; `Cargo.lock` and `deny*.toml`.

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: new-feature
- Confidence: medium
- Recommendation: (set by Do)

## 3. Correctness (Check — chain)
- C1 Spec: none — brief.md
- C2 Reproduction (red pre-fix): none — N/A — close disposition (no patch to verify)
- C3 Change: none — patch.diff
- C4 Verification (red→green): none — N/A — close disposition (no patch to verify)
- C5 Causal adequacy: none — reviewer + human sign-off

## 4. Conformance (Check — stack)
- T1 Structure: none — N/A — close disposition (no patch to verify)
- T2 Shape: none — N/A — close disposition (no patch to verify)
- T3 Runtime: none — N/A — close disposition (no patch to verify)
- T4 Contribution: none — N/A — close disposition (no patch to verify)
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

# Advisory review — SKIPPED (close disposition)

The reviewer leaf was skipped: this bundle's Plan concluded a close / no-fix disposition (split), so there is no patch to review.

- NEEDS-HUMAN — Confirm the close disposition 'split' (no patch was built). Override to a fix path (iterate-to-Do) if the close is wrong.


## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [x] Confirm the close disposition 'split' (no patch was built). Override to a fix path (iterate-to-Do) if the close is wrong.
- [x] **The error contract has no outcome for a known fence whose release fails.** `brief.md:138` permits only the in-doubt error to leave Complete's own fence standing, while `brief.md:324-329` requires that error to carry an unknown commit outcome. Consider D2's definite successful fence and invalid frozen part list, followed by release commits that all return definite `Conflict` without changing the session. This is a supported store behavior: `crates/core/src/multipart/abort.rs:245-247` explicitly handles conflicts caused by locks with nothing moved. Complete must terminate with its fence still standing, but there is no unknown outcome to supply: the proposed model's `InDoubt.outcome` is a mandatory `CommitUnknownResult` (`crates/core/src/multipart/abort.rs:121-134`). Define the error and recovery contract for a known, unfinished fence, including definite store failures after fencing; reconcile the invariant and add a bounded release-failure test with no unknown commit injected.
- [x] **“Retire the prior whenever the key existed” is incompatible with a valid empty prior.** `brief.md:307` requires a generation obligation “if the key existed,” and criterion F repeats that rule (`brief.md:78-79`). The target supports empty objects: `crates/core/src/write.rs:166-168` specifies an empty plan, and `:277-281` publishes that plan as a committed inode with an empty map. But retirement of a flat generation passes its chunks through `checked_chunks` (`crates/core/src/multipart.rs:3427`), which rejects an empty list as `RetireObligationOwesNothing` (`:3272-3277`). Following the brief literally either rejects a legitimate overwrite or writes an obligation the decoder refuses. Specify that an empty flat prior needs no generation retirement obligation, and require Complete-over-empty to publish the next version successfully without that obligation. This avoids silently expanding scope into a retirement-format change.
- [x] **Criterion H cannot detect an omitted bucket precondition on the flip.** It removes the bucket while the session is still Open (`brief.md:105-106`), so the explicit validation read (`brief.md:297-299`) can satisfy H even if publication has no atomic bucket guard. The target's existing helper pins the marker's exact bytes (`crates/core/src/multipart/create.rs:698-705`), and the normative root-flip row requires the bucket precondition (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:663`). Add a wrapper schedule that removes the marker after validation, immediately before the flip; require no publication, `NoSuchBucket`, and a released fence with any written segments evidenced for retirement. Also specify how this conflict is classified: `brief.md:310-312` currently directs only a dirent re-read and eventual `OperationAborted`. Removing the flip's bucket guard should make this new leg fail.

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
- By / date: Eduard Ralph / 2026-10-30

## 10. Act candidates (hints for the next Act review)
- Plan advisory: 3 finding(s); brief revised: yes (plan-advisory-*.md)
- (empty is the common case)
