# core: part commit — a part commits, is replaced, or is refused (657.2a)

> First of the four slices of #927 (657.2). Staging (#932, #933) is merged and hands this slice a
> staged attempt: a held slot and one owned `sidx:` entry per chunk. The design is normative in
> proposal 0016, `docs/design/proposals/draft/0016-multipart-commit-protocol.md`. Read the `part:`
> and `psum:` rows (`0016:351-352`), the **Part commit** and **Live-session compensation** rows
> (`0016:660`, `:673`), the post-staging refusal rule (`0016:1420-1431`) and the quiesce rule
> (`0016:1720-1733`). Citations were verified at `376b106` on 2026-11-01.

- **Slug:** multipart-part-commit-definite
- **Track:** alpha
- **Kind:** enhancement
- **Defect:** Staging can stage a part's chunks, but nothing turns them into a part. This slice adds
  the commit as `wyrd-core` functions for every case where no commit outcome needs classifying. It
  (1) commits the `part:` + `psum:` pair, the slot release and the `sidx:` deletes in one batch after
  a last renewal, sent only with the dispatch margin left on the lease. It (2) replaces a committed
  part number, retiring the prior part's chunks in the same batch. It (3) refuses a part past the
  per-part or cumulative ceiling and compensates. It (4) compensates a settled stop reached before
  any commit was sent. It (5) composes the whole `UploadPart`, answering in the landed vocabulary
  (`UploadPartOutcome` `crates/core/src/multipart.rs:4669`, `Refusal` `:4550`). A `Conflict` or an
  unknown outcome on a sent batch stops safely, and later slices replace those two arms.
- **Success criterion:** A NEW test file `crates/core/tests/multipart_upload_part_commit.rs`, over
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
- **Falsifiability:** RED is reachable in-process on the plain `cargo test -p wyrd-core` target. The
  wrapper forces every red leg: injected outcomes, `ManualClock` moves by hook, planted records, and
  one commit held by hand for the overshoot witness. (E) is the gating `C4-ci` check. Attempt 7's
  finding 2 was reproduced against its patched tree by independent probes.
- **Invariant to restore:** **C-1** (`docs/principles.md` §5 C-1 and its §6 row *storage lifecycle /
  reclamation*), on the commit side. Every staged fragment stays named by a durable owned `sidx:`
  record until a `part:` record or a `retire:` obligation names it, in the same batch that removes the
  `sidx:` record. `Committed` is answered only when the store answered this attempt's own commit as
  landed. A slot is released exactly once, never by an attempt that does not own it. No compensation
  is sent once a commit has been sent. No commit is sent with less than the dispatch margin left. That
  covers dispatch only: a batch applying after the lease ran out is made safe by its `require`s, not by
  the clock (`0016:1851-1852`, `:1859-1863`). **Sources:** the maintainer's rule 2026-07-25 (via
  principles.md §5); 0016's refutation standard (`0016:2809-2813`); the post-staging refusal rule
  (`0016:1420-1431`). SELF-TEST: it binds the commit, the re-upload and the compensation batches, so
  guarding one function cannot satisfy it.
- **Repo + branch target:** getwyrd/wyrd @ main
- **Depends on:**
- **Conflicts with:**
- **Ordering note:** No prerequisite is left: #932 and #933 are merged, so `main` is the base.
  657.2b, 657.2c and 657.2d build on this slice, in that order. After `--accept`, #1008 and #1015 are
  re-pointed here. Intake: created by the split of #927 under the human's per-id cap override of
  2026-11-01 (alpha 21/6 before it).
- **Difficulty:** medium
- **Do model:** opus
- **Scope:** the commit half of `UploadPart` as store-I/O functions in `wyrd-core`, for outcomes
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
- **Repro instruction:** n/a (new functionality). On `main` no code writes a `part:` or `psum:` record;
  `PartRecord` (`multipart.rs:2993`) and `PartSummary` (`:3092`) have decoders and no writer.
