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
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 14.99s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

No implementation defect found in #1030’s change to settle unknown multipart compensations from reads alone and prevent false commit success after slot damage; fitness-to-purpose remains a human decision.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The required outcomes, forbidden resend, damage cases and accepted iteration changes are explicit and falsifiable; `brief.md:33`, `brief.md:135`, `brief.md:404`. |
| C2 Reproduction (red pre-fix) | PASS | With the implementation stashed and regressions retained, the base falsely reports a damaged-slot commit as committed and leaves a landed compensation in doubt; `reviewer-red.log:121`, `reviewer-red.log:181` (71 passed, 21 failed). |
| C3 Change | PASS | The three-file change stays within the authorized core settlement contract and updates its living documentation; `crates/core/src/multipart/commit.rs:1035`, `crates/core/src/multipart/commit.rs:1352`, `docs/design/architecture/05-building-block-view.md:250`. |
| C4 Verification (red→green) | PASS | Restoring the fix makes all 92 integration tests pass; formatting, spelling and docs checks also pass; full CI is supported by frozen evidence, while stacked-base diff coverage remains unmeasured; `reviewer-green-restored.log:99`, `reviewer-checks.log:1`, `gate-logs/C4-ci.log:4838`, `gate-logs/C4-diff-cov.log:10`. |
| C5 Causal adequacy | PASS | Exact obligation bytes identify the compensation’s landing; ordered part/slot/session reads reject absent or torn first-upload proof, and unknown outcomes cannot reach another batch; `crates/core/src/multipart/commit.rs:954`, `crates/core/src/multipart/commit.rs:1035`, `crates/core/src/multipart/commit.rs:1349`. |
| T1 Structure | PASS | The existing metadata trait and shared doubt state contain the change without widening dependencies, persistence formats or clock ownership; `crates/core/src/multipart/commit.rs:879`, `crates/core/src/multipart/commit.rs:1268`, `crates/core/src/multipart/commit.rs:348`. |
| T2 Shape | PASS | Read-gap helpers preserve the existing event order, the changed old outcome is explicitly accepted, and the third damage-only limit is documented; `crates/core/tests/multipart_upload_part_commit.rs:4047`, `crates/core/tests/multipart_upload_part_commit.rs:5738`, `crates/core/src/multipart/commit.rs:155`, `brief.md:404`. |
| T3 Runtime | PASS | Settlement performs at most two laps plus the specified final reads, sends no further batch, and uses the backend’s bounded-operation contract; the empty-part second-lap case executes successfully; `crates/core/src/multipart/commit.rs:1308`, `crates/traits/src/lib.rs:1334`, `crates/core/tests/multipart_upload_part_commit.rs:6195`. |
| T4 Contribution | N/A | Commit/PR artifacts are intentionally absent at Check; their substantive audit is required at publish, not a missing approval here; `gate-logs/T4-contribution.log:10`. |
| T5 Judgment | PASS | The accepted no-resend tradeoff, scope/size decisions and tracked DST deferral remain settled; affected-path prior-art checks found no competing closed-unmerged implementation; `brief.md:391`, `brief.md:404`, `crates/core/tests/multipart_upload_part_commit.rs:5933`, `reviewer-prior-art-paths.log:1`. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Confirm this core-only slice is fit for handoff to #669 — the evidence exercises metadata settlement, while end-user S3 UploadPart integration belongs to that later slice; `crates/core/src/multipart/commit.rs:354`, `brief.md:210`. |

Source citations above are relative to `$PDCA_TARGET`; bundle and log citations are relative to this review directory. The disposable target is coherent, and its restored diff is byte-identical to `patch.diff`.

The independent red→green run closes the frozen verifier’s green-only limitation. I stashed only `commit.rs` and the architecture change, retained the patched integration tests, and ran `cargo test --manifest-path "$PDCA_TARGET/Cargo.toml" --offline --locked -p wyrd-core --test multipart_upload_part_commit`. The red includes actual outcome failures, not only altered read-trace assertions: D1 reports `Committed` with no committed part, and S returns `InDoubt` for an applied compensation. After `git stash pop`, the same command passes all 92 tests (`reviewer-red.log:123`, `reviewer-red.log:183`, `reviewer-green-restored.log:99`). Build and temporary paths were confined to this review directory.

The tests exercise the dangerous interleavings over a real redb store. Held batches are actually submitted when released, and read hooks run after the underlying read, preserving the intended gaps (`crates/core/tests/multipart_upload_part_commit.rs:536`, `crates/core/tests/multipart_upload_part_commit.rs:706`). X3 checks replayed staging restores the exact slot bytes before testing the outcome; the settlement tests check obligation identities, individual releases and whole-store fragment ownership (`crates/core/tests/multipart_upload_part_commit.rs:6293`, `crates/core/tests/multipart_upload_part_commit.rs:1229`, `crates/core/tests/multipart_upload_part_commit.rs:1403`). The iteration requests are present: empty parts settle in the second lap, torn proof bytes keep doubt through a later fault, and the rival-record/erased-slot ambiguity is documented. No capability probe or load-time symptom guard was introduced.

