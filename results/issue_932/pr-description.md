## Summary
**User impact:** none yet, by design. Multipart uploads cannot upload parts today: a session
can be created and aborted, but nothing stores a part's bytes. This PR adds that storage step
without exposing it, so no client behaviour changes until the S3 `UploadPart` verb ships with
the commit, Complete and the reaper. What it buys users later is that an interrupted, aborted
or slow part upload can never leave unreclaimable data behind, and routine operator work
(draining a storage server) never fails an upload.

The change: new core functions that store one part's bytes safely and hand the result to the
part commit, plus one shared home for the drain key so core and the custodian spell it the
same way.

This is the first of two slices #926 was split into; the second adds lease renewal during a
long upload. Tracked in #932.

## What to look at
- The new module `crates/core/src/multipart/stage.rs`. Its two entry points are
  `reserve_slot` and `stage_part`. The module doc explains the order of operations and what is
  and is not guaranteed about write deadlines.
- How it is exercised: `cargo test -p wyrd-core --test multipart_upload_part_stage`. The test
  runs real Create/Abort over an in-memory metadata store, a chunk-store fake that enforces
  write deadlines the way the trait requires, and a clock the test moves by hand, so every
  race in it is scripted, not timing-dependent. Each leg has a watchdog.
- One edit to proposal 0016 (`0016:1714-1719`, the ambiguous-reserve rule), authorized by the
  maintainer: the old text let a reserver probe onward after reading another attempt's id,
  which could leave one attempt holding two slots.

## Root cause
No code wrote `slot:` or `sidx:` records, so `UploadPart` had no way to stage bytes. Staging
is where the protocol's safety lives: a fragment written before it is tracked, after the
session left `Open`, or onto a draining server becomes residue nothing can find.

## Fix
- **Slot reserve** (`stage.rs:960-1143`). Reads the slot range and the session, refuses
  `NoSuchUpload` unless the session is `Open@E`, and commits the slot under
  `require(mpu == Open@E)` + `require_absent(slot)`. On a `CommitUnknownResult` it re-reads
  the slot's exact bytes. With `may_still_commit: true` it retries only the **same** index
  with the same attempt id, and refuses `SlowDown` after a bounded number of rounds. A full
  range is `SlowDown { InflightParts }`, never `NoSuchUpload`.
- **Owned intent per chunk** (`stage.rs:1530-1600`). Before any fragment is written, one batch
  requires the session at `Open@E`, `require_absent(desired:dserver:<S>)` for every server
  in the placement, and the slot at its prior bytes; it then installs the owned `sidx:` entry
  and moves the slot lease forward. Placement is planned against the topology minus draining
  servers (`stage.rs:812-830`). A drain that appears just before commit makes the chunk
  re-plan. Every round, retries included, re-checks every lease of the attempt at a fresh
  clock reading and refuses a lapsed one rather than reviving it.
- **Fragment writes** (`stage.rs:1454-1528`). Each put carries `t_auth + W_write` as its
  deadline, taken before the intent committed, never at dispatch. The wait is raced against
  that same instant (`bounded`, `stage.rs:860-888`). Writes still pending at the deadline are
  abandoned, and a store `WriteEffect::Unknown` is never counted as written. Both stop staging
  with an error classed `Indeterminate` (`most_indeterminate`, `stage.rs:694`), not "nothing
  landed".
- **Body reads**. Each read is bounded by a caller-supplied idle limit, measured from the last
  non-empty piece. An answer seen at or past the limit is refused.
- **Hand-off**. `StagedAttempt` (`stage.rs:324`) carries everything the commit needs on its
  own: the fenced session bytes, the slot, each owned entry in chunk order, and one complete
  `ChunkRef` per chunk (the last one with the short remainder), plus length and SHA-256 digest.
- **Request checks**. One validator (`StageRequest::check`, `stage.rs:216`) is shared by both
  entry points, so an invalid request writes nothing.
- **Supporting edits**: the drain key moves to core (`crates/core/src/metadata.rs:88-115`) and
  the custodian reuses it with the same bytes (`crates/custodian/src/desired_state.rs`). A
  crate-private writer constructor `SlotRecord::reserve` (`crates/core/src/multipart.rs:2757`)
  ensures every slot written decodes back unchanged. `encode_chunk` is made crate-visible
  (`crates/core/src/write.rs:137`). There is a new paragraph in
  `docs/design/architecture/05-building-block-view.md:246`.

