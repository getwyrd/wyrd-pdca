# Design proposal — issue 1030 / multipart-part-commit-settle-compensation

> **core: part commit — a compensation with an unknown outcome is settled or stays in doubt (657.2d).**
> The last of the four slices of #927 (657.2). It builds on 657.2c (#1029).
>
> **Re-planned 2026-11-10** because #1029 changed its protocol at its own re-plan (2026-11-09): after a
> commit answered unknown it now sends **no batch of any kind**, so no compensation ever follows a commit
> in doubt. The same reasoning applies to the compensation, so this slice settles a compensation's
> unknown outcome **from reads alone** and never sends it again (see *Re-plan decision*). The previous
> brief and its plan review are archived in `iteration-v1/`; the re-plan note is `sources/replan-note.md`.
>
> **Citations** were resolved on 2026-11-10 against #1029's PR head,
> `origin/enhancement/1029-multipart-part-commit-settle-unknown` @ `1d1a1d0` (PR #1060), which folds in
> #1027 (merged), #1008 and #1028 (`1f6d6a4`). Re-resolve them on your base.

- **Slug:** multipart-part-commit-settle-compensation
- **Track:** alpha
- **Kind:** enhancement (design proposal)
- **Goal:** Settle a compensation answered `CommitUnknownResult` from reads, and stop the own-commit proof's
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
- **Success criterion:** Appended to `crates/core/tests/multipart_upload_part_commit.rs`. **In every leg below where a
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
- **Falsifiability:** RED is reachable in-process on `cargo test -p wyrd-core --test multipart_upload_part_commit`, the
  same suite the gating `cargo xtask ci` runs (the file has no `cfg` gate; `commit_part`, `CommitError`, `DoubtfulBatch`
  and `Stop` are public, test file `:78-79`). The wrapper forces every leg over a real redb store: unknown and held
  outcomes per batch kind (`Act::unknown`, `:406`; `Act::hold`, `:413`; `script`, `:487`), held batches landed in order
  (`release_held`, `:520`; `land_held`, `:3947`), and erasures, plants, Aborts, teardowns, rollbacks and rivals at each
  read gap (`at_gaps`, `:3971`). On the base, these go red because the base answers the in-doubt error unread
  (`commit.rs:1143`): (S) rows 1 and 3, (X3)'s non-empty run, (O), (T)'s second history for a non-empty part, (T)'s
  missing-session runs (the base's cause is `OutcomeUnknown`, and after the terminal delete its flag stays `true`), and
  (F) (no read is made, so no fault is its cause). D1 goes red because the base answers `Committed`
  (`commit.rs:918-925`). The rest are green on the base: (S) rows 2 and 4, (X3)'s empty run, (T)'s first history, D2
  and D3, and (K)'s new-read legs (the base has no such read). They guard against the wrong fix, and their reds are the demonstrated reds in `Verification posture`. (E) is
  the gating `C4-ci` check.
- **Invariant to restore:** **C-1** (`docs/principles.md` §5 C-1, §6 *storage lifecycle / reclamation*). A batch of the
  attempt is answered as landed only on evidence that it landed which no other batch can produce. For a compensation
  that evidence is its own obligation's exact bytes, or, for an empty part, its own slot gone while a session read made
  after that slot read still finds the bytes it required. It is never a slot held at its bytes (late staging rebuilds
  them), an obligation key found absent (the retirement drain deletes drained obligations whatever the session's
  state, `0016:2218-2222`), or the clock. `Refused(OperationAborted)` and `slot_released: true` promise that the
  attempt's chunks are owed to an obligation. `Committed` is never answered when a read made after the slot read finds
  the part key absent that the commit required absent. **Once its commit or its compensation is answered unknown, the
  attempt sends no further batch** (staging's own retry rounds are unchanged), so at most one of its slot-releasing
  batches lands. **Sources:** the maintainer's rule 2026-07-25; 0016's
  refutation standard (`0016:2811-2813`, "Only availability, latency, capacity and operational costs are acceptable
  trade-offs here"); `0016:1651-1653`, `:1780`, `:1792`; the trait's rule that an unknown outcome is not retried and is
  re-read instead (`crates/traits/src/lib.rs:240-247`, `:254-257`). SELF-TEST: the evidence rule binds every writer of
  the keys the re-read reads (staging's reserve, intent and renewal, rivals, Abort, the reaper's teardown, the retirement
  drain), so guarding one function cannot satisfy it.
- **Repo + branch target:** getwyrd/wyrd @ main
- **Depends on:** 1029
- **Conflicts with:**
- **Ordering note:** `Depends on` 657.2c (#1029, open as PR #1060, itself built on #1028's PR #1051, also open). This slice
  replaces #1029's compensation arm and its own-commit proof, so its base must carry #1029's branch. Both prerequisites
  are COMPLETE but unmerged, so the flow holds this bundle until a run's line carries #1029's branch or #1060 merges
  (`src/pdca_harness/flow.py`, `_runnable`). No `Conflicts with`: none of the planned alpha bundles (1010-1019, 668-670)
  touches `commit.rs`, the test file or the architecture doc; #668-#670 depend on this one, and #1009 (PR #1059) shows
  these files only through its stacked base (its own commit, `a0a6479`, touches neither). **Intake:** a re-plan of an
  iterate-to-Plan bundle the flow is driving, so exempt from wyrd-pdca-P1 (alpha is at 17 of 6 on 2026-11-10,
  `scripts/plan-cap --track alpha`). The bundle is two splits deep, so no split was proposed.
- **Surfaces:** data
- **Scope:** what a compensation's `CommitUnknownResult` means: the route's landed answer on evidence that it landed,
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
- **Difficulty:** medium
- **Do model:** opus
- **Repro instruction:** On the base, (1) script a compensation (any route of `compensating`, test file `:2678`) as
  `Act::unknown(true, false)`: the answer is `InDoubt { batch: Compensation }` although the obligation holds the bytes it
  put and the slot is gone (`commit.rs:1143`). (2) Stage part 1 on a fresh rig (the commit requires the key absent),
  script the commit `Act::hold()`, and erase the slot key at gap 0 (`at_gaps` + `erased`): the answer is `Committed` with
  no `part:` record (`commit.rs:918-925`). **Run both first and record the results in `build-notes.md`;** if either does
  not reproduce, say so and stop that leg.
- **External dependencies:** `typos`, `docs-renderer`
- **Test file:** `crates/core/tests/multipart_upload_part_commit.rs`, **appended** (plus the rework of
  `h4_a_compensation_answered_unknown_is_in_doubt_with_no_further_batch`).
- **Verification posture:** C4-verify reports `PASS (green-only)`, a pre-declared sign-off item: this instance's gate
  earns a red only from an **added** `tests/*.rs` file, and `run-verify.sh --classify` on a synthetic patch of
  `commit.rs`, this test file and the architecture doc prints only `CRATE crates/core` (2026-11-10). The wrapper lives in
  the existing file, as for #1027-#1029. C4-diff-cov will say "patch.diff does not apply on origin/main" until #1051 and
  #1060 merge; that is expected, not a patch defect. **Demonstrated reds (required, recorded in `build-notes.md`).** Each
  temporary edit fails the named leg:
  - the compensation sent again after an unknown round (attempt 7's resend) → (X3) and the one-batch assertions of (S);
  - a slot held at the bytes the compensation required taken as "did not land" → (X3)'s empty run, (S) row 2;
  - the slot-gone proof accepted for a non-empty compensation without its obligation → D3;
  - the obligation read keyed under the commit's epoch instead of the round's → (O)'s rollback leg;
  - no obligation read after the read that found the session moved → (T)'s second history, non-empty;
  - the session check after the slot read made always true → (T)'s first history;
  - the flag lowered on other bytes at the obligation key → (O)'s held run (the held batch, released once the bytes are
    gone, lands although the answer said it could not);
  - the flag lowered on a missing session record with no slot read after it → (T)'s erased-session runs; that slot read
    skipped, the flag kept → (T)'s terminal-delete runs;
  - each doubt-keeping return replaced with a plain stop → (F);
  - the part-key check removed from the own-commit proof → D1;
  - the part-key check made on the lap's first part read (before the slot read) → D2, and #1029's
    `b_read_gap_landed_then_a_rival_is_committed` at gaps 1 and 2.
  **C5 (advisory) survivors** are listed in `build-notes.md` with the reason each survives. **Review depth:** sign-off
  needs a confirmed high-effort review of the final patch.
- **Production reach:** (a) The tests run against a real store; production commits no part in this slice. (b) The S3
  `UploadPart` verb lands with #669, which depends on this slice, the last of the commit slices. (c) This slice **must
  not** add a production caller.
- **Citations expected:** Do must cite `path:line` on the target branch for every change.
  - **What changes:** `compensate`'s unknown arm (`commit.rs:1140-1145`) and `failed` (`:769-783`); the own-commit proof
    in `settle` (`:890-925`). Docs: the module doc's compensation section and stops table (`:174-190`, `:212`), the
    `InDoubt` variant doc (`:361-365`), `Stop::OutcomeUnknown`'s doc (`:457-461`), `slot_released`'s doc (`:379-381`), and
    `docs/design/architecture/05-building-block-view.md:250` ("A compensation with an unknown outcome still stops in
    doubt with no further batch; settling it is still to land"). Tests: the read-trace vocabulary (K) allows updating
    for the new part read.
  - **Peer for when the flag falls:** `Doubt::seen_session` (`commit.rs:810-818`) and `gone_in_doubt` (`:847-869`) for
    the commit; `session_gone` (`:742-758`) for the compensation's `Conflict` path. Peer tests:
    `r_a_missing_session_behind_a_held_commit_is_in_doubt` (test file `:5674`), `t_a_missing_session_with_the_slot_still_held_is_damage`
    (`:3285`), and the last run of `c_a_read_fault_at_each_read_under_a_held_commit_keeps_the_doubt` (`:5147`), which
    erases only the session record, puts it back and lands the held batch.
  - **Peer for reading without sending:** `settle` and `Doubt` (`commit.rs:785-952`): laps, each read folded in as it
    returns so a later fault keeps what earlier reads showed, and the own-commit proof's order (a slot read, then a
    session read after it). Mirror it for the compensation.
  - **Peer for the route's landed answer:** what `Compensated::Released` gives on each route: `refuse` (`:1177-1189`),
    `compensated_stop` (`:1194-1208`), the renewal arm (`:1432-1441`).
  - **Peer tests:** `x_late_staging_that_rebuilds_a_landed_commits_slot_never_compensates` (test file `:4734`) for (X3);
    `read_gap` (`:4623`) for gap histories; `c_a_read_fault_at_each_read_under_a_held_commit_keeps_the_doubt` (`:5147`)
    for (F); `compensating` (`:2678`) for the routes.
  - **Why a held slot proves nothing:** every reserve round puts the same bytes (`crates/core/src/multipart/stage.rs:1296-1298`);
    the reserve requires only the slot key absent and the session (`:1329-1334`); a held reserve round pins its index
    (`:1345-1349`); retry rounds at one clock reading stamp the same value (`:1567-1570`); the intent and the renewal
    require exactly the bytes before them (`:2325-2336`, `:1620-1630`); a batch may land after its error returned
    (`crates/traits/src/lib.rs:240-247`).
  - **Why the obligation's bytes prove landing, and its absence does not prove the reverse:** the key names this
    attempt, part and epoch (`0016:358-366`; `obligation_for`, `commit.rs:1031-1047`); installation is `require_absent`
    (`0016:369-373`); the commit's re-upload obligation uses the same key form with the prior's chunks
    (`commit.rs:1336-1337`) and no commit can land once a compensation is sent (`:1118-1123`); the drain deletes a
    drained obligation whatever the session's state (`0016:2218-2222`).
  - **Why an absent part key disproves a landed commit:** a `part:` record's only remover is the retirement drain, after
    the session leaves `Open` (`commit.rs:102-104`, `0016:351`).
  - **Do not start from #927's attempt 7** (`results/issue_927/iteration-v7/patch.diff`): its `compensate` sends the
    compensation again after an unknown round on the claim that "at most one lands", which #1029's plan review disproved.
- **Prior-art check (triage cycles):** by path on `origin/main` @ `d04e507`, 2026-11-10. `crates/core/src/multipart/commit.rs`
  and the test file: one merged commit, `5e8aa24` (#1027, PR #1041). Open PRs touching them: #1051 (#1028) and #1060
  (#1029), the prerequisites, and #1059 (#1009) only through its stacked base. The architecture doc: recent merges from
  other features only. No closed-unmerged PR since 2026-11-01 touches these paths; before that, #1029's iteration-2 review
  enumerated all 445 closed PRs and found none. Design history: #927's attempts 1-7, #1029's iterations 1-3 and its
  2026-11-09 re-plan, and this bundle's iteration 1. Do not take from #927's v7 or this bundle's iteration 1 that a
  compensation may be sent again after an unknown outcome, or that a held slot proves it did not land.
- **Disposition hint:** new-feature

## Motivation

**A compensation answered unknown should give the answer it earned when the store shows it landed, and a commit that
cannot have landed should never be answered `Committed`.** Two reasons make this worth doing now:

- **It is the last commit slice before production.** #669 wires the S3 `UploadPart` verb onto this commit. With the base
  as it is, a TiKV commit RPC that times out on a compensation turns every refusal on that path (a ceiling, a rival, a
  rollback) into an in-doubt error the client cannot act on, even when the compensation landed.
- **The own-commit proof has one false success.** With only the slot key damaged, it answers `Committed` for a part that
  has no record. Every other damage finding so far was a wrong refusal; this one tells the client a part exists that does
  not. The human chose on 2026-11-01 to close it here, where the store still shows it.

## Re-plan decision (2026-11-10)

**Chosen: a compensation answered unknown is settled from reads alone and never sent again, the same rule #1029 adopted
for the commit.** Four consequences follow from #1029's re-plan (`results/issue_1029/brief.md`, *Re-plan decision* and
*Plan review*):

1. **Compensations come only after a settled stop.** Every route that compensates has sent no commit, or only commits
   answered `Conflict`, which never land (`commit.rs:1118-1123`). So the old leg "a held commit beside an unknown empty
   compensation" is dropped: #1029's one-batch leg, (X) and (X2) already pin that no compensation follows a commit in
   doubt.
2. **"Did not land" has no proof for a compensation either.** A slot held at the bytes it required can be rebuilt by
   late staging (the history #1029's (X) builds), and an absent obligation can be the drain's work (`0016:2218-2222`).
   So the old rows "not landed → sent again", "held, a later round lands" and "every round unknown → four sends" are
   gone. They needed exactly that proof.
3. **The cost is availability and capacity.** A compensation that did not land, or that landed and was then hidden (its
   obligation drained, or an empty part's slot rebuilt), answers in doubt, and its slot stays held until the session
   ends. 0016's refutation standard allows that (`0016:2811-2813`). This is the same trade the human accepted for #1029.
4. **A cross-phase leg aimed at the compensation is added** ((X3)), as the re-plan note asked.

The old brief's damage-only legs and its obligation re-read, teardown and fault legs carry over, re-shaped for a settle
that never sends.

## Design

**The compensation's evidence.** After a compensation round answered unknown, the attempt reads and sends nothing more.
It answers the route's landed answer only on one of two proofs, and otherwise keeps the doubt.

- **Non-empty part: its own obligation, at the bytes the round put**, under the epoch that round required. Only this
  attempt writes that key. The one other batch of the attempt that writes this key form, a re-upload's commit, puts the
  prior's chunks and cannot land once a compensation is sent. No staging batch writes `retire:`. A later session move
  does not erase the obligation (only the drain does), so it is worth reading again after a read that found the session
  moved.
- **Empty part: its slot gone, and a session read made after that slot read still the bytes the compensation
  required.** While the session holds those bytes, only this attempt's commit and compensation remove its slot, and no
  commit can land. A slot gone under a moved session proves nothing, because the reaper's teardown of a fenced session
  removes slots too (test file `:3989-4002`).
- **A non-empty part never settles on the slot alone.** A slot erased by damage looks the same as a released one, and
  the obligation is what tells them apart (D3).

**Never evidence:** a slot held at the bytes the round required, an obligation key found absent, the clock.

**Keeping the doubt.** `InDoubt { batch: Compensation }`. `may_still_commit` starts at the store's report and falls
only on a read that shows a precondition of the round gone for good, by the rules `Doubt` already applies to a commit
(`Doubt::seen_session`, `commit.rs:810-818`; `gone_in_doubt`, `:847-869`):

- **the session moved**, shown by a record at a higher epoch than the round required, `Open` or not (an epoch only
  rises);
- **the session was deleted,** which takes two reads: no session record, **and** a slot read made *after* that session
  read finds the slot absent. The terminal delete takes the session and its slots in one batch (`0016:674`), so this
  pair is its mark. A missing record with the slot still there is damage (`RecordError::SessionRecordMissing`, as
  `session_gone`, `:742-758`), a repair can put the record back, and the flag stays.

Damage settles nothing, so the flag stays: undecodable slot or session bytes, a missing session record with the slot
still there, and **other bytes at the obligation key**. The round requires that key absent (`commit.rs:1095-1097`), but
bytes there are not there for good: the drain deletes obligations whatever the session's state (`0016:2218-2222`), and
a repair removes damage, so a held round can still land while its session and slot match. A fault or damage ends the
reads with the doubt kept, as in `settle`. The re-read order is the base's (obligation, slot, session), plus the slot
read after a session read that found no record.

**The own-commit proof.** When the commit required the part key absent, the proof also needs that key to hold a record
(this attempt's or a rival's) at a read made **after the slot read that found the slot gone and before the session read
that finds `Open@E`**, so the new read sits between the lap's slot read and its last session read. A landed commit
leaves a record there, and only the retirement drain removes it, after the session leaves `Open` (`commit.rs:102-104`).
With `Open@E` read after it, no drain can have run in between, so a key found absent disproves the commit, and the answer
keeps the doubt. A read made before the slot read disproves nothing, since the commit may land in between (D2; #1029's
read-gap legs). Do decides whether the read is made in every lap or only when the slot read found the slot gone; either
way it changes the lap's read trace, which (K) allows. This check can only turn a `Committed` into an in-doubt answer, never the reverse. When the
commit required the prior's bytes, no read can tell, which is the documented limit in `Scope`.

## Alternatives considered

- **Send the compensation again while the slot and session still match** (attempt 7, and this bundle's iteration 1). Late
  staging rebuilds the slot after a landed compensation, and for an empty part nothing else blocks a second landing.
  Rejected: it is #1029's rejected resend in another batch.
- **Take an absent obligation as proof the compensation did not land.** The drain deletes a drained obligation whatever
  the session's state. Rejected.
- **Settle a non-empty compensation on its slot alone,** as for an empty part. It answers a damaged slot as a landed
  compensation, a false `Refused(OperationAborted)` (attempt 7's finding 3). Rejected.
- **Read the part key on the lap's first read for the own-commit fix.** A commit that lands between that read and the
  slot read would lose its `Committed`. Rejected (D2).
- **Leave the damage-only answers as they are.** No legal writer erases a slot, but it is a false success. Rejected by the
  human, 2026-11-01.

## Impact & compatibility

- **Behaviour:** a compensation answered unknown that the store shows landed now gets the route's normal answer instead
  of `InDoubt`. `Committed` is answered less often, only in the damage-only case. Neither change adds a write.
- **API:** no new public type should be needed. If Do adds a variant or field to `CommitError`, `InDoubt` or `Stop`, the
  reviewer should treat it as an API addition.
- **Records and formats:** unchanged. No dependency changes.
- **Risk:** the change sits on the C-1 boundary. A wrong "landed" answer promises an obligation that does not exist. That
  is why every proof above is one no other batch can produce, and why the demonstrated reds are required.

## Open questions

- **Confirm the no-resend rule for compensations.** This brief applies #1029's human-chosen rule to the compensation by
  the same argument. If the human prefers resending an empty part's compensation (whose second landing only deletes the
  attempt's own rebuilt slot), that would be a separate, argued change.
- **The two documented limits** (a re-upload, and an empty part's compensation, each with its slot erased) stay limits of
  the proof. Neither can happen without damage.

## Plan review (2026-11-10)

All three findings of `plan-advisory-plan-reviewer.md` were checked against #1029's head (`1d1a1d0`), held, and were
fixed in this brief:

- **Other bytes at the obligation key** no longer lower `may_still_commit` (*Keeping the doubt*), which matches (O). (O)'s
  held run now removes the planted bytes after the return and lands the held compensation.
- **The new part-key read** changes the lap trace that existing tests assert exactly (`laps_of`, test file
  `:4319-4323`; `read_gap`, `:4652-4656`). (K) now allows updating the trace vocabulary to add that read and nothing
  else, keeps every other assertion and every history, and requires coverage of the new read's gaps. *Design* places the
  read between the slot read and the last session read, which is what makes an absent key a real disproof.
- **A missing session record** lowers the flag only when a slot read made after it finds the slot absent, as
  `gone_in_doubt` does. (T) gains the two runs: erase only the session (flag kept, restore, land), and the terminal
  delete (flag false).

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a
draft PR MAY happen during the cycle (useful for CI feedback). The PR MUST NOT be
marked ready before sign-off accepts.

## Iteration 2 — carry-forward (from the previous attempt)
- Sign-off rationale: Close the implementation gaps; the design (brief) stands. - Fix the false success at the own-commit proof's part read (crates/core/src/multipart/commit.rs ~:1022-1027, `Some(_) => {}`): undecodable bytes at the part key must not fall through to `Committed`. Keep the doubt with `Stop::Record(err)` and leave `may_still_commit` unchanged. Add the adversary's reproducer as a regression (held first-upload commit; erase slot at gap 0; plant b"torn" at the part key, both at gap 0 and at proof gap 3): never `Committed`. - Kill the surviving C5 mutant at commit.rs:1025: add the (K) case held commit + slot erased at gap 0 + torn part bytes at proof gap 3 + session-read fault at gap 4 -> `InDoubt { batch: Commit, may_still_commit: true }` with the fault as cause; then repair slot, remove torn bytes, release the held batch and assert it lands. - Human decisions at sign-off: the no-resend availability/capacity tradeoff is accepted; the plan-review findings (obligation mismatch, trace updates, missing session) are already covered by the brief; the size backstop (118 KB) is overridden — do not split. - The T4 batch review ran in 17 s with an empty review-batch.md; it did not really review this patch. Make sure the next Check gets a real high-effort review of the final patch.
- Sign-off session carry-forward (captured live, before §9 flattened it):
  Close the implementation gaps; the design (brief) stands.
  - Fix the false success at the own-commit proof's part read (crates/core/src/multipart/commit.rs ~:1022-1027, `Some(_) => {}`): undecodable bytes at the part key must not fall through to `Committed`. Keep the doubt with `Stop::Record(err)` and leave `may_still_commit` unchanged. Add the adversary's reproducer as a regression (held first-upload commit; erase slot at gap 0; plant b"torn" at the part key, both at gap 0 and at proof gap 3): never `Committed`.
  - Kill the surviving C5 mutant at commit.rs:1025: add the (K) case held commit + slot erased at gap 0 + torn part bytes at proof gap 3 + session-read fault at gap 4 -> `InDoubt { batch: Commit, may_still_commit: true }` with the fault as cause; then repair slot, remove torn bytes, release the held batch and assert it lands.
  - Human decisions at sign-off: the no-resend availability/capacity tradeoff is accepted; the plan-review findings (obligation mismatch, trace updates, missing session) are already covered by the brief; the size backstop (118 KB) is overridden — do not split.
  - The T4 batch review ran in 17 s with an empty review-batch.md; it did not really review this patch. Make sure the next Check gets a real high-effort review of the final patch.
- Failing gate: C4 diff coverage: changed lines executed by the patch's tests (advisory) — diff coverage not measured — patch.diff does not apply on origin/main
- Failing gate: C5 surviving mutants on the bundle diff (cargo mutants --in-diff) (advisory) — 26 mutants tested in 4m: 1 missed, 15 caught, 7 unviable, 3 timeouts
- Full previous attempt preserved in `iteration-v2/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 3 — carry-forward (from the previous attempt)
- Sign-off rationale: Implementation is correct and was independently reviewed in depth; close two small gaps and record one deviation. The design (brief) stands. - Add the untested second settle lap for an empty part (commit.rs:1284; the mutant `obligation.is_none() && laps == 1` survives all 92 tests): empty part on the rival route, compensation Act::hold(), land_held(Committed) at gap 2 of at_compensation_gaps(&attempt, 0, 2, ...) (after lap 1's session read) -> Refused(OperationAborted) after 2 laps of reads. Add it to (S). - Rival record + erased slot (commit.rs:1032, `Ok(Some(_)) => doubt.cannot_land()`): a held, never-landing first-upload commit, a same-number rival commit and the slot erased at gap 0 answers Committed with this attempt's digest. Document it as a third known limit in the module doc (beside :155-157 and :240) and the living docs, stating that only damage (an erased slot key) reaches it and that it reads the same as a landed commit replaced by a rival re-upload. Narrow the proof instead only if that is small and keeps every existing (K) outcome. - Accepted deviation from (K): r_damage_under_a_held_commit_keeps_the_doubt's last run now answers InDoubt { Commit, may_still_commit: true } (torn part) instead of committed_as. It follows from the iteration-2 rule that torn bytes never count as Committed. Keep it. - The plan-review findings (obligation mismatch, trace updates, missing session) remain covered by the brief. - Size backstop (128 KB) is overridden again: do not split. - The C5 timeouts are caught mutants (they defeat the settle loop's termination), not survivors.
- Sign-off session carry-forward (captured live, before §9 flattened it):
  Implementation is correct and was independently reviewed in depth; close two small gaps and record one deviation. The design (brief) stands.
  - Add the untested second settle lap for an empty part (commit.rs:1284; the mutant `obligation.is_none() && laps == 1` survives all 92 tests): empty part on the rival route, compensation Act::hold(), land_held(Committed) at gap 2 of at_compensation_gaps(&attempt, 0, 2, ...) (after lap 1's session read) -> Refused(OperationAborted) after 2 laps of reads. Add it to (S).
  - Rival record + erased slot (commit.rs:1032, `Ok(Some(_)) => doubt.cannot_land()`): a held, never-landing first-upload commit, a same-number rival commit and the slot erased at gap 0 answers Committed with this attempt's digest. Document it as a third known limit in the module doc (beside :155-157 and :240) and the living docs, stating that only damage (an erased slot key) reaches it and that it reads the same as a landed commit replaced by a rival re-upload. Narrow the proof instead only if that is small and keeps every existing (K) outcome.
  - Accepted deviation from (K): r_damage_under_a_held_commit_keeps_the_doubt's last run now answers InDoubt { Commit, may_still_commit: true } (torn part) instead of committed_as. It follows from the iteration-2 rule that torn bytes never count as Committed. Keep it.
  - The plan-review findings (obligation mismatch, trace updates, missing session) remain covered by the brief.
  - Size backstop (128 KB) is overridden again: do not split.
  - The C5 timeouts are caught mutants (they defeat the settle loop's termination), not survivors.
- Failing gate: C4 diff coverage: changed lines executed by the patch's tests (advisory) — diff coverage not measured — patch.diff does not apply on origin/main
- Failing gate: C5 surviving mutants on the bundle diff (cargo mutants --in-diff) (advisory) — 24 mutants tested in 4m: 14 caught, 7 unviable, 3 timeouts
- Full previous attempt preserved in `iteration-v3/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).
