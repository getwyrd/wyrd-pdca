# core: part commit — a commit with an unknown outcome is settled or stays in doubt (657.2c)

> Third of the four slices of #927 (657.2). It builds on 657.2b's round. Read 0016's commit classification
> on unknown outcomes (`0016:1630-1651`) and its failure rows (`0016:1780`, `:1792`). Citations were verified
> at `376b106` on 2026-11-01; re-resolve them on your base, which folds in 657.2a and 657.2b.

- **Slug:** multipart-part-commit-settle-unknown
- **Track:** alpha
- **Kind:** enhancement
- **Defect:** After 657.2b a commit answered `CommitUnknownResult` returns the in-doubt error at once, even
  when a re-read could show that its own commit landed, or that it did not and never can. A client then
  sees an in-doubt answer for a part that committed, and a slot stays held. This slice settles the outcome
  from re-reads where the evidence allows it, and otherwise keeps the doubt.
- **Success criterion:** Appended to `crates/core/tests/multipart_upload_part_commit.rs`.
  **(B) Settling.**
  - A commit that lands but answers unknown: `Committed`, slot released exactly once.
  - A re-upload that did not land (`may_still_commit: false`): the re-read finds the **prior** part. Never
    `Committed` for it; it ends with its own part committed.
  - **A held batch** (`may_still_commit: true`, landed later): no `Committed` and no compensation on that read
    alone. Every later batch requires its own slot at the bytes the held batch requires, so at most one
    lands. Three runs: (i) released before the next batch; (ii) released after the attempt has returned; (iii)
    every later batch `Conflict` unapplied until the rounds run out, then released. In every run exactly one of
    {its own part, its compensation} is durable at the end, the slot is released once, and `Committed` is
    answered only if its own part landed. (iii) returns the in-doubt error keeping the flag.
  - A same-number rival with this attempt's commit not applied, where the commit required the key **absent**
    (run once per `may_still_commit` value): a slot read made **after** the read that found the rival finds the
    slot still held. Never `Committed`; it compensates and answers `Refused(OperationAborted)`.
  - **A changed prior settles nothing** (decided at sign-off, iteration 4). A held re-upload over a non-empty
    prior that loses to a rival answers the in-doubt error with no compensation. A held re-upload while a repair
    moves the prior away and back, in real re-place batches or their exact shape: no compensation while the
    commit may land, never a false `Refused(OperationAborted)`, slot released once. A rival over an empty prior
    settles nothing, because those bytes can come back.
  - **The read-gap leg.** A non-empty commit is held. At **each** gap between the classifying reads the wrapper
    runs one of four histories. *Landed, then a rival:* no compensation, `Committed`, slot released once.
    *Landed, then a rollback:* no compensation, the in-doubt error, never `Refused(OperationAborted)`. *Not
    landed, a rival* (the commit required the key absent) and *not landed, a rollback:* the attempt reaches a
    slot read made after it saw the change, then compensates and answers `Refused(OperationAborted)`; the held
    batch, released later, answers `Conflict`.
  **(C) Regressions.**
  - **C1.** With a fixed clock, an empty part 1 commits. An empty re-upload in the same millisecond and epoch
    answers unknown, **not applied** (once per flag). Its bytes equal the prior's, and the answer must not be
    `Committed` on them. On return the slot is released or covered by the reported doubt, and the held-slot count
    equals the live attempts (`0016:1779`). Same for an empty rival in the same millisecond.
  - **C2, a torn read.** A commit held and never applied. At **each** read gap the wrapper runs a real Abort and
    deletes the session's `sidx:` and `slot:` records. Never `Committed`; `NoSuchUpload` or the in-doubt error.
  - **C3.** A commit **lands**, answers unknown with `may_still_commit: false`, and a rival replaces the part
    before the re-read: `Committed`. **C3′:** then a third attempt reserves the freed index, so the slot key holds
    another `attempt_id`: still `Committed`, and that attempt's slot is byte-for-byte unchanged.
  - Every commit round answered unknown with `may_still_commit: false`, not applied: it compensates and returns
    `Stopped(CommitRoundsSpent)` reporting the release.
  - **The kept doubt is the round that may still land:** round 1 held, round 2 unknown with `false`, then a read
    faults. The answer keeps `may_still_commit: true`.
  - A slot rewritten under the attempt's own id during a held commit: `InDoubt(SlotLost)`.
  - A slot gone under a moved session: the in-doubt error, never `Committed`, never a compensation.
  **(F) The lease.** A commit in doubt when the lease runs short: no further commit or compensation, the in-doubt
  error. **Delayed after dispatch:** a commit sent with the margin, held past the lease's expiry. (i) It lands before
  the re-read: `Committed`. (ii) It lands after return: the in-doubt error; the part ends durable. (iii) A test-local
  batch in the idle fence's shape (`0016:1859-1863`) lands first: the held batch answers `Conflict`, no `part:`
  exists, the answer is `NoSuchUpload` or in-doubt. (iv) The held batch lands, then that fence: it answers
  `Conflict`, the session stays `Open@E`. No compensation in any of them.
  **(H) A held commit that lands, then a rollback,** both right after the slot read. The next slot read finds the
  slot gone under a moved session, which proves nothing about whose batch removed it: the in-doubt error, no
  compensation, the slot released once, and the attempt's own part durable. This is the read-gap leg's *landed, then a
  rollback* at one named gap. **If the commit has not landed** when the rollback does, that is *not landed, a rollback*,
  and the attempt compensates and answers `Refused(OperationAborted)`.
  **(R) Damage under a held commit keeps the doubt.** Undecodable part or slot bytes, an undecodable session, a
  record at the same or a lower epoch, `Open` or not, and a missing `mpu:` with any record at the slot key: `InDoubt` keeping
  `may_still_commit`. Repairing the record mid-round must not produce `Refused(OperationAborted)` or a compensation,
  and a repaired record lets the held batch land.
  **(E) The whole gate.** `cargo xtask ci` is green on the patched tree.
