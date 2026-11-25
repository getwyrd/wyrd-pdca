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
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): fail — 26 mutants tested in 4m: 1 missed, 15 caught, 7 unviable, 3 timeouts

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): pass — review-branch: 0 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_1030/review-
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 16.10s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Review #1030’s settlement of unknown multipart compensations and prevention of false commit success: the reproduced defects are fixed, with one confirmed regression-test gap remaining.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The landed-answer evidence, no-resend invariant, damage cases and exclusions make the intended behavior falsifiable; `brief.md:33`, `brief.md:135`, `brief.md:162`. |
| C2 Reproduction (red pre-fix) | PASS | With only the source fix stashed, the new tests reproduced both an applied compensation incorrectly remaining in doubt and an unlanded commit incorrectly succeeding; `reviewer-evidence/red-compensation.log:183`, `reviewer-evidence/red-commit.log:12`. |
| C3 Change | PASS | The changes stay within the two specified settlement paths, their tests and living documentation; existing histories retain their outcomes except the explicitly revised H4 case; `crates/core/src/multipart/commit.rs:1021`, `crates/core/src/multipart/commit.rs:1329`, `crates/core/tests/multipart_upload_part_commit.rs:2774`. |
| C4 Verification (red→green) | PASS | Restoring the fix makes all 90 multipart tests pass; frozen full CI is green, while the independent full-CI attempt lacks the snapshot’s RustSec gitlink, a target-state limitation rather than a patch defect; `reviewer-evidence/green-suite.log:97`, `gate-logs/C4-ci.log:4834`, `reviewer-evidence/ci.log:3650`. |
| C5 Causal adequacy | NEEDS-HUMAN [impl] | Add a regression for corrupt bytes at the post-slot part read followed by a session-read fault — the existing suite tolerates a mutation that clears `may_still_commit` although the held commit subsequently lands after repair; `crates/core/src/multipart/commit.rs:1025`, `crates/core/tests/multipart_upload_part_commit.rs:6938`. |
| T1 Structure | PASS | Reusing the existing doubt and record-validation helpers preserves the MetadataStore seam and error classification without a new public type or persisted format; `crates/core/src/multipart/commit.rs:863`, `crates/core/src/multipart/commit.rs:955`. |
| T2 Shape | PASS | The tests force the relevant interleavings over a real store and check ownership and releases, while the living architecture explains the changed guarantees; `crates/core/tests/multipart_upload_part_commit.rs:6202`, `crates/core/tests/multipart_upload_part_commit.rs:6612`, `docs/design/architecture/05-building-block-view.md:250`. |
| T3 Runtime | PASS | Settlement has bounded read laps, performs no further batch after an unknown result, and introduces no clock source; individual calls inherit the backend termination contract; `crates/core/src/multipart/commit.rs:1285`, `crates/core/src/multipart/commit.rs:1330`, `crates/traits/src/lib.rs:1334`. |
| T4 Contribution | N/A | Contribution artifacts are intentionally drafted after Check and must be audited again at publish; affected-path prior-art checks independently found no closed-unmerged overlap; `gate-logs/T4-contribution.log:10`, `reviewer-evidence/prior-art.json:58`. |
| T5 Judgment | PASS | The proof addresses the specified cause without a capability probe or load-time workaround; production wiring and seeded DST remain the explicit #669/#660 deferrals, which are settled for this review; `crates/core/src/multipart/commit.rs:341`, `crates/core/src/multipart/commit.rs:346`. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Accept the no-resend availability/capacity tradeoff before production wiring — a compensation whose landing cannot be proved can retain its slot until the session ends; `brief.md:282`, `brief.md:363`, `crates/core/src/multipart/commit.rs:263`. |

**One builder-fixable finding: protect the new proof read’s corruption behavior with a regression.** The implementation currently preserves the flag correctly, but K’s cases cover rivals, rollbacks and read faults without covering undecodable bytes at this new read (`crates/core/tests/multipart_upload_part_commit.rs:6940`). The surviving mutation changes the decode guard at `crates/core/src/multipart/commit.rs:1025` to `true`, treating repairable damage as proof that the commit cannot land. This is a non-equivalent survivor; the repository’s mutation policy calls for a regression (`.cargo/mutants.toml:11`).