Independent formatting, spelling, documentation lint and 101-page rendering/link-audit checks passed (`reviewer-checks.log:5`, `reviewer-checks.log:9`, `reviewer-checks.log:13`, `reviewer-docs.log:2`). Both declared external dependencies were exercised; no shim or code-read substitutes for the compile/test evidence. The frozen full CI log also records these tools running and finishes successfully; the TiKV log records actual crate/server feature compilation (`gate-logs/C4-ci.log:11`, `gate-logs/C4-ci.log:4838`, `gate-logs/host-tikv.log:211`). I did not rerun the entire workspace/TiKV gates. Diff coverage is still unavailable because its wrapper applies against the older `origin/main`; this is a stacked-base evidence limitation, not a patch compilation defect (`gate-logs/C4-diff-cov.log:10`).

The independent mutation check found no surviving mutant. Across all 24 candidates, it reproduced the frozen result: 14 caught, 7 unviable and 3 timeouts. The first local run rejected seven extra mutants on warning lints; rerunning exactly those seven with `--cap-lints=true` caught six and timed out the remaining loop-bound mutant (`reviewer-mutants.log:27`, `reviewer-mutants-lints.log:9`). The three timeouts remove progress or the lap bound at `crates/core/src/multipart/commit.rs:1279` and `crates/core/src/multipart/commit.rs:1308`; they detect nontermination, not surviving behavior. Thus the advisory gate’s nonzero result does not establish a patch defect (`gate-logs/C5-mutants.log:13`).

Prior art was independently checked by all three affected paths: merged history matches the brief, and none of the 19 closed-unmerged PRs returned by the paginated GitHub API touches those paths (`reviewer-prior-art-paths.log:1`, `reviewer-prior-art-paths.log:21`). The frozen batch-review log reports zero findings but contains no review text; the substantive judgment here comes from independently checking control flow, read ordering, test mechanics and the standing rubric (`gate-logs/T4-batch-review.log:10`). The publish-time contribution check remains N/A at this stage.

<!-- pdca:leaf-complete -->

### Advisory — adversary

# Adversarial review — issue 1030 (657.2d), iteration 4

**Verdict: I could not refute the fix.** I re-ran the red→green myself, planted eleven bugs, and
tried the histories most likely to break the new compensation settle and the own-commit proof's
new part-key read. Every planted bug was caught. One doc sentence overstates what a cause means
(damage-only, not a wrong answer). One gate row is not the evidence it claims to be.

## The evidence

- **Red→green is real and runs the production path.** I copied the patched tree to scratch and
  ran `cargo test -p wyrd-core --test multipart_upload_part_commit`: 92 passed. With
  `crates/core/src/multipart/commit.rs` put back to the base (`git show HEAD:…`) and the patched
  test file kept, 21 of 92 fail. The tests call the public `commit_part` over a real redb store
  through the scripted wrapper, so the code under test is the production code, not a copy of it.
  Of the 21, `b_a_landed_commit_answered_unknown_is_committed`, `b_read_gap_landed_then_*`,
  `h4_a_commit_answered_unknown_is_in_doubt_with_no_further_batch` and
  `r_a_missing_session_behind_a_held_commit_is_in_doubt` fail only because the lap's read trace
  gained the new part read, which (K) allows. The rest fail on outcome, cause or flag. One small
  error in the brief, and it is harmless: its *Falsifiability* says (T)'s first history is green
  on the base, but `t_an_empty_compensations_slot_gone_under_a_moved_session_is_in_doubt` is red
  there. The base keeps the store's `true` flag, and the test asserts `false`, so the test is
  stricter than the brief, not weaker. C4-verify's `PASS (green-only)` was pre-declared in the
  brief, and the red above fills that gap.
- **Eleven planted bugs, all killed** (each applied alone to `commit.rs`, then the suite run):
  - the new part read's record no longer lowers the flag (`commit.rs:1046`): `k_the_own_commit_proofs_part_read_keeps_the_coverage_of_the_reads_before_it` fails;
  - torn bytes at that read taken as a record (`commit.rs:1050`): three D1/K/R tests fail;
  - the part read made in every lap, not only when the slot is gone (`commit.rs:1035`): ten tests fail;
  - the compensation settle never lowers the flag on a moved session (`commit.rs:1292`): three T/F tests fail;
  - an empty part settles on "slot gone" under any session, Open(E′)/Left included (`commit.rs:1298`): the (T) empty-part test fails;
  - other bytes at the obligation key read as absent (`commit.rs:957`), or any bytes there read as its own (`commit.rs:956`): the (O) test fails in both cases;
  - a rewritten slot under a compensation answered `OutcomeUnknown` (`commit.rs:1304`): the (F) test fails;
  - the last obligation read dropped (`commit.rs:1314-1318`): four tests fail;
  - a missing session record lowers the flag with no slot read after it (`commit.rs:1301-1303`): two tests fail;
  - the terminal delete keeps the flag (`commit.rs:978-981`): two tests fail.

  C5 agrees: 0 missed. Its 3 timeouts (`commit.rs:1279`, `:1308`) break the settle loop's
  termination, so they count as caught.