- **External dependencies:** `typos`, `docs-renderer`
- **Test file:** `crates/core/tests/multipart_upload_part_commit.rs`, a **NEW** file. C4-verify treats
  an added `crates/*/tests/*.rs` as the per-fix discriminator; confirmed with `run-verify.sh --classify`.
- **Verification posture:** (a) net-new. The commit functions do not exist on `main`, so the test
  cannot compile on C4-verify's reverted base, and C4-verify reports **UNVERIFIABLE** (exit 77), a
  pre-declared sign-off item. **Built and exercised at Check:** every path named in Scope, driven by the
  new test over a real store and a chunk-store double. **Demonstrated reds (required, recorded in
  `build-notes.md` with the exact edit and the failing test).** Do makes each temporary edit, shows the
  named leg fail, then restores the file:
  - the lease check removed, and separately off by one → (F);
  - the over-limit check removed → (D)'s per-part leg;
  - the over-limit check reading only `chunk_bound_exceeded()` → (D)'s whole-body witness;
  - a re-upload's own prior counted in the ceiling → (D);
  - an empty re-upload that still writes an obligation → (D);
  - a settled stop that returns without compensating → (G), including the cap witness;
  - a compensation after `SlotLost`, or after a renewal `InDoubt` → (G);
  - the epoch check moved after the renewal's error arms, and separately `<=` changed to `==` → (H2);
  - the check moved above the `match` so that it drops a renewal `InDoubt`'s doubt → (H2);
  - the guard reading the attempt id from the batch's own bytes → (H3);
  - `UploadPartError::source` returning `None` → (H4).
  **C5 (advisory) survivors:** `build-notes.md` lists every mutant `cargo mutants --in-diff` leaves
  alive, with the test that kills it or why it is equivalent. **Review depth:** a clean T4 row at default
  settings is weak evidence (iteration 6: three passes of 10-16 s and 0 commands; iteration 7: 36.92 s in
  total). `scripts/review-branch` sets no effort level, so sign-off needs a confirmed high-effort review
  of the final patch.
- **Production reach:**
  - (a) The test exercises the commit against a real store. Production stages and commits no part in this slice.
  - (b) #658 consumes the `part:` records; the S3 verb lands with #669 under #508, after 657.2d, per
    0016's same-release rule (`0016:234-268`).
  - (c) This slice **must not** add a production caller. So its two safe-stop arms (H4) reach no client.
- **Citations expected:** Do must cite `path:line` on the target branch for every change.
  - **Start from attempt 7.** Do MAY open, as a narrow exception to reading this file only,
    `results/issue_927/iteration-v7/patch.diff`, which applies cleanly to `376b106`. Take from it only
    what this slice's legs need, since `clippy -D warnings` rejects unused harness. This brief overrides
    it where they differ. Its tests for this slice: `a_parts_staged_out_of_order_each_commit_with_records_that_agree`,
    `b_a_rollback_before_the_quiesce_renewal_commits_under_the_live_epoch`, `d_a_reupload_retires_the_prior_chunks_in_the_same_batch`,
    `d_a_part_past_the_per_part_bound_is_refused_and_compensated`, `d_the_cumulative_ceiling_refuses_and_its_overshoot_is_bounded`,
    `d_the_cumulative_ceiling_leaves_out_the_parts_own_prior`, `d_a_reupload_under_a_newer_epoch_owes_the_prior_under_that_epoch`,
    `d_the_quiesce_renewal_adoption_is_rejected_when_the_epoch_has_not_risen`, `f_a_commit_round_is_sent_with_exactly_the_margin_and_not_with_a_millisecond_less`,
    `f_a_lease_used_up_by_the_reads_sends_no_commit`, `g_a_settled_renewal_stop_compensates_and_keeps_its_error`,
    `g_settled_stops_never_fill_the_slot_cap`, `h_a_composed_error_keeps_its_class_and_its_message`,
    `the_slot_bytes_guard_reads_the_recorded_owner_not_the_batchs_own_bytes`, the committed-part legs of
    `c4_empty_parts_never_install_an_empty_obligation`, the renewal legs of
    `g_a_stop_that_cannot_show_its_records_are_its_own_does_not_compensate`, and the unit test in `commit.rs`.
  - **Records and keys:** `part_key` (`multipart.rs:1321`), `psum_key` (`:1336`), `PartRecord` (`:2993`),
    `PartSummary` (`:3092`), `retire_key` (`:1517`), `decode_retire_obligation` (`:3954`),
    `RetireObligation` (`:3976`), the one writer-side route for an obligation: add the per-part `{chunks}`
    row there, as the doc at `:3679-3681` asks. `SessionRecord::open` is the model for a crate-private
    writer-side constructor (its doc, `:2350-2353`). Also `MAX_PART_CHUNKS` (`:5037`), `MAX_INFLIGHT_PARTS`
    (`:5097`), `MAX_STAGED_CHUNKS` (`:5108`), `E_TX_MILLIS` (`:5118`).
  - **Staging's hand-off:** `StageOutcome` (`crates/core/src/multipart/stage.rs:362`), `RenewOutcome::Covered`
    (`:375-384`), `StagedAttempt::renew` (`:563`), `StageError` (`:610`), `Stop` (`:644`).
  - **The store's timing contract:** `crates/traits/src/lib.rs:1332-1345`.
