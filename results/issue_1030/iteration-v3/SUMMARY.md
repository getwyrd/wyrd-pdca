# Result — issue 1030 / multipart-part-commit-settle-compensation

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect: Settle a compensation answered `CommitUnknownResult` from reads, and stop the own-commit proof's
  one false success. Today, on #1029's base, two answers are wrong:
  - **A compensation answered unknown stops in doubt without reading anything**
    (`crates/core/src/multipart/commit.rs:1140-1145`, `// deferred: #1030`; `failed`, `:769-783`). It does so
    even when its own `retire:` obligation shows it landed, so a client gets an in-doubt error for a refusal
    or stop that in fact completed. The base's own test asserts this
    (`crates/core/tests/multipart_upload_part_commit.rs:2696-2738`, the `applied: true` runs).
  - **The own-commit proof answers `Committed` for a commit that never landed** when only the slot key is
    damaged. `settle` answers `Committed` on "slot gone, and a later session read still `Open@E`", whatever
    the part key holds (`commit.rs:918-925`): a part key found absent where the commit required it absent
    changes nothing (`:900-910`). So a held or unapplied commit whose slot key was erased answers
    `Committed` with no `part:` record. #927's attempt 7 hit the same false answer
    (`results/issue_927/iteration-v7/SUMMARY.md:342`), and the human decided on 2026-11-01 to fix it here,
    where the store can still show it (`results/issue_927/split-proposal.md:131-134`).
