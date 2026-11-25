<!-- pdca:split-proposal v1 -->
# Split proposal — issue 927

**Intake-cap override (wyrd-pdca-P1), 2026-11-01.** `scripts/plan-cap --track alpha --need 4`
answered, when this proposal was written:

```
planned 21/6 (cap, track alpha) — room for 0, need 4: Plan intake closed
  PLANNED           15  1008 1009 1010 1011 1012 1013 1014 1015 1016 1017 1018 1019 668 669 670
  BUILT              6  508 625 633 654 658 659
```

The human (Eduard Ralph) was shown that line and chose the override for #927 in this session,
2026-11-01 ("go ahead with the split please"). If accepted, alpha goes from 21/6 to 25/6. #927 is
UNPLANNED, so it is not in the count today, and its four children enter it. The override covers
this split of #927 only. It is not a standing exception.

**Actual, after `--accept` (2026-11-01): alpha is 26/6, not 25/6.** A split parent counts as BUILT
until it signs off, so #927 entered the count along with its four children.

**Who drafted this.** The planner session wrote this file directly. `pdca split 927` refuses a
bundle with no `brief.md` (`do_split`, `src/pdca_harness/leaves.py`), which an iterate-to-Plan
leaves behind, and the splitter is an interactive leaf. No sizer verdict was available.

> Every child below is a complete brief carved from `results/issue_927/iteration-v7/brief.md`, with
> the sign-off decisions of iterations 2 to 7 already applied. Citations were verified against
> `getwyrd/wyrd` `origin/main` @ `376b106` on 2026-11-01. Do re-resolves every `path:line` on the
> base it is given.

## Why this slice is oversized

Seven attempts took the patch from 170 KB to 333 KB against a 100 KB signal, and each one still
found something new. One bundle holds four outcomes that can ship in order, and every review has
to attack all of them at once. Attempt 7 is 67 KB of `commit.rs`, 15 KB of `multipart.rs` and a
231 KB test file (about 65 KB of harness and 160 KB across 60 tests).

The earlier reason for not splitting no longer holds as stated. Iteration 2 said classifying and
compensating a lost commit depend on each other. They do, and 657.2b keeps them together. But a
commit whose outcome is **unknown** needs neither answer at first: it can return the in-doubt
error, leaving its slot and entries in place. #658's split used the same seam.

### The seams

1. **No commit sent yet vs a commit sent.** Before a commit is sent, nothing can be in doubt, so a
   refusal can compensate with no classification at all. That gives a part that commits, retires
   a replaced part and refuses over-limit parts: 657.2a.
2. **A definite `Conflict` vs an unknown outcome.** A `Conflict` means the batch did not land, so
   classification needs only "who beat me". That is 657.2b. Each child until then stops safely,
   with the residue left in place.
3. **A commit's unknown outcome vs a compensation's.** A compensation is settled by a different
   read (its own obligation) and only runs after the commit is settled. Until 657.2d its unknown
   outcome is the in-doubt error.

### The four outcomes

1. **657.2a: a part commits, is replaced, or is refused.** Findings 2 and 4 of attempt 7.
2. **657.2b: a commit that lost is classified and compensated.** Finding 1, and the `Open@E'`
   rollback.
3. **657.2c: a commit with an unknown outcome is settled, or stays in doubt.** The own-commit
   proof, the read order, held batches, and damage under a held commit.
4. **657.2d: a compensation with an unknown outcome is settled, or stays in doubt.** Finding 3.

### Where attempt 7's findings go

| # | Finding | Goes to |
|---|---|---|
| 1 | Terminal delete after the slot read answers `SessionRecordMissing` | 657.2b, both the commit and the compensation |
| 2 | A settled renewal error skips the adopted-session epoch check | 657.2a |
| 3 | Slot erased: false `Committed` / false `OperationAborted` | 657.2d (fix decided by the human, 2026-11-01) |
| 4 | Slot guard accepts A's batch with B's bytes; no lower-epoch test | 657.2a |
| 5 | `commit.rs:843-845` cites `build-notes.md` for a known gap | 657.2a cites #1026, filed 2026-11-01 |
| 6 | T4 row does not show a high-effort review | Not code. Every child's posture states it, and it goes to Act (`scripts/review-branch` sets no effort level) |

The two plan-review findings on the v7 brief (dispatch-only lease, read order) were already applied
to it and stay applied in these children.

