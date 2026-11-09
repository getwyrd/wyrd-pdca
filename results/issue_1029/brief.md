# core: part commit — a commit with an unknown outcome is settled or stays in doubt (657.2c)

> Third of the four slices of #927 (657.2). It builds on 657.2b's round (#1028). **Re-planned at
> iteration 3 (2026-11-09):** iteration 2 was rejected because its "did not land" proof could compensate a
> part that had landed (see *Re-plan decision* below). **Revised after the plan review (2026-11-09):** the
> retry after an unknown answer goes too, because the same history lets a resent commit land twice (see
> *Plan review* below). The protocol is now: **after a commit round is answered unknown, the attempt sends
> no further batch**, neither a second commit nor a compensation; it reads, and answers `Committed` or the
> in-doubt error. Citations were re-resolved on the stack base `pdca-integration/main-r9dd123c9ed70`
> (`1f6d6a4`, which folds in #1027 and #1028) on 2026-11-09.

- **Slug:** multipart-part-commit-settle-unknown
- **Track:** alpha
- **Kind:** enhancement
- **Defect:** After 657.2b a commit answered `CommitUnknownResult` returns the in-doubt error at once, with no
  read (`crates/core/src/multipart/commit.rs:1065`, `// deferred: #1029`), even when a re-read could show that
  its own commit landed. A client then sees an in-doubt answer for a part that committed. This slice answers
  `Committed` when a re-read proves this attempt's own commit landed, and otherwise keeps the doubt. After an
  unknown outcome it sends **nothing more**: no compensation and no second commit. The store holds no evidence
  that such a commit did **not** land that a late staging batch of the same attempt, or a rival writing the same
  bytes, cannot recreate. So a compensation could orphan a landed part, and a resent commit could land twice.
- **Success criterion:** Appended to `crates/core/tests/multipart_upload_part_commit.rs`. **Across every leg below
  in which a commit round was answered unknown, the attempt sends no further batch after that round (the wrapper
  records no second commit batch and no compensation batch from it), and the answer is never
  `Refused(OperationAborted)`.**
  **(B) Settling.**
  - A commit that lands but answers unknown (once per `may_still_commit` value): `Committed`, slot released exactly once.
  - A re-upload that did not land (`may_still_commit: false`): the re-read finds the **prior** part. Never `Committed`
    on it: the in-doubt error with `may_still_commit: false`, the slot still held, one commit batch. (This is the
    availability cost the *Plan review* section names.)
  - **A held batch** (`may_still_commit: true`, landed later): no `Committed` on a read that finds the slot held. Two
    runs: (i) released before the re-read: `Committed`; (ii) released after return: the in-doubt error keeping
    `may_still_commit: true`, then the held batch lands. In both runs there is one commit batch, it releases the slot
    once, and the attempt's own part is durable.
  - **A same-number rival**, this attempt's commit not applied, the commit having required the key **absent** (once per
    `may_still_commit` value): never `Committed`; the in-doubt error with `may_still_commit: false`; the slot still held.
    In the held run, the held batch released later answers `Conflict`.
  - **A changed prior settles nothing.** A held re-upload over a non-empty prior that loses to a rival: the in-doubt
    error keeping `may_still_commit: true`. A held re-upload while a repair moves the prior away and back, in real
    re-place batches or their exact shape: the in-doubt error keeping `may_still_commit: true`; the held batch, released
    once the prior is back, lands: own part durable, slot released once. A rival over an empty prior: the in-doubt error.
  - **The read-gap leg.** A non-empty commit is held. At **each** gap between the classifying reads the wrapper runs
    one of four histories. *Landed, then a rival:* `Committed`, slot released once. *Landed, then a rollback:* the
    in-doubt error. *Not landed, then a rival* (the commit required the key absent) and *not landed, then a rollback:*
    the in-doubt error, the slot still held, and the held batch, released later, answers `Conflict`.
  **(X) The cross-phase leg** (iteration 2's rejection; it must fail on iteration 2's patch). Fixed clock. `Reserve`,
  `Intent` and the quiesce `Renewal` are each scripted `[hold, pass]`; one chunk is staged; the commit applies but
  answers unknown (once per `may_still_commit` value). Before the commit's re-read, the three held batches are released
  in that order. The test asserts each answers `Committed`, and that the slot then holds exactly the bytes the commit
  required and the owned entry is back, so the history is the real one. The answer is the in-doubt error; no `retire:`
  obligation names the attempt's own chunks; the attempt's own part is durable.
  **(X2) The replay leg** (the plan review's finding; it must fail on iteration 2's patch). Fixed clock and `Open@E`. An
  empty part 1, P, commits first. A is a non-empty re-upload of part 1 staged with (X)'s `[hold, pass]` reserve, intent
  and renewal. B is a same-number **empty** attempt in the same millisecond and epoch, so the part B writes is
  byte-for-byte P. Two runs:
  - **(a) Applied, before the re-read** (once per `may_still_commit` value). A's commit applies but answers unknown. At
    the gap before A's re-read, A's three held staging batches are released in order, then B commits over A's part,
    installing a `retire:` obligation, keyed by B's attempt, that owes A's chunks. The test asserts each of those four
    batches answers `Committed`, and that the part key then holds P's exact bytes and A's slot the exact bytes A's commit
    required, so the history is the real one. A's answer is the in-doubt error.
  - **(b) Held, then released after return.** A's commit is scripted held (`may_still_commit: true`). After A returns,
    the test releases A's held staging batches in order, then commits B, then releases A's held commit.
  In both runs: the wrapper records **exactly one** commit batch from A; at most one of A's commit batches lands; and at
  the end no `part:` record names a chunk that a `retire:` obligation owes.
  **(C) Regressions.**
  - **C1.** With a fixed clock, an empty part 1 commits. An empty re-upload in the same millisecond and epoch answers
    unknown, **not applied** (once per flag). Its bytes equal the prior's: never `Committed` on those bytes; the in-doubt
    error, the slot still held. Same-millisecond empty rival, the commit having required the key absent, not applied:
    the in-doubt error. On return, every slot still held belongs to an attempt that is live or returned the in-doubt
    error (`0016:1779`).
  - **C2, a torn read.** A commit held and never applied. At **each** read gap the wrapper runs a real Abort and deletes
    the session's `sidx:` and `slot:` records. Never `Committed`; `NoSuchUpload` or the in-doubt error.
  - **C3.** A commit **lands**, answers unknown with `may_still_commit: false`, and a rival replaces the part before the
    re-read: `Committed`. **C3′:** then a third attempt reserves the freed index, so the slot key holds another
    `attempt_id`: still `Committed`, and that attempt's slot is byte-for-byte unchanged.
  - **One batch, then reads.** For each `(applied, may_still_commit)` pair: exactly one commit batch and no compensation;
    applied: `Committed`; not applied: the in-doubt error keeping the flag, the slot still held. This reworks 657.2b's
    `h4_a_commit_answered_unknown_is_in_doubt_with_no_further_batch` (stack base, test file `:2581`) and keeps its
    `no retry` assertion.
  - **A conflict, then a held round.** Round 1 answers a `Conflict` that no read explains (657.2b sends the same batch
    again), round 2 is held, then a read faults: the in-doubt error keeping `may_still_commit: true`, no third batch; the
    held round then lands once.
  - **Read faults keep the doubt.** A read fault under a held commit at **each** read of the re-read (the part read, the
    first session read, the slot read, the last session read, and the slot re-read behind a missing session): the
    in-doubt error, `may_still_commit: true`, the fault as its cause. In each, the held batch is then released and lands:
    own part, one release.
  - A slot rewritten under the attempt's own id during a held commit: `InDoubt(SlotLost)`.
  - A slot gone under a moved session: the in-doubt error, never `Committed`.
  **(F) The lease, delayed after dispatch.** A commit sent with the margin, held past the lease's expiry. (i) It lands
  before the re-read: `Committed`. (ii) It lands after return: the in-doubt error; the part ends durable. (iii) A
  test-local batch in the idle fence's shape (`0016:1859-1863`) lands first: the held batch answers `Conflict`, no
  `part:` exists, the answer is `NoSuchUpload` or in-doubt. (iv) The held batch lands, then that fence: it answers
  `Conflict`, the session stays `Open@E`.
  **(H) A held commit and a rollback, both right after the slot read.** If the commit lands first: the next slot read finds
  the slot gone under a moved session, which proves nothing about whose batch removed it: the in-doubt error, slot released
  once, own part durable. If the rollback lands first: the in-doubt error, the slot still held, and the held batch,
  released later, answers `Conflict`.
  **(R) Damage under a held commit keeps the doubt.** Undecodable part or slot bytes, an undecodable session, a record at
  the same or a lower epoch, `Open` or not, and a missing `mpu:` with any record at the slot key: `InDoubt` keeping
  `may_still_commit`. Repairing the record mid-round must not produce a compensation, and a repaired record lets the held
  batch land.
  **(E) The whole gate.** `cargo xtask ci` is green on the patched tree.
