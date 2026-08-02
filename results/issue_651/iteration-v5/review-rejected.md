# review-rejected.md — issue 651

Machine-readable triage decisions for `scripts/review-branch` (T4-batch-review). Each
non-comment line is `<file:line> | <CLASS> | <MATCH> | <reason>`, where MATCH is a phrase
from the finding's own rationale (case-insensitive substring). Everything **not** listed
here was FIXED in this iteration and should leave the next run on its own.

## Fixed in iteration 4 (recorded here only so the human can see the disposition)

All **eleven** blockers from iteration 3's batch review are FIXED, not rejected — each with
a test that goes red when only that fix is reverted (`build-notes.md` §3):

- `resolve.rs:97` / `:132` (five of the eleven, one finding seen three ways) — a committed
  record under a non-canonical `inode:` key is now **contained and named**
  (`crates/custodian/src/resolve.rs:111-120`), never silently skipped, and the shared
  parser is canonical-only (`crates/core/src/metadata.rs:2158-2173`).
- `backfill.rs:126` — a segmented record is charged to `unassessed` **only when it holds a
  placement this pass would have filled** (`crates/custodian/src/backfill.rs:146-151`), so a
  fully-placed segmented store certifies instead of blocking forever.
- `reconstruction.rs:387` — a duplicate committed chunk id is recorded as **ambiguous** and
  repaired by neither reference (`crates/custodian/src/reconstruction.rs:397-412`,
  `:434-441`, `:537-539`); the shared obligation stays queued and both objects are named.
- `rebalance.rs:121` — the no-drains fast path now **samples the level at zero**
  (`crates/custodian/src/rebalance.rs:123-131`).
- `metadata.rs:3048` (twice, BUG + CONVENTION) — both arms of the repoint pin the **exact
  bytes the resolve read** (`crates/core/src/metadata.rs:3049`, `:3118`), through a
  `RootGeneration` that can only be built by decoding them (`:2815-2853`).