I independently reproduced the survivor with `cargo mutants --cap-lints true` (`reviewer-evidence/mutant-capped.log:4`). A diagnostic test in an isolated copy then held a first-upload commit, erased its slot, planted undecodable part bytes immediately before the post-slot part read, and faulted the final session read. After the response, it repaired the slot and removed the corrupt part bytes, then released the held commit. The current code reported `may_still_commit=true` and the batch landed; the mutant reported `false` and the same batch still landed. The diagnostic passes on the patch and fails on the mutant (`reviewer-evidence/survivor-probe-original.log:177`, `reviewer-evidence/survivor-probe-mutant.log:7`). Its runnable test body is retained in `reviewer-evidence/survivor-probe.rs`; no diagnostic test or mutation was left in the target.

**The verification evidence supports the fix, with two explicit measurement limits.** I stashed only `crates/core/src/multipart/commit.rs`, retained the patched tests, ran the exact S and D1 tests to red, restored the stash, and ran `cargo test --locked -p wyrd-core --test multipart_upload_part_commit`: 90 passed. The target diff remains byte-identical to the supplied `patch.diff`.

- **C4-ci:** the frozen log records the full gate passing, including real `typos`, docs rendering/link audit, compilation, tests, dependency checks and DST (`gate-logs/C4-ci.log:11`, `gate-logs/C4-ci.log:4194`, `gate-logs/C4-ci.log:4834`). The independent rerun also exercised spelling and rendered 101 pages, but the disposable Git index omits `.github/advisory-db`: `git ls-files -s -- .github/advisory-db` returned empty, explaining the advisory test and pin-check failures (`reviewer-evidence/ci.log:3650`). The rerun finished with exactly those two failures; its seeded DST campaign also passed (`reviewer-evidence/ci.log:4215`, `reviewer-evidence/ci.log:4295`). This is a snapshot limitation; neither that gitlink nor its tests is changed by the patch.
- **C4-verify:** its frozen result is explicitly green-only (`gate-logs/C4-verify.log:162`). The independent source-only stash run supplies the missing red→green evidence.
- **C4-diff-cov:** coverage was not measured because the patch did not apply to `origin/main` (`gate-logs/C4-diff-cov.log:11`); the brief identifies the unmerged prerequisites (`brief.md:191`). The supplied target matches the patch and builds. No coverage percentage or patch-application defect is inferred.
- **C5-mutants:** the frozen scan reports 15 caught, 7 unviable, 3 timed out and 1 missed (`gate-logs/C5-mutants.log:17`). I independently confirmed and distinguished the missed mutation above; the other classifications are frozen evidence, not fresh reruns.
- **T4-batch-review:** the frozen log reports zero blocking findings (`gate-logs/T4-batch-review.log:10`); this is a log-based adjudication.
- **host-tikv:** I independently reran both supplied clippy commands; the metadata crate and server selections compile successfully (`reviewer-evidence/host-tikv.log:103`, `reviewer-evidence/host-tikv.log:202`). This corroborates the frozen log; compilation is not a live-cluster test.
- **T4-contribution:** N/A until publish, as the frozen deferred row requires (`gate-logs/T4-contribution.log:10`). The declared external dependencies, `typos` and the docs renderer, were actually exercised; neither remains undischarged.

**Prior art was checked by all three affected paths.** GitHub’s merged history confirms #1027 for the source and test files; the architecture history contains the other documented feature work. Of 445 closed PRs, all 19 unmerged PRs had their changed-file lists checked: none touches these paths (`reviewer-evidence/prior-art.json:3`, `reviewer-evidence/prior-art.json:58`). The documented #660 DST deferral is not re-raised.

<!-- pdca:leaf-complete -->

### Advisory — adversary

# Adversarial review — issue 1030 (compensation settle + own-commit part read)

I rebuilt the patched tree in scratch and re-ran `cargo test -p wyrd-core --test multipart_upload_part_commit`. With the patch: 90/90 pass. With the base `commit.rs` and the patched tests: 18 fail, D1 among them (the base answers `Committed`). Then I applied, one at a time, each wrong-fix edit the brief lists, and each one turned its named leg red (details at the end). So the red→green evidence holds. One gap in the fix and one test gap remain, both on the new part read.

## Findings

