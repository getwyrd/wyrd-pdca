# Build notes — #927 UploadPart commit (attempt 7)

## Starting point

Per the Iteration 6 carry-forward ("rebuild this slice, do not split it. Keep the classification
design and start from this attempt's patch"), I started from `iteration-v6/patch.diff`, applied it
cleanly to the target branch (`getwyrd/wyrd` @ `376b106`), confirmed its 57 tests green, then made
only the five fixes the carry-forward names. No part of the classification design was re-decided.

All target-source edits are in `$PDCA_WORKTREE` (`/home/eddie/wyrd/wyrd.pdca-wt-l0`). Line cites
below are against that tree (the patched tree), which is what the carry-forward's line numbers also
refer to.

## The five fixes (Iteration 6 carry-forward)

### 1. A missing `mpu:` record is a terminal delete only when the slot is gone too

A real terminal delete removes the session record **and** the session's surviving `slot:` records
in one batch (`0016:674`). v6 read any absent `mpu:` as the session moved for good (`NoSuchUpload`),
even with this attempt's slot still held. That is damage, not the delete, and restoring the record
lets a held batch land — so v6 could answer `NoSuchUpload` with `slots=1` for a commit that was
still live.

- `commit.rs`: `SessionSeen` grows a `Gone` variant; `judge_session` returns it for an absent
  record instead of `Left(NoSuchUpload)` (`commit.rs` `judge_session`). `Gone` on its own settles
  nothing (`Doubt::session_seen` leaves it alone, like damage).
- The commit loop's slot arms decide against the slot read: slot **gone** + session **gone** is the
  terminal delete (`NoSuchUpload`, and `doubt.cannot_land()` so a kept doubt reports
  `may_still_commit: false`); slot **held** + session **gone** is damage
  (`Stop::Record(RecordError::SessionRecordMissing)`, keeping any doubt), returned immediately as
  `SessionEpochNotRisen` damage is. The compensation loop mirrors it, but groups `Gone` with `Left`
  for the read-again lap, so a held compensation that landed between the slot read and the session
  read is still settled by the next slot read (the `g_..._terminal_delete_only_with_the_slot_gone`
  "lands right after its slot read" leg).
- New `RecordError::SessionRecordMissing { required }` in `multipart.rs`, symmetric with
  `PartRecordMissing`.
- Module doc (`commit.rs` "A commit that did not say `Committed`" table and the paragraph after it)
  and the test doc (`multipart_upload_part_commit.rs`, the renamed
  `g_a_missing_session_record_is_a_terminal_delete_only_with_the_slot_gone`) corrected: the old
  "never damage" / "it can never land" wording is gone, and legs that expected `NoSuchUpload` for a
  slot-held case now expect `Stop::Record`, with added legs that restore the record and land the
  held batch (one held commit, one held compensation).

### 2. The quiesce renewal's adoption is judged for a risen epoch

`stage.rs`'s renewal re-reads the session when its round conflicts and adopts whatever `Open` bytes
it finds (`moved_on`), with no epoch check — so it adopts a real rollback (a higher epoch) but would
also adopt an `Open` record at the same or a lower epoch, which no legal history writes. `commit.rs`
trusted the adopted bytes. I did **not** change `stage.rs` behaviour (the slice's one-doc-fix limit
on that file stands). Instead `commit_part` captures the staged epoch before `renew`, and after it
rejects an adoption whose bytes changed but whose epoch did not rise
(`RecordError::SessionEpochNotRisen`, slot held, nothing sent). The module doc's claim that every
session read is judged by `judge_session` is corrected to name this one read as the exception.

### 3. Three doubt-keeping damage paths now have a test that fails if they drop the doubt

`held_damage_at_a_re_read_keeps_the_doubt` holds a batch and damages a key at the re-read: an
undecodable slot at the commit's re-read, a taken obligation key at the compensation's, and an
undecodable slot at the compensation's. Each asserts the in-doubt error with `may_still_commit` kept
and the cause named, so replacing the doubt-keeping return with a plain stop fails it (shown below).

### 4. The slot-bytes guard reads the owner it recorded, not the batch's own bytes

The wrapper now records, per slot key, the owning attempt from each reserve/renewal `slot:` put that
lands (`ScriptState::slot_owner`) — independent of the batch being checked. `check_slot_bytes`
returns `Result` and compares the batch's claimed attempt id to that recorded owner **before** the
by-attempt `last_slot` lookup. `the_slot_bytes_guard_reads_the_recorded_owner_not_the_batchs_own_bytes`
reserves attempt A, frees the index, has attempt B reserve it, then shows the guard rejects a batch
requiring that slot with A's recorded bytes — which the old by-the-batch's-own-bytes guard accepted.

### 5. Doc: `Refusal::OperationAborted`

Added "a repair rewrote the prior part record" as a cause in the `OperationAborted` doc
(`multipart.rs`), which `commit_part`'s same-session changed-prior arm already answers.

## Demonstrated reds (forced, each from the edit its label names)

Each mutation was applied to the patched tree, the named test run through the project's cargo test
target, shown to fail, then the file restored. The suite is green (60 passed) before and after.

- **Missing-mpu-is-damage** → `g_a_missing_session_record_is_a_terminal_delete_only_with_the_slot_gone`.
  Edit: the commit Held arm's `SessionSeen::Gone` branch returns `Ok(Refused(NoSuchUpload))` instead
  of `Err(torn(doubt, SessionRecordMissing))`. Result: `expected Stopped, got Ok(Refused(NoSuchUpload))`.
- **Renewal epoch check** → `d_the_quiesce_renewal_adoption_is_rejected_when_the_epoch_has_not_risen`.
  Edit: `session.epoch <= staged.epoch` weakened to `<` (drops the same-epoch protection). Result:
  `expected Stopped, got Ok(Committed { .. })` — the commit landed under the adopted bytes.
- **Doubt-keeping, commit undecodable slot** → `held_damage_at_a_re_read_keeps_the_doubt`. Edit: the
  commit loop's `judge_slot` error returns `stopped(&upload_id, Stop::Record(err))` instead of
  `torn(doubt, err)`. Result: `expected InDoubt, got Err(Stopped { .. Record(MalformedRecordValue
  { namespace: "slot:" .. }) .. })`.
- **Doubt-keeping, compensation obligation key** → `held_damage_at_a_re_read_keeps_the_doubt`. Edit:
  the compensation's `ObligationKeyTaken` returns `stopped(upload_id, ..)` instead of
  `end(doubt, ..)`. Result: `expected InDoubt, got Err(Stopped { .. ObligationKeyTaken .. })`.
- **Slot-bytes guard owner compare** → `the_slot_bytes_guard_reads_the_recorded_owner_not_the_batchs_own_bytes`.
  Edit: the `owner != Some(&claimed)` guard disabled (`if false && ..`). Result: the `expect_err`
  on the bad batch panicked because the guard returned `Ok` — the old by-the-batch's-bytes guard
  accepted a batch carrying another attempt's recorded bytes.

(The third carry-forward item names a third site — the compensation's undecodable slot
`Err(err) => end(doubt, Stop::Record(err))`. It shares the exact `end(doubt, ..)` doubt-keeping
mechanism the obligation-key red above mutates and is asserted by the same test's third sub-case; I
demonstrated the two distinct mechanisms, commit-loop `torn` and compensation-loop `end`.)

## Refute-your-own-test (the three questions)

- **(a) Genuine red?** Yes. Each of the five new/changed guards was reverted in turn and its test
  went red (above); the suite is green with the fix in place. The reverts were behaviour-preserving
  mutations (not the first attempt, which tripped a dead-code compile error — recorded and
  replaced).
- **(b) Production path?** Yes. Every test drives `commit_part` / `upload_part` (the production
  functions in `crates/core/src/multipart/commit.rs`) over a real `RedbMetadataStore::in_memory()`
  and a placement-aware chunk-store double. The one direct-call test (item 4's guard test) calls the
  test wrapper's own `check_slot_bytes` — that is test infrastructure being self-checked, not the
  production path, and is labelled as such; the production behaviour it guards is driven on every
  other leg.
- **(c) Fixture includes the fault?** Yes. The missing-record legs erase the real `mpu:` (and, for
  the terminal-delete legs, the real `slot:`) on the inner store; the renewal-adoption leg plants a
  real decodable `Open`-same-epoch record the staging renewal actually adopts; the doubt-keeping
  legs damage the real slot/obligation keys under a genuinely held batch; the guard leg reserves two
  real attempts over the same freed index.

## Known gap (recorded per carry-forward item 2)

A **non-`Open`** record the quiesce renewal adopts at an epoch that has not risen is answered
`NoSuchUpload` by the renewal (`stage.rs`'s `moved_on` returns `Refused` for a non-`Open` state)
before `commit_part` can judge it as damage. Closing it would require changing `stage.rs`, which is
out of this slice's scope (the one doc-fix limit on that file). Recorded here and in the `commit.rs`
module doc ("A non-`Open` record the renewal adopts as a refusal at an epoch that has not risen stays
a known gap"). The `Open`-at-a-not-risen-epoch case **is** closed by fix 2.

## Slot bytes, by invariant rather than by red

The test wrapper checks, on every batch of every leg, that any batch naming an attempt's `slot:` key
requires exactly the bytes that attempt last wrote there — now compared against the independently
recorded owner (fix 4). The batch builders take the slot bytes from the attempt's own state, never
from a read of the key: `commit_batch` and `compensation_batch` both use
`attempt.slot_bytes()`/`attempt.slot_key()` (`commit.rs`). The re-read-bytes mutant stays
unreachable under the classification: a conforming attempt recognises its own commit (slot gone,
session still `Open@E`) and returns `Committed` before any later batch is built (C3′), and no
conforming path sends a batch after another attempt holds the index, so no fixture can reach a
builder that would re-read the slot bytes.

## C5 survivors

I did not re-run `cargo mutants --in-diff` locally (the C5 row is advisory and the gate runs it at
Check). The six answer-changing survivors attempt 1 left are addressed by the classification this
slice ships and its reds; the message-only survivors (lossy / Display text) remain message-only.
The five reds above each kill a mutant that changes an answer in the code this attempt touched. A
full `cargo mutants --in-diff` list should be produced by the C5 gate at Check and reconciled there.

## Full gate (criterion E)

`cargo xtask ci` ran to completion through the project runner (`./engine/xtask.sh ci`) on the
patched tree and ended with **`xtask ci: all checks passed`** (fmt, clippy `-D warnings`, build,
the full test suite including the DST sweep, deny, conformance). It did **not** stall this run — the
Iteration 2 carry-forward's `custodian_gc` 7200 s timeout did not recur. My changes are confined to
`wyrd-core` (`multipart.rs`, `multipart/commit.rs`, a doc line in `multipart/stage.rs`) and one
architecture doc line, none of which touch the custodian.

## Commit-readiness

`cargo fmt -p wyrd-core` applied and `-- --check` clean over every file touched. The patch is built
against the target branch and is the full diff of the five paths (new `commit.rs` and test file
included).