### Sizes

Measured from attempt 7 by assigning each of its 60 tests to a child by script; none is left over.
Unlike a bottom-up estimate, these describe code that exists. Code is an estimate within
`commit.rs`, whose one 316-line `commit_part` function holds several outcomes.

| child | code | harness | legs | total |
|---|---|---|---|---|
| 657.2a | 45–50 KB (`commit.rs` 30, `multipart.rs` 15, docs) | 40 KB | 34 KB | **105–125 KB** |
| 657.2b | 15 KB | 8 KB | 43 KB | **65–80 KB** |
| 657.2c | 15 KB | 12 KB | 60 KB | **85–105 KB** |
| 657.2d | 7 KB | 5 KB | 23 KB | **35–50 KB** |

**657.2a sits over the 100 KB signal, and I found no honest cut inside it.** About 60% of it is the
harness every later child reuses, and a commit with no refusal path breaks 0016's post-staging rule
(`0016:1420-1431`). 657.2d is small, but merging it into 657.2c gives about 135 KB in the area where
most findings have landed. So four is the fewest that keep each child near the band.

**These children cannot be split again** without `--force`: #927 is depth 1, so they are depth 2 and
`--accept` refuses a further split by default.

### Test file and C4-verify

657.2a ships `crates/core/tests/multipart_upload_part_commit.rs` as a new file. `commit_part` does
not exist on its reverted base, so C4-verify reports UNVERIFIABLE (exit 77). 657.2b, 657.2c and
657.2d append to that file and do not move the harness into a `tests/support/` module:
`engine/scripts/run-verify.sh:141-144` classifies every added `*/tests/*.rs` as a test target. Each
of those three gets C4-verify `PASS (green-only)`, so its red rests on the demonstrated reds its
brief lists, as #933's did. Confirmed with `run-verify.sh --classify` on synthetic patches.

## Wave sketch

A straight line: 657.2a → 657.2b → 657.2c → 657.2d. Each child rewrites the arms the one before left
as a safe stop, in the same function and the same test file, so no two can share a wave. No child
is independent of the one before it.

## After `--accept`: bundles that name #927

**A split parent counts as met at once** (the `Ordering note` of #1015 says so, and `flow._runnable`
gates on COMPLETE). So after `--accept`, every `Depends on: 927` stops holding its bundle back, and
those bundles would build with no `commit_part` on their base. These edits must follow the accept,
in this session, once the child ids exist. Editing a brief's fields is exempt from the cap.

| Bundle | Re-point `927` to | Why |
|---|---|---|
| #1008 | 657.2a | Its legs need parts to publish, and a part that commits is all it reads |
| #1015 | 657.2a | It reads the `part:` and `retire:` records; it does not call the commit |
| #668 | 657.2d | It leads into the production caller; 0016's same-release rule (`0016:234-268`) |
| #669 | 657.2d | It calls the commit in production, and its leg (H) holds a commit |
| #670 | 657.2d | It follows #669 |

`#1007` names #927 only as optional. The notes in #1015, #668, #669 and #670 that say to add `927`
to a later list need the same re-point. Tracker comments on #927 stay as they are.

## Decided by the human, 2026-11-01

1. **Finding 3, damage-only false success:** fix it in 657.2d, where the store can still show the
   commit did not land. A re-upload with its slot erased stays a documented limit.
2. **Finding 5, the tracking issue:** filed as #1026. 657.2a cites it.

<!-- pdca:child child-1 -->
# core: part commit — a part commits, is replaced, or is refused (657.2a)

> First of the four slices of #927 (657.2). Staging (#932, #933) is merged and hands this slice a
> staged attempt: a held slot and one owned `sidx:` entry per chunk. The design is normative in
> proposal 0016, `docs/design/proposals/draft/0016-multipart-commit-protocol.md`. Read the `part:`
> and `psum:` rows (`0016:351-352`), the **Part commit** and **Live-session compensation** rows
> (`0016:660`, `:673`), the post-staging refusal rule (`0016:1420-1431`) and the quiesce rule
> (`0016:1720-1733`). Citations were verified at `376b106` on 2026-11-01.