- **Falsifiability:** RED is reachable in-process on `cargo test -p wyrd-core`, and the wrapper forces every leg.
  Unknown and held outcomes are injected; C1 pins the clock, so identical bytes are certain; C2, C3 and the read-gap
  histories run by hook at each gap; (F) moves a `ManualClock`. (E) is the gating `C4-ci` check.
- **Invariant to restore:** **C-1** (`docs/principles.md` §5 C-1, §6 *storage lifecycle / reclamation*). `Committed`
  is answered only on evidence that **this attempt's own** commit landed, evidence no other writer's record or teardown
  can produce. No compensation is sent while the attempt's own commit may have landed or may still land. An expired lease
  is never evidence either way. **Sources:** the maintainer's rule 2026-07-25; 0016's refutation standard (`0016:2809-2813`,
  "only availability, latency, capacity and operational costs are acceptable trade-offs"); `0016:1630-1651`, `:1780`, `:1792`.
  SELF-TEST: it binds every commit round's re-read and the decision to compensate, so guarding one function cannot satisfy it.
- **Repo + branch target:** getwyrd/wyrd @ main
- **Depends on:** 1028
- **Conflicts with:**
- **Ordering note:** `Depends on` 657.2b: an unknown commit settled as not landed enters its classification. 657.2d builds on
  this slice's doubt. Intake: created by the split of #927 under the human's per-id cap override of 2026-11-01.
- **Difficulty:** medium
- **Do model:** opus
- **Scope:** what a commit's `CommitUnknownResult` means: the own-commit proof, settling, the kept doubt, and retrying
  the same batch. / out of scope:
  - a **compensation's** unknown outcome: 657.2a's in-doubt stop stays until 657.2d;
  - the damage-only false `Committed` with the slot erased (657.2d);
  - any behaviour change in `stage.rs`; proposal 0016 and the ADRs, including the wording at `0016:1727-1728`; `Cargo.lock`
    and `deny*.toml`; the gateway and any production caller; Complete, the drain, the reaper, DST.
- **Repro instruction:** n/a (new functionality). On 657.2b's base a commit answered unknown returns the in-doubt error at
  once, with no read.
