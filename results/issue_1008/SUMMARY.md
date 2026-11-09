# Result — issue 1008 / multipart-complete-publish-flat

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect: After child-1 a valid request fences, but nothing publishes. This slice adds the
  public `CompleteMultipartUpload` function. It composes child-1's fence phase with the publish
  phase for objects of at most `MAX_MAP_CHUNKS` chunks:
  - assemble the named parts' chunk lists in part order;
  - publish through #653's committer;
  - flip the root in one batch, which:
    - moves the session to `Completed` (recording the ETag and the request fingerprint);
    - installs the retirement obligations (`retire:records:` for the published parts,
      `retire:bytes:` for unnamed parts, and `retire:bytes:` for a superseded generation that has
      chunks);
    - pins the validated `bucket:` marker;
  - on a lost flip, re-read and retry with the version recomputed from the prior;
  - release the fence when the flip cannot win, when the bucket is gone, or when the store
    refuses the flip for good.

  A list over `MAX_MAP_CHUNKS` chunks is refused with an error until child-3.
- Success criterion: New legs appended to `crates/core/tests/multipart_complete.rs`, reusing
  child-1's wrapper, and adding a test-local placement-aware chunk store. Parts are staged by
  #927's `UploadPart`, and sessions are made by #894's Create. It shows all of the following.
  **(A) Publish and read back.** Parts staged **out of order**, at least one **non-final** part
  whose length is not a whole multiple of the chunk size, total at most `MAX_MAP_CHUNKS` chunks.
  Complete naming all of them answers `Published`, and:
  - `read_object` (`crates/core/src/read.rs:510`) returns the parts' bytes concatenated,
    byte-identical;
  - the root is a flat map;
  - the inode's ETag and the session's `Completion.etag` both equal `multipart_etag` over the
    named `(part, digest)` list (`multipart.rs:4392`);
  - the session decodes as `Completed` with `complete_fingerprint` of that list (`:4423`).

  An identical retry then answers `AlreadyCompleted` with that publication.
  **(D2-tail)** After child-1's D1 and D2 rejections, a valid Complete answers `Published`, not
  `CompleteAttemptsExhausted`.
  **(C) Subset Complete** (`0016:1034`). Stage parts 1, 2, 3. Complete naming 1 and 3 publishes
  parts 1 + 3. The flip installed `retire:records:` naming **only** parts 1 and 3, and
  `retire:bytes:` naming part 2 (its bytes and its records). No chunk of part 1 or 3 appears in
  any `retire:bytes:` obligation. Every obligation decodes under `decode_retire_obligation`
  (`multipart.rs:3891`).
  **(F) The version comes from the re-read prior** (`0016:1025`).
  - Key `K` exists at version `v`. The wrapper lands an ordinary overwrite of `K` (version `v+1`)
    between the fence and the flip. Complete retries the flip and publishes `v+2`, and installs a
    `retire:bytes:` obligation naming the superseded generation.
  - When every flip attempt loses (`R_PUBLISH`, `multipart.rs:5192`), Complete releases the fence
    and answers `Refused(OperationAborted)`. The session is `Open`, with `attempts` restored.

  **(F2) An empty prior owes nothing.** Key `K` exists as a zero-byte object
  (`crates/core/src/write.rs:166-168`, `:277-281`). Complete publishes over it at the next version
  and answers `Published`. It installs **no** generation obligation for the empty prior, and every
  obligation it installs decodes.
  **(H2) Bucket removed just before the flip.** The wrapper deletes the `bucket:` marker after
  validation, immediately before the flip commit. The flip pins the marker's exact bytes
  (`crates/core/src/multipart/create.rs:698-705`, `0016:663`), so it conflicts. Complete answers
  `Refused(NoSuchBucket)`, not `OperationAborted` and not a retried flip. No dirent or inode exists
  for the key, and the session is `Open@E+2` with `attempts` restored.
  **(J) A flip the store refuses for good.** The wrapper answers the flip with a definite `Err`
  that is neither `Conflict` nor `CommitUnknownResult`. Complete sends no further flip and
  releases the fence (`Open@E+2`, `attempts` restored). It returns child-1's stopped error with
  that store error as its cause, saying the fence was released. No dirent or inode exists.
  **(G0-flip)** The wrapper answers the flip `CommitUnknownResult` (once applied, once not).
  Complete returns the in-doubt error carrying that outcome and sends no further commit.
  **(K) Flat only, for now.** Parts totalling more than `MAX_MAP_CHUNKS` chunks (`multipart.rs:4959`):
  Complete returns the stopped error with a cause naming segmented publication as unsupported.
  It sends zero commits when the pre-fence read already shows the excess. When only the frozen
  set shows it, the fence is released with `attempts` restored. child-3 replaces this leg with B.