- **Slug:** multipart-part-commit-definite
- **Track:** <leave as is — `pdca split --accept` writes the parent's track in>
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
  - A part past the per-part bound (`StagedAttempt::chunk_bound_exceeded`, `stage.rs:534`) answers
    `Refused(EntityTooLarge { .. })` and compensates in one batch, through `commit_part` and through
    the composed `UploadPart`. No `part:` record exists afterwards.
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
  `Stop::EntryLost`, `Stop::Record`, `Stop::Store`: the log shows no compensation batch and the answer
  claims no release.
  **(H2) A damaged session adopted by the renewal is never compensated under** (attempt 7's finding
  2). Run each on a renewal that ends covered, ends `RenewalRoundsSpent` and ends `CoverShort`, with an
  `Open` record planted under other bytes at the same epoch, and at a **lower** epoch (staged at epoch
  2, planted at 0). The answer is `Stop::Record(SessionEpochNotRisen)`, with no commit and no
  compensation. A renewal `StageError::InDoubt` in the same history keeps its doubt.
  **(H3) The slot guard names the submitter** (attempt 7's finding 4). On every batch in every leg,
  the wrapper checks that a batch naming a `slot:` key requires exactly the bytes **the attempt that
  submitted it** last wrote. The submitter's identity is supplied to the wrapper apart from the batch
  and from the slot's current occupant. A named leg shows it rejects attempt A's batch carrying
  attempt B's bytes, after A released the index and B reserved it.
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

**Reads, then one batch.** Refuse an over-limit attempt first (the attempt's `len()` and `digest()`
describe only a prefix). Read the bounded `psum:<id>:` range, leaving out this part's own prior, and
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
| `SlotLost`, `EntryLost` | No compensation: the attempt cannot show the records are its own |
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
`part:` record, so its first step refuses such an attempt.

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
<!-- pdca:end child-1 -->

<!-- pdca:child child-2 -->
# core: part commit — a commit that lost is classified and compensated (657.2b)

> Second of the four slices of #927 (657.2). It builds on 657.2a, whose commit stops on any `Conflict`
> with the slot held. Read 0016's commit classification (`0016:1584-1653`) and the **Live-session
> compensation** and **Terminal session delete** rows (`0016:673`, `:674`). Citations were verified at
> `376b106` on 2026-11-01; re-resolve them on your base, which folds in 657.2a.

- **Slug:** multipart-part-commit-classify-conflict
- **Track:** <leave as is — `pdca split --accept` writes the parent's track in>
- **Kind:** enhancement
- **Defect:** After 657.2a a commit or compensation answered `Conflict` stops and leaves its slot held.
  Under a live session nothing else frees that slot before `W_session`, because the reaper does not
  walk an `Open` session (`0016:1600-1620`). This slice classifies a `Conflict` from a re-read:
  the session fenced, the session rolled back to a newer `Open@E'`, a same-number rival, or a benign
  retry. It compensates where the session is live, chases a further rollback, and bounds every loop.
- **Success criterion:** Appended to `crates/core/tests/multipart_upload_part_commit.rs`, with the
  wrapper extended by **an action between two reads**. In every leg nothing is in doubt: no batch of the
  attempt answered unknown.
  **(B) A lost commit is classified, never compensated blind** (`0016:1584-1653`).
  - The session fenced to `Aborting` before the commit: `Refused(NoSuchUpload)`, no write (the residue
    is the reaper's).
  - **The session rolled back to `Open@E'` (`E' > E`)** between the last renewal and the commit: the attempt
    compensates under `require(mpu == Open@E')` (`0016:1607-1620`), one obligation for its own chunks,
    its slot released and its `sidx:` entries deleted in one batch. It answers exactly
    `Refused(OperationAborted)`, never `Committed` and never `NoSuchUpload`, because the session is `Open`.
  - A same-part rival committed first: the loser commits one obligation for its **own** chunks (it
    decodes under `decode_retire_obligation`, `multipart.rs:3954`), releases its slot, deletes its
    `sidx:` entries, and answers exactly `Refused(OperationAborted)`. An empty loser writes no obligation.
  - A plain repair changed the part with nothing in doubt: compensate and answer
    `Refused(OperationAborted)`. No retry against the new record (decided at sign-off, iteration 4).
  - A conflict no read explains (the part as required, the session `Open@E`, the slot held): the same
    batch is retried.
  **(T) A terminal delete is not damage** (attempt 7's finding 1). The commit answers `Conflict`; the
  reads find the part, the session (`Aborting`) and the slot (held); **then** the terminal delete lands,
  removing the session, slots and owned entries in one batch (`0016:674`), and the last session read finds
  nothing. The answer is `Refused(NoSuchUpload)`, never `Stop::Record(SessionRecordMissing)`. The same
  for a compensation when the delete lands right after its slot read. Attempt 7 pinned the wrong answer
  (`multipart_upload_part_commit.rs:4687-4688`, "no second lap of reads"): that must not come across.
  A slot **still held** on a slot read made after the session was found gone is damage,
  `Stop::Record(SessionRecordMissing)` (decided at sign-off, iteration 6).
  **(R) Damage, with nothing in doubt, is a record stop, never a rival or a fence.** No compensation in
  any of these. Bytes at the part key that do not decode. A prior part record gone while the session is
  still `Open@E` (`RecordError::PartRecordMissing`). A session record that does not decode. A record under
  other bytes at the same or a lower epoch, `Open` or not (`SessionEpochNotRisen`). A damaged part record
  under a moved session. A slot rewritten or lost: `Stop::SlotLost`, with no release claimed.
  **(O) A taken obligation key.** Other bytes planted at the obligation's key, then a re-upload over a
  non-empty prior: those bytes and the prior are kept, ending in `Stop::ObligationKeyTaken`.
  **(L) Bounds and the lease.**
  - Every loop (commit rounds, compensation rounds, the epoch chase) has a stated maximum. With a wrapper
    answering every batch `Conflict` unapplied, each ends in a typed error after **exactly** that many
    commits. The test compares against a **number written in the test**, never the imported constant.
  - **The epoch chase's real bound:** one rollback beats the commit, then two more beat compensations:
    three rollbacks, `MAX_COMPLETE_ATTEMPTS`, the most a session can see. The next compensation lands.
  - During a compensation: a session found fenced answers `Refused(NoSuchUpload)` with no further write;
    a compensation that spends its rounds returns its own typed error, which claims no release.
  - `Conflict` rounds separated by slow reads take the lease below the margin before a later round: no
    further commit is sent and the attempt compensates. The answer is a typed stop, never `Committed`.
  - Commit rounds spent with nothing in doubt compensate.
  **(E) The whole gate.** `cargo xtask ci` is green on the patched tree.
- **Falsifiability:** RED is reachable in-process on `cargo test -p wyrd-core`. The wrapper forces every
  red leg: the rollback and Abort are written by hook, (T)'s delete lands by hook between two reads, and
  rounds are answered `Conflict` by script. Attempt 7's finding 1 was reproduced against its patched tree by
  independent probes. (E) is the gating `C4-ci` check.
- **Invariant to restore:** **C-1** (`docs/principles.md` §5 C-1, §6 *storage lifecycle / reclamation*). A
  commit that lost to a live session releases its slot and names its own chunks in an obligation, in one
  batch, because no reaper will. An `Open` session is never answered `NoSuchUpload`. A record that does not
  decode, or an epoch that did not rise, is never evidence of a rival or a fence (ADR-0045). A slot read made
  before the read that found the change proves nothing. **Sources:** the maintainer's rule 2026-07-25;
  0016's refutation standard (`0016:2809-2813`); the classification (`0016:1584-1653`); ADR-0045. SELF-TEST:
  it binds the commit's and the compensation's re-reads, so guarding one function cannot satisfy it.
- **Repo + branch target:** getwyrd/wyrd @ main
- **Depends on:** child-1
- **Conflicts with:**
- **Ordering note:** `Depends on` 657.2a: this slice replaces its `Conflict` stops, in its function and test
  file. 657.2c builds on this slice's round. Intake: created by the split of #927 under the human's per-id
  cap override of 2026-11-01.
- **Difficulty:** medium
- **Do model:** opus
- **Scope:** what a `Conflict` means for a commit and for a compensation: the re-read, the classification,
  the retry, the epoch chase, the loop bounds, and the wider doc of `Refusal::OperationAborted`, with no new
  variant. / out of scope:
  - any batch answered `CommitUnknownResult`: 657.2a's in-doubt stop stays until 657.2c and 657.2d;
  - slots left held when a Complete fence rolls back (decided at sign-off, iteration 2: accept until
    `W_session`, raise against 0016, do not change it here);
  - any behaviour change in `stage.rs`; proposal 0016 and the ADRs; `Cargo.lock` and `deny*.toml`; the
    gateway and any production caller; Complete, the drain, the reaper, DST.
- **Repro instruction:** n/a (new functionality). On 657.2a's base a commit answered `Conflict` stops with
  the slot held, and that stop has a test.
- **External dependencies:** `typos`, `docs-renderer`
- **Test file:** `crates/core/tests/multipart_upload_part_commit.rs`, **appended**, on the harness 657.2a shipped.
- **Verification posture:** C4-verify reports `PASS (green-only)`, because no test file is added
  (`run-verify.sh --classify`), so a pre-declared sign-off item. **Demonstrated reds (required, recorded in
  `build-notes.md`).** Each temporary edit fails the named leg:
  - a slot read made before the session read taken as final, with no further slot read → (T), commit and compensation;
  - a missing session record always read as a terminal delete → (T)'s still-held leg;
  - compensating on any `Conflict` without the re-read → (B)'s fenced leg;
  - the rolled-back `Open@E'` sent to the reaper → (B)'s rollback leg;
  - the rollback or rival loser answering any refusal other than `OperationAborted` → (B);
  - a damaged part read as a rival → (R); a non-`Open` record at an unrisen epoch read as a fence → (R);
  - `require_absent` removed from the obligation key → (O);
  - a loop counter that never reaches its bound → (L); `COMPENSATION_ROUNDS` lowered by one → (L)'s chase leg;
  - the lease check skipped on a later round → (L).
  **C5 (advisory) survivors** are listed in `build-notes.md`. **Review depth:** sign-off needs a confirmed
  high-effort review of the final patch; a clean T4 row at default settings is weak evidence.
- **Production reach:** (a) The test runs against a real store; production commits no part in this slice.
  (b) The S3 verb lands with #669 after 657.2d. (c) This slice **must not** add a production caller.
- **Citations expected:** Do must cite `path:line` on the target branch for every change.
  - **Peer for the classification:** #895's Abort, `crates/core/src/multipart/abort.rs:282`.
  - **Start from attempt 7.** Do MAY open `results/issue_927/iteration-v7/patch.diff` as a reference; its
    round reads the part, the session, the slot, then the session again, and this brief overrides it on (T).
    Use that read order: 657.2c needs it. Its tests for this slice:
    `b_a_fenced_session_answers_no_such_upload_and_writes_nothing`, `b_a_rolled_back_session_is_compensated_under_its_newer_epoch`,
    `b_a_same_part_loser_compensates_its_own_chunks`, `c_every_loop_ends_after_exactly_its_bound`,
    `d_a_taken_obligation_key_keeps_those_bytes_and_the_prior_part`, `f_a_lease_used_up_by_retries_stops_the_rounds_and_compensates`,
    `g_a_slot_lost_or_a_damaged_record_at_a_re_read_stops_without_a_release`, `g_a_settled_stops_compensation_chases_stops_or_reports_itself`,
    `h_the_epoch_chase_lands_within_its_bound_after_every_rollback_a_session_can_see`, `g_a_damaged_part_record_at_the_re_read_is_a_record_stop_not_a_rival`,
    `g_a_session_whose_epoch_has_not_risen_is_damage_not_a_rollback_or_a_fence`, `g_a_prior_part_record_gone_under_the_same_session_is_damage`,
    `g_a_damaged_part_record_under_a_moved_session_is_a_record_stop`, the loser leg of `c4_empty_parts_never_install_an_empty_obligation`,
    the stop legs of `g_a_stop_that_cannot_show_its_records_are_its_own_does_not_compensate`, and the no-doubt legs of
    `g_a_missing_session_record_is_a_terminal_delete_only_with_the_slot_gone`.
  - **The doc to widen:** `Refusal::OperationAborted` (`multipart.rs:4557-4559`), which says only "fenced into `Completing`".
- **Prior-art check (triage cycles):** as 657.2a, on `origin/main` @ `376b106`, 2026-11-01. No merged or open
  work classifies a part commit. #927's attempts 1 to 7 are the source.
- **Disposition hint:** new-feature

## Design

**After a `Conflict`, classify from a re-read** (`0016:1597-1647`). Each round reads the part key, the
session, the attempt's own slot, then the session again. Only the last session read decides the answer.

| The session (last read) | The part key | Answer |
|---|---|---|
| still `Open@E` | what the commit requires | a conflict no read explains (on TiKV, a lock another writer held): retry the same batch |
| still `Open@E` | changed: a same-number rival won, or a repair rewrote the prior | compensate under `Open@E`; `Refused(OperationAborted)` |
| back at a newer `Open@E'` | any | compensate under `Open@E'`; `Refused(OperationAborted)` |
| not `Open`, at a higher epoch (a fence) | any | `Refused(NoSuchUpload)`; the residue is the reaper's |
| gone, and a slot read made **after** it found the slot gone | any | `Refused(NoSuchUpload)`; the delete took them in one batch (`0016:674`) |
| gone, and that later slot read found the slot still held | any | `Stop::Record(SessionRecordMissing)`: damage |

**A session read is evidence only once judged.** It has **moved** when the record decodes at a higher epoch
than the bytes the batch required, whether `Open` (a rollback) or not (a fence). Every fence and rollback
bumps the epoch (`0016:364`, `:704-708`). Bytes that do not decode, and any record under other bytes at the
same or a lower epoch, whatever its state, are damage. At an earlier read damage is passed over; at the
round's last read it stops the attempt.

**A slot found gone with nothing in doubt** means it was lost: `SlotLost`, unless a moved or vanished session
explains it, as in the table. "Gone" means absent **or** holding another attempt's `attempt_id`
(`multipart.rs:2796`). Every batch requires the bytes **this attempt last wrote**, never bytes re-read from the key.

**The answer for a lost commit on a live session** is exactly `Refused(OperationAborted)`: a conflicting
operation intervened and the client may retry the part (ratified at sign-off). Widen that variant's doc to
name a rival, a rollback and a repair rewriting the prior. Add no new `Refusal`. #669 maps it to the wire.

**The epoch chase.** A compensation that lost to a newer `Open` epoch is sent again under that epoch, up to
`COMPENSATION_ROUNDS` (`MAX_COMPLETE_ATTEMPTS + 1`, `multipart.rs:5279`). One that finds the session no longer
`Open` stops, the residue the reaper's. **Empty chunk lists:** no obligation, and the slot release and `sidx:`
deletes stay in the same batch.

## Alternatives considered

- **Compensate on any non-`Committed` outcome.** It orphan-marks a part that exists (`0016:1792`). Rejected.
- **A retry against a repaired part's new record.** Rejected at sign-off, iteration 4.

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a
draft PR MAY happen during the cycle (useful for CI feedback). The PR MUST NOT be
marked ready before sign-off accepts.
<!-- pdca:end child-2 -->

<!-- pdca:child child-3 -->
# core: part commit — a commit with an unknown outcome is settled or stays in doubt (657.2c)

> Third of the four slices of #927 (657.2). It builds on 657.2b's round. Read 0016's commit classification
> on unknown outcomes (`0016:1630-1651`) and its failure rows (`0016:1780`, `:1792`). Citations were verified
> at `376b106` on 2026-11-01; re-resolve them on your base, which folds in 657.2a and 657.2b.

- **Slug:** multipart-part-commit-settle-unknown
- **Track:** <leave as is — `pdca split --accept` writes the parent's track in>
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
  **(H) A commit held while a rollback lands** right after the slot read: the in-doubt error, no compensation.
  **(R) Damage under a held commit keeps the doubt.** Undecodable part or slot bytes, an undecodable session, a
  record at the same or a lower epoch, `Open` or not, and a missing `mpu:` with the slot held: `InDoubt` keeping
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
- **Depends on:** child-2
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
terminal delete (iteration 6).

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
<!-- pdca:end child-3 -->

<!-- pdca:child child-4 -->
# core: part commit — a compensation with an unknown outcome is settled or stays in doubt (657.2d)

> Last of the four slices of #927 (657.2). It builds on 657.2c. Citations were verified at `376b106` on 2026-11-01;
> re-resolve them on your base, which folds in 657.2a, 657.2b and 657.2c.

- **Slug:** multipart-part-commit-settle-compensation
- **Track:** <leave as is — `pdca split --accept` writes the parent's track in>
- **Kind:** enhancement
- **Defect:** After 657.2c a compensation answered `CommitUnknownResult` returns the in-doubt error at once, even when
  its own obligation shows it landed. And, with only a key damaged, the own-commit proof can answer `Committed` for a
  commit that never landed. This slice settles a compensation's outcome from re-reads and closes that false answer where the
  store still shows it.
- **Success criterion:** Appended to `crates/core/tests/multipart_upload_part_commit.rs`.
  - **An unknown compensation** (landed; not landed with `may_still_commit: false`; held) is settled by re-reading its own
    obligation, its own slot and the session (for an empty part, slot and session alone). Each run ends with exactly one
    obligation and the slot released once, or the in-doubt error carrying the doubt.
  - **The obligation re-read.** A compensation lands with an unknown outcome, then a client Abort runs before the re-read. Only
    the obligation can settle this: the answer is not in-doubt. A second run plants other bytes at the key and ends in
    `Stop::ObligationKeyTaken`, or `InDoubt` with `ObligationKeyTaken` as its cause under a held compensation.
  - **The epoch chase re-reads every obligation it sent:** the loser's first compensation is held; right after its slot read it
    lands and the session rolls back; the second round conflicts. The answer is `Refused(OperationAborted)`.
  - **A held commit beside an unknown empty compensation.** An empty re-upload over an empty part 1, clock fixed. The commit is
    held; an empty rival lands in the same millisecond. If a compensation is sent, the wrapper first lands the held commit, then
    answers it unknown. The log shows **no compensation batch** while the commit was unsettled; the answer is never
    `Refused(OperationAborted)`; at the end the slot is released once and no `retire:` key exists.
  - **A held compensation under a teardown.** An empty part loses to a rival, its compensation is held, then an Abort and a
    teardown run: the in-doubt error, never `Refused(OperationAborted)`.
  - **A slot gone under a moved session at the compensation's re-read:** the in-doubt error.
  - **Faults and damage under a held compensation.** A store fault at the next obligation, slot or session read, and undecodable
    slot or session bytes: `InDoubt { batch: Compensation, may_still_commit: true }`. Each fails if its doubt-keeping return is
    replaced with a plain stop.
  - **Damage only, no false success** (attempt 7's finding 3; fix decided by the human, 2026-11-01). A commit that required the key **absent** is
    held, or answered unknown with `may_still_commit: false`, and never applied; the slot key is erased with the session untouched.
    The answer is not `Committed`, because a landed commit leaves a record at that key and none is there: it is `Stop::Record` or the
    in-doubt error, with the doubt kept. A non-empty compensation, unknown and unapplied, with its slot erased: no obligation is
    there, so it is not `Refused(OperationAborted)`.
  - **(E) The whole gate.** `cargo xtask ci` is green on the patched tree.
- **Falsifiability:** RED is reachable in-process on `cargo test -p wyrd-core`. Held and unknown compensations, the erased slot and
  each fault are forced by the wrapper. Finding 3 was reproduced against attempt 7's patched tree by independent probes. (E) is the
  gating `C4-ci` check.
- **Invariant to restore:** **C-1** (`docs/principles.md` §5 C-1, §6 *storage lifecycle / reclamation*). A compensation is answered as
  landed only on evidence that it landed, and `Refused(OperationAborted)` promises the attempt's chunks are named in an obligation.
  `Committed` is never answered when the store shows no such commit can have landed. A slot is released exactly once. **Sources:** the
  maintainer's rule 2026-07-25; 0016's refutation standard (`0016:2809-2813`); `0016:1651-1653`, `:1792`. SELF-TEST: it binds the
  compensation's re-read and the own-commit proof, so guarding one function cannot satisfy it.
- **Repo + branch target:** getwyrd/wyrd @ main
- **Depends on:** child-3
- **Conflicts with:**
- **Ordering note:** `Depends on` 657.2c, whose settled commit is the only thing that lets a compensation be sent in doubt-free form.
  This is the last slice: #668, #669 and #670 are re-pointed here after `--accept`, because a production caller needs the whole
  commit. Intake: created by the split of #927 under the human's per-id cap override of 2026-11-01.
- **Difficulty:** low
- **Do model:** opus
- **Scope:** what a compensation's `CommitUnknownResult` means, and the damage-only false answers of the own-commit proof and the
  compensation. / out of scope:
  - a re-upload over a non-empty or empty prior with its slot erased: no read can tell that apart, since a landed commit and a rival
    both leave a record there. It stays a documented limit of the proof, and no legal writer does this;
  - any behaviour change in `stage.rs`; proposal 0016 and the ADRs; `Cargo.lock` and `deny*.toml`; the gateway and any production caller;
    Complete, the drain, the reaper, DST.
- **Repro instruction:** On 657.2c's base, hold a commit that required the key absent, erase the slot key, and read the answer: `Committed`
  with no `part:` record (attempt 7, `commit.rs:1012-1018`).
- **External dependencies:** `typos`, `docs-renderer`
- **Test file:** `crates/core/tests/multipart_upload_part_commit.rs`, **appended**.
- **Verification posture:** C4-verify reports `PASS (green-only)`, a pre-declared sign-off item. **Demonstrated reds (required, recorded in
  `build-notes.md`).** Each temporary edit fails the named leg:
  - an unknown compensation treated as "did not land" with `may_still_commit: true` → the unknown-compensation leg;
  - the obligation re-read skipped, and its byte comparison made always true → the re-read leg;
  - the chase keeping only the latest obligation → the chase leg;
  - a compensation sent while a commit is held → the held-commit-beside leg;
  - the session check after an unknown compensation made always true → the teardown leg;
  - each doubt-keeping return replaced with a plain stop → the faults and damage leg;
  - the part-key read removed from the own-commit proof → the damage-only leg.
  **C5 (advisory) survivors** are listed in `build-notes.md`. **Review depth:** sign-off needs a confirmed high-effort review of the final patch.
- **Production reach:** (a) The test runs against a real store; production commits no part in this slice. (b) #669 calls this commit, and this
  slice is its last prerequisite among the commit slices. (c) This slice **must not** add a production caller.
- **Citations expected:** Do must cite `path:line` on the target branch for every change.
  - **Start from attempt 7.** Do MAY open `results/issue_927/iteration-v7/patch.diff` as a reference (`compensate`, `commit.rs:1012-1018`, `:1290`).
    Its tests for this slice: `b_a_held_commit_beside_an_unknown_empty_compensation_never_answers_operation_aborted`, `c_an_unknown_compensation_outcome_is_settled_by_a_re_read`,
    `h_a_held_compensation_under_a_teardown_is_in_doubt`, `h_the_obligation_re_read_settles_a_landed_compensation_and_catches_other_bytes`,
    `h_the_epoch_chase_re_reads_every_obligation_it_sent`, `g_a_slot_gone_under_a_moved_session_at_the_compensations_re_read`,
    `h_a_damaged_session_under_a_held_compensation_keeps_the_doubt`, `h_a_store_fault_while_a_held_compensation_is_settled_keeps_the_doubt`, the compensation
    half of `held_damage_at_a_re_read_keeps_the_doubt`, the unknown-outcome leg of `c4_empty_parts_never_install_an_empty_obligation`, and the held-compensation legs of
    `g_a_missing_session_record_is_a_terminal_delete_only_with_the_slot_gone`.
- **Prior-art check (triage cycles):** as 657.2a, on `origin/main` @ `376b106`, 2026-11-01. #927's attempts 1 to 7 are the source.
- **Disposition hint:** new-feature

## Design

**Re-read, in order:** the obligation (when there is one), the attempt's own slot, then the session. The obligation holding the bytes the
compensation put, or the slot gone while the session is still the bytes a doubtful compensation required, means it landed. That second test
is sound **only because no commit of this attempt is outstanding** when it compensates (657.2c): the slot compare-and-set shows that one of this
attempt's slot-releasing batches landed, not which one. For an empty part there is no obligation, so slot and session alone decide. With
`may_still_commit: true` the doubt is kept as for the commit, and a slot read made after the session move is needed before letting it go.

**The damage-only answers.** Add one read to the own-commit proof when the commit required the key **absent**: a landed commit leaves a record
there, and a `part:` record is removed only after the session leaves `Open`, so an absent key disproves it. For a non-empty compensation, a slot gone
with no obligation disproves the landing. In both, keep the doubt. An obligation's drain cannot make this a false success: at worst it costs an
in-doubt answer, which is an allowed availability cost.

## Alternatives considered

- **Leave the damage-only answers as they are.** No legal writer does this, but it is a false success, where the earlier damage findings were
  wrong refusals. Rejected by the human, 2026-11-01.

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a
draft PR MAY happen during the cycle (useful for CI feedback). The PR MUST NOT be
marked ready before sign-off accepts.
<!-- pdca:end child-4 -->