- **External dependencies:** `typos`, `docs-renderer`
- **Test file:** `crates/core/tests/multipart_upload_part_commit.rs`, **appended**.
- **Verification posture:** C4-verify reports `PASS (green-only)`, a pre-declared sign-off item. **Demonstrated reds
  (required, recorded in `build-notes.md`).** Each temporary edit fails the named leg:
  - "a part is there" taken as `Committed` without checking it is its own → (B)'s re-upload leg;
  - part bytes alone taken as proof → C1;
  - the session's state taken from a read before the slot read, never re-checked → C2;
  - the possibly-landed commit dropped from the doubt before the search → C3;
  - "my slot is gone" read as "the key is absent", ignoring `attempt_id` → C3′;
  - the rival-record branch checked before the own-commit proof → C3;
  - a compensation sent while a commit is held → (H);
  - a non-empty prior treated as settling → (B)'s changed-prior legs;
  - a rival or moved session seen **after** the last slot read counted as settling → the read-gap leg, landed runs;
  - settling never allowed, so the attempt only returns in-doubt → the read-gap leg, not-landed runs;
  - the doubt search replaced with `.last()` → the kept-doubt leg;
  - a lease past expiry taken as proof that a held commit did not land → (F)'s delayed leg;
  - the doubt dropped on each damage arm → (R).
  **C5 (advisory) survivors** are listed in `build-notes.md`. **Review depth:** sign-off needs a confirmed high-effort review
  of the final patch; the high-effort passes caught what default-effort passes missed (iteration 4: 3 of 3 against 0 of 3).
- **Production reach:** (a) The test runs against a real store; production commits no part in this slice. (b) The S3 verb
  lands with #669 after 657.2d. (c) This slice **must not** add a production caller.
- **Citations expected:** Do must cite `path:line` on the target branch for every change.
  - **Peer for the doubt:** #895's Abort, `crates/core/src/multipart/abort.rs:264-282`, and its held-batch test,
    `crates/core/tests/multipart_abort.rs:961-980`. **Peer for the read order:** `stage.rs:1713-1732`, which reads the slot again
    after seeing a session move, before letting a doubt go.
  - **The reconstruction that can restore a prior:** `crates/custodian/src/reconstruction/staged.rs`, `0016:671`.
  - **Start from attempt 7.** Do MAY open `results/issue_927/iteration-v7/patch.diff` as a reference; its `Doubt` type and module
    doc state this design, and no history in its last review broke the commit's settle order. Its tests for this slice:
    `b_a_landed_commit_answered_unknown_is_committed`, `b_an_unlanded_reupload_is_not_committed_on_the_prior_part`,
    `b_a_held_commit_is_never_compensated_and_at_most_one_lands`, `b_a_rival_after_an_unapplied_unknown_commit_is_compensated`,
    `c1_an_empty_part_is_never_committed_on_its_bytes`, `c2_a_torn_read_never_answers_committed`, `c3_a_landed_commit_then_a_rival_is_committed`,
    `b_read_gap_a_precondition_that_dies_between_the_reads_settles_only_at_a_later_slot_read`, `f_a_short_lease_with_a_commit_in_doubt_keeps_the_doubt`,
    `f_a_commit_delayed_past_its_lease_is_settled_by_reads_never_by_the_clock`, `h_a_commit_held_while_a_rollback_lands_is_in_doubt`,
    `b_a_damaged_part_record_under_a_held_commit_keeps_the_doubt`, `b_a_rival_over_an_empty_prior_settles_nothing_because_those_bytes_can_come_back`,
    `b_the_kept_doubt_is_the_round_that_may_still_land`, `g_a_slot_gone_under_a_moved_session_at_the_commits_re_read`,
    `b_a_damaged_session_under_a_held_commit_keeps_the_doubt`, `b_a_session_damaged_and_repaired_mid_round_settles_nothing`,
    `g_every_commit_round_out_of_flight_compensates_and_reports_the_release`, `b_a_slot_rewritten_under_the_attempts_own_id_during_a_held_batch_is_slot_lost`,
    `b_a_held_reupload_over_a_non_empty_prior_that_loses_to_a_rival_keeps_the_doubt`, `b_a_held_reupload_survives_a_repair_that_moves_the_prior_away_and_back`,
    the commit half of `held_damage_at_a_re_read_keeps_the_doubt`, and the held-commit legs of `g_a_missing_session_record_is_a_terminal_delete_only_with_the_slot_gone`.
- **Prior-art check (triage cycles):** as 657.2a, on `origin/main` @ `376b106`, 2026-11-01. #927's attempts 1 to 7 are the source. The
  v7 brief's rule that a changed non-empty prior "never comes back" is **false** and was corrected at iteration 4; do not take it from there.