- Success criterion: Appended to `crates/core/tests/multipart_upload_part_commit.rs`. **In every leg below where a
  batch of the attempt is answered unknown, the wrapper records no further batch from the attempt after it** (no
  second compensation, and no commit), **and every leg ends with `assert_every_fragment_named`.** "The route's landed
  answer" means what that route answers today when its compensation is answered `Committed`
  (`Compensated::Released`): `Refused(OperationAborted)` after a same-number rival, `Refused(EntityTooLarge)` after a
  ceiling, `CommitError::Renewal` or `CommitError::Stopped` with `slot_released: true` after a renewal stop or a short
  lease.
  **(S) Settling a compensation answered unknown.** Row 1 runs on every route `compensating` scripts (test file
  `:2678`) and on a same-number rival; the other rows run on the rival route. The empty column skips routes an empty
  part cannot reach (a ceiling).

  | The compensation | Non-empty part | Empty part |
  |---|---|---|
  | applies, answered unknown (once per `may_still_commit` value) | the route's landed answer; one compensation batch; one obligation, naming this attempt's own chunks; slot released once | the route's landed answer; one batch; no `retire:` key; slot released once |
  | not applied, `may_still_commit: false` | `InDoubt { batch: Compensation, may_still_commit: false }`; one batch; no obligation; slot still held | the same |
  | held, released at the gap before the re-read | the route's landed answer, as in row 1 | the same |
  | held, released after the attempt returned | `InDoubt { batch: Compensation, may_still_commit: true }`; one batch; then the held batch lands: one obligation, slot released once | the same, with no `retire:` key |

  This reworks the base's `h4_a_compensation_answered_unknown_is_in_doubt_with_no_further_batch` (test file `:2699`),
  whose `applied: true` runs assert the in-doubt error.
  **(X3) The cross-phase leg** (the re-plan note's request; #1029's (X) aimed at a compensation). Fixed clock. The
  reserve, the intent and the quiesce renewal are each scripted `[hold, pass]` (`hold_each_staging_round_once`, test file
  `:4282`), on a route where the quiesce renewal runs before the compensation (the cumulative ceiling, or a same-number
  rival; an empty part reaches no ceiling, so its run uses the rival). The compensation applies but answers unknown (once per `may_still_commit` value). Before the re-read, the held
  staging batches are released in order, and the test asserts that each answers `Committed` and that the slot then
  holds exactly the bytes the compensation required (and, for a non-empty part, that the owned entry is back), so the
  history is the real one. Non-empty part: the route's landed answer, because the obligation proves it. Empty part (no
  intent batch): the in-doubt error. Both: **exactly one compensation batch.** This leg must fail on a patch that takes
  a slot held at those bytes as "did not land" and sends the compensation again (attempt 7's `compensate`).
  **(O) The obligation is the witness.**
  - A non-empty compensation applies, answered unknown, then a real client Abort (`abort`, test file `:1127`) fences
    the session before the re-read: the route's landed answer, never in-doubt.
  - After a rollback, the compensation runs under the newer epoch `E′` (directly, or after the epoch chase). That
    round applies but answers unknown: the route's landed answer, settled by the obligation keyed under `E′`, the
    epoch that round required.
  - Other bytes are planted at the obligation key (`plant`, `:1189`) and the compensation is not applied: never the
    landed answer, and the attempt leaves the planted bytes where they are. With `may_still_commit: false`,
    `Stop::ObligationKeyTaken` (as a stop claiming no release, or as the cause of `InDoubt`). Under a held compensation,
    `InDoubt` with `ObligationKeyTaken` as its cause and `may_still_commit: true`, because bytes at that key are not
    there for good (*Keeping the doubt*); **then the test removes the planted bytes (`erased`, `:1160`) and releases
    the held batch, and it lands:** one obligation naming this attempt's own chunks, slot released once. This run fails
    on a patch that lowers the flag on other bytes at the obligation key.
  **(T) A slot gone under a moved or missing session.**
  - An empty part's compensation is held; an Abort and the reaper's teardown (`teardown`, `:3989`) run before the
    re-read: `InDoubt { batch: Compensation, may_still_commit: false }`, never the landed answer; the held batch,
    released afterwards, answers `Conflict`.
  - A compensation is held and lands at the gap right **after** the re-read's slot read, then the session rolls back.
    Non-empty part: the route's landed answer (the obligation shows it). Empty part: the in-doubt error with
    `may_still_commit: false`.
  - **A missing session record is judged by a slot read made after it** (the tracker's Design; as `gone_in_doubt` does
    for the commit). Run on a non-empty and an empty part on the rival route, the compensation held, the event at the
    gap before the re-read:
    - **Only the session record is erased** (`erased`, `:1160`), the slot kept: `InDoubt { batch: Compensation,
      may_still_commit: true }` with `Stop::Record(SessionRecordMissing)` as its cause, never the landed answer. Then
      the test puts the session bytes back (`plant`) and releases the held batch, and it lands: slot released once, one
      obligation naming this attempt's own chunks for a non-empty part, no `retire:` key for an empty one.
    - **The terminal delete** (`terminal_delete`, `:1174`) takes the session and the slots in one batch: `InDoubt {
      batch: Compensation, may_still_commit: false }` (`SessionMoved`, as `gone_in_doubt`), never the landed answer;
      the held batch, released afterwards, answers `Conflict`.
  **(F) Faults and damage under a held compensation.** A store fault at each read the settle makes, and undecodable
  slot or session bytes: `InDoubt { batch: Compensation, may_still_commit: true }` with the fault or damage as its
  cause; the held batch released afterwards lands (slot released once, one obligation for a non-empty part). A fault
  at a read made after a read that showed the round can no longer land (the session moved) keeps
  `may_still_commit: false`. Each run fails if its doubt-keeping return is replaced with a plain stop.
  **(D) Damage only, no false success** (the human's decision of 2026-11-01).
  - **D1.** A commit that required the part key **absent** is held, or answered unknown with `may_still_commit: false`
    and never applied. At the gap before the re-read the slot key is erased (`erased`, `:1160`); the session is
    untouched. Never `Committed`. The held run: `InDoubt { batch: Commit, may_still_commit: true }`. The `false` run:
    `InDoubt { batch: Commit, may_still_commit: false }` or a stop claiming no release. Both: one commit batch, no
    compensation, no `part:` record; the held batch, released afterwards, answers `Conflict`.
  - **D2.** A held commit that required the part key absent lands at the gap after the lap's part read (gaps 1 and 2 of
    `at_gaps`, `:3971`), with nothing else happening: `Committed`, slot released once. A part key found absent by a read
    made before the slot read disproves nothing.
  - **D3.** A non-empty compensation answered unknown with `may_still_commit: false`, never applied; at the gap before
    the re-read its slot key is erased, the session untouched. No obligation is there, so the answer is not the landed
    one (not `Refused(OperationAborted)`, `slot_released()` false): `InDoubt { batch: Compensation }`, one batch.
  **(K) Every other test in the file stays green with the same behaviour**, #1029's legs among them: (X), (X2), the
  one-batch leg, the read-gap legs (`read_gap`, `:4623`) and (R). The only existing test whose *outcomes* this slice
  changes is the h4 one named in (S). The own-commit proof's new part-key read (*Design*) changes the reads a lap makes,
  so Do **may update the read-trace vocabulary, and only to add that read**: `lap` and `laps_of` (test file
  `:4259-4275`), `at_gaps` and its gap numbering (`:3966-3985`), the read lists built from `lap` (`:5163`, `:5237`,
  `:5692`), and every exact `reads_after` comparison (`:2629`, `:4319-4323`, `:4389`, `:4652-4656`, `:5089`, `:5475`,
  and the rest). Do may change the helpers' shape if the read is made only in some laps. Every other assertion of an
  existing test stays as it is (outcome, cause, flag, batch counts, releases, records, `assert_every_fragment_named`),
  and so does every history it builds: an event between two reads stays between the same two reads, renumbered if its
  gap moved. **The new read gets the old reads' coverage**, each on a commit that required the part key absent and
  landed before the lap's slot read: a rival landing at each gap the new read adds answers `Committed` (any record
  there is a record); a rollback there answers the in-doubt error with `SessionMoved`; a store fault at the new read
  keeps the doubt with the fault as its cause, never `Committed` and never a plain stop.
  **(E) The whole gate.** `cargo xtask ci` is green on the patched tree.