- Repo + branch target: getwyrd/wyrd @ main
- Scope: in `crates/core/src/multipart/complete.rs`:
  - the public `CompleteMultipartUpload` function;
  - assembly of the ordered chunk list;
  - the flat publish through #653's committer, with the flip contribution (session →
    `Completed { completion }` with inode, version, ETag, `completed_at_millis`,
    `complete_fingerprint`; the obligations through `RetireObligation`, each under
    `require_absent` with a token per `0016:358-373`; the `bucket:` marker pin);
  - the lost-flip loop with dirent and marker re-read;
  - the release on exhaustion, gone bucket or refused flip;
  - the temporary flat-only refusal;
  - the docs sentence.

  Out of scope:
  - segmented publication and the segment-charging rule (child-3);
  - recovery after unknown outcomes (child-4);
  - the value ceiling (child-5);
  - the drain and `unlink` / `PutObject` routing (#659);
  - the reaper (#625);
  - the S3 verb (#670);
  - DST (#660);
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
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass —                as its own file to earn the full red->green.
- C4 diff coverage: changed lines executed by the patch's tests: fail — diff coverage not measured — patch.diff does not apply on origin/main
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): fail — 41 mutants tested in 2m: 1 missed, 22 caught, 18 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): pass — review-branch: 0 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_1008/review-
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 14.63s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Review of #1008’s flat multipart completion, retirement obligations and lost-flip handling: no grounded implementation defect found; prior-art confirmation and human sign-off decisions remain.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The brief defines falsifiable publication, retirement, retry and failure outcomes, and the supplied base contains the prerequisite fence, part-commit and test files; `brief.md:33`, `reviewer-target-evidence.log:16`. |
| C2 Reproduction (red pre-fix) | PASS | Stashing the implementation exposes the missing new API; six deliberate fault variants also fail at runtime, establishing behavioral reds beyond that compile failure; `reviewer-red.log:172`, `reviewer-probes.log:1`. |
| C3 Change | PASS | The four-file change supplies the scoped core operation and required record constructors without absorbing prerequisites or adding a production caller; `crates/core/src/multipart/complete.rs:1165`, `crates/core/src/multipart.rs:4224`, `docs/design/architecture/05-building-block-view.md:254`. |
| C4 Verification (red→green) | PASS | Restoring the candidate passes 594 core tests, including all 59 completion tests, plus core clippy, formatting, spelling and docs checks; the six behavioral reds return to green; `reviewer-verification.log:4`, `reviewer-probes.log:7`. |
| C5 Causal adequacy | PASS | Atomic fence/bucket pins and disjoint retirement scopes address the stated ownership failures directly; the required counterexamples fail, and the sole surviving scanner mutation is equivalent; `crates/core/src/multipart/complete.rs:1362`, `crates/core/src/multipart/complete.rs:1428`, `reviewer-probes.log:1`, `crates/traits/src/lib.rs:1523`. |
| T1 Structure | PASS | Publication composes the existing committer through the MetadataStore seam; record construction remains in multipart rather than introducing concrete-backend coupling; `crates/core/src/multipart/complete.rs:1225`, `crates/core/src/multipart.rs:4260`. |
| T2 Shape | PASS | Tests assert bytes, versions, obligation contents and fence states, rather than only successful calls or counts; the living architecture documents the new API; `crates/core/tests/multipart_complete.rs:3360`, `crates/core/tests/multipart_complete.rs:3513`, `docs/design/architecture/05-building-block-view.md:254`. |
| T3 Runtime | PASS | Flat-map admission and bounded flip/release loops limit work; clock stamps remain caller-supplied, and external awaits inherit the store’s termination contract; `crates/core/src/multipart/complete.rs:708`, `crates/core/src/multipart/complete.rs:1282`, `crates/core/src/multipart/complete.rs:1133`, `crates/traits/src/lib.rs:1334`. |
| T4 Contribution | NEEDS-HUMAN | Confirm affected-path prior art across merged history and closed/rejected work — this snapshot has one synthetic commit and no remote, while the brief cites only the parent’s check, so duplication or a previously declined approach cannot be excluded; `reviewer-target-evidence.log:2`, `brief.md:161`. |
| T5 Judgment | NEEDS-HUMAN | Confirm J’s release-and-stop response and K’s temporary flat-only refusal — the brief explicitly leaves these choices for sign-off, and they determine retry behavior and handling of larger uploads; `brief.md:189`, `crates/core/src/multipart/complete.rs:1249`, `crates/core/src/multipart/complete.rs:714`. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Accept flat completion as the intended core-only deliverable for this slice — byte preservation and forced-race outcomes are exercised, but whether this staged scope satisfies the intended milestone remains the maintainer’s decision; `brief.md:149`, `crates/core/tests/multipart_complete.rs:3330`. |

Source citations above resolve under `$PDCA_TARGET` (`target/`); brief and evidence citations resolve in this review directory. The candidate matches `patch.diff`: reverse-apply checking succeeds, the prerequisites exist in its base, and its tracked diff is byte-identical after stash/pop (`reviewer-target-evidence.log:16`, `reviewer-stash.log:1`). No candidate source was edited to address findings.

The independent experiments support the implementation’s causal claims. The stash experiment retained the new tests and failed with missing `complete_upload`/`PublishRequest` imports; it proves API absence only. The additional experiments used the scanner’s disposable copy and injected the six faults required by the brief. Each compiled and failed its named test at runtime; removing the faults restored all 59 completion tests (`reviewer-probes.py:1`, `reviewer-probes.log:1`).

| Probe | Observed failure | Evidence |
|-------|------------------|----------|
| C: retire every staged part’s records | Obligation names parts 1–3 instead of only 1 and 3. | `reviewer-probe-C.log:12` |
| F: freeze the retry’s inode and completion versions at the fixture’s fence-time value, 2 | Publication returns version 2 instead of 3 after the intervening overwrite. | `reviewer-probe-F.log:12` |
| F2: attempt retirement for an empty prior | Completion stops with RetireObligationOwesNothing instead of publishing. | `reviewer-probe-F2.log:12` |
| H2: remove the bucket pin | Completion publishes after bucket deletion instead of refusing. | `reviewer-probe-H2.log:12` |
| J: answer OperationAborted without releasing after a definite flip error | The expected stopped error and its store-error classification disappear. | `reviewer-probe-J.log:12` |
| G0: retry an unknown flip outcome | Completion returns AlreadyCompleted instead of the required in-doubt error. | `reviewer-probe-G0.log:12` |

The gate evidence has two non-defect caveats and one expected deferral:

- **Diff coverage remains unmeasured.** `C4-diff-cov` failed to apply the patch to `origin/main`; it recorded no coverage result. This is the stacked-base limitation described by the brief, not a compile or applicability defect in the supplied target (`gate-logs/C4-diff-cov.log:10`, `reviewer-target-evidence.log:23`).
- **The surviving mutation is equivalent.** The frozen scanner reports 22 caught, 18 unviable and one missed; my rerun reports 14 caught, 26 unviable and the same missed mutation. Local unviable variants include unused-binding/import errors under warnings-as-errors; unviable results are not behavioral kills. The survivor replaces `no_segments`’s `WriteBatch::new()` with `Default::default()`, exactly what `new()` calls (`gate-logs/C5-mutants.log:13`, `reviewer-mutants.log:20`, `crates/core/src/multipart/complete.rs:1309`, `crates/traits/src/lib.rs:1523`). No test gap follows from that survivor.
- **T4-contribution: N/A.** Contribution artifacts are intentionally drafted later, and their substantive check must rerun at publish. This deferred row creates no human-clearance item (`gate-logs/T4-contribution.log:10`).

The remaining frozen logs support their recorded outcomes within their stated scope. Full workspace CI reports successful build/test/lint, dependency audits, conformance and DST checks (`gate-logs/C4-ci.log:4135`, `gate-logs/C4-ci.log:4768`); the verification wrapper explicitly reports **green-only** (`gate-logs/C4-verify.log:161`). TiKV feature compilation completes successfully (`gate-logs/host-tikv.log:211`), and the batch-review wrapper reports zero blocking findings, without supplying its underlying review narratives (`gate-logs/T4-batch-review.log:10`). These are frozen-log judgments, distinct from the independent core checks recorded in `reviewer-verification.log:4`.

Both named external dependencies were exercised: real `typos` succeeded, and the renderer produced 101 pages with a successful link audit (`reviewer-verification.log:7`, `reviewer-doc-render.log:3`). The tests use real redb metadata transactions, composed UploadPart/ordinary overwrite operations, and placement-aware stored fragments; their wrappers actually force the relevant losing and unknown outcomes (`crates/core/tests/multipart_complete.rs:2895`, `crates/core/tests/multipart_complete.rs:3562`, `crates/core/tests/multipart_complete.rs:4093`). No unmet external dependency was found for the scoped proof. The explicit deferrals to #660, #1009, #1010, #1011 and #670 remain settled; no capability-probe symptom guard was introduced. No `INTEGRATION.md` was supplied or found under the target; this review applies the supplied standing rubric and the brief’s explicit sign-off questions.

<!-- pdca:leaf-complete -->

### Advisory — adversary

# Adversarial review: #1008 (flat publication, lost flips, bucket guard)

**Bottom line:** I could not break the flip's behaviour. All six of the brief's required reds go red when I re-apply them to the production code, and so does every extra mutation I tried except one (a branch nothing can reach today). One real finding: a coupling gap with #511's bucket deletion fence. The rest are notes.

## How I attacked it

I copied the patched target into scratch and ran `cargo test -p wyrd-core --test multipart_complete`: 59 passed. Then I re-applied each of the brief's demonstrated reds to `crates/core/src/multipart/complete.rs`, one at a time, and re-ran the tests:

| Red (brief) | Mutation | Leg that went red |
|---|---|---|
| `retire:records:` names every staged part | `published` built from `fence.frozen.keys()` (`complete.rs:1364`) | C (`multipart_complete.rs:3515`) |
| Version frozen at fence time | no target re-read after a lost flip (`complete.rs:1286-1289`) | F, F-exhausted, the read-fault leg |
| Generation obligation for an empty prior | drop the empty-flat early return (`complete.rs:1487`) | F2 |
| Bucket pin dropped | drop `.require(bucket_key, marker)` (`complete.rs:1431`) | H2 and A |
| Definite flip error answered `OperationAborted` with no release | `complete.rs:1255` | J |
| Unknown flip outcome treated as `Conflict` | `Err(unknown)` falls through to the lost-flip path (`complete.rs:1244-1248`) | G0-flip |

Extra mutations, all caught: checking the marker only for presence instead of exact bytes (H2's "recreated" case), no `require_absent` on obligations, no unnamed-parts obligation, no generation obligation (F and F3), ignoring a session that moved after a lost flip, no release on `R_PUBLISH` exhaustion, no release on H2, accepting a `part:` record that disagrees with its summary, `>=` instead of `>` in the flat-size check, and turning the flat-size check off.

The tests run the production path end to end: a real redb store, Create (#894), `upload_part` (#1027), the ordinary `write.rs` overwrite as the interloper, #653's committer, and `read_object`. None of it is a parallel copy of production logic.

## Findings

- NEEDS-HUMAN [impl] — `crates/core/src/multipart/complete.rs:1429-1431` hand-writes the bucket pin (`.require(bucket_key, fence.bucket_marker)`). It does not reuse `require_bucket` at `crates/core/src/multipart/create.rs:698-705`, which says it keeps the ADR-0046 §4 precondition "in this one place" and carries `// deferred: #511 … #511 adds the fence's require_absent here`. The flip has no `#511` marker of its own. 0016:663 asks the flip for "the bucket-existence pair of ADR-0046 §4" (marker **and** deletion fence), and 0016's failure-mode table says Complete "MUST NOT strand an object in a deleted bucket". Concrete failing case once #511 adds the fence to `require_bucket`: DeleteBucket installs `bucket-deleting:<name>`; its emptiness scan finds no dirent (the multipart object isn't published yet); this flip, which pins only the marker, commits the dirent; DeleteBucket then removes the fence and the marker. The result is an object stranded in a deleted bucket. Create picks up the fix automatically and the flip doesn't. The lost-flip check at `complete.rs:1273-1281` also reads only the marker. It is unreachable today (nothing writes a deletion fence, and no DeleteBucket exists in `crates/`). The cheap fix is to make `require_bucket` `pub(super)` and call it from `FlipPlan::flip`, or at least to add a `// deferred: #511` marker at `complete.rs:1431` so #511 finds this caller.

## Notes (no action needed to accept)

- `complete.rs:146-147` says the publication stamp "is one reading, and it stamps both records the flip publishes: that field and the root's ADR-0047 `modified`." That is not quite true when the prior is stamped at or after `completed_at_millis`. `overwrite_modified` (`metadata.rs:2429`) then sets the root to `prior + 1`, while `Completion.completed_at_millis` and the answered `Publication` keep the raw stamp. Example: prior stamped 3_000, Complete at 2_500 gives root `modified` 3_001 and session 2_500. Lines 111-113 of the same doc state the ordering correctly. No test covers this case (F's interloper stamps 1_200, below `COMPLETED_AT` 2_500). The fix is to reword the doc. Both values come from one clock, so the one-clock rule holds.
- The committer-refusal branch at `complete.rs:1251-1252` (`downcast_ref::<PublicationError>` → `Stop::Publication`) has no test. I changed it to answer `Stop::EpochExhausted` and all 59 tests still passed. The "at the last version" case (`multipart_complete.rs:3908-3946`) asserts `Stop::Publication(VersionOverflow)`, but that value comes from the pre-check at `complete.rs:1398`, not from the committer. With current inputs the committer can only refuse on value size, which is #1011's deferred ceiling, so this is low value.
- Gate verdicts in `check-gates.json`:
  - **C5 "1 missed"** (`no_segments -> Default::default()`, `complete.rs:1309`) is a mutant that changes nothing: `WriteBatch::new()` is `Self::default()` (`crates/traits/src/lib.rs:1523-1525`). It is not a test gap.
  - **C4-diff-cov "fail"** means coverage was never measured, not that it was low. The patch can't apply to `origin/main` because the base holds #1007. My mutation runs above stand in for it on the flip's branches.
  - **C4-verify "PASS (green-only)"** is what the brief predicted for a base that already holds #1007. I reproduced the per-fix reds myself (table above), so the green-only verdict is backed by evidence, not just assumed.

## What I tried that did not break it

- Collisions on obligation tokens. Each fence epoch mints its own `s:<id>:<E+1>` tokens. Every other writer that could install one has to move the session off the fence bytes, so its batch and the flip exclude each other.
- The CAS on the prior failing forever because of re-encoding. `InodeRecord` uses `skip_serializing_if` on its optional fields (`metadata.rs:1708-1712`), so decode→encode is byte-identical.
- An interloper creating or deleting the key between the fence and the flip. The re-read switches between `Fresh` and `Supersede`, and the version is recomputed each time.
- A session moved by a rival, the reaper or a teardown during the flip. The flip's exact-bytes pin fails and the request is answered from the new state with no release (tested).
- Flip batch size. It stays within 0016's "≤ 4·V + O(1)". A value over the ceiling is #1011's (deferred).
- The flat-size check summing only named parts. Validation guarantees every named part is in the frozen set, and `FlipPlan::read` requires each record's chunk count to match its summary.
- A production caller sneaking in. `complete_upload` and `PublishRequest` appear only in `multipart.rs`, `complete.rs` and the test file.

<!-- pdca:leaf-complete -->

### Advisory — code-review

No findings: the diff is clean on introduced correctness bugs and actionable reuse, simplification, or efficiency issues within this slice’s scope.

The frozen mutation survivor at `crates/core/src/multipart/complete.rs:1310` is equivalent: `WriteBatch::new()` delegates to `Self::default()` (`crates/traits/src/lib.rs:1523`). It does not demonstrate a test gap.

Validation used the frozen gate evidence; no tests were rerun. CI and core tests passed. Diff coverage was not measured because the patch did not apply to `origin/main`.

<!-- pdca:leaf-complete -->

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [x] T4 Contribution — Confirm affected-path prior art across merged history and closed/rejected work — this snapshot has one synthetic commit and no remote, while the brief cites only the parent’s check, so duplication or a previously declined approach cannot be excluded; `reviewer-target-evidence.log:2`, `brief.md:161`.
- [x] T5 Judgment — Confirm J’s release-and-stop response and K’s temporary flat-only refusal — the brief explicitly leaves these choices for sign-off, and they determine retry behavior and handling of larger uploads; `brief.md:189`, `crates/core/src/multipart/complete.rs:1249`, `crates/core/src/multipart/complete.rs:714`.
- [x] Validation — fitness-to-purpose — Accept flat completion as the intended core-only deliverable for this slice — byte preservation and forced-race outcomes are exercised, but whether this staged scope satisfies the intended milestone remains the maintainer’s decision; `brief.md:149`, `crates/core/tests/multipart_complete.rs:3330`.
- [x] `crates/core/src/multipart/complete.rs:1429-1431` hand-writes the bucket pin (`.require(bucket_key, fence.bucket_marker)`). It does not reuse `require_bucket` at `crates/core/src/multipart/create.rs:698-705`, which says it keeps the ADR-0046 §4 precondition "in this one place" and carries `// deferred: #511 … #511 adds the fence's require_absent here`. The flip has no `#511` marker of its own. 0016:663 asks the flip for "the bucket-existence pair of ADR-0046 §4" (marker **and** deletion fence), and 0016's failure-mode table says Complete "MUST NOT strand an object in a deleted bucket". Concrete failing case once #511 adds the fence to `require_bucket`: DeleteBucket installs `bucket-deleting:<name>`; its emptiness scan finds no dirent (the multipart object isn't published yet); this flip, which pins only the marker, commits the dirent; DeleteBucket then removes the fence and the marker. The result is an object stranded in a deleted bucket. Create picks up the fix automatically and the flip doesn't. The lost-flip check at `complete.rs:1273-1281` also reads only the marker. It is unreachable today (nothing writes a deletion fence, and no DeleteBucket exists in `crates/`). The cheap fix is to make `require_bucket` `pub(super)` and call it from `FlipPlan::flip`, or at least to add a `// deferred: #511` marker at `complete.rs:1431` so #511 finds this caller.
- [x] **The declared base does not contain the prerequisite this slice extends.** `brief.md:6-7` assumes a base that folds in child-1; `brief.md:28-29` requires its existing test wrapper, and `brief.md:123-126` derives “green-only” verification from appending to its test file. Yet the supplied target is `376b106248d2ac2e816c5d258609e04de09a425f` (`HEAD = main = origin/main`), where `crates/core/src/multipart.rs:153-155` declares only `abort`, `create` and `stage`; neither `crates/core/src/multipart/complete.rs` nor `crates/core/tests/multipart_complete.rs` exists in that tree. `dependency-state.json:2-5` marks #1007 `COMPLETE`, but that does not establish its presence on this base. Revise the target/stack instructions to identify how #1007's implementation enters the execution base, and make the append-only verification claim conditional on that resolved tree. Otherwise Do must absorb the fence implementation and create the test file, changing both scope and the stated verification posture. The repo and `main` resolve; both declared dependency IDs exist, with #1027 still `PLANNED` (`dependency-state.json:7-10`).
- [x] size backstop — this slice is behaving oversized: patch is 147 KB (threshold 100 KB). Recommend answering `iterate-plan` at sign-off and authoring the split in the re-plan (`pdca split`), rather than `iterate-do`: a slice that is too big yields implementation-shaped findings every round, and splitting authors briefs, which is Plan's beat.

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
- Plan advisory: 1 finding(s); brief revised: yes (plan-advisory-*.md)
- (empty is the common case)
- #511 follow-up: Complete's flip hand-writes the bucket pin (`crates/core/src/multipart/complete.rs:1429-1431`, lost-flip re-read `:1273-1281`) instead of calling `require_bucket` (`create.rs:698-705`); #511 must cover this caller when it adds the deletion fence.