- **Prior-art check (triage cycles):** searched by path (`crates/core/src/multipart.rs`,
  `crates/core/src/multipart/`, `crates/core/tests/multipart*`, `docs/design/architecture/05-building-block-view.md`)
  on `origin/main` @ `376b106` and on the tracker, 2026-11-01. Merged: the record family (#654), knobs
  (#801), Create (#908), Abort (#909), staging (#942, #965). Iterated: #927's attempts 1 to 7, whose
  final patch is the starting point. Open PRs touching these paths: none. `commit.rs` and the test file
  do not exist on `main`.
- **Disposition hint:** new-feature

## Design

**Quiesce, then commit** (`0016:660`, `:1720-1733`). Run the staged attempt's last renewal
(`StagedAttempt::renew`), with `cover_millis` at least `E_TX_MILLIS`, and send no renewal after it. If
the renewal adopted a session, compare the epoch before and after it: an `Open` record at a lower epoch,
or under other bytes at the same epoch, is `Stop::Record(SessionEpochNotRisen)` (decided at sign-off,
iteration 6). Apply that check to **every arm that would go on to send a batch**: `Covered`, and an
error where the table below says compensate. A renewal `InDoubt` keeps its doubt, so do not move the
check above the `match` unchanged (attempt 7's finding 2). Do not change `stage.rs` to do this.

**The known gap.** A renewal that finds a non-`Open` record at an unrisen epoch answers `NoSuchUpload`
inside `stage.rs`. Sign-off accepted that (iteration 6). The comment recording it cites #1026
(`// deferred: #1026`), never `build-notes.md`, which does not exist in the repo after merge.

**Reads, then one batch.** Refuse an over-limit attempt first: truncated (the attempt's `len()` and
`digest()` describe only a prefix), or holding more than `MAX_PART_CHUNKS` chunks. Read the bounded `psum:<id>:` range, leaving out this part's own prior, and
refuse past `MAX_STAGED_CHUNKS`. That read is unfenced on purpose: concurrent overshoot is bounded by
the slot cap and charged in `U_REF` (`0016:1441-1451`), so no precondition on the total belongs in the
batch. Do not add a shared counter or a session-wide fence (`0016:1686-1742`). Read the part key. Then
commit one batch: `require(mpu == Open@E)`; `require_absent(part:…)`, or `require(part:… == prior)`;
`require(slot:<id>:<k> ==` the bytes this attempt last wrote`)`; put `part:` and `psum:`; delete the
slot and the attempt's `sidx:` entries; for a re-upload over a non-empty prior, put one `retire:bytes:`
obligation for the prior's chunks under `require_absent` of its key (token per `0016:358-373`, under the
session's live epoch). The digest is the SHA-256 basis `Digest` already uses (`multipart.rs:1035-1052`,
ADR-0047).

**The lease promise covers dispatch only** (ratified at sign-off). Read the clock before the commit and
send only with `E_TX_MILLIS` left. Do **not** renew again to buy time: that rewrites the slot the commit
requires. `E_TX_MILLIS` is 0016's chosen margin, not a deadline on `commit`
(`crates/traits/src/lib.rs:1332-1345`). Safety rests on the `require`s: the commit and the idle fence
each require what the other changes, so whichever lands first makes the other fail. An expired lease is
never evidence about whether a commit landed.

**Stops in this slice:**

| The stop | Action |
|---|---|
| Renewal `CoverShort`, `RenewalRoundsSpent`, `LeaseLapsed`, `InvalidRequest` | Compensate |
| Lease short before the commit | Compensate |
| Over the per-part or cumulative ceiling | Compensate, `Refused(EntityTooLarge)` |
| Renewal `InDoubt` | Keep the doubt; no compensation |
| `SlotLost`, and `EntryLost` from staging | No compensation: the attempt cannot show the records are its own |
| An owned entry lost after staging | The renewal ends `RenewalRoundsSpent`: compensate |
| Session has left `Open` | Answer the refusal; the residue is the reaper's |
| `Stop::Store`, `Stop::Record` | No compensation; the residue stays, as after a crash (`0016:1778`) |
| A sent commit answered `Conflict` or unknown | (H4): stop, keep the slot, no compensation |

**Compensation** is one batch under `require(mpu == Open@E)` and `require(slot ==` the bytes last
written`)`: one `retire:bytes:` obligation naming this attempt's own chunks (only when non-empty), the
slot delete and the `sidx:` deletes (`0016:673`). A lapsed lease does not block it: compensation revives
nothing and lands only if the session and slot are unchanged (`0016:1651-1653`).

**Shape for the later slices.** Keep `commit_part`'s commit round in its own function, so 657.2b can add
rounds and 657.2c a doubt without rewriting the quiesce, the reads or the refusals.

**Writer constructor.** `PartRecord` has none today (`multipart.rs:2990`). Add a crate-private one in the
shape of `SessionRecord::open`, so every value written is one the decoder accepts. Build `psum:` from the
`PartRecord` it summarizes, so the pair cannot disagree (`:3085-3089`).

**An over-limit body** (decided 2026-10-30). `stage_part` answers `StageOutcome::Staged` for a body past
`max_part_chunks`. Its doc says "every chunk is registered and its fragments are written" (`stage.rs:363`),
which is wrong for this case: correct it. Do not add a variant. `commit_part` is the only route to a
`part:` record, so its first step refuses such an attempt, and any attempt past `MAX_PART_CHUNKS`
however it was staged. Attempt 7 checks only the truncation flag; this brief overrides it.

**Docs currency** (`AGENTS.md:169-172`). Add one **new, short paragraph** to
`docs/design/architecture/05-building-block-view.md`, between `:246` and `:248`, and edit neither: those
are very long single lines, and rewriting one has cost 20 KB of diff. Say: `part:` and `psum:` now have
their writer, the commit lands them with the slot release and `sidx:` deletes in one batch, and nothing
calls it in production until #669.

## Alternatives considered

- **A per-session in-flight counter.** A shared writable key on the part path (`0016:1686-1742`). Rejected.
- **Renew again when the lease runs short.** It adds a writer of the slot the commit requires. Rejected.
- **A new `StageOutcome` variant for an over-limit body.** A larger edit to merged code than the risk warrants. Rejected at sign-off, 2026-10-30.
- **An attempt id in `PartRecord`.** It changes a landed record shape that #658 and #659 read. Rejected.

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a
draft PR MAY happen during the cycle (useful for CI feedback). The PR MUST NOT be
marked ready before sign-off accepts.

Plan-review response (2026-11-01, revision pass): all three findings hold at `376b106`. The commit now enforces `MAX_PART_CHUNKS` itself, with a whole-body witness; H3 separates reservations from later batches; and `EntryLost` is a composed-upload leg, with owned-entry loss during quiesce stated as `RenewalRoundsSpent` and compensated, with no change to `stage.rs`.