- **Disposition hint:** new-feature

## Design

**Settle identity, not presence** (`0016:1630-1651`). `PartRecord` carries no attempt id and an empty part carries no chunk ids, so the
part's bytes never identify the attempt (C1). The commit batch deletes the attempt's own slot. While the session is `Open@E`, the writers
of slot index `k` are this attempt and, once it released the index, whichever later attempt reserves it; no reaper touches an `Open`
session (`0016:1625-1628`).

**The own-commit proof, checked first.** A commit of this attempt is in doubt, no compensation was sent, the slot is gone (absent, or
holding another `attempt_id`), and a session read **after** that slot read still found the `Open@E` bytes the commit required. That
answers `Committed`, whatever the part key holds (C3, C3′).

**Settled as not landed.** The attempt compensates only once **every** commit round it sent has both facts:

- *Unable to land later:* its outcome was `may_still_commit: false`; or a read found the session moved (an epoch only rises); or the commit
  required the key **absent** and it now holds a record (a `part:` record is removed only after the session leaves `Open`). **A changed prior
  settles nothing, empty or not:** an empty rival writes an empty prior's bytes again, and staged reconstruction can move a part's placement
  and back while the session is `Open@E` (iteration 4).
- *Did not land:* a slot read made **after** that error returned, or after that precondition read, found the slot still at the bytes the attempt
  last wrote.

A slot read made **before** the read that found the change proves nothing: a held commit can land between them. A change first seen after the
last slot read sends the attempt back to read the slot again, with no commit sent if none could land. The two proofs need opposite orders, which
is why one round reads the part and the session, then the slot, then the session again. A slot found gone while the session has moved proves
nothing about whose batch removed it, so with a commit in doubt that is `InDoubt`.

**Until settled**, the attempt re-reads and retries **the same batch**, requiring the same slot bytes, so at most one lands. It never compensates.
When the rounds run out or the lease runs short, it returns `CommitError::InDoubt` with `may_still_commit`, as Abort does. **Damage settles nothing:**
a round that may still land keeps `may_still_commit` however the damage is later repaired. A missing `mpu:` with the slot still held is damage, not a
terminal delete (iteration 6). The same holds for another attempt's record there (657.2b's rule).

**Decided at sign-off, keep:** the dispatch-only lease promise; `Refused(OperationAborted)`; slots left held after a rollback; more in-doubt answers
are an allowed availability cost.

## Alternatives considered

- **Compensate on an unknown outcome.** It orphan-marks a part that exists (`0016:1792`). Rejected.
- **An attempt id in `PartRecord`.** It changes a landed record shape, and the slot already identifies the attempt. Rejected.
- **A changed non-empty prior as proof the commit cannot land.** False under staged reconstruction. Rejected at iteration 4.

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a
draft PR MAY happen during the cycle (useful for CI feedback). The PR MUST NOT be
marked ready before sign-off accepts.

Plan-review response (2026-11-01, revision pass): the finding holds: as written, H and the read-gap leg demanded opposite answers for one history. H now names the commit landing first, as attempt 7's `h_a_commit_held_while_a_rollback_lands_is_in_doubt` does, and the unlanded history is left to the read-gap leg. (R) also carries #1028's foreign-slot rule, so the two briefs agree.

