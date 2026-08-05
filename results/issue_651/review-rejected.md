# review-rejected.md — issue 651 (re-scoped 2026-08-02, slice 4a of 7; iteration 12)

Machine-readable triage decisions for `scripts/review-branch` (T4-batch-review). Each
non-comment line is `<file:line> | <CLASS> | <MATCH> | <reason>`, where MATCH is a phrase
from the finding's own rationale (case-insensitive substring). Everything **not** listed
here was FIXED in this iteration and should leave the next run on its own.

> **Reading the history below:** the round-6…9 notes describe the *cross-object chunk-id
> ambiguity* apparatus, which the 2026-08-03 re-scope **dropped** (`brief.md` § Out of scope).
> Their `restore.rs` / `segmented_map_restore.rs` line references point at code that is no
> longer in the patch; they are kept so an old finding's disposition is not read as an
> oversight, not as a map of the current tree. The rows the gate actually matches on are the
> two sections at the bottom, re-filed against this iteration's line numbers.

> **The pre-split file is archived at `iteration-v5/review-rejected.md`.** Its entries are
> keyed to `resolve.rs`, `backfill.rs`, `reconstruction.rs`, `rebalance.rs` and
> `core/src/metadata.rs` — files this re-scoped slice does not touch at all (they are
> **#681** / **#682**), so those lines can no longer match anything here. The three standing
> rejections from `brief.md` § Do-not-re-earn are restated below at the lines of THIS patch
> where each finding can re-land.
>
> **Round-6 findings (`review-batch.md`) were all FIXED, none rejected:** the two
> `Expected`-keyed-by-`ChunkId` BUGs (the report half no longer regroups the aggregate
> reference set — it walks the records and judges one entry per committed *reference*,
> `crates/custodian/src/restore.rs:600`), and the CLI TEST-GAP (the verdict is now a value
> the command prints and exits on, `crates/server/src/cli.rs:1262`, pinned from
> `crates/server/src/cli.rs:2693`).
>
> **Round-7's one blocking finding was FIXED, not rejected, and is deliberately absent from
> this file:** v7 asserted `report.dangling.is_empty()` on the ground that "the bytes are one
> hop away", which codified the cross-object conflation. It is **inverted**
> (`crates/custodian/tests/restore_reconcile.rs:960`: under a shared chunk id with the
> reference's own placement empty, the chunk is `dangling`, never `misplaced`).
>
> **Round-8's four findings were ALL FIXED, none rejected** — recorded here only so their
> absence is not read as an oversight:
> 1. *The ambiguity rule keyed on how many D SERVERS are named, not on how many committed
>    REFERENCES exist* — rebuilt. It is now `CommittedChunks::ambiguous`
>    (`crates/custodian/src/restore.rs:562`), a count of committed chunk *references* per id
>    taken in `committed_chunks` (`restore.rs:600`), asked by both halves
>    (`restore.rs:469`, `restore.rs:561`). The holder-keyed `attributable` helper and the
>    `holders.iter().all(..)` mark test are **gone**. Two claimants that name the SAME D
>    server — the shape that defeated round 8, and the common one — are now covered in the
>    discriminator's own legs.
> 2. *The `chunk_id_minter` collision premise is unsupported by the minting code* —
>    withdrawn and replaced everywhere it was stated. The rule no longer claims ids collide;
>    it states that uniqueness is a property of the ALLOCATOR, that this pass runs over an
>    imported namespace whose allocator state it cannot verify, and that it therefore
>    **observes** rather than assumes (`restore.rs:221`). The runbook's own claim was
>    corrected in the same patch
>    (`docs/design/architecture/m4-first-deployment-blueprint.md:694`: the gateway minter is
>    inode-independent, the CLI cluster minter is not).
> 3. *The CLI DANGLING paragraph asserted a cause the patch concedes is false* — hedged
>    (`crates/server/src/cli.rs:1290`), pinned by
>    `the_dangling_paragraph_does_not_assert_a_cause_it_cannot_know`.
> 4. *`is_clean()` had no production caller, so criterion (2a) was pinned on a predicate
>    nothing reads* — `RestoreReport::needs_human` (`restore.rs:192`) is now the command's
>    exit status (`cli.rs:1262`), `is_clean` is written **in terms of** it (`restore.rs:178`)
>    so the two cannot drift, both are stated on the audit summary (`restore.rs:866`), and
>    the discriminator's justification no longer claims `is_clean` is what the command exits
>    on.

> **Round-9's findings were ALL FIXED, none rejected** — the three `review-batch.md` BUGs were
> one finding seen by three passes, and the adversary's six were separate. Recorded so their
> absence is not read as an oversight:
> 1. *`anywhere = placed` credits one ownerless fragment to every same-placement claimant, so an
>    ambiguous store can be reported healthy* (all three batch findings, plus C5 / T3) — the
>    fallback is **gone**. Under an ambiguous id the attributable count is **zero**
>    (`crates/custodian/src/restore.rs:562`), so every claimant is `dangling` whatever the fleet
>    holds; the discriminator now runs the report leg over three shapes including
>    same-placement-WITH-the-copy-on-it, which was the false-clean
>    (`crates/custodian/tests/segmented_map_restore.rs:730`).
> 2. *An ambiguous id that never becomes `dangling` is silent and the run is certified* — cannot
>    happen now: ambiguity always decides the verdict, so `ambiguous-chunk-id` is emitted for
>    every ambiguous reference (`restore.rs:568`), and the mark half emits its own
>    `ambiguous-mark-withheld` wherever ambiguity changed a decision (`restore.rs:938`).
> 3. *The ambiguity gate sat ABOVE the `already`-marked check, so a mark an earlier run wrote was
>    reported "kept" while GC still deleted it* — the gate moved below it (`restore.rs:469`),
>    `already_marked` is truthful again, and the stale mark is **withdrawn**
>    (`restore.rs:418`), pinned red→green by
>    `a_stale_mark_on_a_shared_chunk_id_is_withdrawn_rather_than_left_to_gc`.
> 4. *`is_clean()` had no true-branch assertion anywhere in the tree* — added on a store the pass
>    read in full and found nothing wrong with, together with `!needs_human()`
>    (`crates/custodian/tests/restore_reconcile.rs:274`).
> 5. *Two unasserted C5 mutants on added lines* — `displaced_kept` on the ambiguity path is now
>    asserted (`segmented_map_restore.rs:868`), and the `withheld` subtraction is gone: the event
>    carries `claims`, `withheld` and `at_placement` as plain counts, asserted from the audit JSON
>    (`segmented_map_restore.rs:800`).
> 6. *The UNREADABLE paragraph asserted a fleet-wide fact the report cannot carry* — the sentence
>    is now DERIVED from `report.stranded_marked` (`crates/server/src/cli.rs:1337`), pinned by
>    `the_unreadable_paragraph_derives_its_marking_claim_from_the_run`.
> 7. *T5 / validation open question: are duplicate committed ids a supported restore state, or
>    #652's?* — answered in the patch rather than deferred. The **reachable** post-restore reuse
>    (a live record vs. a dead file's re-minted ids) has ONE committed claimant and is
>    indistinguishable from displacement, so it stays `misplaced` and both the audit event and the
>    CLI paragraph now qualify the restage they ask for (`restore.rs:851`, `cli.rs:1301`); the
>    duplicate-**committed**-claim state is not produced by the shipped minters and is treated as
>    an observation the pass makes rather than an assumption it inherits (`restore.rs:240`). The
>    runbook says both (`docs/design/architecture/m4-first-deployment-blueprint.md:694`).

> **Round-11's findings (iteration 12, this patch): the blocking one was FIXED, not rejected.**
> *The attribution-order gap* — an object `referenced_fragments` had already found unreadable was
> named only after `orphan_leases` / `pending_chunks` / `committed_chunks` all succeeded, so a
> genuine store fault in any of them ended the pass with an `Err` carrying nothing and the
> operator never learned the record to repair. Each read's blockers are now emitted **the instant
> that read returns** (`crates/custodian/src/restore.rs:261-278`, mirroring `gc.rs:155-165`),
> pinned red→green by
> `a_record_already_known_unreadable_is_named_before_a_later_read_can_fail`
> (`crates/custodian/tests/segmented_map_restore.rs:638`). The one entry below re-files the
> standing timeout rejection at THIS patch's line numbers — the code moved, the rule did not.

## Standing rejections (`brief.md` § Do-not-re-earn), in the gate's format

<!-- Line numbers are re-filed against THIS iteration's patch on every rebuild: a rejection
     binds to (file:line, CLASS) + a MATCH phrase, so a moved line is an unsuppressed finding
     even when the rule behind it is unchanged. Every await in the pass is listed, under both
     classes a reviewer files this class of finding as. -->

<!-- (i) caller-side await timeout — rejected 3x across #508/#636, and re-raised at
     restore.rs:521 in round 11 purely because `committed_chunks` moved. The ChunkStore /
     MetadataStore IMPLEMENTATION owns the network bound, not the caller
     (crates/traits/src/lib.rs:1000-1012); no custodian await carries one, and this slice
     does not start. -->
crates/custodian/src/restore.rs:644 | CONVENTION | timeout | standing rejection (i), #508/#636 x3: the `MetadataStore` implementation owns its own network bound. This is the same `metadata::resolve_chunk_map` await `gc::referenced_fragments` makes over the same records (`gc.rs:402`) with no caller-side bound; adding one would put a runtime dependency in a crate whose seam boundary is traits/core/tracing (ADR-0010). Fail-closed either way: the error propagates or contains the object, never "this object owns no bytes".
crates/custodian/src/restore.rs:644 | BUG | timeout | standing rejection (i), as above.
crates/custodian/src/restore.rs:644 | CONVENTION | bounded | standing rejection (i), as above.
crates/custodian/src/restore.rs:644 | BUG | bounded | standing rejection (i), as above.
crates/custodian/src/restore.rs:625 | CONVENTION | timeout | standing rejection (i), as above — `committed_chunks`'s `inode:` scan is the pass's own pre-existing walk (origin/main `restore.rs:392`), unchanged but for the containment; it adds no store read the base did not already make.
crates/custodian/src/restore.rs:625 | BUG | timeout | standing rejection (i), as above.
crates/custodian/src/restore.rs:299 | CONVENTION | timeout | standing rejection (i), as above — the `committed_chunks` call moved ahead of the fleet walk (it was `restore.rs:326` on origin/main); moving a read does not make its bound the caller's.
crates/custodian/src/restore.rs:299 | BUG | timeout | standing rejection (i), as above.
crates/custodian/src/restore.rs:299 | CONVENTION | bounded | standing rejection (i), as above.
crates/custodian/src/restore.rs:299 | BUG | bounded | standing rejection (i), as above.
crates/custodian/src/restore.rs:283 | CONVENTION | timeout | standing rejection (i), as above; unchanged from origin/main (`restore.rs:200`).
crates/custodian/src/restore.rs:283 | BUG | timeout | standing rejection (i), as above.
crates/custodian/src/restore.rs:293 | CONVENTION | timeout | standing rejection (i), as above; unchanged from origin/main (`restore.rs:201`).
crates/custodian/src/restore.rs:293 | BUG | timeout | standing rejection (i), as above.
crates/custodian/src/restore.rs:294 | CONVENTION | timeout | standing rejection (i), as above; unchanged from origin/main (`restore.rs:202`).
crates/custodian/src/restore.rs:294 | BUG | timeout | standing rejection (i), as above.
crates/custodian/src/desired_state.rs:188 | CONVENTION | timeout | standing rejection (i), as above; this await is unchanged from origin/main and the drain status gains no store read in this slice.
crates/custodian/src/desired_state.rs:188 | BUG | timeout | standing rejection (i), as above.

<!-- (ii) "a genuine store fault should be contained too" — no. A record-level read failure
     is contained (it is that object's own fault); a store fault under the read propagates,
     because a walk that cannot read the metadata store has no reference set at all and
     containing that as "one object is unreadable" would be the wrong answer for every
     object in it. Pinned on the base by #650's
     `a_genuine_store_fault_during_resolve_propagates_rather_than_being_absorbed`, and in
     this patch by `a_record_already_known_unreadable_is_named_before_a_later_read_can_fail`
     (the pass still fails on the injected store fault — it just names what it already knew
     first). -->
crates/custodian/src/restore.rs:644 | BUG | contain | standing rejection (ii): only a RECORD-level read failure is contained (the `ChunkMapError` downcast, the same rule `gc::referenced_fragments` applies at `gc.rs:405-415`); a store fault still propagates. Containing a store outage as "one object is unreadable" would report every healthy object as read when none was.
crates/custodian/src/restore.rs:644 | CONVENTION | contain | standing rejection (ii), as above.
crates/custodian/src/desired_state.rs:188 | BUG | contain | standing rejection (ii), as above — a store fault must still blank this query, since nothing about ANY server can be said when the store cannot be read; only a named record's own fault is contained (`PendingUnresolvable`).

<!-- (iii) "`Completed` releases its admission slot" — withdrawn as unsatisfiable. Recorded
     only so the counters this slice adds are not re-litigated as one: they are plain
     monotonic observation counters, with no session/tombstone concept anywhere near them. -->
crates/custodian/src/restore.rs:827 | CONVENTION | admission slot | standing rejection (iii), withdrawn as unsatisfiable. `restore_unresolvable_records` is a monotonic counter of records this pass could not read — no admission slot, session or tombstone exists in this surface.
crates/custodian/src/desired_state.rs:260 | CONVENTION | admission slot | standing rejection (iii), as above. `drain_unresolvable_records` counts observations (one per blocking record per status read), documented as such at the emitter.

## Scope declines (`brief.md` § Scope — out of scope)

<!-- Findings that are real but belong to a named sibling slice get a decline-with-issue
     reference per AGENTS.md's reviewer protocol ("Out of scope"), not an in-PR fix. -->
crates/custodian/src/restore.rs:0 | BUG | repoint | out of scope — **#682** owns `repoint_chunk`, the record ceilings and the repair/evacuation write path. This slice writes NOTHING to a chunk map; it only reads.
crates/custodian/src/restore.rs:616 | BUG | reconstruction | out of scope — **#681** owns the resolving namespace walk shared by reconstruction / backfill / rebalance, and unifying it with this pass's own scan is that slice's (marked `deferred: #681` at `restore.rs:616`). This slice adds no `crate::resolve` module and no new walk: it upgrades the walk origin/main already had at `restore.rs:390` in place.
crates/custodian/src/desired_state.rs:0 | BUG | evacuat | out of scope — rebalance's evacuation behaviour over an unreadable record is **#681**'s. This slice changes only what the drain STATUS reports; `rebalance.rs` is untouched.
crates/server/src/cli.rs:0 | BUG | chunk-id floor | out of scope — **#652** owns the chunk-id allocator floor that narrows post-restore id reuse. This slice does not change how ids are minted, and carries no cross-object chunk-id rule at all: `brief.md` § Out of scope DROPPED that apparatus on 2026-08-03 (the gateway mints ids >= 2^127, `crates/server/src/lib.rs:238-241`, so two live records cannot collide).
crates/custodian/src/restore.rs:0 | BUG | owner attribution | out of scope — naming the OWNING `inode:` key on the `dangling` / `misplaced` audit events is declined by `brief.md` § Scope explicitly ("No report-schema churn"): threading the owner through `Expected` / `emit_dangling` / `emit_misplaced` widens the report schema for a gap that existed only under the cross-object rule this brief dropped. The blockers this slice DOES add are named by `inode:` key, in the report, in the CLI paragraph and on the audit seam.

## Round-13 advisory findings against the CLI's PRE-EXISTING paragraphs (declined)

<!-- (iv) "the `dangling` / `misplaced` NEEDS-HUMAN paragraphs print only a COUNT and send the
     operator to the audit log, while this slice promises to NAME blockers" — declined, on two
     independent grounds, and directed by the human at the iteration-13 sign-off ("NOT defects in
     this patch … Record-reject these rather than rebuilding the CLI output shape").

     1. The text is VERBATIM pre-existing on the base: `git show origin/main:crates/server/src/cli.rs`
        carries the same two paragraphs at `:1213-1215` and `:1221-1226`, including "See the audit
        log for each chunk id". This patch preserved that wording and inserted its own UNREADABLE
        paragraph beside it; it is not a behaviour this slice introduced.
     2. Widening them is `brief.md` § Scope's explicit "No report-schema churn": `dangling` /
        `misplaced` keep their `Vec<ChunkId>` shape, and the naming promise this slice makes is
        about the records it could not READ (`RestoreReport::unresolvable`), which the command DOES
        name in full (`crates/server/src/cli.rs:1322`, bounded and counted at `:1351`).

     A widening of the base's own loss paragraphs is a real (if small) improvement and belongs on
     its own issue against the CLI's report surface — not in a slice whose scope is the
     completeness surfaces. -->
crates/server/src/cli.rs:1282 | BUG | audit log | declined (iv): verbatim base text (`origin/main:cli.rs:1213-1215`); naming chunk ids here is report-schema churn `brief.md` § Scope declines.
crates/server/src/cli.rs:1282 | CONVENTION | audit log | declined (iv), as above.
crates/server/src/cli.rs:1284 | BUG | audit log | declined (iv), as above.
crates/server/src/cli.rs:1284 | CONVENTION | audit log | declined (iv), as above.
crates/server/src/cli.rs:1290 | BUG | audit log | declined (iv): verbatim base text (`origin/main:cli.rs:1221-1226`), same decline.
crates/server/src/cli.rs:1290 | CONVENTION | audit log | declined (iv), as above.
crates/server/src/cli.rs:1292 | BUG | audit log | declined (iv), as above.
crates/server/src/cli.rs:1292 | CONVENTION | audit log | declined (iv), as above.
crates/server/src/cli.rs:1282 | BUG | chunk id | declined (iv), as above.
crates/server/src/cli.rs:1290 | BUG | chunk id | declined (iv), as above.
crates/server/src/cli.rs:2720 | TEST-GAP | audit log | declined (iv): the test pins the verdict/exit-status agreement, not the base paragraphs' wording; changing that wording is the declined widening.
crates/server/src/cli.rs:2755 | TEST-GAP | audit log | declined (iv), as above.