- **Falsifiability:** RED is reachable in-process on `cargo test -p wyrd-core`, and the test wrapper forces every leg:
  unknown and held outcomes are injected per batch kind (`Scripted::script`, `multipart_upload_part_commit.rs:468`), held
  batches land in FIFO order on the real redb store (`release_held`, `:501-513`), C1, (X) and (X2) pin a `ManualClock` so
  identical bytes are certain, C2, C3 and the read-gap histories run by hook at each gap, and (F) moves the clock. (X) goes
  RED on iteration 2's patch (the code review's history, confirmed at sign-off). (X2) goes RED on iteration 2's patch in
  both runs, by the plan review's history (not yet run; Do records it): its `Judged::Again` (v2 `commit.rs:1150`, `:1331`)
  sends the commit again, which lands, so the part names chunks B's obligation owes. (E) is the gating `C4-ci` check.
- **Invariant to restore:** **C-1** (`docs/principles.md` §5 C-1, §6 *storage lifecycle / reclamation*). `Committed` is
  answered only on evidence that **this attempt's own** commit landed, evidence that no other batch can produce: not another
  writer's record or teardown, and not a late batch of this attempt's own staging. No compensation is sent while the
  attempt's own commit may have landed or may still land, because a compensation orphan-marks chunks that a landed part
  references (`0016:1792`, X38 at `0016:2579`). **No second commit batch either:** a `WriteBatch` is not guaranteed
  idempotent (`crates/traits/src/lib.rs:254-257`), and after a commit lands, every precondition it required can be rebuilt
  (the slot and owned entries by a late staging batch, an empty prior's bytes by a same-number rival), so a resent batch
  can land a second time and publish chunks already owed to retirement. An expired lease is never evidence either way.
  **Sources:** the maintainer's rule 2026-07-25; 0016's refutation standard (`0016:2811-2813`, "Only availability,
  latency, capacity and operational costs are acceptable trade-offs here"); `0016:1625-1651`, `:1780`, `:1792`; the
  trait's late-landing contract (`crates/traits/src/lib.rs:240-247`) and its no-retry rule (`:254-257`). SELF-TEST: the
  evidence rule binds every writer of the keys the re-read reads (staging's reserve, intent and renewal batches, rivals,
  Abort, staged reconstruction), so guarding one function cannot satisfy it.
- **Repo + branch target:** getwyrd/wyrd @ main
- **Depends on:** 1028
- **Conflicts with:**
- **Ordering note:** `Depends on` 657.2b (#1028, open as PR #1051): this slice replaces the `// deferred: #1029` arms of its
  round. #1030 (657.2d) builds on this slice's doubt; its brief assumed this slice could settle a commit as not landed and
  resend the commit; it must be re-read against both decisions (no compensation and no second commit after an unknown
  answer) before it is built. Intake: this is a re-plan of an ITERATE_PLAN brief, not new intake; alpha is at 18 of 6,
  and the bundle is two splits deep, so no split was proposed.
- **Difficulty:** medium
- **Do model:** opus
- **Scope:** what a commit's `CommitUnknownResult` means: `Committed` on proof that the attempt's own commit landed, and
  otherwise the kept doubt; no further batch (no second commit, no compensation) after any unknown outcome; the living
  architecture doc kept current. / out of scope:
  - releasing the slot of a commit that provably did not land, **or sending its batch again**: both need a "did not
    land" proof, or a mark a landed commit leaves, that no late staging batch and no same-bytes rival can forge. That is
    a staging or record change (deferred, not filed: alpha intake is closed);
  - 657.2b's resend after a definite `Conflict` that no read explains: unchanged (a `Conflict` round never lands);
  - a **compensation's** unknown outcome: 657.2a's in-doubt stop stays until #1030;
  - the damage-only false `Committed` with the slot erased (#1030);
  - any change in `stage.rs`; proposal 0016 and the ADRs, including the wording at `0016:1727-1728`; `Cargo.lock` and
    `deny*.toml`; the gateway and any production caller; Complete, the drain, the reaper, DST.
- **Repro instruction:** n/a (new functionality). On the stack base a commit answered unknown returns the in-doubt error at
  once, with no read (`commit.rs:1065`). For (X): on iteration 2's patch the same history compensates and answers
  `Refused(OperationAborted)`. For (X2): on iteration 2's patch the attempt sends the commit again and it lands, so part 1
  names A's chunks while B's `retire:` obligation owes them.
- **External dependencies:** `typos`, `docs-renderer`
- **Test file:** `crates/core/tests/multipart_upload_part_commit.rs`, **appended**.
- **Verification posture:** C4-verify reports `PASS (green-only)`, a pre-declared sign-off item: the gate earns a red only
  from an **added** `tests/*.rs` file (`run-verify.sh --classify` on iteration 2's patch prints only `CRATE crates/core`), and
  this suite's wrapper lives in the existing file. C4-diff-cov will say "patch.diff does not apply on origin/main" until #1028
  merges (the row has `verifies_base = false`); that is expected, not a patch defect. The plan review's NEEDS-HUMAN finding
  was answered by dropping the resend after an unknown answer, an availability cost (*Plan review* below); the human chose
  this route at plan revision (2026-11-09), so it is not a sign-off item. **Demonstrated reds (required, recorded in `build-notes.md`).** Each temporary
  edit fails the named leg:
  - iteration 2's settle put back (a slot found at the bytes the commit required taken as proof it did not land, then a
    compensation) → (X), and the rival and read-gap not-landed legs;
  - the same batch sent again after an unknown round (iteration 2's `Judged::Again`) → (X2) in both runs, (B)'s re-upload
    leg, the one-batch leg;
  - "a part is there" taken as `Committed` without the own-commit proof → (B)'s re-upload leg;
  - part bytes alone taken as proof → C1;
  - the session's state taken from a read before the slot read, never re-checked → C2;
  - "my slot is gone" read as "the key is absent", ignoring `attempt_id` → C3′;
  - the rival-record branch checked before the own-commit proof → C3;
  - the own-commit proof removed, so an unknown commit is never `Committed` → (B)'s landed leg, C3, the one-batch leg;
  - a compensation sent while a commit is held → (H);
  - a non-empty prior treated as proof the commit cannot land → (B)'s changed-prior legs (the reported flag);
  - a lease past expiry treated as evidence (a compensation, or the flag dropped) → (F);
  - the doubt dropped on each damage arm → (R);
  - the doubt dropped at each read-fault site (`doubt.end` replaced by a plain stop) → the read-fault legs.
  **C5 (advisory) survivors** are listed in `build-notes.md` with the reason each one survives. **Review depth:** sign-off needs
  a confirmed high-effort review of the final patch (iteration 2's T4 batch review took 14.85 s on a 153 KB patch and could
  not be confirmed).
- **Production reach:** (a) The test runs against a real store; production commits no part in this slice. (b) The S3 verb
  lands with #669 after #1030. (c) This slice **must not** add a production caller.
- **Citations expected:** Do must cite `path:line` on the target branch for every change.
  - **Start from iteration 2.** Do MAY open `results/issue_1029/iteration-v2/patch.diff`. Keep the own-commit proof, the
    read order, the damage arms, the read-fault handling and `InDoubt`'s cause. **Remove its "did not land" settle:** a slot
    read that finds the slot held at the bytes the commit required no longer clears a round's doubt (its `slot_held`, v2
    `commit.rs:808`), and no path with a round in doubt reaches a compensation. **Remove its resend after an unknown
    answer:** `Judged::Again` for a round in doubt (v2 `commit.rs:1150`, `:1331`) goes, so once a round is answered unknown
    the loop sends nothing more. Its `Doubt` (v2 `commit.rs:768`) then holds at
    most one round, the last, since only 657.2b's definite `Conflict` rounds come before it. Rework the tests that asserted
    a compensation after an unknown outcome (`b_a_rival_after_an_unapplied_unknown_commit_is_compensated`, the not-landed
    runs of `b_read_gap_a_precondition_that_dies_between_the_reads_settles_only_at_a_later_slot_read`,
    `c_every_commit_round_out_of_flight_compensates_and_reports_the_release`, C1's rival run, H's unlanded run) or a
    second commit batch (`b_an_unlanded_reupload_is_not_committed_on_the_prior_part`,
    `b_a_held_commit_is_never_compensated_and_at_most_one_lands`,
    `b_a_held_reupload_survives_a_repair_that_moves_the_prior_away_and_back`, `c1_an_empty_part_is_never_committed_on_its_bytes`,
    `c_the_kept_doubt_is_the_round_that_may_still_land`); drop `c_a_later_round_failing_definitely_keeps_the_held_rounds_doubt`
    and `f_a_short_lease_with_a_commit_in_doubt_keeps_the_doubt`, which need a batch after an unknown one; put back the
    stack base's `h4_a_commit_answered_unknown_is_in_doubt_with_no_further_batch` as the one-batch leg; keep the rest.
  - **Peer for "no further batch":** the stack base's own rule, `commit.rs:1017` ("stop in doubt on an unknown outcome,
    sending nothing more") and its test, `crates/core/tests/multipart_upload_part_commit.rs:2577-2603`. This slice adds the
    read before that stop; it keeps the stop.
  - **Why a held slot proves nothing** (the history (X) builds): every reserve round puts the same bytes
    (`crates/core/src/multipart/stage.rs:1296-1298`); a held reserve round pins its index, so the next round targets the same
    slot (`stage.rs:1347-1349`); retry rounds at one reading stamp the same value (`stage.rs:1567-1570`); the reserve
    requires only the slot key absent (`stage.rs:1329-1334`), and the intent and renewal require the exact bytes before them
    (`stage.rs:2325-2336`, `:1620-1630`); the commit deletes the slot (`commit.rs:791`), so a late reserve can land again; a
    batch may land after its error returned (`crates/traits/src/lib.rs:240-247`).
  - **Why the same batch again can land twice** (the history (X2) builds): the commit requires only the session, the part
    key and the slot (`commit.rs:775-781`); a `PartRecord` holds no attempt id (`crates/core/src/multipart.rs:3134-3140`), so
    two empty parts in one millisecond and epoch are the same bytes; an empty prior gives the commit no `retire:` obligation
    and so no absent-key guard (`commit.rs:1050-1052`, `obligation_for` at `:752-753`, the guard at `:782-784`); a rival's
    obligation is keyed by the rival's attempt (`multipart.rs:4210`, `:4237`).
  - **Peer for the doubt:** Abort, `crates/core/src/multipart/abort.rs:261-273`, and its held-batch test,
    `crates/core/tests/multipart_abort.rs:961-984`.
  - **The reconstruction that can restore a prior:** `crates/custodian/src/reconstruction/staged.rs`, `0016:671`.
  - **Docs currency:** `docs/design/architecture/05-building-block-view.md:250` ("settling it is still to land") and the
    `commit.rs:285` doc comment must say what settling now does, including that no batch follows an unknown outcome.
- **Prior-art check (triage cycles):** by path on `origin/main` @ `d04e507`, 2026-11-09. `crates/core/src/multipart/commit.rs` and
  the test file: one commit, `5e8aa24` (#1027, PR #1041, merged). #1028 is open as PR #1051. Staging (`stage.rs`) came from PRs
  #942 and #965 (merged); its same-bytes rounds are the source of iteration 2's rejection. No closed-unmerged PR touches these
  paths (iteration 2's review enumerated all 445 closed PRs; a title search on 2026-11-09 found none new). #927's attempts 1 to
  7 and this bundle's iterations 1 and 2 are the design history. Do not take from #927's v7 brief that a changed non-empty prior
  "never comes back" (false, corrected at its iteration 4), nor from iteration 2 that a held slot proves "did not land" or that
  "the same batch again" can land at most once.
- **Disposition hint:** new-feature

## Re-plan decision (iteration 3, 2026-11-09)

**Chosen: never compensate after an unknown commit outcome (option B).** The human rejected iteration 2 for a C-1 path the
code review found and the human confirmed against the base. The commit applies but answers unknown. A late reserve, intent and
renewal of the same attempt then land in order and rebuild exactly the slot bytes the commit required. The re-read reads "slot
still held at my bytes" as "did not land", and the attempt compensates a part that exists. The `slot:` key's history can repeat
a value after the release, so its bytes cannot prove a commit did not land. The own-commit proof is unaffected: while the session is `Open@E`, only the commit
and the compensation delete a slot (`commit.rs:791`, `:814`), and no staging batch deletes one.

Iteration 1's carry-forward still binds: the store-fault legs in (C) are the tests it asked for. The "H against read-gap"
conflict from iteration 2's review is resolved; under this decision both answer the in-doubt error for an unlanded commit.

## Plan review (2026-11-09): no second commit either

**The re-plan kept iteration 2's resend ("the same batch again, requiring the same slot bytes, so at most one lands"), and
that claim is false.** The plan review built the history, and it holds on the stack base. Once A's commit lands, a late
reserve, intent and renewal rebuild A's slot, and a same-number empty rival B rewrites the empty prior's exact bytes while
installing a `retire:` obligation for A's chunks. Every precondition of A's commit then holds again, and A's resent batch,
or its held first batch, lands a second time and publishes chunks B's obligation owes. A's empty prior gave A's commit no
absent-key guard to stop it. The same reason that rules out the compensation rules out the resend: nothing the re-read can
see tells "did not land" from "landed, then rebuilt".

**Decision: no batch of any kind after an unknown outcome.** This narrows retry eligibility rather than reopening the
staging or record change the re-plan rejected. It matches the stack base's own rule (`commit.rs:1017`, H4) and the trait's
(`lib.rs:254-257`). 657.2b's resend after a definite `Conflict` stays, because a `Conflict` round never lands, so the
attempt still has at most one batch that can land.

**The cost is availability.** A commit answered unknown that did not land, or that landed and was then replaced while late
staging rebuilt its slot, answers the in-doubt error instead of `Committed`. The client sends the part again as a new attempt,
and the old slot stays held until the session ends. 0016's refutation standard allows that (`0016:2811-2813`), and the human
accepted more in-doubt answers at sign-off. **The human chose this route (option A, over the staging or record change) at plan revision, 2026-11-09.**

## Design

**Settle identity, not presence** (`0016:1630-1651`). `PartRecord` carries no attempt id and an empty part carries no chunk ids,
so the part's bytes never identify the attempt (C1). The commit batch deletes the attempt's own slot. While the session is
`Open@E`, the writers of slot index `k` are this attempt and, once it released the index, whichever later attempt reserves it;
no reaper touches an `Open` session (`0016:1625-1628`).

**The own-commit proof, checked first.** A commit of this attempt is in doubt, no compensation was sent, the slot is gone
(absent, or holding another `attempt_id`), and a session read **after** that slot read still found the `Open@E` bytes the commit
required. That answers `Committed`, whatever the part key holds (C3, C3′). A slot found gone while the session has moved proves
nothing about whose batch removed it, so with a commit in doubt that is `InDoubt`.

**Until proven, keep the doubt, and send nothing.** Once a round is answered unknown, the attempt reads and does not write. If
the reads do not give the own-commit proof, it returns `CommitError::InDoubt`, as Abort does. A held slot, an unchanged
session and an unchanged part key are not grounds to send the batch again: each can be rebuilt after the commit landed (*Plan
review*). `may_still_commit` reports whether the round may still land. A round cannot land later when its outcome was
`may_still_commit: false`, a read found the session moved (an epoch only rises), or the commit required the part key absent and
it now holds a record. **A changed prior settles nothing:** an empty rival writes an empty prior's bytes again, and staged
reconstruction can move a part's placement and back while the session is `Open@E`. **Damage settles nothing:** a round that
may still land keeps `may_still_commit` however the damage is later repaired. A missing `mpu:` with any record at the slot key
is damage, not a terminal delete.

**No compensation after an unknown outcome.** "Cannot land later" is not "did not land": the commit may have landed already. The
attempt therefore never compensates once a round was answered unknown, and its slot stays held until the session ends. That
costs capacity, which 0016's refutation standard allows (`0016:2811-2813`).

**Decided at sign-off, keep:** the dispatch-only lease promise; `Refused(OperationAborted)` for a definite loss under 657.2b;
slots left held after a rollback; more in-doubt answers are an allowed availability cost.

## Alternatives considered

- **Compensate on an unknown outcome.** It orphan-marks a part that exists (`0016:1792`). Rejected.
- **Take a slot still held at the commit's bytes as proof it did not land** (iteration 2). A late reserve, intent and renewal
  rebuild those bytes after the commit deleted them. Rejected at re-plan.
- **Send the same batch again while the slot, session and part key still match** (iteration 2 and this brief before its plan
  review). A landed commit's preconditions can all be rebuilt, so the resend can land twice (X2). Rejected at plan review.
- **Resend only over a non-empty prior,** whose `retire:` key, keyed by this attempt, would block a second landing. Its safety
  would rest on that key surviving until the session ends, which the retirement path decides, not this slice, and it saves
  in-doubt answers only for re-uploads. Rejected at plan review.
- **Make staging never repeat a slot value**, which keeps iteration 2's settle sound. It is a `stage.rs` behaviour change, may
  need a slot record shape change in proposal 0016, and grows a 153 KB patch. Rejected at re-plan; it is the route to releasing
  the slot later.
- **Staging reports whether any of its batches may still land,** and the slot proof is used only when none can. It changes the
  stage API and must also cover abandoned renewals. Rejected at re-plan.
- **An attempt id in `PartRecord`.** It changes a landed record shape. Rejected.

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a
draft PR MAY happen during the cycle (useful for CI feedback). The PR MUST NOT be
marked ready before sign-off accepts.

## Iteration 3 — carry-forward (from the previous attempt)
- Sign-off rationale: Fix R1 only; size backstop overridden by the human (the bulk is the required test suite, the slice is not two outcomes — do not split). Fold each settle read into Doubt as it returns (or return the partial reads with the error), so a later read fault keeps what earlier reads proved: `may_still_commit` must go false once a moved session or a required-absent rival has been seen, still answering InDoubt with the read fault as its cause and sending no further batch. Add regression legs next to `c_a_read_fault_at_each_read_under_a_held_commit_keeps_the_doubt` (test file :5147): (1) held commit, session moved to a higher epoch, first session read sees it, then the slot read faults; (2) held commit requiring the part absent, rival part lands, part read sees it, then the first session read faults. Each must answer may_still_commit: false, and the held batch released afterwards answers Conflict. Pin `commit.rs:922` (the first-session-read fold): removing it must turn a test red. Keep everything else as is. The plan-review retry finding is already resolved by the brief (no resend after an unknown answer, covered by X2).
- Sign-off session carry-forward (captured live, before §9 flattened it):
  Fix R1 only; size backstop overridden by the human (the bulk is the required test suite, the slice is not two outcomes — do not split).
  Fold each settle read into Doubt as it returns (or return the partial reads with the error), so a later read fault keeps what earlier reads proved: `may_still_commit` must go false once a moved session or a required-absent rival has been seen, still answering InDoubt with the read fault as its cause and sending no further batch.
  Add regression legs next to `c_a_read_fault_at_each_read_under_a_held_commit_keeps_the_doubt` (test file :5147): (1) held commit, session moved to a higher epoch, first session read sees it, then the slot read faults; (2) held commit requiring the part absent, rival part lands, part read sees it, then the first session read faults. Each must answer may_still_commit: false, and the held batch released afterwards answers Conflict.
  Pin `commit.rs:922` (the first-session-read fold): removing it must turn a test red.
  Keep everything else as is. The plan-review retry finding is already resolved by the brief (no resend after an unknown answer, covered by X2).
- Failing gate: C4 diff coverage: changed lines executed by the patch's tests (advisory) — diff coverage not measured — patch.diff does not apply on origin/main
- Failing gate: C5 surviving mutants on the bundle diff (cargo mutants --in-diff) (advisory) — 31 mutants tested in 3m: 16 caught, 12 unviable, 3 timeouts
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 3 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_1029/review-
- Full previous attempt preserved in `iteration-v3/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).