## Iteration 1 — carry-forward (from the previous attempt)
- Sign-off rationale: Close the test gap found by the adversary review; the production logic is judged correct, so do not change it. - Add tests pinning the doubt-keeping store-fault sites in crates/core/src/multipart/commit.rs: :1325 (a definite failure of a later round), :1197 (first session read), :1202 (slot read), :1211 (last session read), :741 (session_gone's slot re-read). Today replacing `doubt.end(upload_id, Stop::Store(fault))` with `stopped(...)` at any of them leaves all 67 tests green. - Start with the concrete leg: round 1 Act::hold(), round 2 Act::fail() -> InDoubt { may_still_commit: true, cause: Store }, no compensation; then a read fault under a held round at each of the other four positions, asserting InDoubt, may_still_commit: true and no compensation. - Size backstop (146 KB > 100 KB) is overridden by the human: the slice is one outcome and its size is test volume, so do NOT split; keep the patch otherwise as is.
- Sign-off session carry-forward (captured live, before §9 flattened it):
  Close the test gap found by the adversary review; the production logic is judged correct, so do not change it.
  - Add tests pinning the doubt-keeping store-fault sites in crates/core/src/multipart/commit.rs: :1325 (a definite failure of a later round), :1197 (first session read), :1202 (slot read), :1211 (last session read), :741 (session_gone's slot re-read). Today replacing `doubt.end(upload_id, Stop::Store(fault))` with `stopped(...)` at any of them leaves all 67 tests green.
  - Start with the concrete leg: round 1 Act::hold(), round 2 Act::fail() -> InDoubt { may_still_commit: true, cause: Store }, no compensation; then a read fault under a held round at each of the other four positions, asserting InDoubt, may_still_commit: true and no compensation.
  - Size backstop (146 KB > 100 KB) is overridden by the human: the slice is one outcome and its size is test volume, so do NOT split; keep the patch otherwise as is.
- Failing gate: C4 diff coverage: changed lines executed by the patch's tests (advisory) — diff coverage not measured — patch.diff does not apply on origin/main
- Failing gate: C5 surviving mutants on the bundle diff (cargo mutants --in-diff) (advisory) — 37 mutants tested in 3m: 1 missed, 26 caught, 10 unviable
- Full previous attempt preserved in `iteration-v1/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 2 — carry-forward (from the previous attempt)
- Sign-off rationale: Rejected for the code-review P1 (§6 item 4), confirmed against the base: the new settle can compensate a part that landed. A held Reserve stays outstanding after its retry is adopted (stage.rs:1345-1375; TiKV may apply it later, traits/src/lib.rs:244-247). Once the commit deletes the slot, its require_absent is true again, so it can recreate the slot, and a delayed Renewal with an identical retry stamp (stage.rs:1567) can restore the exact bytes the commit required. Settle then reads "slot still held at my bytes" as "did not land", compensates, and leaves a durable part with a retirement obligation for its own chunks. That is a C-1 data-loss path that this diff introduces; the old code stopped in doubt. Re-plan must decide the protocol: either staging guarantees that no earlier staging batch can restore the slot/entry evidence (a scope change, since the brief excludes stage.rs), or commit settlement stops treating unchanged slot bytes as proof that its own commit did not land. Add the cross-phase regression (Reserve, Intent and the quiesce Renewal each [hold, pass], commit applies but answers unknown, release the held batches before the re-read). Also: drop the stale "H vs read-gap" item (the brief already resolves it); diff coverage failed only because #1028 is unmerged; reconsider the split (153 KB) while in Plan.
- Sign-off session carry-forward (captured live, before §9 flattened it):
  Rejected for the code-review P1 (§6 item 4), confirmed against the base: the new settle can compensate a part that landed. A held Reserve stays outstanding after its retry is adopted (stage.rs:1345-1375; TiKV may apply it later, traits/src/lib.rs:244-247). Once the commit deletes the slot, its require_absent is true again, so it can recreate the slot, and a delayed Renewal with an identical retry stamp (stage.rs:1567) can restore the exact bytes the commit required. Settle then reads "slot still held at my bytes" as "did not land", compensates, and leaves a durable part with a retirement obligation for its own chunks. That is a C-1 data-loss path that this diff introduces; the old code stopped in doubt.
  Re-plan must decide the protocol: either staging guarantees that no earlier staging batch can restore the slot/entry evidence (a scope change, since the brief excludes stage.rs), or commit settlement stops treating unchanged slot bytes as proof that its own commit did not land. Add the cross-phase regression (Reserve, Intent and the quiesce Renewal each [hold, pass], commit applies but answers unknown, release the held batches before the re-read).
  Also: drop the stale "H vs read-gap" item (the brief already resolves it); diff coverage failed only because #1028 is unmerged; reconsider the split (153 KB) while in Plan.
- Failing gate: C4 diff coverage: changed lines executed by the patch's tests (advisory) — diff coverage not measured — patch.diff does not apply on origin/main
- Failing gate: C5 surviving mutants on the bundle diff (cargo mutants --in-diff) (advisory) — 37 mutants tested in 3m: 1 missed, 26 caught, 10 unviable
- Full previous attempt preserved in `iteration-v2/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).
