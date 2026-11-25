# Result — issue 1027 / multipart-part-commit-definite

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect: Staging can stage a part's chunks, but nothing turns them into a part. This slice adds
  the commit as `wyrd-core` functions for every case where no commit outcome needs classifying. It
  (1) commits the `part:` + `psum:` pair, the slot release and the `sidx:` deletes in one batch after
  a last renewal, sent only with the dispatch margin left on the lease. It (2) replaces a committed
  part number, retiring the prior part's chunks in the same batch. It (3) refuses a part past the
  per-part or cumulative ceiling and compensates. It (4) compensates a settled stop reached before
  any commit was sent. It (5) composes the whole `UploadPart`, answering in the landed vocabulary
  (`UploadPartOutcome` `crates/core/src/multipart.rs:4669`, `Refusal` `:4550`). A `Conflict` or an
  unknown outcome on a sent batch stops safely, and later slices replace those two arms.
- Success criterion: A NEW test file `crates/core/tests/multipart_upload_part_commit.rs`, over
  the same harness as `crates/core/tests/multipart_upload_part_stage.rs`:
  `RedbMetadataStore::in_memory()`, a test-local placement-aware chunk store, a `ManualClock`, and a
  store wrapper that logs batches and can inject `Conflict`, `CommitUnknownResult`, a held batch
  landed later, a clock move, or a write at a chosen point. Sessions come from #894's Create and
  parts from `stage_part`. The test shows all of the following.
  **(A) Commit.** Parts staged **out of order** (3, then 1, then 2), at least one of more than one
  chunk, each commit to `Committed { part_number, digest }`. After each, `part:` and `psum:` decode
  and agree (chunk count, length, digest), and no `sidx:` entry or `slot:` record of that attempt
  remains. The log shows the last renewal **before** the commit's reads, and no renewal between those
  reads and the commit (`0016:1720-1733`). A rollback before the quiesce renewal is adopted by that
  renewal, and the part commits under the live epoch.
  **(D) Re-upload and ceilings.**
  - A re-upload replaces the part under `require(part == prior)` and puts the prior's chunks in one
    `retire:bytes:` obligation in the same batch, under the live epoch when the session has moved up.
  - An empty re-upload over an empty prior writes no `retire:` key. An empty re-upload over a
    non-empty prior writes one. `RetireObligation` keeps refusing an empty list (`multipart.rs:3752`;
    `crates/core/tests/multipart_retire_obligation.rs:467-473`).
  - **The per-part ceiling is `MAX_PART_CHUNKS` (`multipart.rs:5037`), and the commit enforces it.**
    `stage_part` cannot: `StageRequest.max_part_chunks` is the caller's, checked only against the
    renewal budget (`stage.rs:325-329`, up to 248 today), and a whole body within that limit returns
    `chunk_bound_exceeded() == false` (`stage.rs:2100-2126`). Two witnesses, each through `commit_part`
    and through the composed `upload_part`: a body truncated at the bound
    (`StagedAttempt::chunk_bound_exceeded`, `stage.rs:534`), and a **whole** body of
    `MAX_PART_CHUNKS + 1` chunks staged with the limit set to `MAX_PART_CHUNKS + 1`. Each answers
    `Refused(EntityTooLarge { .. })` and compensates in one batch, and no `part:` record exists
    afterwards. A whole body of exactly `MAX_PART_CHUNKS` chunks commits.
  - The cumulative ceiling is a read-time check, and 0016 allows the race it leaves (`0016:1441-1451`;
    `multipart.rs:5108`). Past `MAX_STAGED_CHUNKS` the commit refuses and compensates. **Witness:**
    `psum:` planted one chunk below the ceiling, two attempts on two part numbers each read that
    headroom (the wrapper holds the first commit until the second has committed), both commit to
    `MAX_STAGED_CHUNKS + 1`, and a third refuses. A re-upload's own prior is left out of the sum: with
    `psum:` at `MAX_STAGED_CHUNKS - 1` and a committed one-chunk part 1, a one-chunk re-upload of part
    1 commits.
  **(F) Every commit is sent with the margin left** (`stage.rs:376-378`, `0016:1726-1728`). The
  clock moves by hook. With exactly `E_TX_MILLIS` (`multipart.rs:5118`) left the commit is sent; with
  one millisecond less it is not. Reads that use up the lease send no commit and compensate. On
  **every** leg, the wrapper fails the test if a part commit is sent with less than that margin on the
  lease the attempt last wrote.
  **(G) A settled stop before any commit compensates** (`0016:1420-1431`, `:673`). The quiesce renewal
  stopping with `CoverShort`, `RenewalRoundsSpent`, `LeaseLapsed` or `InvalidRequest`, and the lease
  short before the commit: the store then holds one obligation for the attempt's own chunks (none for
  an empty part), no slot and no `sidx:` entry. The answer keeps the original stop's error class and
  reports that the slot was released. **Cap witness:** `MAX_INFLIGHT_PARTS` + 1 attempts each end in a
  settled stop, the held-slot count is 0, and the next reserve succeeds instead of `SlowDown`
  (`stage.rs:1137`). **Not compensated:** a renewal `StageError::InDoubt`, `Stop::SlotLost`,
  `Stop::Record`, `Stop::Store`: the log shows no compensation batch and the answer claims no release.
  **`Stop::EntryLost` is a staging stop, so it has its own leg.** Its one construction is intent
  settlement (`stage.rs:2361-2381`), before a `StagedAttempt` exists. Through the composed `upload_part`:
  the wrapper applies a chunk's intent, answers it unknown, and deletes that `sidx:` entry before the
  re-read. The answer is the stage error with `EntryLost`, and the log shows no commit and no compensation.
  **An owned entry lost after staging is not `EntryLost`.** Delete one between staging and the quiesce,
  with the session and slot unchanged and the clock live: the renewal requires that entry
  (`stage.rs:1616-1627`), conflicts every round, finds the session and slot as they were (`:1793-1806`),
  and ends `RenewalRoundsSpent` (`:1652-1664`). That compensates like any other settled stop, and the
  obligation names all the attempt's chunks. Add no new detection path.
  **(H2) A damaged session adopted by the renewal is never compensated under** (attempt 7's finding
  2). Run each on a renewal that ends covered, ends `RenewalRoundsSpent` and ends `CoverShort`, with an
  `Open` record planted under other bytes at the same epoch, and at a **lower** epoch (staged at epoch
  2, planted at 0). The answer is `Stop::Record(SessionEpochNotRisen)`, with no commit and no
  compensation. A renewal `StageError::InDoubt` in the same history keeps its doubt.
  **(H3) The slot guard names the submitter** (attempt 7's finding 4). On every batch in every leg,
  the wrapper checks a batch naming a `slot:` key against the attempt that submitted it. The
  submitter's identity is supplied to the wrapper apart from the batch and from the slot's current
  occupant. Two shapes:
  - **A reservation** has no prior bytes (`stage.rs:1325-1329`): it requires the slot absent, and the
    value it puts names the submitter's `attempt_id`.
  - **Every later batch** requires exactly the slot bytes the submitter holds from its own last
    landed write.

  A named leg shows the guard rejects attempt A's batch carrying attempt B's bytes, after A released
  the index and B reserved it.
  **(H4) The stops later slices replace.** A commit answered `Conflict` returns a typed stop that says
  the slot is still held, with no compensation and no second commit. A commit or compensation answered
  `CommitUnknownResult` returns `CommitError::InDoubt` keeping the flag as reported, with no further
  batch, even for `may_still_commit: false`, since it may have landed. A compensation answered
  `Conflict` returns a typed stop that claims no release. An in-doubt error through the composed
  `upload_part`, read through `wyrd_traits::classify` (`crates/traits/src/lib.rs:786-803`), is
  `Indeterminate`.
  **(E) The whole gate.** `cargo xtask ci` is green on the patched tree.
- Repo + branch target: getwyrd/wyrd @ main
- Scope: the commit half of `UploadPart` as store-I/O functions in `wyrd-core`, for outcomes
  needing no classification: the commit and its last renewal; the lease check before the commit; the
  re-upload with the prior part's chunks retired; the per-part and cumulative refusals; the live-session
  compensation of a stop reached before a commit is sent; and the composed `UploadPart`. Also: the
  writer-side constructor the part record lacks, its `psum:` summary derived from it, the per-part
  `{chunks}` row of `RetireObligation`, the corrected doc of `StageOutcome::Staged` (`stage.rs:363`),
  the docs paragraph, and the new test file. / out of scope:
  - a second commit round, the classification of a `Conflict`, the epoch chase and `Refused(OperationAborted)` (657.2b);
  - settling a commit's unknown outcome (657.2c) or a compensation's (657.2d);
  - any behaviour change in `stage.rs`, and any new `StageOutcome` variant. The doc fix above is the
    whole of this slice's edit to that file. A stop during staging keeps leaving its residue for the reaper;
  - Complete and publication (#658, #653); draining obligations and the terminal delete (#659); the
    reaper (#625); seeded DST races (#660);
  - the gateway / S3 verb and any production caller (#669 under #508);
  - `crates/core/src/lib.rs`, `docs/design/architecture/08-crosscutting-concepts.md`; any edit to
    proposal 0016 or to any ADR; `Cargo.lock` and `deny*.toml`.

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
- C4 diff coverage: changed lines executed by the patch's tests: fail — diff coverage not measured — patch.diff does not apply on origin/main
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): pass — 49 mutants tested in 4m: 34 caught, 15 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): pass — review-branch: 0 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_1027/review-
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 17.37s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Review of #1027: add multipart part commit, replacement, refusal compensation, and composed `UploadPart`; no implementation defect found, with verification and fitness-to-purpose decisions remaining for sign-off.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The brief defines observable outcomes and fragment-retention, ownership, and dispatch invariants for this core-only slice, consistent with the protocol's atomic commit and compensation rows (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:660`, `:673`). |
| C2 Reproduction (red pre-fix) | N/A | This is new functionality: reverting production removes the callable API, so the retained test cannot execute; the independent run confirms compilation failure rather than a behavioral red (`reviewer-evidence/red.log:2`; `crates/core/tests/multipart_upload_part_commit.rs:44`). |
| C3 Change | PASS | All five affected files match the supplied patch, stay within the declared surface, and document the new core operation; staging's executable behavior is unchanged (`reviewer-evidence/source-integrity.log:1`; `crates/core/src/multipart/stage.rs:362`; `docs/design/architecture/05-building-block-view.md:250`). |
| C4 Verification (red→green) | NEEDS-HUMAN | Accept the declared net-new RED exception and reliance on frozen CI for the sandbox's missing Git submodule entry — the restored discriminator passes 29 tests, but no pre-fix test executes and the local full gate cannot reproduce the index-dependent checks (`reviewer-evidence/red.log:2`; `reviewer-evidence/ci-rerun.log:3584`; `gate-logs/C4-ci.log:4674`). |
| C5 Causal adequacy | PASS | The missing durable handoff is supplied without losing fragment ownership or concealing uncertain outcomes; boundary/error witnesses pass and mutation validation leaves no unresolved survivor (`crates/core/src/multipart/commit.rs:476`, `:512`, `:606`; `reviewer-evidence/mutants-capped.log:4`; `reviewer-evidence/mutants-record-error.log:3`). |
| T1 Structure | PASS | Backend dependencies remain behind the granted traits, and validated writer constructors preserve the existing record boundary (`crates/core/src/multipart/commit.rs:636`, `:712`; `crates/core/src/multipart.rs:3095`, `:4159`). |
| T2 Shape | PASS | Later conflict/doubt work has a separate commit-round seam; error composition preserves indeterminate classification, and empty retirement obligations remain invalid (`crates/core/src/multipart/commit.rs:333`, `:571`, `:782`; `crates/core/tests/multipart_upload_part_commit.rs:2598`). |
| T3 Runtime | PASS | The same lease clock governs dispatch and stamping, new work has bounded scans/chunk loops and no new retry loop, and metadata awaits rely on the backend's explicit termination contract (`crates/core/src/multipart/commit.rs:436`, `:583`, `:648`; `crates/traits/src/lib.rs:1333`). |
| T4 Contribution | N/A | Contribution artifacts are absent by design; their substantive audit is owed at publish, as the deferred gate explicitly records (`gate-logs/T4-contribution.log:10`). The separate batch-review and TiKV compilation evidence is accounted for below. |
| T5 Judgment | PASS | The review found no scope expansion or unresolved implementation concern; affected-path prior-art searches corroborate the brief, and tracked deferrals remain settled (`reviewer-evidence/prior-art.json:2`; `brief.md:197`; `crates/core/src/multipart/commit.rs:99`, `:405`, `:611`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Confirm that an internal core milestone satisfies the intended purpose — these store-level tests establish this slice's behavior, while there is intentionally no production S3 caller or end-to-end upload experience yet (`docs/design/architecture/05-building-block-view.md:250`). |

No builder-fixable finding was established. The evidence supports the scoped behavior; the remaining decisions concern the declared verification posture and acceptance of the core-only milestone.

- **The behavioral checks pass independently.** In the disposable target, `cargo test --offline -p wyrd-core --test multipart_upload_part_commit` passed all 29 tests. Stashing only the production changes, keeping the test, reproduced E0432/E0599; restoring the stash returned all 29 to green (`reviewer-evidence/red.log:2`, `reviewer-evidence/restored-green.log:36`). The source review checked successful atomic handoff, replacement retirement, both per-part ceiling witnesses, cumulative overshoot, exact dispatch boundaries, settled-stop compensation, damaged epoch adoption, independent submitter identity, and unknown-result propagation. The C5 capability-probe smell-test found no optional-capability or load-time-side-effect guard.
- **Independent coverage and mutation evidence support the tests.** LCOV from the new integration suite intersects 395 added executable production lines: 392 executed (99.24%), including all 338 in `commit.rs` (`reviewer-evidence/diff-coverage.json:2`). The three unexecuted lines reject an empty retirement list; the separate unit test exercises that rejection (`crates/core/src/multipart.rs:4167`; `crates/core/src/multipart/commit.rs:782`). With lint capping, the 49-mutation run yielded 33 caught, 15 unviable, and one formatter mutation surviving the selected integration suite; independently rerunning that mutation against the full core suite caught it (`reviewer-evidence/mutants-capped.log:4`, `reviewer-evidence/mutants-record-error.log:3`). This resolves the survivor and agrees with the frozen gate's 34 caught / 15 unviable (`gate-logs/C5-mutants.log:13`). The earlier uncapped run's additional compile-time rejections are not counted as behavioral kills.
- **The full-gate limitation is a checkout caveat, not a patch defect.** The independent `cargo xtask ci` run completed with two reported failures arising from the same missing `.github/advisory-db` gitlink: its index assertion and the advisory pin check (`reviewer-evidence/ci-rerun.log:3584`, `:4230`). Both `git ls-tree HEAD .github/advisory-db` and `git ls-files --stage .github/advisory-db` returned no entry, although the supplied base's `.gitmodules` declares it; the patch touches neither. The frozen CI log records the real pinned advisory database, successful dependency audits, and full gate success (`gate-logs/C4-ci.log:4027`, `:4038`, `:4674`). Spelling, docs lint/render, formatting, clippy, conformance, and DST ran successfully in the independent rerun. The brief's `typos` and docs-renderer dependencies were exercised (`reviewer-evidence/ci-rerun.log:2`, `:8`).

The frozen diff-coverage gate produced no measurement because its own `origin/main` rejected the patch (`gate-logs/C4-diff-cov.log:10`). The supplied target accepts the reverse patch check and all affected blob hashes match; the independent LCOV measurement above supplements that missing result without claiming the harness wrapper succeeded. The frozen batch-review log records only its aggregate zero-findings result (`gate-logs/T4-batch-review.log:10`); this review independently examined production logic, the harness, protocol/rubric compliance, and executed evidence. The frozen TiKV log shows actual crate/server feature compilation and a successful finish (`gate-logs/host-tikv.log:2`, `:211`). No missing-wrapper finding is inferred from instance-scoped scripts being outside the target.

Prior-art queries covered each of the five affected paths in `main` history and closed/unmerged PR searches. They found the merged staging/renewal lineage and no history for the new commit module or test. The broader closed/unmerged multipart search returned PR #647; its file list overlaps none of these five paths. Query scope and returned identifiers are retained in `reviewer-evidence/prior-art.json`. Source citations above are grounded in `$PDCA_TARGET`; evidence paths are relative to this review directory. The restored target retains the supplied patch unchanged.

<!-- pdca:leaf-complete -->

### Advisory — adversary

# Adversarial review: issue #1027 (part commit, 657.2a)

**Result:** I did not find a failing input in the commit, re-upload, refusal or compensation logic. I did find one false claim about the base, one review gate that can't carry the weight it needs to, one commit under a damaged session, and two test gaps. The toolchain was available, so everything below was run, not inferred.

## Findings

- NEEDS-HUMAN [human] — **The patch is not based on `main`, and the green gates ran on that other base.** `brief.md:120` says "`main` is the base" and `Depends on:` is blank (`brief.md:118`). The target's base, `635e0bd`, is 8 unmerged `pdca-integrate` commits ahead of GitHub `main` (`376b106`), including #1007's `complete.rs`. On a fresh fetch of `main`, `git apply --check` fails: 3 hunks in `crates/core/src/multipart.rs` and 1 in `docs/design/architecture/05-building-block-view.md` are rejected. The rejected text depends on #1007: `pub mod complete;` at `crates/core/src/multipart.rs:166`, the doc links at `crates/core/src/multipart.rs:12` and `:117`, and the context line at `docs/design/architecture/05-building-block-view.md:252`. So `C4-ci`'s pass (and `overall: pass`) says nothing about the tree the brief names as the target. `C4-diff-cov`'s advice to "rebase Do" also reads wrongly, because here the base is ahead of `main`, not behind it. The risk looks small: `commit.rs`, the test file and the `stage.rs` hunk apply cleanly, and with `pub mod commit;` added by hand, 75 lib tests and all 29 new tests pass on real `main`. I did not run the full `cargo xtask ci` there. A human needs to decide: land after the stack and record the dependency, or rebase onto `main` and re-run the gates. If it is rebased, the links to `complete` must go too.

- NEEDS-HUMAN [human] — **The `T4-batch-review` pass is not the review the brief requires.** `check-gates.json:105` reports pass with 0 findings in 19.51 s, for three passes over a 167 KB patch. `brief.md:165-168` calls a clean row like this weak evidence, cites 36.92 s in iteration 7 as an example, and requires "a confirmed high-effort review of the final patch" at sign-off. This run was about half that time. Nothing in `gate-logs/T4-batch-review.log` shows the effort level or that any commands ran. Don't count this row as that review.

- NEEDS-HUMAN [human] — **A damaged session can still get a part committed if the epoch rises and then falls during one renewal.** `crates/core/src/multipart/commit.rs:408` compares only the final session with the staged one, but the renewal can adopt a session on several rounds. Probe: staged at epoch 2, then planted `Open@5` before round 1 and `Open@3` before round 2. The answer was `Committed`, and the part record carries `session_epoch: 3`. The renewal saw epoch fall from 5 to 3, which `crates/core/src/multipart.rs:383` calls damage ("no batch is sent under it"; also `crates/core/src/multipart/commit.rs:28`). No legal history does this, and the code matches the brief's "before and after" wording, so this is low severity. A proper fix needs the renewal to judge each adoption, and `stage.rs` is out of scope here (`brief.md:135`). Suggest deciding whether to add this case to #1026, which covers the neighbouring case and is open.

- NEEDS-HUMAN [impl] — **No test pins "refuse an over-limit attempt first".** `brief.md:219` and `:267` require this as `commit_part`'s first step, and production does it (`crates/core/src/multipart/commit.rs:645`). But moving that check after the quiesce renewal passes all 29 tests, because `assert_refused_and_compensated` (`crates/core/tests/multipart_upload_part_commit.rs:1675`) never counts renewals. With that mutation, an over-limit attempt sends a needless renewal write. A version that checks after the renewal's `match` would also answer a lapsed lease with `LeaseLapsed` instead of `EntityTooLarge`; that second effect comes from reading the code, not from a run. Fix: assert no `Kind::Renewal` batch on the over-limit legs, and add a leg with an over-limit attempt and a lapsed lease.

- NEEDS-HUMAN [impl] — **The writer constructors' scheme check has no test.** The doc on `PartRecord::commit` says it refuses what the decoder refuses, and `brief.md:260-261` wants every written value to be one the decoder accepts. Deleting `checked_chunk_scheme` from `crates/core/src/multipart.rs:3102` (`PartRecord::commit`) or `:4172` (`RetireObligation::part_chunks`) passes every test, because the harness only feeds them supported schemes. `C5-mutants` shows 0 survivors only because it never tried these deletions. Fix: add crate-level unit tests beside `the_per_part_obligation_refuses_an_empty_list`.

## Lower-priority note (no action needed for sign-off)

- An empty part, or a re-upload of the same size, is refused `EntityTooLarge` on a session already past the ceiling through the overshoot 0016 allows, even though it adds no chunks (`crates/core/src/multipart/commit.rs:679`). This fits 0016's "would exceed" wording, so I don't count it as a defect, but `crates/core/src/multipart/commit.rs:626` says "would take … past", which reads differently. No test pins this: skipping the refusal for empty attempts passes all 29 tests.

## Attacks that did not refute the patch

- **Red→green evidence.** `C4-verify` is `unverifiable` as `brief.md:147-149` predicted, because the test can't compile without the new module. So I re-created the reds myself in a scratch copy, without `build-notes.md`. All of these turned the new tests red: lease check removed and off by one, over-limit check removed and reading only the truncation flag, own prior counted, settled stop not compensating, compensating after `SlotLost` or `InDoubt`, epoch check dropped from the error arm, moved above the `match`, and `<=` changed to `<`, and `source` returning `None`. The tests fail in the named legs and run the production path over a real redb store. Not re-run: the "empty re-upload writes an obligation" and "guard reads the id from the batch" edits.
- **Other mutations, all killed:** the retire `require_absent` removed, wrong epoch on the part record, wrong `psum:` length, digest or stamp, a read added before the scan, a stale clock reading, and compensation failures ignored on each route.
- **`C4-diff-cov` reads `fail`, but nothing is hidden by it.** Measured directly on the target's base with `cargo llvm-cov`: 381 of 381 instrumented lines in `commit.rs` and 57 of 57 added lines in `multipart.rs` ran.
- **Part numbers above the session cap.** `upload_part` commits part 10,001 and part 999,999, and neither `stage.rs` nor `commit.rs` checks `MAX_PARTS_PER_SESSION`. I'm not filing this, because #669's acceptance refuses numbers above 10,000 at the S3 verb, which fits the constant's doc ("checked where a part number is admitted").
- **Reasoned through, no failing case found:**
  - Slot bytes after each renewal outcome match the last landed write.
  - A renewal in doubt never reaches compensation, since `compensates_after` matches only `StageError::Stopped`.
  - The commit batch's operation count stays inside `MAX_BATCH_OPS` (`8 + N`).
  - A compensation's oversize obligation stays under `V` at `SIZING_SCHEME`: 248 × 315 B is about 78 KB, against 100 KB. I did not check schemes wider than that.
  - `failed` uses one direct downcast, which is the documented contract at `crates/traits/src/lib.rs:216`.
- **Settled, not re-raised:** a compensation or commit losing to a rollback (#1028), unknown outcomes (#1029, #1030), DST races (#660), and a non-`Open` session at an unrisen epoch (#1026). `CommitConflict`'s "slot is still held" message makes a claim no read has checked, but the brief mandates it (`brief.md:94-95`) and #1028 replaces that arm.

<!-- pdca:leaf-complete -->

### Advisory — code-review

No findings on either lens: no introduced correctness bugs or actionable reuse, simplification, or efficiency issues identified.

Reviewed the commit/compensation flow, record constructors, and test harness against the read-only target source and the stated scope. Explicitly deferred behavior was treated as settled.

Validation evidence: frozen CI passed, including all 29 new integration tests; mutation testing reported 34 caught and 15 unviable mutants, with no survivors. Tests were not rerun. Diff coverage was not measured because its gate could not apply the patch to origin/main; the pre-fix test could not compile without the new API, as anticipated by the brief.

<!-- pdca:leaf-complete -->

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [x] C4 Verification (red→green) — Accept the declared net-new RED exception and reliance on frozen CI for the sandbox's missing Git submodule entry — the restored discriminator passes 29 tests, but no pre-fix test executes and the local full gate cannot reproduce the index-dependent checks (`reviewer-evidence/red.log:2`; `reviewer-evidence/ci-rerun.log:3584`; `gate-logs/C4-ci.log:4674`).
- [x] Validation — fitness-to-purpose — Confirm that an internal core milestone satisfies the intended purpose — these store-level tests establish this slice's behavior, while there is intentionally no production S3 caller or end-to-end upload experience yet (`docs/design/architecture/05-building-block-view.md:250`).
- [x] **The patch is not based on `main`, and the green gates ran on that other base.** `brief.md:120` says "`main` is the base" and `Depends on:` is blank (`brief.md:118`). The target's base, `635e0bd`, is 8 unmerged `pdca-integrate` commits ahead of GitHub `main` (`376b106`), including #1007's `complete.rs`. On a fresh fetch of `main`, `git apply --check` fails: 3 hunks in `crates/core/src/multipart.rs` and 1 in `docs/design/architecture/05-building-block-view.md` are rejected. The rejected text depends on #1007: `pub mod complete;` at `crates/core/src/multipart.rs:166`, the doc links at `crates/core/src/multipart.rs:12` and `:117`, and the context line at `docs/design/architecture/05-building-block-view.md:252`. So `C4-ci`'s pass (and `overall: pass`) says nothing about the tree the brief names as the target. `C4-diff-cov`'s advice to "rebase Do" also reads wrongly, because here the base is ahead of `main`, not behind it. The risk looks small: `commit.rs`, the test file and the `stage.rs` hunk apply cleanly, and with `pub mod commit;` added by hand, 75 lib tests and all 29 new tests pass on real `main`. I did not run the full `cargo xtask ci` there. A human needs to decide: land after the stack and record the dependency, or rebase onto `main` and re-run the gates. If it is rebased, the links to `complete` must go too.
- [x] **The `T4-batch-review` pass is not the review the brief requires.** `check-gates.json:105` reports pass with 0 findings in 19.51 s, for three passes over a 167 KB patch. `brief.md:165-168` calls a clean row like this weak evidence, cites 36.92 s in iteration 7 as an example, and requires "a confirmed high-effort review of the final patch" at sign-off. This run was about half that time. Nothing in `gate-logs/T4-batch-review.log` shows the effort level or that any commands ran. Don't count this row as that review.
- [x] **A damaged session can still get a part committed if the epoch rises and then falls during one renewal.** `crates/core/src/multipart/commit.rs:408` compares only the final session with the staged one, but the renewal can adopt a session on several rounds. Probe: staged at epoch 2, then planted `Open@5` before round 1 and `Open@3` before round 2. The answer was `Committed`, and the part record carries `session_epoch: 3`. The renewal saw epoch fall from 5 to 3, which `crates/core/src/multipart.rs:383` calls damage ("no batch is sent under it"; also `crates/core/src/multipart/commit.rs:28`). No legal history does this, and the code matches the brief's "before and after" wording, so this is low severity. A proper fix needs the renewal to judge each adoption, and `stage.rs` is out of scope here (`brief.md:135`). Suggest deciding whether to add this case to #1026, which covers the neighbouring case and is open.
- [x] **No test pins "refuse an over-limit attempt first".** `brief.md:219` and `:267` require this as `commit_part`'s first step, and production does it (`crates/core/src/multipart/commit.rs:645`). But moving that check after the quiesce renewal passes all 29 tests, because `assert_refused_and_compensated` (`crates/core/tests/multipart_upload_part_commit.rs:1675`) never counts renewals. With that mutation, an over-limit attempt sends a needless renewal write. A version that checks after the renewal's `match` would also answer a lapsed lease with `LeaseLapsed` instead of `EntityTooLarge`; that second effect comes from reading the code, not from a run. Fix: assert no `Kind::Renewal` batch on the over-limit legs, and add a leg with an over-limit attempt and a lapsed lease.
- [x] **The writer constructors' scheme check has no test.** The doc on `PartRecord::commit` says it refuses what the decoder refuses, and `brief.md:260-261` wants every written value to be one the decoder accepts. Deleting `checked_chunk_scheme` from `crates/core/src/multipart.rs:3102` (`PartRecord::commit`) or `:4172` (`RetireObligation::part_chunks`) passes every test, because the harness only feeds them supported schemes. `C5-mutants` shows 0 survivors only because it never tried these deletions. Fix: add crate-level unit tests beside `the_per_part_obligation_refuses_an_empty_list`.
- [x] C4 per-fix red->green: this patch's test red pre-fix, green post-fix unverifiable —                why this slice has no isolable red (the cargo output is above).
- [x] **The per-part ceiling criterion misses a fully staged oversized part.** `brief.md:40-42` uses `chunk_bound_exceeded`, and `brief.md:198-199` assumes an over-limit attempt contains only a prefix. But `StageRequest.max_part_chunks` is caller-supplied (`crates/core/src/multipart/stage.rs:286-290`); its validation checks only the renewal operation budget (`:325-329`). That allows 248 chunks at the current 500-operation budget, whereas the shipped `MAX_PART_CHUNKS` is 158 (`crates/core/src/multipart.rs:5010-5012`, `:5037`, `:5171`). A complete 159-chunk body staged with a limit of 248 returns with `chunk_bound_exceeded == false` (`stage.rs:2094-2126`); the part decoder also has no chunk-count ceiling (`multipart.rs:3051-3069`). The specified test can therefore pass with a commit that checks only the truncation flag. State the authoritative commit ceiling or validated-profile precondition, and add an untruncated above-ceiling witness through both `commit_part` and composed `upload_part`.
- [x] **H3's universal guard rejects legitimate reservation batches.** `brief.md:69-73` requires every batch naming a slot to require the bytes its submitter last wrote. The composed upload necessarily starts with a reservation that instead uses `require_absent(slot)` and has no prior bytes from that attempt (`crates/core/src/multipart/stage.rs:1325-1329`). Applying H3 literally fails before staging; changing reservation behavior would violate `brief.md:115-116`. Specify separate assertions for reservation (absence precondition and the submitter's identity in the new value) and subsequent batches (the submitter's exact prior slot bytes).
- [x] **G conflates an intent failure with the quiesce renewal's failure vocabulary.** `brief.md:55-63` includes `EntryLost` in its no-compensation cases without identifying a staging-only witness. Its sole construction is in staging's intent settlement (`crates/core/src/multipart/stage.rs:2361-2381`), before a `StagedAttempt` is returned. Deleting an owned entry after staging, while leaving the session and slot unchanged and the clock live, instead makes renewal's entry precondition conflict (`:1616-1624`); reconciliation checks only the session and slot (`:1789-1806`), eventually returning `RenewalRoundsSpent` (`:1652-1664`), which G requires compensating. Make `EntryLost` explicitly a composed-upload staging leg, and state the expected behavior for owned-entry loss during quiesce if that is also intended coverage. Otherwise Do must invent a new detection path to satisfy the presumed witness despite the prohibition on staging behavior changes.
- [x] size backstop — this slice is behaving oversized: patch is 163 KB (threshold 100 KB). Recommend answering `iterate-plan` at sign-off and authoring the split in the re-plan (`pdca split`), rather than `iterate-do`: a slice that is too big yields implementation-shaped findings every round, and splitting authors briefs, which is Plan's beat.

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
- By / date: Eduard Ralph / 2026-11-02

## 10. Act candidates (hints for the next Act review)
- Plan advisory: 3 finding(s); brief revised: yes (plan-advisory-*.md)
- (empty is the common case)
- Add the rise-then-fall epoch case to #1026.
- Add the two missing tests (over-limit check order, scheme check) to #1028.
- T4 ran on a stale checkout, without #288 (review settings fix) and #287 (security reviewer).