- NEEDS-HUMAN [impl] — `crates/core/src/multipart/commit.rs:1022-1027` with `:1035-1043`: **the own-commit proof still answers `Committed` for a commit that never landed when the part key holds bytes that don't decode.** The proof read treats undecodable bytes as "something there" (`Some(_) => {}`, `:1027`), so `no_record` stays false and `(Same, Gone)` returns `Committed`. Concrete case, reproduced in scratch: stage part 1 on a fresh rig (the commit requires the key absent), script the commit `Act::hold()`, and at gap 0 erase the slot key and plant `b"torn"` at the `part:` key. The session is untouched. The answer is `Ok(Committed { part_number: 1, .. })`, the `part:` key still holds `b"torn"`, and the held batch, released afterwards, answers `Conflict`, so no commit ever landed. The same answer comes when the first part read finds the key absent and the torn bytes are planted at proof gap 3. This is the D1 false-success class (the client is told a part exists that has no record) under two pieces of damage. It conflicts with the brief's Design ("the proof also needs that key to hold a **record**", brief.md:327-329), with "Damage settles nothing" (brief.md:320), and with the rubric's ADR-0045 rule that structural damage surfaces as an error, never as a value. The reviewer likely accepted the module doc's "bytes that do not decode are not an absent key" (`commit.rs:151-152`) because the brief's Invariant sentence (brief.md:141-142) only names "absent". Fix: when the proof read finds bytes that don't decode, keep the doubt with `Stop::Record(err)` and the flag unchanged instead of falling through to `Committed`. A real landed commit leaves a decodable record, so the only cost is availability under damage, which 0016 allows.

- NEEDS-HUMAN [impl] — `crates/core/src/multipart/commit.rs:1025`: **the C5 MISSED mutant is a real test gap.** No test pins that undecodable bytes at the proof read leave `may_still_commit` alone. Replacing the guard with `true` keeps all 90 tests green (I reproduced this). Here is a killing case that I ran: held first-upload commit, with `at_proof_gaps` used to erase the slot at gap 0, plant `b"torn"` at the part key at gap 3, and fail reads of the session key at gap 4. Expect `InDoubt { batch: Commit, may_still_commit: true }` with the store fault as its cause. The patch passes; the mutant answers `may_still_commit: false`. Add it to (K) (`crates/core/tests/multipart_upload_part_commit.rs:6938`). If the first finding is adopted, that arm changes, but its flag still needs this test.

- NEEDS-HUMAN [human] — `check-gates.json` row `T4-batch-review`: the row claims "3x codex, union, triaged" with "0 blocking, 0 recorded-rejected, 0 noise-dropped", and the run took **17.13 s** (`gate-logs/T4-batch-review.log`). The diff is about 1,500 lines, so three model review passes in 17 s are implausible. The result may be cached from an earlier patch, or empty. The brief requires "a confirmed high-effort review of the final patch" (brief.md:208-209). Before sign-off, confirm that `results/issue_1030/review-batch.md` was produced for this `patch.diff`. I didn't read it, since it's outside my inputs.

- Note (no action): `C4-verify` is green-only by design (brief.md:188-191), so the gate never showed a red. My re-run supplies that red (above). Some brief claims of "green on base" are off, though they don't affect the fix. `t_an_empty_compensations_slot_gone_under_a_moved_session_is_in_doubt` (test file `:6445`) is red on base: the base keeps `may_still_commit: true`, and the test expects `false`. The brief (brief.md:132) says (T)'s first history is green on base. `d2_…` (`:6822`) and `k_…` (`:6938`) are red on base only through their read-trace assertions. Their value as guards comes from the mutation reds below, which do hold.

## Attempted, could not refute