- Repo + branch target: getwyrd/wyrd @ main
- Scope: what a compensation's `CommitUnknownResult` means: the route's landed answer on evidence that it landed,
  and otherwise the kept doubt, with no further batch; and the own-commit proof's false `Committed` when the commit
  required the part key absent and that key is still absent; the living docs kept current. / out of scope:
  - sending a compensation again after an unknown outcome, or releasing the slot of one that provably did not land:
    both need a "did not land" proof that late staging and the drain cannot forge (the same staging or record change
    #1029 deferred; not filed, alpha intake is closed);
  - a re-upload (the commit required the prior's bytes) with its slot erased: a landed commit and a rival or repair
    can both leave those bytes, so no read tells them apart. It stays a documented limit;
  - an empty part's compensation with its slot erased by damage: it looks exactly like a landed one (no obligation, no
    owned entry), and answering it as landed promises nothing false. It stays a documented limit;
  - late staging batches that land after a compensation and rebuild the slot or an owned entry: unchanged, as after a
    compensation answered `Committed`;
  - #1029's commit-in-doubt protocol (no batch after an unknown commit): unchanged;
  - any change in `stage.rs` or `multipart.rs` record shapes; proposal 0016 and the ADRs; `Cargo.lock` and `deny*.toml`;
    the gateway and any production caller; Complete, the drain, the reaper, DST.

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
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): fail — 24 mutants tested in 4m: 14 caught, 7 unviable, 3 timeouts

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): pass — review-branch: 0 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_1030/review-
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 13.99s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Issue #1030’s settlement of unknown multipart compensations and rejection of false commit success has no remaining patch defect identified; fitness-to-purpose remains for human sign-off.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The acceptance histories distinguish landed, unapplied, late and damaged outcomes; the carry-forward explicitly requires rejecting torn part records and preserving doubt (brief.md:33; brief.md:394). |
| C2 Reproduction (red pre-fix) | PASS | With only the implementation stashed, the landed-compensation, erased-slot and torn-part regressions each fail on the intended wrong answer (reviewer-red.log:185; reviewer-red-d1.log:14; reviewer-red.log:215). |
| C3 Change | PASS | The final change stays within the settlement, regression-test and living-documentation surfaces authorized by the brief, including the revised torn-part outcome (crates/core/src/multipart/commit.rs:1024; crates/core/src/multipart/commit.rs:1254; docs/design/architecture/05-building-block-view.md:250; brief.md:394). |
| C4 Verification (red→green) | PASS | Restoring the implementation makes all 92 tests pass under the pinned toolchain; frozen full CI is green, while the independent full rerun encounters a missing Git submodule entry in the disposable target (reviewer-green-pinned.log:99; gate-logs/C4-ci.log:4840; reviewer-ci.log:3652). |
| C5 Causal adequacy | PASS | The tests distinguish genuine witnesses from rebuilt slots, wrong obligation bytes and torn records, and preserve late-landing uncertainty; independent mutation checks leave no surviving mutant (crates/core/tests/multipart_upload_part_commit.rs:6224; crates/core/tests/multipart_upload_part_commit.rs:6393; crates/core/tests/multipart_upload_part_commit.rs:7123; reviewer-mutants-capped.log:10). |
| T1 Structure | PASS | Backend independence and the existing error contract are preserved by sharing the narrow MetadataStore/Doubt machinery; no new production caller is introduced (crates/core/src/multipart/commit.rs:868; crates/core/src/multipart/commit.rs:1254; crates/core/src/multipart/commit.rs:343). |
| T2 Shape | PASS | Stored-record and public-type compatibility remain intact, and malformed part values cannot stand in for decoded records at the success boundary (crates/core/src/multipart/commit.rs:1026; crates/core/src/multipart.rs:3294). |
| T3 Runtime | PASS | Settlement has a fixed lap budget, bounded additional reads and no new clock or helper task; each external await uses the backend’s termination contract (crates/core/src/multipart/commit.rs:1294; crates/core/src/multipart/commit.rs:1300; crates/traits/src/lib.rs:1334). |
| T4 Contribution | N/A | Commit/PR contribution artifacts are intentionally drafted after Check; their substantive audit is owed at publish, as the deferred gate records (gate-logs/T4-contribution.log:10). |
| T5 Judgment | PASS | Independent semantic, test-fidelity and rubric passes found no unresolved implementation concern; path-based prior art was checked, and the accepted scope decisions and tracked DST deferral remain settled (reviewer-prior-art.json:4; brief.md:396; crates/core/tests/multipart_upload_part_commit.rs:5933). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Accept the final implementation as sufficient for this pre-production commit slice — the exercised storage histories support its contract, while client-facing UploadPart integration belongs to #669 (crates/core/src/multipart/commit.rs:343; brief.md:210). |