**Known limit, stated, not solved here.** The write deadline guarantees no store *begins*
publishing after it, but the storage trait allows a publish that started in time to finish
late (`crates/traits/src/lib.rs:1128-1141`). Such a straggler is always on a position a
durable owned entry names, so it stays tracked while that entry lives. An absolute landing
bound (what the orphan grace in 0016 `:1553-1578` assumes) needs a backend-side bound or a
reclaimer that tolerates a late straggler. That needs an owner before the multipart verbs ship.

**Follow-ups found in review, deliberately not in this PR** (low severity, none breaks the
tracked-before-written rule):
- A held intent followed by an Abort seen by the session read: the `NoSuchUpload` branch at
  `stage.rs:1655-1662` has no test that runs it.
- The slot-read adopt path (`stage.rs:1623-1639`) does not re-judge the session, so the comment
  at `:1659-1660` holds on only one of two paths. Either re-check or reword it, then test both.
- The reserve's `doubt.settle()` (`stage.rs:1075`) has no covering test (over-reporting doubt
  is the safe direction).
- `max_part_chunks: 0` passes `StageRequest::check`; it should be an invalid request.
- An over-limit body returns `StageOutcome::Staged` with `chunk_bound_exceeded` set. That is
  an API-shape question for the part-commit slice (#927), and the doc at `stage.rs:267`
  overstates it.

## Verification
- **Claim:** every staged fragment is named by a durable owned entry, with its placement,
  before it is written.
  **Checked:** `stage.rs:1582-1594` (intent batch) and the hand-off in `stage.rs:324`.
  **Test:** `a_stages_uneven_pieces_in_order_and_hands_off_complete_chunk_refs`
  (`crates/core/tests/multipart_upload_part_stage.rs:1403`) asserts every chunk's `sidx:`
  commit precedes its first fragment put, and that the `ChunkRef`s sum to the body.
- **Claim:** no owned entry once the session has left `Open`, or naming a draining server.
  **Checked:** `stage.rs:1583-1589`.
  **Test:** `e_prime_1_…` / `e_prime_2_…` (`:3935`, `:3982`, a real Abort just before the intent
  commits); the drain legs `b1_…` (`:2371`), `b2_…` (`:2424`) and `b3_…` (`:2460-2519`). Removing only the session `require`, or only
  the per-server `require_absent`, turns them red.
- **Claim:** an `Open` session is never refused `NoSuchUpload`, and no attempt holds two slots.
  **Checked:** `stage.rs:983-1143`.
  **Test:** `c_stage_part_with_every_slot_held_answers_slow_down` (`:2631`),
  `c_an_ambiguous_reserve_retries_the_same_index` (`:2819`),
  `c_a_held_reserve_landing_after_its_refusal_is_one_counted_slot` (`:2870`).
- **Claim:** the write deadline is exact, and an unverified write is never counted either way.
  **Checked:** `stage.rs:1454-1528`, `:694`.
  **Test:** `a_prime_every_fragment_deadline_is_t_auth_plus_w` (`:1556`),
  `a_prime_a_held_put_released_after_expiry_is_refused_and_not_published` (`:1726`),
  `a_prime_a_publication_straddling_the_deadline_is_never_counted` (`:1859`).
- **Claim:** no wait on outside work is unbounded; empty pieces are not progress.
  **Checked:** `stage.rs:860-901`.
  **Test:** `a_double_prime_*` (`:2073-2335`).
- **Claim:** unknown outcomes are settled by re-read, and every loop is bounded.
  **Test:** the `e_*` legs (`:3401-3838`, including store reads that fault while a batch is in
  doubt) and `f_*` (`:4007-4057`, exact commit counts under constant `Conflict`).
- **Red/green:** the staging functions do not exist on `main`, so the test cannot compile
  there; there is no pre-fix run. In its place, 49 hand-made mutants of the production code,
  each named against the leg it must break, all failed their leg. `cargo mutants --in-diff`
  gave 144 mutants: 111 caught, 33 unviable (missing `Default` impls), 0 missed. The file
  passed 40+ repeated runs with no flakes.
- **Whole gate:** `cargo xtask ci` passes on the patched tree, including clippy, the full test
  suite, `cargo deny`, the docs lint/render, and the DST tier.

Fixes #932