- **Resending after an unknown outcome** (attempt 7's `compensate`): red in `s_…`, `x3_…`, `h4_a_compensation_…`, `d3_…`.
- **Obligation keyed under the commit's (staged) epoch** instead of the round's: red in `o_a_compensation_under_a_newer_epoch_…`. The re-upload commit's obligation uses the same key form, but every commit sent before a compensation was answered `Conflict` (`commit.rs:1541`, `:1554-1569`), so it can't forge those bytes.
- **Empty part, slot gone accepted under any session**: red in both (T) runs. Late staging only adds slots, so it can't fake "gone"; the X3 empty run keeps the doubt.
- **Non-empty part settling on its slot alone**: red in `d3_…`.
- **Dropping the final obligation read**: red in `t_a_compensation_landing_before_a_rollback_…`, `t_a_missing_session_…`, `f_…`, `s_…`.
- **Lowering the flag on other bytes at the obligation key**: red in `o_other_bytes_…`.
- **Lowering the flag on a missing session without the later slot read, or skipping that read with the flag kept**: red in `t_a_missing_session_…` and `f_…`.
- **Turning torn slot or session bytes into a plain stop**: red in `f_…`.
- **Removing the part-key check**: red in `d1_…`.
- **Moving the decision to the lap's first part read**: red in `d2_…`, `b_read_gap_landed_then_a_rival_is_committed`, `k_…`, `r_a_session_damaged_and_repaired_mid_lap_…`.
- **A rival's decodable record at the part key plus an erased slot** also answers `Committed` without this attempt landing. I did not raise it: brief.md:328 explicitly accepts "this attempt's or a rival's" record.
- **Docs currency**: `docs/design/architecture/05-building-block-view.md:250` is updated, and the module doc tables match the code paths I traced.

The probe tests and mutations lived only in my scratch copy and are not part of the patch.

<!-- pdca:leaf-complete -->

### Advisory — code-review

- NEEDS-HUMAN [impl] — `crates/core/tests/multipart_upload_part_commit.rs:6939`: Add a damaged-record case for the new second part read. The frozen C5 log reports that replacing the decode guard at `crates/core/src/multipart/commit.rs:1025` with `true` survives. This changes observable behavior: hold a first-upload commit, erase its slot, plant undecodable part bytes before the second part read, then fault the final session read. The mutant incorrectly clears `may_still_commit`; removing the damaged part bytes and restoring the required slot lets the held commit land. Assert that this history preserves the flag and that the repaired batch lands. Existing held-commit part-damage tests keep the slot held, while the new fault cases read valid part bytes, so neither protects this distinction.

No introduced implementation defect or actionable reuse, simplification, or efficiency issue found. Review used the target source and frozen gate evidence; no builds were rerun.

<!-- pdca:leaf-complete -->

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C5 Causal adequacy — Add a regression for corrupt bytes at the post-slot part read followed by a session-read fault — the existing suite tolerates a mutation that clears `may_still_commit` although the held commit subsequently lands after repair; `crates/core/src/multipart/commit.rs:1025`, `crates/core/tests/multipart_upload_part_commit.rs:6938`.
- [x] Validation — fitness-to-purpose — Accept the no-resend availability/capacity tradeoff before production wiring — a compensation whose landing cannot be proved can retain its slot until the session ends; `brief.md:282`, `brief.md:363`, `crates/core/src/multipart/commit.rs:263`.
- [ ] `crates/core/src/multipart/commit.rs:1022-1027` with `:1035-1043`: **the own-commit proof still answers `Committed` for a commit that never landed when the part key holds bytes that don't decode.** The proof read treats undecodable bytes as "something there" (`Some(_) => {}`, `:1027`), so `no_record` stays false and `(Same, Gone)` returns `Committed`. Concrete case, reproduced in scratch: stage part 1 on a fresh rig (the commit requires the key absent), script the commit `Act::hold()`, and at gap 0 erase the slot key and plant `b"torn"` at the `part:` key. The session is untouched. The answer is `Ok(Committed { part_number: 1, .. })`, the `part:` key still holds `b"torn"`, and the held batch, released afterwards, answers `Conflict`, so no commit ever landed. The same answer comes when the first part read finds the key absent and the torn bytes are planted at proof gap 3. This is the D1 false-success class (the client is told a part exists that has no record) under two pieces of damage. It conflicts with the brief's Design ("the proof also needs that key to hold a **record**", brief.md:327-329), with "Damage settles nothing" (brief.md:320), and with the rubric's ADR-0045 rule that structural damage surfaces as an error, never as a value. The reviewer likely accepted the module doc's "bytes that do not decode are not an absent key" (`commit.rs:151-152`) because the brief's Invariant sentence (brief.md:141-142) only names "absent". Fix: when the proof read finds bytes that don't decode, keep the doubt with `Stop::Record(err)` and the flag unchanged instead of falling through to `Committed`. A real landed commit leaves a decodable record, so the only cost is availability under damage, which 0016 allows.
- [ ] `crates/core/src/multipart/commit.rs:1025`: **the C5 MISSED mutant is a real test gap.** No test pins that undecodable bytes at the proof read leave `may_still_commit` alone. Replacing the guard with `true` keeps all 90 tests green (I reproduced this). Here is a killing case that I ran: held first-upload commit, with `at_proof_gaps` used to erase the slot at gap 0, plant `b"torn"` at the part key at gap 3, and fail reads of the session key at gap 4. Expect `InDoubt { batch: Commit, may_still_commit: true }` with the store fault as its cause. The patch passes; the mutant answers `may_still_commit: false`. Add it to (K) (`crates/core/tests/multipart_upload_part_commit.rs:6938`). If the first finding is adopted, that arm changes, but its flag still needs this test.
- [ ] `check-gates.json` row `T4-batch-review`: the row claims "3x codex, union, triaged" with "0 blocking, 0 recorded-rejected, 0 noise-dropped", and the run took **17.13 s** (`gate-logs/T4-batch-review.log`). The diff is about 1,500 lines, so three model review passes in 17 s are implausible. The result may be cached from an earlier patch, or empty. The brief requires "a confirmed high-effort review of the final patch" (brief.md:208-209). Before sign-off, confirm that `results/issue_1030/review-batch.md` was produced for this `patch.diff`. I didn't read it, since it's outside my inputs.
- [ ] `crates/core/tests/multipart_upload_part_commit.rs:6939`: Add a damaged-record case for the new second part read. The frozen C5 log reports that replacing the decode guard at `crates/core/src/multipart/commit.rs:1025` with `true` survives. This changes observable behavior: hold a first-upload commit, erase its slot, plant undecodable part bytes before the second part read, then fault the final session read. The mutant incorrectly clears `may_still_commit`; removing the damaged part bytes and restoring the required slot lets the held commit land. Assert that this history preserves the flag and that the repaired batch lands. Existing held-commit part-damage tests keep the slot held, while the new fault cases read valid part bytes, so neither protects this distinction.
- [x] **The obligation-mismatch rule contradicts the required result and can discard real doubt.** `brief.md:274-276` says to lower `may_still_commit` when the obligation key holds other bytes, but (O), `brief.md:68-71`, explicitly requires `may_still_commit: true` for that held-compensation case. The compensation requires the obligation key absent (`crates/core/src/multipart/commit.rs:1091-1096`); occupation is not permanent evidence against landing, because the drain deletes obligations (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:2218-2222`). A held batch can land after those bytes are removed while its session and slot still match. Revise the design to preserve the flag on mismatched obligation bytes, consistently with (O), and require a regression that removes the planted bytes after return and then lands the held compensation.
- [x] **The added part read cannot satisfy the promise that all other tests remain unchanged.** `brief.md:96-97` permits reworking only h4, while `brief.md:278-282` requires a part-key read after the slot read. The existing landed-commit test asserts the exact four-read trace through `laps_of` (`crates/core/tests/multipart_upload_part_commit.rs:4319-4323`, helper at `:4261-4274`); `read_gap` likewise pins exact traces at `:4652-4656`. Those traces are part, session, slot, session: none contains the newly required read. Thus a conforming implementation fails existing assertions even with unchanged outcomes. Explicitly permit the necessary trace/helper updates and require preservation of the existing behavioral assertions and interleaving histories, including coverage of the new read gap.
- [x] **The missing-session rule omits the proof that distinguishes terminal deletion from repairable damage.** `brief.md:274-276` allows lowering the flag when the session “was deleted,” but supplies no criterion for a missing session with a surviving slot. The tracker explicitly requires “a slot read made after the session move ... before letting it go” (`notes.json`, body, Design). On the declared base, `Doubt::seen_session` preserves doubt on a missing record (`crates/core/src/multipart/commit.rs:810-817`), and `gone_in_doubt` clears it only when a subsequent slot read is also absent (`:847-866`). Revise the compensation design to retain that distinction and add both histories: erase only the session, return with the flag true, restore it and land the held compensation; versus terminal deletion, established by a later absent-slot read, with the flag false. The existing commit-only tests do not verify the new compensation path.
- [x] size backstop — this slice is behaving oversized: patch is 118 KB (threshold 100 KB). Recommend answering `iterate-plan` at sign-off and authoring the split in the re-plan (`pdca split`), rather than `iterate-do`: a slice that is too big yields implementation-shaped findings every round, and splitting authors briefs, which is Plan's beat.

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
- Iteration delta (if iterating): Close the implementation gaps; the design (brief) stands. - Fix the false success at the own-commit proof's part read (crates/core/src/multipart/commit.rs ~:1022-1027, `Some(_) => {}`): undecodable bytes at the part key must not fall through to `Committed`. Keep the doubt with `Stop::Record(err)` and leave `may_still_commit` unchanged. Add the adversary's reproducer as a regression (held first-upload commit; erase slot at gap 0; plant b"torn" at the part key, both at gap 0 and at proof gap 3): never `Committed`. - Kill the surviving C5 mutant at commit.rs:1025: add the (K) case held commit + slot erased at gap 0 + torn part bytes at proof gap 3 + session-read fault at gap 4 -> `InDoubt { batch: Commit, may_still_commit: true }` with the fault as cause; then repair slot, remove torn bytes, release the held batch and assert it lands. - Human decisions at sign-off: the no-resend availability/capacity tradeoff is accepted; the plan-review findings (obligation mismatch, trace updates, missing session) are already covered by the brief; the size backstop (118 KB) is overridden — do not split. - The T4 batch review ran in 17 s with an empty review-batch.md; it did not really review this patch. Make sure the next Check gets a real high-effort review of the final patch.
- By / date: Eduard Ralph / 2026-11-10

## 10. Act candidates (hints for the next Act review)
- Plan advisory: 3 finding(s); brief revised: yes (plan-advisory-*.md)
- (empty is the common case)