The evidence supports the patch’s two intended outcomes, with one limitation in the disposable checkout and one unmeasured advisory metric. Project-source citations above resolve under `$PDCA_TARGET`; input and review-evidence paths resolve in this report’s directory.

**Independent reproduction and verification.** I stashed only `crates/core/src/multipart/commit.rs`, leaving the patched tests in place. The S regression returned `InDoubt` after an applied compensation; D1 returned `Committed` after the slot was erased without a part record; the torn-part regression also returned `Committed`. All three executed and failed with exit 101. After stash restoration, `cargo test --offline --locked -p wyrd-core --test multipart_upload_part_commit` passed 92/92 with Cargo 1.96.0. The restored Git diff is byte-identical to `patch.diff`, and `git diff --check` passes.

**Causality and test fidelity.** Non-empty compensation success requires the exact obligation bytes under the epoch that the compensation round required. Empty compensation success requires the slot observation followed by the unchanged session. The unknown-outcome arm returns directly into read-only settlement, so no retry or compensation follows it (crates/core/src/multipart/commit.rs:943; crates/core/src/multipart/commit.rs:1284; crates/core/src/multipart/commit.rs:1335). The wrapper releases held batches through the real redb store, checks their actual commit result, and explicitly reconstructs the late-staging history (crates/core/tests/multipart_upload_part_commit.rs:536; crates/core/tests/multipart_upload_part_commit.rs:6261). These fixtures can exhibit the forbidden failures; they do not assume the desired verdict. The new malformed-part check is a storage-proof requirement, not a capability probe masking a load-time cause.

The regression at `crates/core/tests/multipart_upload_part_commit.rs:7123` observes the later read fault, preserves `may_still_commit`, repairs the damage, and successfully releases the held batch. Changing the earlier torn-part test from success to doubt follows the explicit carry-forward instruction at `brief.md:394`; it is not an unauthorized weakening of criterion K. Seeded DST work remains deferred to #660 as recorded in the source, and the accepted no-resend and size decisions are not reopened.

**Mutation evidence.** I independently ran `cargo mutants --in-diff ../patch.diff --package wyrd-core` against the multipart integration suite, with a 30-second timeout. The first run reported 8 caught, 14 unviable and 2 timeouts (`reviewer-mutants.log:27`). Seven unviable cases were only unused-code warning failures, so I reran those with `--cap-lints true`: 6 caught and 1 timeout (`reviewer-mutants-capped.log:10`). Across the same 24 distinct mutants, that is 14 test failures, 7 genuine compile failures, 3 timeouts and 0 survivors, matching the frozen aggregate (`gate-logs/C5-mutants.log:16`). The three timed-out mutations defeat the settlement loop’s termination condition; they are not surviving successful test runs.