## The fix: refutation attempts that failed

- **Obligation bytes as the witness (`commit.rs:947-958`).** I checked whether a legal writer
  could leave *other* bytes at a landed compensation's key. If it could, the code would wrongly
  answer `ObligationKeyTaken` with the flag kept. 0016's drain marks fragments idempotently
  ("never re-stamp") and deletes the obligation only when it is fully drained
  (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:2220-2222`). It never rewrites
  the obligation in place, and the drain is not yet in the tree. Not refuted.
- **Key collisions.** The re-upload commit uses the same key form (`commit.rs:1551`,
  `obligation_for` at `:1171-1187`), but every route into `compensate` (`:1541`, `:1580`,
  `:1586`, `:1591`, `:1636`, `:1650`, `:1676`) comes before any commit, or after commits
  answered `Conflict`. So that commit cannot land. The epoch chase recomputes the obligation under
  the chased epoch before each round (`:1342`), and `settle_compensation` gets that same object
  (`:1354`). Not refuted.
- **Empty-part proof (`commit.rs:1298-1300`).** I tried a teardown, an Abort, a rollback, and a
  rival reserving the freed index. The first three move the session bytes, so they never reach
  `Same`. The rival is a legal release. The only way to slot-gone under `Same` without a landing
  is damage, which is the documented limit at `commit.rs:248-251` in the module doc. Not refuted.
- **Own-commit part read timing (`commit.rs:1035-1052`).** A commit landing between the slot read
  and the new part read cannot happen without late staging rebuilding the slot first, and then
  the commit really did land, so `Committed` is true. The rival-record limit is documented and was
  settled at the iteration-3 sign-off. Not refuted.

## Findings

- `crates/core/src/multipart/commit.rs:531-533` (also `:150` and the comment at `:1038-1039`):
  the new `Stop::SlotLost` text says a slot gone under `Open@E`, with the part key still absent,
  means "no commit of this attempt landed". **Concrete case** (probed in scratch): the commit is
  scripted `Act::unknown(true, true)` (applied), and the part key is erased at gap 0 by damage.
  The answer is `InDoubt { batch: Commit, may_still_commit: true, cause: SlotLost }`, yet
  `rig.releases(&attempt) == 1`: the commit *did* land, and damage took the part record, not the
  slot. The answer itself is safe: it is in doubt, claims no release, and is never a false
  `Committed`. Only damage reaches it. But the public doc claims more than the read shows. A
  wording fix would be "no landed commit of this attempt can be shown; damage took the slot or
  the part record". Advisory only: I'm not routing it back, because a damage-only doc nit is not
  worth a rebuild.
- NEEDS-HUMAN [human] — `check-gates.json:103-113` (`gate-logs/T4-batch-review.log:10`): the
  gating T4 batch review reports `pass`, with "0 blocking, 0 recorded-rejected, 0 noise-dropped",
  in **20.8 s** on a 135 KB, ~2.2k-line patch. That is the same pattern the iteration-2 sign-off
  rejected ("ran in 17 s with an empty review-batch.md; it did not really review this patch"). The
  brief's *Verification posture* requires "a confirmed high-effort review of the final patch".
  This row's `pass` is not that evidence, and I cannot see `review-batch.md` from here. Before
  accepting, a human should confirm that a real review of *this* patch happened, by this round's
  reviewer leaf or by hand. Otherwise the "independently reviewed in depth" claim carried over
  from iteration 3 applies to an earlier patch, not this one.
- `check-gates.json:55-63` (C4-diff-cov `fail`, "does not apply on origin/main"): the brief
  predicted this, because the base is stacked on unmerged #1051 and #1060. It is not a defect in
  the patch, and I'm not counting it as a refutation.

Scratch used: one copy of the patched tree plus a cargo build dir under `$PDCA_SCRATCH`
(`pdca-adversary-1030-work`, 1.2 GB). Both are removed.

<!-- pdca:leaf-complete -->

### Advisory — code-review

- No introduced correctness bugs found. Reviewed the additional part-record proof (`crates/core/src/multipart/commit.rs:1035`), compensation settlement and doubt propagation (`crates/core/src/multipart/commit.rs:1268`), and the scripted interleavings and regression assertions (`crates/core/tests/multipart_upload_part_commit.rs:6110`, `crates/core/tests/multipart_upload_part_commit.rs:7154`).
- No actionable reuse, simplification, or efficiency findings. The settlement paths share `Doubt` and the existing record judges; the test hooks share `gap_chain` (`crates/core/src/multipart/commit.rs:889`, `crates/core/tests/multipart_upload_part_commit.rs:4333`).

Validation used frozen gate evidence, without rerunning builds or modifying the target. CI passed, including all 92 tests in the affected suite. Per-fix verification was green-only; diff coverage was not measured because the patch did not apply to origin/main. Mutation testing reported 14 caught, 7 unviable, 3 timeouts, and no surviving mutants.

<!-- pdca:leaf-complete -->

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [x] Validation — fitness-to-purpose — Confirm this core-only slice is fit for handoff to #669 — the evidence exercises metadata settlement, while end-user S3 UploadPart integration belongs to that later slice; `crates/core/src/multipart/commit.rs:354`, `brief.md:210`.
- [x] `check-gates.json:103-113` (`gate-logs/T4-batch-review.log:10`): the gating T4 batch review reports `pass`, with "0 blocking, 0 recorded-rejected, 0 noise-dropped", in **20.8 s** on a 135 KB, ~2.2k-line patch. That is the same pattern the iteration-2 sign-off rejected ("ran in 17 s with an empty review-batch.md; it did not really review this patch"). The brief's *Verification posture* requires "a confirmed high-effort review of the final patch". This row's `pass` is not that evidence, and I cannot see `review-batch.md` from here. Before accepting, a human should confirm that a real review of *this* patch happened, by this round's reviewer leaf or by hand. Otherwise the "independently reviewed in depth" claim carried over from iteration 3 applies to an earlier patch, not this one.
- [x] **The obligation-mismatch rule contradicts the required result and can discard real doubt.** `brief.md:274-276` says to lower `may_still_commit` when the obligation key holds other bytes, but (O), `brief.md:68-71`, explicitly requires `may_still_commit: true` for that held-compensation case. The compensation requires the obligation key absent (`crates/core/src/multipart/commit.rs:1091-1096`); occupation is not permanent evidence against landing, because the drain deletes obligations (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:2218-2222`). A held batch can land after those bytes are removed while its session and slot still match. Revise the design to preserve the flag on mismatched obligation bytes, consistently with (O), and require a regression that removes the planted bytes after return and then lands the held compensation.
- [x] **The added part read cannot satisfy the promise that all other tests remain unchanged.** `brief.md:96-97` permits reworking only h4, while `brief.md:278-282` requires a part-key read after the slot read. The existing landed-commit test asserts the exact four-read trace through `laps_of` (`crates/core/tests/multipart_upload_part_commit.rs:4319-4323`, helper at `:4261-4274`); `read_gap` likewise pins exact traces at `:4652-4656`. Those traces are part, session, slot, session: none contains the newly required read. Thus a conforming implementation fails existing assertions even with unchanged outcomes. Explicitly permit the necessary trace/helper updates and require preservation of the existing behavioral assertions and interleaving histories, including coverage of the new read gap.
- [x] **The missing-session rule omits the proof that distinguishes terminal deletion from repairable damage.** `brief.md:274-276` allows lowering the flag when the session “was deleted,” but supplies no criterion for a missing session with a surviving slot. The tracker explicitly requires “a slot read made after the session move ... before letting it go” (`notes.json`, body, Design). On the declared base, `Doubt::seen_session` preserves doubt on a missing record (`crates/core/src/multipart/commit.rs:810-817`), and `gone_in_doubt` clears it only when a subsequent slot read is also absent (`:847-866`). Revise the compensation design to retain that distinction and add both histories: erase only the session, return with the flag true, restore it and land the held compensation; versus terminal deletion, established by a later absent-slot read, with the flag false. The existing commit-only tests do not verify the new compensation path.
- [x] size backstop — this slice is behaving oversized: patch is 132 KB (threshold 100 KB). Recommend answering `iterate-plan` at sign-off and authoring the split in the re-plan (`pdca split`), rather than `iterate-do`: a slice that is too big yields implementation-shaped findings every round, and splitting authors briefs, which is Plan's beat.

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
- Plan advisory: 3 finding(s); brief revised: yes (plan-advisory-*.md)
- (empty is the common case)
- T4 batch review (scripts/review-branch) can't be audited on a clean result: raw pass output is discarded (review-batch.md is a bare header), codex runs at default model/effort, and runs take ~13–50 s regardless of patch size (here 20.8 s on 135 KB). Need: keep per-pass evidence and review depth that scales with the patch — file upstream (pdca-harness).
