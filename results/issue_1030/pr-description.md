## Summary
**User impact:** when the storage backend times out while a part upload is being
cleaned up after a refusal (part too large, another upload of the same part
number won, or the upload rolled back), the client gets an "outcome unknown"
error it cannot act on, even when the cleanup actually finished. In one
damage-only case the opposite happens: a part upload is reported as saved when
no part was saved.

This PR makes the part upload check the store after a timed-out cleanup and
return the normal refusal when the store shows the cleanup landed, and stops
reporting a part as saved when the store shows it was not. A cleanup that
timed out is never sent a second time.

This is the last of the part-commit changes before the S3 `UploadPart` call is
wired to it. **It is stacked on #1060 (#1029) and should be reviewed and merged
after it**; until then the diff against `main` includes #1060's commits.

## What to look at
- The new function that settles a timed-out cleanup by reading the store, and
  the one extra read added to the check that decides "the part was saved". Both
  are in `crates/core/src/multipart/commit.rs`.
- The rule it follows: a cleanup counts as landed only on proof no other write
  can produce. A slot still held, an empty obligation key, or elapsed time never
  count. Without proof, the answer stays "outcome unknown".
- To try it: run `cargo test -p wyrd-core --test multipart_upload_part_commit`.
  The tests drive the real commit code over a real redb store, with a wrapper
  that makes chosen writes answer "unknown", holds writes and lands them later,
  and injects rivals, aborts, rollbacks, read faults and damaged keys between
  the reads.

## Root cause
The compensation's `CommitUnknownResult` arm stopped with
`InDoubt { batch: Compensation }` straight away and read nothing (marked
`// deferred: #1030` on the base), so a compensation that landed was never
recognised. Separately, the commit's own-commit proof answered `Committed` on
"slot gone, then a later session read still `Open@E`", whatever the part key
held, so a first upload whose slot key was erased by damage was answered
`Committed` with no `part:` record.

## Fix
- **Settling a compensation in doubt.** `compensate` sends a
  `CommitUnknownResult` to a new `settle_compensation`, which mirrors the
  commit's `settle`: laps of reads (obligation key for a part with chunks,
  slot, session), each read folded into the `Doubt` as it returns, then the
  obligation once more. It returns `Compensated::Released`, so each route keeps
  the landed answer it already gives (`Refused(OperationAborted)`,
  `Refused(EntityTooLarge)`, or a stop with `slot_released: true`), only on:
  - a part with chunks: its own obligation at exactly the bytes that round put,
    keyed under the epoch that round required (after an epoch chase, the newer
    one);
  - an empty part: slot gone and a later session read unchanged. A part with
    chunks never settles on the slot alone, since damage can erase a slot too.

  Otherwise it keeps the doubt with a cause. `may_still_commit` falls only when
  the session moved to a higher epoch, or the session record is gone and a slot
  read after that finds the slot gone (the terminal delete). Damage, read
  faults and other bytes at the obligation key keep the flag. The old `failed`
  helper is removed; `Doubt` now carries which batch is in doubt, and
  `gone_in_doubt` is shared by both settles.
- **Own-commit proof.** When the commit required the part key absent and the
  lap's slot read found the slot gone, the lap reads the part key between that
  slot read and the last session read. Absent → in doubt (`SlotLost`);
  undecodable → in doubt (`Record`), flag kept; a record → `Committed`. The
  read is made only in that case, so the trace of other laps and the gap
  numbering in existing tests do not move.
- **Documented limits**, each reached only through damage: a re-upload whose
  slot was erased; a same-number rival's record behind an erased slot (it reads
  exactly like this commit landing and then being replaced by the rival's
  re-upload; no chunk is orphaned, and Complete checks the digest against the
  record that stands); and an empty part's compensation whose slot was erased.
- **Docs:** the `commit.rs` module doc and item docs, and
  `docs/design/architecture/05-building-block-view.md:250`.

No public type, variant or field changes. One `Display` text changed: `Stop::SessionMoved`
now says "the batch" instead of "the commit", since a compensation can now have
it as its cause. No record format, dependency or production-caller changes.

## Verification
Line numbers are on this PR's branch (stacked on #1060).

- **Claim:** a compensation answered unknown is answered as landed only on its
  own evidence, and is never sent again.
  - **Checked:** `crates/core/src/multipart/commit.rs:1349-1356` (the unknown
    arm hands off to the settle, no resend); `:1268-1320` (`settle_compensation`:
    obligation proof via `Doubt::owns` `:947-959`; empty-part proof `:1298`;
    slot gone under a moved session never settles `:1306`).
  - **Tests:** `s_a_compensation_answered_unknown_is_settled_from_reads`
    (test file `:6110`, every refusing route × both flags × with/without
    chunks, plus held/released histories);
    `x3_late_staging_that_rebuilds_a_compensated_slot_never_sends_it_again`
    (`:6255`); the reworked
    `h4_a_compensation_answered_unknown_sends_no_further_batch` (`:2774`).
    Every leg asserts no batch after the one answered unknown and ends with the
    check that every chunk is named by a part, an owned entry or an obligation.
- **Claim:** the obligation is the witness, under the round's epoch; other bytes
  there never count and do not lower the flag.
  - **Tests:** the three `o_…` tests (`:6344`, `:6375`, `:6424`); `d3_…`
    (`:7020`): a part with chunks whose slot was erased by damage is not
    answered as landed.
- **Claim:** a moved, deleted or missing session is judged as the commit's
  settle judges it.
  - **Checked:** `commit.rs:968` (`gone_in_doubt`, shared).
  - **Tests:** the three new `t_…` tests (`:6498`, `:6535`, `:6589`).
- **Claim:** faults and damage keep the doubt with the fault as cause.
  - **Tests:** `f_a_fault_or_damage_under_a_held_compensation_keeps_the_doubt`
    (`:6665`).
- **Claim:** `Committed` is never answered when a read after the slot read
  finds the required-absent part key still absent or undecodable, and a commit
  landing before the slot read is still `Committed`.
  - **Checked:** `commit.rs:1028-1052` (the part read and its three outcomes),
    `:1060-1067` (the answer).
  - **Tests:** `d1_a_slot_erased_under_an_unlanded_commit_is_never_committed`
    (`:6813`), `d1_torn_bytes_at_the_part_key_under_an_unlanded_commit_are_never_committed`
    (`:6876`), `d2_a_commit_landing_after_the_laps_part_read_is_committed`
    (`:6955`), and the two `k_…` tests (`:7071`, `:7154`) giving the new read
    the same rival / rollback / fault coverage as the reads around it.
- **Red before, green after:** on #1060's head, 21 of the file's 92 tests fail
  (the compensation legs on the unread in-doubt answer, `d1_…` on a false
  `Committed`, and the existing exact read traces that now include the new
  read). With this patch, all 92 pass. Each wrong fix the design rules out
  (resending, trusting a held slot, the slot-only proof for a part with
  chunks, the wrong epoch, lowering the flag on damage, reading the part key
  too early, and others) was applied as a temporary edit and turns its named
  test red.
- **Whole gate:** `cargo xtask ci` passes on the patched tree.

Fixes #1030