**Gate caveats.** The frozen C4-verify log proves green-only execution, not red→green (`gate-logs/C4-verify.log:162`); the independent runs above supply the missing red evidence. C4-diff-cov did not measure coverage because the stacked patch did not apply to `origin/main` (`gate-logs/C4-diff-cov.log:10`), consistent with the declared prerequisite branches (`brief.md:153`). This is an ordering/base-state limitation, not evidence of a patch defect. No changed-line coverage percentage is claimed.

The independent `cargo xtask ci` run exercised real `typos`, documentation lint and rendering; the renderer produced 101 pages and passed its link audit (`reviewer-ci.log:2; reviewer-ci.log:10`). The disposable repository lacks `.github/advisory-db` in both its index and base tree. Consequently the unchanged submodule-pin test fails and the dependency wall cannot run (`reviewer-ci.log:3652; reviewer-ci.log:3670`). I did not fabricate that Git entry or bypass the check. The original frozen run demonstrates the real pin test and dependency audits passing (`gate-logs/C4-ci.log:3751; gate-logs/C4-ci.log:4203; gate-logs/C4-ci.log:4206; gate-logs/C4-ci.log:4209`). This is a target-state caveat, not a C4 patch failure or an undischarged original dependency. The named external dependencies, `typos` and `docs-renderer`, were exercised independently as well as in the frozen run.

The full independent CI run completed with exactly those two check failures, both caused by the missing gitlink (`reviewer-ci.log:4297`). Its remaining checks, including DST, passed. Both TiKV feature compilations also passed independently: `cargo clippy --offline --locked -p wyrd-metadata-tikv --features tikv --tests` and `cargo clippy --offline --locked -p wyrd-server --features tikv,etcd --tests` (`reviewer-tikv.log:202`), corroborating `gate-logs/host-tikv.log:112` and `gate-logs/host-tikv.log:211`.

The frozen batch-review log reports zero findings but provides no review content or evidence of depth (`gate-logs/T4-batch-review.log:10`); I do not treat that summary as a substitute for this independent review. The contribution audit remains N/A until its mandatory publish rerun.

**Prior art.** I queried merged history by each of the three affected file paths. Both Rust paths have the merged #1027 commit `5e8aa249`; the architecture history contains that commit and other features. I also enumerated all 19 closed, unmerged PRs and checked each PR’s changed filenames: none intersects the affected paths, and none closed since 2026-11-01. Query URLs, commit identifiers and all file lists are retained in `reviewer-prior-art.json`. No unresolved prior-art decision remains.

<!-- pdca:leaf-complete -->

### Advisory — adversary

# Adversarial review — issue 1030 (compensation in doubt, own-commit part read)

The fix holds up. I could not make a compensation answered unknown give a false landed answer,
and the red→green evidence is real. Two gaps are left: a test gap in one lap of the settle, and
one damage-only false `Committed` that the patch does not close and the docs do not name.

## Findings

