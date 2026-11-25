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
- C4 diff coverage: changed lines executed by the patch's tests: pass — diff coverage 99.0% — 388 of 392 instrumentable changed lines executed (floor 80%); 392 of 1063 changed lines were instr
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): pass — 72 mutants tested in 3m: 47 caught, 25 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): pass — review-branch: 0 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_1007/review-
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 15.29s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

No confirmed implementation defect found in #1007's multipart Complete fence, frozen-part validation, release, and refundable attempt budget; the sign-off decisions below remain.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | Acceptance clearly separates client rejection, another publisher's fence, definite failure, and unknown commit outcomes, preserving retriable uploads without publishing in this slice (`brief.md:88`, `brief.md:162`; `docs/design/proposals/draft/0016-multipart-commit-protocol.md:994`). |
| C2 Reproduction (red pre-fix) | N/A | This is a new API, with no callable pre-fix fence implementation; the retained test fails at its missing import rather than reproducing a behavioral failure (`brief.md:126`; `review-red.log:2`). |
| C3 Change | PASS | The five affected files stay within the authorized fence phase and accompanying tests/docs; no production caller or publication path was introduced, and the living architecture describes the new API (`crates/core/src/multipart/complete.rs:557`; `docs/design/architecture/05-building-block-view.md:248`). |
| C4 Verification (red→green) | NEEDS-HUMAN | Decide whether the declared new-API verification exception and frozen full-CI evidence suffice — the independent restored run passes 43 tests, but the reverted test never executes and the disposable target lacks the advisory-db gitlink (`brief.md:133`; `review-restored-green.log:50`; `review-ci.log:3353`; `gate-logs/C4-ci.log:4538`). |
| C5 Causal adequacy | PASS | The budget is restored from the session actually fenced, including after a competing move to a later Open; exact-byte release preconditions preserve ownership, and the independent mutation results resolve to 47 caught / 25 unviable / zero survivors (`crates/core/src/multipart/complete.rs:600`, `crates/core/src/multipart/complete.rs:887`; `crates/core/tests/multipart_complete.rs:1733`; `review-mutants-summary.txt:5`). |
| T1 Structure | PASS | The core operation depends on the MetadataStore trait and existing record/answer helpers; the held-fence value has controlled construction, and no concrete backend or shared mutable global enters production code (`crates/core/src/multipart/complete.rs:98`, `crates/core/src/multipart/complete.rs:194`). |
| T2 Shape | PASS | State-first answers and shared fingerprint validation avoid divergent ordering rules; error states distinguish a standing, released, moved, and unreadable fence. The larger test suite remains within the explicit do-not-split decision (`crates/core/src/multipart/complete.rs:324`, `crates/core/src/multipart/complete.rs:689`; `brief.md:214`). |
| T3 Runtime | PASS | At most four fence/rejection-release rounds, 128-entry pages with strict cursor progress, and the backend termination contract bound work; the request supplies the lifecycle clock and mismatched sources are rejected (`crates/core/src/multipart/complete.rs:117`, `crates/core/src/multipart/complete.rs:585`, `crates/core/src/multipart/complete.rs:780`; `crates/traits/src/lib.rs:1334`). |
| T4 Contribution | NEEDS-HUMAN | Confirm the parent's rejected-work comparison or supply its file-level record — merged history and closed/unmerged searches ran for all five affected paths, but indexed PR text cannot establish the discarded #636 patch's file-level disposition (`brief.md:155`; `review-prior-art-paths.json:1`; `review-prior-art-closed.json:36`). |
| T5 Judgment | NEEDS-HUMAN | Settle the explicitly reserved same-name bucket-recreation policy — a session retains no original bucket marker and validation accepts the current marker, so an old upload is not bound to its original bucket incarnation; this remains a scope decision, not a requested implementation change (`brief.md:229`; `crates/core/src/multipart.rs:2364`; `crates/core/src/multipart/complete.rs:749`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Approve the public held-fence API and fence-only delivery boundary — these tests establish the core phase, while usable S3 completion and publication remain later slices; visibility may need narrowing when those callers arrive (`brief.md:145`, `brief.md:188`; `crates/core/src/multipart/complete.rs:194`). |

The implementation addresses the previous sign-off's test gaps and documentation mismatches. The release-success-after-conflict test pins four rounds independently; another-parent requests are checked across states; a retried fence restores the later session's attempt count; and combined failures pin bucket-first and first-bad-part behavior (`crates/core/tests/multipart_complete.rs:1219`, `:922`, `:1733`, `:1805`, `:1855`). Store faults retain their cause through an unknown release, and the docs now describe the moved-session answer exception (`crates/core/src/multipart/complete.rs:60`, `:75`, `:914`; `crates/core/src/multipart/abort.rs:52`). No capability-probe or load-time symptom guard was added. The explicit DST deferral to #660 is settled under the standing rubric (`crates/core/src/multipart/complete.rs:495`).

Independent evidence and its limits:

- **Behavior:** `cargo test --offline -p wyrd-core --test multipart_complete` passed 43 tests. Stashing production changes while retaining the test produced E0432; popping the stash restored 43 passing tests. This confirms the declared absence of a behavioral pre-fix red, not a red→green regression proof (`review-red.log:2`, `review-restored-green.log:50`; frozen counterpart `gate-logs/C4-verify.log:15`). The source still matches `patch.diff`: `git apply --reverse --check ../patch.diff` succeeds, and the temporary stash is gone.
- **CI and toolchain:** independent typos, formatting, docs lint/render, feature compilation, conformance, and DST checks ran. Both TiKV feature clippy commands passed (`review-tikv.log:103`, `:202`). Full `cargo xtask ci` exited 1 with exactly two failures attributable to the snapshot's absent `.github/advisory-db` gitlink (`review-ci.log:3998`); the patch does not touch that path. The frozen log actually shows the pin test, pinned advisory database, dependency walls, and final CI success (`gate-logs/C4-ci.log:3454`, `:3894`, `:3905`, `:4538`). This is a target-state caveat, not a C4 patch defect. The brief's external dependency, `typos`, was exercised (`review-ci.log:2`).
- **Mutation and coverage:** the first independent mutation run caught 32 mutants and rejected 40 at build time. Fifteen were rejected only because warnings are denied; a targeted `--cap-lints true` rerun caught all fifteen, plus four already-caught variants. Combined, the original 72 have 47 caught and 25 genuinely unviable, matching the frozen aggregate (`review-mutants-summary.txt:1`; `gate-logs/C5-mutants.log:13`). Independent LLVM coverage ran all 43 tests and measured 374/377 lines in the new module, 99.2% (`review-coverage-summary.txt:3`). The frozen diff-specific metric is separately 388/392 instrumentable changed lines, 99.0%, with 671 changed lines unscored (`gate-logs/C4-diff-cov.log:69`, `:86`); unscored lines are not claimed covered.
- **Contribution evidence:** the frozen batched review records zero blocking findings, without supplying its individual review transcripts (`gate-logs/T4-batch-review.log:10`). **T4-contribution: N/A** — publication artifacts are absent by design, and their substantive audit must rerun at publish (`gate-logs/T4-contribution.log:10`). This deferral creates no human-clearance item. Path-filtered merged history confirms the existing multipart prerequisites and no prior version of either new file. Closed/unmerged PR searches returned no indexed path matches, and [issue #636](https://github.com/getwyrd/wyrd/issues/636) is closed as not planned; its discarded patch is outside the supplied evidence, which explains the narrower T4 decision above.

Source citations are relative to `$PDCA_TARGET`; brief and evidence citations are relative to this review directory. No source fix was made. No `INTEGRATION.md` is present in the supplied target or bundle; the supplied standing rubric was applied.

<!-- pdca:leaf-complete -->

### Advisory — adversary

# Adversarial review: issue #1007, Complete fence

I could refute one behaviour and two test claims. Fence ownership, release and the budget held up. All probes and mutations ran on a scratch copy of the target (since removed). The suite passes there: 43 of 43 tests.

## Findings

- NEEDS-HUMAN [impl] — `crates/core/src/multipart/complete.rs:609`: **a pre-fence rejection answers from a session state that may be out of date.** The session is read at `complete.rs:570`, the list is checked at `:607`, and a rejection returns at `:609` without looking at the session again. So "the state answers first" (`complete.rs:687`) only holds at the moment of the first read.
  - Probe: a store wrapper lets the first `mpu:` read return `Open`, then writes a `Completed` tombstone (the record a published session leaves behind) and deletes the `psum:` and `part:` records, as the retirement drain would.
  - Result: an identical retry answers `Refused(InvalidPart { part_number: 1, reason: Absent })`. The same request sent again answers `AlreadyCompleted`, which is what the `0016:976` table requires.
  - Variants: a tombstone plus a deleted bucket answers `NoSuchBucket`. `Aborting` plus deleted `psum:` records answers `InvalidPart` where the table says `NoSuchUpload`.
  - Impact: the client is told a published upload failed. The window is small and needs publication and the drain (#1008, #659), which have not landed, so nothing can hit this in production today. It would be cheap to fix now: on a pre-fence rejection, re-read `mpu:`, and if the bytes differ from `read`, go round the loop and answer from the new state. That costs one read and no commit. The sign-off note says to keep the logic "unless a new test proves otherwise", and this probe is such a test.

- NEEDS-HUMAN [impl] — `crates/core/tests/multipart_complete.rs:2437`: **this test's claim about the cause can't fail.** Its doc comment says "the cause stays the fault that stopped validation", and `complete.rs:75` makes the same promise for every error in the module doc's table. But `assert_read_fault` (`multipart_complete.rs:661`) only compares the text to `FAULT`, and the failing scan and the failing re-read both produce that same text.
  - Mutation: at `complete.rs:896`, report the re-read fault as the cause and drop the validation fault. All 43 tests still pass.
  - This is the sign-off's iteration-2 item 6 (keep the scan fault visible), so the promise is untested on this row.
  - Fix: give the armed scan fault and the armed get fault different text, and assert the scan fault's text here.

- NEEDS-HUMAN [impl] — `crates/core/tests/multipart_complete.rs:1101`: **no test shows the held bucket marker comes from the read behind the fence.** `complete.rs:251` documents it as "read behind the fence", and the brief wants it for the publication's precondition. The only value assertion compares it to a `MARKER` constant that never changes.
  - Mutation: return the marker from the pre-fence read at `complete.rs:661`. All 43 tests still pass.
  - The same mutation for the frozen part set is caught by `the_held_fence_carries_the_set_read_behind_the_fence` (`multipart_complete.rs:1970`). The marker has no matching leg.
  - Fix: rewrite `bucket:` with new bytes just before the fence commits, and assert the held fence carries the new bytes.

- NEEDS-HUMAN [human] — `crates/core/src/multipart/complete.rs:39`: **the patch narrows the brief, and a missing `part:` record now passes.** The brief's scope (`brief.md:107`) says "the frozen `psum:` / `part:` ranges", but the patch reads only `psum:`.
  - Probe: delete `part:` for a named part and keep its `psum:` record. The request returns `Held`. A `part:` record whose digest disagrees with its `psum:` record is also `Held`.
  - The part commit writes both records in one batch (`0016:660`), so reaching this needs store damage, and the module doc gives a sound reason (chunk lists can reach tens of megabytes). But it does sit in the rubric's "absent entries, never silent success" class, and the valid-list path has no test that shows the choice.
  - Decision needed: confirm `psum:`-only validation, and confirm that #1008 must treat a missing `part:` behind a held fence as an error. A key-only presence check of `part:` is the cheaper alternative if you want this slice to catch it.

- NEEDS-HUMAN [human] — `check-gates.json:104-113`: **the logs don't show that the T4 review did real work.** The gating row is `pass`, but `gate-logs/T4-batch-review.log` shows three passes over a 176 KB patch finishing in 19.25 seconds, with "0 blocking, 0 recorded-rejected, 0 noise-dropped" and no per-pass counts. Zero noise dropped from three passes is unusual. This may be fine (a cached or parallel run, say), but I did not have `review-batch.md`, the file that would settle it. Please check it before counting this row as a deep review.

## Tried and could not refute

- **Red-to-green evidence.** C4-verify is `unverifiable` because the module doesn't exist on the base, which the brief declared in advance. That can't be reproduced as a plain revert, so I forced the reds through the production code instead. Each of these made the suite fail:
  - a rejected request that doesn't release (15 tests);
  - attempts not restored;
  - a definite release failure answered as a refusal (D4);
  - an unknown fence treated as success (G0);
  - a release or fence commit with its `require` removed.

  I did not separately run the brief's other two reds: a rejected request charged against `attempts` (D1), and a request that releases another publisher's fence (E). D1's zero-commit assertions and E's byte-identity assertions should catch them. The tests run over real redb through the production `fence_complete`, with no parallel copy.
- **Mutation depth beyond the C5 gate.** 42 hand-written mutations ran, all caught except the two above and two I discount: one is equivalent to the original code, and one only truncates the walk past 200 parts, deeper than the two-page tests go. The bounds `RELEASE_ROUNDS` and `FENCE_ROUNDS` are pinned by numbers written in the test: a release bound off by one in either direction fails, and so does one extra fence round.
- **C-1 (no answer leaves this Complete's own fence up).** I walked every return path in `release` and `moved` (`complete.rs:875-953`). Each answer finds the fence down, and only an error can leave it standing. `NotPlaced` is never reported after a `Committed` fence.
- **Serialization identity.** A session with a `content_type` and an object name containing a quote and a non-ASCII character went through fence and release. The stored bytes differ from the originals only in `epoch` and still decode as canonical.
- **Epoch and attempts edges, and fence lost to a later `Open`.** From reading the code and its existing legs (I wrote no new probe here): the bound checks and the re-read after a lost fence hold, and the release restores the attempts of the session actually fenced.
- **Paging.** Clause 4 of the `scan_page` contract permits duplicates, while `complete.rs:795` treats one as a store fault. Clauses 1 and 2 make a repeat at or below the cursor impossible on a conforming backend, so this is not a false fault in practice.
- **Top-level `downcast_ref::<CommitUnknownResult>` (`complete.rs:630`, `:909`).** The trait documents this as how to recognise that class on any backend (`crates/traits/src/lib.rs:216`), so a wrapped outcome is outside the contract.
- **Not raised, as settled or out of scope:** DST coverage (deferred to #660 in the code), the deleted-and-recreated bucket question (open with the human per the sign-off), and the `bucket` field not being cross-checked against the session's object (the session stores no bucket, and the S3 verb composes both from one path).

<!-- pdca:leaf-complete -->

### Advisory — code-review

- NEEDS-HUMAN [impl] — **P2: Refresh the timestamp immediately before each fence commit.** `crates/core/src/multipart/complete.rs:598` stamps every attempt with the immutable `request.fenced_at_millis`, supplied before the initial read, validation scans, and any conflict retries (`:607`, `:624`). Those waits therefore count toward the persisted fence's age before it is acquired. If they consume `W_completing`, a newly committed fence is already eligible for rollback under the specified reaper rule, shortening or eliminating the publisher's window and potentially charging an attempt for a premature rollback. Decision 3 explicitly measures this window from the fence instant (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:1005`). Accept a caller-provided clock seam and sample the session's clock after validation on each attempt. Add a deterministic test advancing that clock during a scan/conflict retry and asserting the committed stamp uses the later reading. The reaper is deferred to #625, but this slice persists the timestamp it will consume.

No additional correctness findings or actionable reuse, simplification, or efficiency findings. Review used the target source and frozen gate evidence; tests were not rerun. The recorded suite passes all 43 tests, and mutation testing reports 47 caught and 25 unviable mutants, with none missed.

<!-- pdca:leaf-complete -->

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [x] C4 Verification (red→green) — Decide whether the declared new-API verification exception and frozen full-CI evidence suffice — the independent restored run passes 43 tests, but the reverted test never executes and the disposable target lacks the advisory-db gitlink (`brief.md:133`; `review-restored-green.log:50`; `review-ci.log:3353`; `gate-logs/C4-ci.log:4538`).
- [x] T4 Contribution — Confirm the parent's rejected-work comparison or supply its file-level record — merged history and closed/unmerged searches ran for all five affected paths, but indexed PR text cannot establish the discarded #636 patch's file-level disposition (`brief.md:155`; `review-prior-art-paths.json:1`; `review-prior-art-closed.json:36`).
- [x] T5 Judgment — Settle the explicitly reserved same-name bucket-recreation policy — a session retains no original bucket marker and validation accepts the current marker, so an old upload is not bound to its original bucket incarnation; this remains a scope decision, not a requested implementation change (`brief.md:229`; `crates/core/src/multipart.rs:2364`; `crates/core/src/multipart/complete.rs:749`).
- [x] Validation — fitness-to-purpose — Approve the public held-fence API and fence-only delivery boundary — these tests establish the core phase, while usable S3 completion and publication remain later slices; visibility may need narrowing when those callers arrive (`brief.md:145`, `brief.md:188`; `crates/core/src/multipart/complete.rs:194`).
- [x] `crates/core/src/multipart/complete.rs:609`: **a pre-fence rejection answers from a session state that may be out of date.** The session is read at `complete.rs:570`, the list is checked at `:607`, and a rejection returns at `:609` without looking at the session again. So "the state answers first" (`complete.rs:687`) only holds at the moment of the first read.
- [x] `crates/core/tests/multipart_complete.rs:2437`: **this test's claim about the cause can't fail.** Its doc comment says "the cause stays the fault that stopped validation", and `complete.rs:75` makes the same promise for every error in the module doc's table. But `assert_read_fault` (`multipart_complete.rs:661`) only compares the text to `FAULT`, and the failing scan and the failing re-read both produce that same text.
- [x] `crates/core/tests/multipart_complete.rs:1101`: **no test shows the held bucket marker comes from the read behind the fence.** `complete.rs:251` documents it as "read behind the fence", and the brief wants it for the publication's precondition. The only value assertion compares it to a `MARKER` constant that never changes.
- [x] `crates/core/src/multipart/complete.rs:39`: **the patch narrows the brief, and a missing `part:` record now passes.** The brief's scope (`brief.md:107`) says "the frozen `psum:` / `part:` ranges", but the patch reads only `psum:`.
- [x] `check-gates.json:104-113`: **the logs don't show that the T4 review did real work.** The gating row is `pass`, but `gate-logs/T4-batch-review.log` shows three passes over a 176 KB patch finishing in 19.25 seconds, with "0 blocking, 0 recorded-rejected, 0 noise-dropped" and no per-pass counts. Zero noise dropped from three passes is unusual. This may be fine (a cached or parallel run, say), but I did not have `review-batch.md`, the file that would settle it. Please check it before counting this row as a deep review.
- [x] **P2: Refresh the timestamp immediately before each fence commit.** `crates/core/src/multipart/complete.rs:598` stamps every attempt with the immutable `request.fenced_at_millis`, supplied before the initial read, validation scans, and any conflict retries (`:607`, `:624`). Those waits therefore count toward the persisted fence's age before it is acquired. If they consume `W_completing`, a newly committed fence is already eligible for rollback under the specified reaper rule, shortening or eliminating the publisher's window and potentially charging an attempt for a premature rollback. Decision 3 explicitly measures this window from the fence instant (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:1005`). Accept a caller-provided clock seam and sample the session's clock after validation on each attempt. Add a deterministic test advancing that clock during a scan/conflict retry and asserting the committed stamp uses the later reading. The reaper is deferred to #625, but this slice persists the timestamp it will consume.
- [x] C4 per-fix red->green: this patch's test red pre-fix, green post-fix unverifiable —                why this slice has no isolable red (the cargo output is above).
- [x] size backstop — this slice is behaving oversized: patch is 172 KB (threshold 100 KB). Recommend answering `iterate-plan` at sign-off and authoring the split in the re-plan (`pdca split`), rather than `iterate-do`: a slice that is too big yields implementation-shaped findings every round, and splitting authors briefs, which is Plan's beat.

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
- (empty is the common case)
- #1008 must carry from #1007: on a pre-fence rejection, re-read the session and answer from any new state (`complete.rs:609`).
- #1008 must carry from #1007: give the scan and re-read faults different text, so the cause test can fail (`multipart_complete.rs:2437`).
- #1008 must carry from #1007: test that the held bucket marker comes from the read behind the fence (`multipart_complete.rs:1101`).
- #1008 must carry from #1007: treat a missing `part:` record behind a held fence as an error (validation reads only `psum:`, `complete.rs:39`).
- T4 batch review ran 3 passes over 176 KB in 19 seconds with no per-pass record; make `review-branch` keep per-pass evidence so its depth can be checked.
- #625 / #670: stamp `fenced_at_millis` as late as possible; a stamp taken before reads and retries makes the fence look older and risks early rollback (`complete.rs:598`).