- T3 (the reviewer's runtime cell, not in the batch list) — backfill walks the namespace
  **once** per pass; the population gauge is derived from that walk
  (`crates/custodian/src/backfill.rs:77-90`, `:242-248`).

## Standing rejections (`brief.md` § Do-not-re-earn), in the gate's format

<!-- (iv) backfill skipping a segmented record is the settled answer, not a coverage gap.
     Recorded at the skip, at the emitter, and at the gauge, because the finding re-lands
     wherever that branch is. The "silent success" half of the finding was NOT rejected —
     it is FIXED: the decline is stated on the audit seam with the record's name and the
     count it declined to fill, counted on `backfill_records_unassessed`, and the pass
     returns `Reconciled::Blocked`. What is rejected is only "make backfill resolve and
     rewrite a segmented record". -->
crates/custodian/src/backfill.rs:146 | CONVENTION | segmented | settled in the slice issue's body (brief.md Prior-art-check (iv)): backfill's binding commit rewrites the whole chunk map, and a segmented chunk's ChunkRef is not in the inode — rewriting one it never read would retire every segment record behind a root that no longer names them. Declining with a stated reason is the accepted answer; the decline is attributed, counted, and makes the pass report Blocked rather than success.
crates/custodian/src/backfill.rs:147 | CONVENTION | segmented | as above — the accepted disposition, not a gap.
crates/custodian/src/backfill.rs:148 | BUG | segmented | as above; the honesty half is fixed — the decline is counted on `backfill_records_unassessed` and the pass returns Blocked, so a zero can no longer be read as a drained population.
crates/custodian/src/backfill.rs:277 | CONVENTION | segmented | the emitter for the same settled decline (iv).

<!-- (i) caller-side fan-out timeout — rejected 3x across #508/#636. The ChunkStore /
     MetadataStore IMPLEMENTATION owns the network bound, not the caller
     (crates/traits/src/lib.rs:1000-1012); no custodian loop in this repo wraps one, and
     this slice does not start. Applies to every store await this patch adds. -->
crates/custodian/src/resolve.rs:90 | BUG | timeout | standing rejection (i), #508/#636 x3: the store implementation owns its own network bound; no custodian await carries a caller-side timeout, and adding one would put a runtime dependency in a crate whose seam boundary is traits/core/tracing (ADR-0010).
crates/custodian/src/resolve.rs:128 | BUG | timeout | standing rejection (i), as above.
crates/custodian/src/reconstruction.rs:0 | BUG | timeout | standing rejection (i), as above.
crates/custodian/src/rebalance.rs:0 | BUG | timeout | standing rejection (i), as above.
crates/core/src/metadata.rs:0 | BUG | timeout | standing rejection (i), as above — the homed resolver's reads are bounded by the store seam, exactly as `resolve_chunk_map`'s already are.

<!-- (ii) retraction of already-published bytes — rejected 4x in #638. A repoint that LOSES
     its compare-and-swap does not delete the destination bytes it wrote; they are
     collectable garbage on GC's own terms, exactly as on origin/main. Note the REFUSAL path
     writes nothing at all, so this applies only to a lost CAS. -->
crates/custodian/src/reconstruction.rs:0 | BUG | roll back | standing rejection (ii), #638 x4: a losing repoint never retracts the bytes it wrote. Unchanged from origin/main; the refusal path, which this slice adds, writes nothing to retract.
crates/custodian/src/rebalance.rs:0 | BUG | delete the newly-written | standing rejection (ii), as above.

<!-- The destination pre-mark and the destination drain fence are DEFERRED, not missing:
     `deferred: #653` is stated in repoint_chunk's own doc. Per AGENTS.md's reviewer
     protocol a deferral answered with a tracking issue is settled for review purposes.
     They close nothing without their other half (the retirement drain re-reading each
     segment's current placement, 0016:2416-2430), they would change the FLAT path too
     (both shapes go through the one builder), and iteration 2 shipped them and earned 14
     findings for it. -->
crates/core/src/metadata.rs:3007 | BUG | pre-mark | deferred: #653 (stated in-code at metadata.rs:3007). The pre-mark's counterpart is the retirement drain of 0016:2416-2430, which is #653's; half of it here writes evidence no drain consumes. This slice instead removes the exposure: the callers write nothing before the batch is built.
crates/core/src/metadata.rs:3007 | BUG | drain fence | deferred: #653, as above — `require_absent(desired:dserver:<destination>)` has no counterpart on this base, and origin/main's flat evacuation CAS carries no such precondition either; adding it in the shared builder would silently change the flat path.

<!-- (iii) "`Completed` releases its admission slot" — withdrawn as unsatisfiable; a
     Completed tombstone stays counted. This slice adds no admission gauge or session
     tombstone (the committer/session state is #653's), so it is recorded only so the new
     level gauges are not re-litigated as one. -->
crates/custodian/src/reconstruction.rs:0 | CONVENTION | admission slot | standing rejection (iii), withdrawn as unsatisfiable. The gauges this slice adds (`reconstruction_unassessable`, `rebalance_unresolvable_records`, `backfill_records_unassessed`) are plain rise/return-to-zero levels over unassessed records, carrying no tombstone concept.

## Iteration 5 — dispositions for iteration 4's eight blockers

**Fixed (six of the eight; they should leave the next run on their own):**

- `crates/core/src/metadata.rs:2883` **BUG**, reported three times (the initial
  `RootGeneration` was never checked for `Committed`) — FIXED at the entry of
  `resolve_chunk_homes` (`crates/core/src/metadata.rs:2830-2841`): a generation that is not
  committed answers `Ok(None)` whatever shape its map is, so the contract is enforced where
  it is stated rather than by each caller's own scan filter. Bound by
  `crates/core/tests/segmented_map_resolution.rs:1012-1064`
  (`resolving_homes_refuses_a_generation_that_is_not_committed`, flat **and** segmented).
- `crates/custodian/src/reconstruction.rs:157` **BUG** (an empty repair queue bypassed the
  walk and certified) — FIXED: the walk now runs first and the idle path returns
  `Reconciled::Blocked` while any committed object is unreadable
  (`crates/custodian/src/reconstruction.rs:150-183`). Bound by
  `crates/custodian/tests/segmented_map_repair.rs:1461-1525`
  (`an_idle_repair_pass_over_an_unreadable_object_still_refuses_to_certify`).
- `crates/core/src/metadata.rs:4311` **TEST-GAP** (the segment CAS was never tested against
  non-canonical stored segment bytes) — FIXED: the identity test now respells the `seg:`
  record too and asserts both preconditions plus the written record
  (`crates/core/src/metadata.rs:4266-4310`).
- `crates/custodian/src/restore.rs:340` **TEST-GAP** (no test drove an unresolvable object
  through post-restore reconciliation) — FIXED: the criterion (1) leg now seeds one and
  asserts it is named, `is_clean` is false, nothing is marked, and a readable object's loss
  is still reported (`crates/custodian/tests/segmented_map_repair.rs:558-608`).

**Rejected, with the reason (the gate's format):**

<!-- Preferring a readable reference while some other object is unresolvable. The two
     invariants this slice restores pull opposite ways here, and the brief settles which
     governs: "a repair obligation is never retired on an INCOMPLETE READING" forbids
     concluding "this chunk belongs to no object" (that path already stays queued —
     `Found::Unassessable`), while "containment is per object, and the walk goes on — a
     queued repair for a healthy chunk is still assessed" forbids stopping every repair in
     the fleet because one record is damaged. A repair that completes retires its
     obligation because the redundancy was REBUILT, not because the pass concluded the
     chunk was unreferenced; and the pass reports `Blocked` for as long as any object is
     unreadable, so nothing is certified either way. -->
crates/custodian/src/reconstruction.rs:464 | BUG | unresolvable | rejected: the obligation is retired by the repair COMPLETING (redundancy restored at the placement the readable reference names), never by concluding the chunk is referenced by nothing — that path is `Found::Unassessable` and stays queued (`reconstruction.rs:468-470`). Refusing every repair while any object is unreadable is the starvation the brief's own containment invariant forbids ("a damaged object does not starve the healthy ones; a queued repair for a healthy chunk is still assessed"), and the pass already reports `Reconciled::Blocked` while `unresolvable` is non-empty, so no certification rests on the incomplete reading. A duplicate reference in a map the pass CAN read is a different case and is refused (`Location::Ambiguous`, `reconstruction.rs:467`).
crates/custodian/src/reconstruction.rs:540 | BUG | unresolvable | as above — the consumer of the same preference; the repair path retires an obligation only on a successful repoint, and the pass is non-certifying while any object is unreadable.
crates/custodian/src/reconstruction.rs:454 | BUG | unresolvable | as above, recorded at the line iteration 4's batch reported it (the `Location::At` arm), since the finding binds to the branch rather than to a line.