- NEEDS-HUMAN [impl] — **An empty part's second settle lap is never tested.** `crates/core/src/multipart/commit.rs:1284`:
  narrowing the empty part's proof to the first lap (`if obligation.is_none() && laps == 1`) leaves all 92
  tests green. The second lap is there to catch a compensation that lands after lap 1's slot read, and no
  test lands one there. (S) row 3 lands before lap 1 (`multipart_upload_part_commit.rs:6158`); row 4 lands
  only after the return. Concrete case: an empty part on the rival route, the compensation `Act::hold()`, and
  `land_held(Committed)` at gap 2 of `at_compensation_gaps(&attempt, 0, 2, …)` (after lap 1's session read).
  I ran it in a scratch copy. The patch gives `Refused(OperationAborted)` with 2 laps of reads, which is
  correct. The mutant gives `InDoubt { Compensation, may_still_commit: true, OutcomeUnknown }`. Fix: add
  this run to (S). It costs availability, not C-1 safety, but nothing in the suite would catch the
  regression.

- NEEDS-HUMAN [human] — **One damage-only false `Committed` is left: a rival's record plus an erased slot.**
  `commit.rs:1032` (`Ok(Some(_)) => doubt.cannot_land()`) takes any record that decodes as proof
  (`:151`, "Any record there is a record, this attempt's or a rival's"). Concrete case, run in a scratch
  copy: a first-upload commit is held and never lands. At gap 0 a same-number rival commits and the
  attempt's slot key is erased; the session is untouched. Result: `Ok(Committed { digest: <this attempt's> })`.
  But the record is the rival's (`is_own` is false), the held batch later answers `Conflict`, and
  `releases == 0`. The brief's (K) asks for "any record there is a record", so this is by design. A record
  left by this history reads the same as a landed commit later replaced by a rival re-upload. Still, it is
  the "silent success" the rubric's *Absent or unsupported entries* class names. Neither the module doc's
  limits (`:155-157`, re-upload only; `:240`, empty compensation) nor the brief's *Scope* lists it. Decide
  whether to accept it as a third documented limit (a one-paragraph doc change) or narrow the proof.

- NEEDS-HUMAN [human] — **(K) says only the h4 test's outcomes change. A second one changed too.**
  `multipart_upload_part_commit.rs:5642-5758`: the last run of `r_damage_under_a_held_commit_keeps_the_doubt`
  went from `committed_as` to `InDoubt { Commit, may_still_commit: true }` with a torn-part cause. In that run
  the commit **did** land and its part key was damaged afterwards. This follows from the iteration-2
  sign-off ("undecodable bytes at the part key must not fall through to `Committed`"). The history is
  forced: it gives the same reads as the unlanded D1 torn run. But it breaks the brief's literal (K)
  promise. Sign-off should record it as an accepted deviation, so a reviewer does not wave it through as
  "same behaviour".

- NEEDS-HUMAN [human] — **The T4 batch review looks empty again.** `check-gates.json` row `T4-batch-review` is a
  gating `pass` ("0 blocking, 0 recorded-rejected, 0 noise-dropped") after 26.44 s for a 131 KB patch
  (`gate-logs/T4-batch-review.log`). The `review-batch.md` it names is 563 bytes on disk. I only saw the
  file size, not the contents, since the file is not one of my inputs. The iteration-2 sign-off flagged the
  same thing (17 s, empty file) and asked for "a real high-effort review of the final patch" this round.
  This gate row does not show one happened. Confirm before sign-off.

## What I tried and could not break

- **Red→green, re-run at `$PDCA_TARGET`.** The patched tree is 92/92 green. With the base `commit.rs` and the
  patched test file, 21 tests go red, among them every new leg (S, X3, O×3, T×3, F, D1×2, D2, K×2) and the
  reworked h4. The tests drive the real `commit_part` through the wrapper over redb
  (`multipart_upload_part_commit.rs:1098-1101`), not a copy of it. That fills in C4-verify's pre-declared
  `PASS (green-only)`.
- **Hand-made mutants: 24 of 25 caught.** These include every demonstrated red the brief lists: the
  compensation sent again after an unknown round; slot-gone accepted for a non-empty part (D3); the
  obligation read under the first epoch, not the round's (O rollback); no obligation read after a moved
  session (`:1300-1304`, T second history); a moved session accepted for an empty part (T first history);
  other bytes at the obligation key taken as proof, as absent, or lowering the flag (O); a missing session
  lowering the flag with or without the later slot read (T); a plain stop in place of each kept doubt (F); the
  part-key check removed, or made on the lap's first read (`:1024-1038`, D1, D2, read-gap legs); a torn
  part ignored. The only survivor is the first finding above.
- **Forging the non-empty proof.** The re-upload commit's obligation shares the key (`commit.rs:1157-1173`),
  but every commit sent before a compensation was answered `Conflict`, so none landed. A chased round keys
  under its own epoch (`:1328`, `:1340`). Payload equality uses the same `encode` the batch puts
  (`:945`, `:1229`). No forgery found.
- **Forging the empty proof.** Another attempt's slot at the freed index can only appear after this slot
  was released. A teardown or Abort changes the session bytes first, and an epoch never comes back. The
  only forgery is damage that erases the slot, which is the documented limit (`:240`).
- **Gates.** C5's "fail" is 3 timeouts, all infinite-loop mutants on the lap guard (`:1265`, `:1294`), with 0
  missed. C4-diff-cov's "does not apply" is the expected stale base. C4-ci reports all checks passed.
<!-- pdca:leaf-complete -->

### Advisory — code-review

No findings. The diff is clean on both requested lenses: introduced correctness bugs and actionable reuse, simplification, or efficiency issues.

Reviewed the part-record proof (`crates/core/src/multipart/commit.rs:1024`), compensation settlement (`crates/core/src/multipart/commit.rs:1254`), and new regression tests (`crates/core/tests/multipart_upload_part_commit.rs:6109`, `crates/core/tests/multipart_upload_part_commit.rs:6845`, `crates/core/tests/multipart_upload_part_commit.rs:7123`) against `$PDCA_TARGET`, including read ordering, error propagation, epoch selection, and test-hook fidelity.

Validation evidence came from the frozen gate logs; no builds or tests were rerun. C4-ci passed. C5 reported 14 caught mutants, 7 unviable, and 3 timeouts, with no surviving mutants reported.

<!-- pdca:leaf-complete -->

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] Validation — fitness-to-purpose — Accept the final implementation as sufficient for this pre-production commit slice — the exercised storage histories support its contract, while client-facing UploadPart integration belongs to #669 (crates/core/src/multipart/commit.rs:343; brief.md:210).
- [ ] **An empty part's second settle lap is never tested.** `crates/core/src/multipart/commit.rs:1284`: narrowing the empty part's proof to the first lap (`if obligation.is_none() && laps == 1`) leaves all 92 tests green. The second lap is there to catch a compensation that lands after lap 1's slot read, and no test lands one there. (S) row 3 lands before lap 1 (`multipart_upload_part_commit.rs:6158`); row 4 lands only after the return. Concrete case: an empty part on the rival route, the compensation `Act::hold()`, and `land_held(Committed)` at gap 2 of `at_compensation_gaps(&attempt, 0, 2, …)` (after lap 1's session read). I ran it in a scratch copy. The patch gives `Refused(OperationAborted)` with 2 laps of reads, which is correct. The mutant gives `InDoubt { Compensation, may_still_commit: true, OutcomeUnknown }`. Fix: add this run to (S). It costs availability, not C-1 safety, but nothing in the suite would catch the regression.
- [ ] **One damage-only false `Committed` is left: a rival's record plus an erased slot.** `commit.rs:1032` (`Ok(Some(_)) => doubt.cannot_land()`) takes any record that decodes as proof (`:151`, "Any record there is a record, this attempt's or a rival's"). Concrete case, run in a scratch copy: a first-upload commit is held and never lands. At gap 0 a same-number rival commits and the attempt's slot key is erased; the session is untouched. Result: `Ok(Committed { digest: <this attempt's> })`. But the record is the rival's (`is_own` is false), the held batch later answers `Conflict`, and `releases == 0`. The brief's (K) asks for "any record there is a record", so this is by design. A record left by this history reads the same as a landed commit later replaced by a rival re-upload. Still, it is the "silent success" the rubric's *Absent or unsupported entries* class names. Neither the module doc's limits (`:155-157`, re-upload only; `:240`, empty compensation) nor the brief's *Scope* lists it. Decide whether to accept it as a third documented limit (a one-paragraph doc change) or narrow the proof.
- [ ] **(K) says only the h4 test's outcomes change. A second one changed too.** `multipart_upload_part_commit.rs:5642-5758`: the last run of `r_damage_under_a_held_commit_keeps_the_doubt` went from `committed_as` to `InDoubt { Commit, may_still_commit: true }` with a torn-part cause. In that run the commit **did** land and its part key was damaged afterwards. This follows from the iteration-2 sign-off ("undecodable bytes at the part key must not fall through to `Committed`"). The history is forced: it gives the same reads as the unlanded D1 torn run. But it breaks the brief's literal (K) promise. Sign-off should record it as an accepted deviation, so a reviewer does not wave it through as "same behaviour".
- [ ] **The T4 batch review looks empty again.** `check-gates.json` row `T4-batch-review` is a gating `pass` ("0 blocking, 0 recorded-rejected, 0 noise-dropped") after 26.44 s for a 131 KB patch (`gate-logs/T4-batch-review.log`). The `review-batch.md` it names is 563 bytes on disk. I only saw the file size, not the contents, since the file is not one of my inputs. The iteration-2 sign-off flagged the same thing (17 s, empty file) and asked for "a real high-effort review of the final patch" this round. This gate row does not show one happened. Confirm before sign-off.
- [ ] **The obligation-mismatch rule contradicts the required result and can discard real doubt.** `brief.md:274-276` says to lower `may_still_commit` when the obligation key holds other bytes, but (O), `brief.md:68-71`, explicitly requires `may_still_commit: true` for that held-compensation case. The compensation requires the obligation key absent (`crates/core/src/multipart/commit.rs:1091-1096`); occupation is not permanent evidence against landing, because the drain deletes obligations (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:2218-2222`). A held batch can land after those bytes are removed while its session and slot still match. Revise the design to preserve the flag on mismatched obligation bytes, consistently with (O), and require a regression that removes the planted bytes after return and then lands the held compensation.
- [ ] **The added part read cannot satisfy the promise that all other tests remain unchanged.** `brief.md:96-97` permits reworking only h4, while `brief.md:278-282` requires a part-key read after the slot read. The existing landed-commit test asserts the exact four-read trace through `laps_of` (`crates/core/tests/multipart_upload_part_commit.rs:4319-4323`, helper at `:4261-4274`); `read_gap` likewise pins exact traces at `:4652-4656`. Those traces are part, session, slot, session: none contains the newly required read. Thus a conforming implementation fails existing assertions even with unchanged outcomes. Explicitly permit the necessary trace/helper updates and require preservation of the existing behavioral assertions and interleaving histories, including coverage of the new read gap.
- [ ] **The missing-session rule omits the proof that distinguishes terminal deletion from repairable damage.** `brief.md:274-276` allows lowering the flag when the session “was deleted,” but supplies no criterion for a missing session with a surviving slot. The tracker explicitly requires “a slot read made after the session move ... before letting it go” (`notes.json`, body, Design). On the declared base, `Doubt::seen_session` preserves doubt on a missing record (`crates/core/src/multipart/commit.rs:810-817`), and `gone_in_doubt` clears it only when a subsequent slot read is also absent (`:847-866`). Revise the compensation design to retain that distinction and add both histories: erase only the session, return with the flag true, restore it and land the held compensation; versus terminal deletion, established by a later absent-slot read, with the flag false. The existing commit-only tests do not verify the new compensation path.
- [ ] size backstop — this slice is behaving oversized: patch is 128 KB (threshold 100 KB). Recommend answering `iterate-plan` at sign-off and authoring the split in the re-plan (`pdca split`), rather than `iterate-do`: a slice that is too big yields implementation-shaped findings every round, and splitting authors briefs, which is Plan's beat.

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
- Iteration delta (if iterating): Implementation is correct and was independently reviewed in depth; close two small gaps and record one deviation. The design (brief) stands. - Add the untested second settle lap for an empty part (commit.rs:1284; the mutant `obligation.is_none() && laps == 1` survives all 92 tests): empty part on the rival route, compensation Act::hold(), land_held(Committed) at gap 2 of at_compensation_gaps(&attempt, 0, 2, ...) (after lap 1's session read) -> Refused(OperationAborted) after 2 laps of reads. Add it to (S). - Rival record + erased slot (commit.rs:1032, `Ok(Some(_)) => doubt.cannot_land()`): a held, never-landing first-upload commit, a same-number rival commit and the slot erased at gap 0 answers Committed with this attempt's digest. Document it as a third known limit in the module doc (beside :155-157 and :240) and the living docs, stating that only damage (an erased slot key) reaches it and that it reads the same as a landed commit replaced by a rival re-upload. Narrow the proof instead only if that is small and keeps every existing (K) outcome. - Accepted deviation from (K): r_damage_under_a_held_commit_keeps_the_doubt's last run now answers InDoubt { Commit, may_still_commit: true } (torn part) instead of committed_as. It follows from the iteration-2 rule that torn bytes never count as Committed. Keep it. - The plan-review findings (obligation mismatch, trace updates, missing session) remain covered by the brief. - Size backstop (128 KB) is overridden again: do not split. - The C5 timeouts are caught mutants (they defeat the settle loop's termination), not survivors.
- By / date: Eduard Ralph / 2026-11-10

## 10. Act candidates (hints for the next Act review)
- Plan advisory: 3 finding(s); brief revised: yes (plan-advisory-*.md)
- (empty is the common case)
