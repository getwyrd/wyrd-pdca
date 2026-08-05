# build-notes — issue 651, iteration 10 (withheld from the reviewer)

Base: `origin/main` = `d50f0ca` (verified: `git -C $PDCA_WORKTREE rev-parse origin/main`).
Started from `iteration-v9/patch.diff` as the brief instructs (applied to the worktree, then
changed in place) — **not** rebuilt from scratch. Everything below is the delta from v9 plus the
evidence for it.

## 1. What iteration 9 was returned for, and what each item became

The sign-off's carry-forward listed nine items. All nine are closed **in the patch**; none is
deferred, and none of the rejected approaches is re-submitted.

### (1) THE CORE GAP — `anywhere = placed` credited one ownerless fragment to both same-ID claimants

This was the C5 row, T3, and all three T4 batch findings (one finding seen by three passes).

v9 answered *"under ambiguity, fall back to what sits at this reference's own placement"*. That is
exactly wrong in the common shape: two claimants that name the **same** D server are answered by the
**same physical fragment**, so both counted it, both came out healthy, `is_clean()` was true and the
command exited 0 over a store where at most one of the two objects can be read.

The fallback is **gone**. Under an ambiguous id the attributable count is **zero**
(`crates/custodian/src/restore.rs:561-562` — `let attributable = if ambiguous { 0 } else {
by_id_alone }`), so the only verdict a claimant can be given is loss:
`restore.rs:565` `if ambiguous || attributable < k`. Rationale for zero rather than any weaker
number: bytes bearing an id two records claim are, in C-1's own words, neither *"protected by a
record that names them"* nor *"evidenced for reclamation"* — a fragment at claimant A's placement is
exactly as consistent with *"B's bytes, displaced onto A's server"* as with *"A's"*, and there is
nothing on a fragment that names its generation. Crediting it certifies an object nobody can be
shown to have the bytes of. This is `gc::ReferenceSet::protection`'s trade one level down
(`gc.rs:306-334`): a leak plus a loud report, never a deletion, never a hollow green.

Considered and rejected: **"count only fragments at placements no other claimant names"** (i.e.
attribute A's fragment to A when B names a different server). It gives a prettier report — one
claimant lost instead of two — but it is unsound in precisely the case the rule exists for: a
rebalance can have moved B's fragment onto A's server, and then A is the lost one and the report
certifies it. Cost of the rejected variant: ~6 lines *less* code (a set of contested servers
instead of the constant `0`), and one wrong all-clear per collision. Not bought.

### (2) The reachable collision path (T5 + the adversary's [human] finding) — RESOLVED, not deferred

The question the human left open: *"either name the actual reachable collision path, or fold this
concern into #652"*. I read the minters (the brief cites them) rather than re-asserting v9's claim:

* `crates/server/src/cli.rs:1798` `chunk_id_minter` — `(inode << 64) | seq`.
* `crates/server/src/cli.rs:1773` `seed_next_inode_floor` — a gateway raises `meta:next_inode` above
  every inode already committed at startup, so **two live records can never share an inode**, hence
  never a chunk id.
* `crates/server/src/lib.rs:238` `Gateway::mint_chunk_id` — per-process random epoch, ADR-0019.
* `crates/core/src/metadata.rs:1615` `rename` — one dirent mutation, *"the inode is untouched"*, so
  no second `inode:` record is ever created for one object. `CopyObject` is 501 in the S3 gateway.

So the adversary is right: **the shipped writers do not produce two committed claimants of one id.**
What a restore *does* reach is the mirror image — the rewound allocator re-mints the ids of a file
created after the restore point, a file whose record the restore **erased**. That leaves ONE
committed claimant plus a dead file's leftover bytes, and those bytes are indistinguishable, in the
committed namespace, from a fragment a repair moved (same id, same inode, no generation on the
fragment). The answer, therefore, is both halves of the question, and both are in the patch:

* The **duplicate-committed-claim** rule stays, framed honestly as an **observation** rather than a
  claim about reachability (`restore.rs:240-266`): this is the one pass whose job is to audit a
  namespace it did not mint, the check is free (the walk already reads one entry per *reference*),
  and it is inert on every store the shipped allocators produced — which criterion (4c) pins.
* The **reachable** reuse is declared as something this pass *cannot see*, in a section of its own
  (`restore.rs:268-281` "What this rule cannot see, and #652 owns"), and the verdict it produces now
  says so where an operator acts on it: `emit_misplaced` (`restore.rs:851`) and the CLI's MISPLACED
  paragraph (`crates/server/src/cli.rs:1301-1312`) both tell the operator to confirm a fragment is
  the object's own before restaging, and name #652. The runbook's claim — which v9 got wrong — is
  corrected at `docs/design/architecture/m4-first-deployment-blueprint.md:694`.

Rejected: **widening the oracle to catch the reachable case.** The only signal is "a fragment bearing
this id sits somewhere no placement names", which is *definitionally* the displaced case criterion
(4c) requires to stay `misplaced`. Widening it means `anywhere = placed` always, i.e. deleting
`present_anywhere` (2 lines removed, `restore.rs:552-556`) — and then every stale placement in a
restored fleet reports as **data loss**, which is the "conservative everywhere" failure the brief
forecloses and the worst thing this command can say. The cause is removable only at the allocator:
#652.

### (3) Ambiguity was silent unless it produced `dangling`

Now impossible: ambiguity **always** decides the report verdict, so `ambiguous-chunk-id` fires for
every ambiguous reference (`restore.rs:568`), and the mark half has its own event
(`ambiguous-mark-withheld`, `restore.rs:938`) emitted at each of the three places the ambiguity
changed a decision — the displaced arm (`restore.rs:405`), the withdrawal (`restore.rs:439`) and the
gate itself (`restore.rs:471`). The `emit_displaced` text (*"the placement is stale, not the data;
repair repoints it"*) is no longer used for an ambiguous copy — it was the wrong instruction.

### (4) The gate sat above the `already`-marked check — and the stale mark itself

Moved below it, as instructed (`restore.rs:418` → `restore.rs:469`), so `already_marked` is truthful
again and matches `origin/main`'s honest `1`.

I also **withdrew** the stale mark (`restore.rs:435-440`), which the adversary left as a decision.
Reasoning, since this is the one place the patch adds a *write* the base does not make:

* A mark is an authorization to delete. `gc::ReferenceSet::protection` (`gc.rs:306-316`) has a clause
  for an unreadable map but **none for a duplicated id**, and `gc.rs` is out of scope for this slice —
  so leaving the record in place means GC reclaims that copy after the grace window. That is the (4b)
  data-loss leg, reached through a mark written before the second claimant existed. The brief names
  an **Invariant to restore**, which outranks minimal-diff: the smallest change that restores it,
  not the smallest diff.
* It is not a data delete and cannot become one: the only key it removes is an `orphan:` record, and
  removing one only ever *cancels* an authorization. The pass's own contract line is amended to say
  exactly that (`restore.rs:28-31`, `restore.rs:225`).
* It commits **on its own** rather than riding the mark batch. Two reasons: a cancellation must not
  be lost to a batch that fails for unrelated reasons, and a per-item commit needs no new batch
  accounting — which would have added an arithmetic expression (`batched.len() + withdrawn.len() >=
  MARK_BATCH`) whose `*` mutant no test could kill. Volume is bounded by "already-marked copies of an
  ambiguous id", a state the shipped allocators do not produce at all.
* Idempotent: a second run finds no record, falls to the ambiguity gate, and keeps the copy. If the
  ids are later untangled the fragment is markable again on a **fresh** clock — later reclamation,
  never earlier.

Declined, recorded here so the human has it: an ambiguous id whose chunk still holds a **`pending:`
lease** is counted `pending_skipped` and its off-placement copy remains reclaimable by GC's
expired-pending sweep. This pass must not touch a `pending:` record (it is a committing writer's
claim — that arm exists to protect it), the runbook requires writers stopped so the set should be
empty, and the cause is again #652. Reordering the arms would not change it (the lease record, not
the mark, is what GC acts on), so no code buys anything here.

### (5) `is_clean()` had no true-branch assertion

`crates/custodian/tests/restore_reconcile.rs:274-283` — on a store the pass read in full and found
nothing wrong with, `is_clean()` is asserted **true** and `needs_human()` **false**. Chosen there
rather than in the CLI unit tests because `cargo mutants` scopes each mutant's test run to the
package of the mutated file (`--package=wyrd-custodian@0.0.0`, seen in `mutants.out/log/*.log`), so
a `wyrd-server` assertion cannot kill a `restore.rs` mutant — which is why v9's four
`is_clean`/`needs_human` mutants survived despite `cli.rs` asserting them.

### (6) Two unasserted mutants on added lines

* `displaced_kept += 1` on the ambiguity path — asserted at
  `crates/custodian/tests/segmented_map_restore.rs:868`.
* The `by_id_alone - anywhere` subtraction — **removed**: `withheld` is now the plain count
  `by_id_alone` (`restore.rs:568`), asserted from the audit JSON together with `claims`
  (`segmented_map_restore.rs:800-805`). No arithmetic left to mutate.

### (7) The UNREADABLE paragraph asserted a fleet-wide fact

Now derived from `report.stranded_marked` (`crates/server/src/cli.rs:1337-1353`), with the
non-zero branch telling the operator those marks are suspect and why (the two reads that make up the
report are not one instant apart). Pinned both ways by
`the_unreadable_paragraph_derives_its_marking_claim_from_the_run` (`cli.rs:2851`).

### (8)/(9) The DANGLING summary line, and validation-fitness

The v9 review found the *summary* line still asserting *"the restore resurrected maps whose bytes
were already reclaimed"* while the paragraph below it hedged. Fixed at `cli.rs:1266-1267`, and the
test now asserts the absence of that phrase anywhere in the printed output
(`cli.rs:2822`, `the_loss_paragraphs_do_not_assert_causes_they_cannot_know`).

On the fitness-to-purpose question: the two things it named are no longer trades to accept. The
same-placement hollow green **cannot happen** (item 1) and the DANGLING misdirection is hedged in
three places (summary line, paragraph, audit event). What a live rehearsal would add is topology, and
this rule reads only the committed namespace and the fleet's `list_fragments` — the shapes that
matter are exactly the ones the discriminator seeds (identity placement, one fragment per chunk,
which is what an empty placement vector and the M0–M2 route produce). A full FDB
backup→restore→`wyrd custodian --reconcile-after-restore` rehearsal is still worth a human's eyes at
sign-off, for the *runbook text* this patch changes rather than for the rule; I have not fabricated
it here, and I am not claiming it.

## 2. Forced self-refutation (the three questions)

**(a) Genuine red?** Yes — measured by the project's own gate, not by hand.
`PDCA_BUNDLE=… ./engine/scripts/run-verify.sh` applies `patch.diff` to a clean `origin/main`
worktree, then reverts the production files and keeps the added test:

```
test result: FAILED. 1 passed; 7 failed
  a_segmented_object_no_longer_stops_the_post_restore_pass          SegmentedMapUnsupported
  an_unreadable_object_is_contained_and_the_run_is_not_certified    SegmentedMapUnsupported
  an_unreadable_object_does_not_starve_the_objects_the_pass_...     SegmentedMapUnsupported
  a_drain_over_an_incomplete_reference_set_names_the_blocking_...   empty audit capture
  a_shared_chunk_id_does_not_certify_another_object_s_chunk_...     misplaced:[45312], dangling:[]
  a_shared_chunk_id_does_not_authorize_reclaiming_another_...       stranded_marked: 1
  a_stale_mark_on_a_shared_chunk_id_is_withdrawn_rather_than_...    orphan: record still present
run-verify.sh: PASS — red without the fix, green with it.
```

All seven are **assertion/behaviour** failures, none a compile error — the discriminator names no
symbol this patch introduces. The eighth (`a_displaced_fragment_no_other_object_claims_...`,
criterion 4c) passes on the base **by design**: it pins that a singly-claimed id is judged exactly
as before, so a rule made conservative everywhere would fail it.

**(b) Production path?** Yes. The discriminator drives `wyrd_custodian::reconcile_after_restore` and
`wyrd_custodian::desired_state::reconciliation_status` — the real entry points, over in-memory
`MetadataStore`/`ChunkStore` **trait doubles** (the seams, not re-implementations of the pass). The
CLI legs drive `restore_verdict`, the production function `cmd_custodian` prints and exits on
(`cli.rs:1230-1235`). Nothing under test is a copy of the logic: the withdrawal assertion reads the
real `orphan:` key out of the real store the pass wrote to.

**(c) Fixture includes the fault?** Yes, and this is where v9 failed. The report leg now runs three
shapes, including the one v9 curated away — **both claimants naming d0 with the copy actually sitting
on d0** (`segmented_map_restore.rs:730`, `0xB6_00`), the store that produced the false clean. The
withdrawal leg seeds the failing element itself: a real `orphan:` record written by production
`mark_orphaned` before the pass runs (`segmented_map_restore.rs:895`), so "the mark is gone" is a
statement about the record GC would have acted on, not about a hypothetical.

## 3. Gates run here (all through the project's own runners)

| runner | result |
|---|---|
| `./engine/scripts/run-verify.sh` (C4-verify) | **PASS — red without the fix, green with it** (7/8 red on the reverted tree; 8/8 green) |
| `./engine/xtask.sh ci` (C4-ci: fmt, clippy, build, workspace tests, docs lint+render, typos, machete, dependency wall, conformance, statics, 50-seed DST) | **all checks passed** |
| `scripts/mutants-in-diff` (C5) | **51 mutants: 30 caught, 21 unviable, 0 MISSED** (v9: 44 tested, 6 missed) |

`cargo fmt --all` was run over every touched file, and `xtask ci`'s `fmt` step re-checks it — so the
target's own commit hooks have nothing left to reject.

## 4. Size

8 files (budget ≤ 8). 2,195 added lines / **1,245** non-blank non-comment, against v9's 1,851 /
1,061 measured identically. The +184 is entirely the nine carry-forward items: `restore.rs` +39 (the
withdrawal, the strict rule, the "cannot see" section), the discriminator +66 (the third report
shape, the withdrawal leg, the `claims`/`withheld` assertions), `cli.rs` +53 (the derived UNREADABLE
sentence, the MISPLACED qualification, two paragraph tests), the runbook +13, `restore_reconcile.rs`
+13 (the `is_clean` true branch, the inverted shared-id expectation).

That is over the brief's ≤950, as v9 was. I did **not** hand back a split, because the sign-off that
produced this round explicitly ruled on it: *"Size budget only nominally exceeded … accepted as-is,
not a re-slice trigger … Carry forward ALL reviewer/adversary/mutant findings for the next Do round
to address in place."* Addressing nine findings in place cannot shrink the diff; I trimmed where I
could (merged two CLI paragraph tests into one, dropped the `withheld` arithmetic, avoided a
retraction counter and its batch accounting) and spent the rest on evidence.

## 5. Other decisions a reader may want

* **`dangling` carries one entry per committed *reference*, so an ambiguous id appears twice.** That
  is the base's existing shape (the walk is per-reference) and it is the honest count: two *objects*
  lost that chunk. Asserted as `vec![shared, shared]`.
* **No new report field or CLI cell for a colliding id** — the brief declines both. The collision
  surfaces through `dangling` + the audit seam, and `RestoreReport::dangling` / `misplaced` keep
  their `Vec<ChunkId>` shape. The withdrawal likewise gets no counter: `already_marked` counts the
  fragment (it did arrive marked) and the audit event carries `withdrawn=true`.
* **An id claimed twice inside ONE object's own map** also counts as ambiguous (`claims` is taken
  over every `ChunkRef` the walk reaches, `restore.rs:722`). That map is corrupt by construction —
  two chunks at different offsets sharing an id means one read fetches the other's bytes — so
  reporting it unreadable is the right answer, and the audit event says `claims: 2`.
* **`desired_state.rs` is untouched this round.** Criterion (3) passed in v7/v8/v9 and no finding
  landed on it; its 22 semantic lines are v9's, carried unchanged.
* **`gc.rs` / `scrub.rs` remain untouched**, as the brief requires — which is exactly why the stale
  mark has to be withdrawn here: adding an ambiguity clause to `ReferenceSet::protection` would have
  been the other way to close it, and it is out of scope.

## 7. Self-review against the target's standing rubric (`AGENTS.md` § Review rubric & protocol)

Walked the hard conventions and the recurring-defect classes over this diff before shipping:

* **One clock per lifecycle (ADR-0009)** — no clock read added or moved; `now_millis` is still the
  caller's single source. The withdrawal deliberately *removes* a mark rather than re-stamping it,
  which is the same "never reset a grace clock" direction the pass already documents.
* **Trait seams / dependency direction (ADR-0010, ADR-0016)** — `restore.rs` still depends on
  `wyrd_traits` + `wyrd_core::metadata` + `tracing` only; `WriteBatch::delete` was already in scope.
* **Docs currency** — the CLI's operator output and the report's semantics changed, so
  `06-runtime-view.md` §6.2 step 2 and the m4 runbook changed in the same patch; `xtask ci` re-runs
  the docs lint and renderer over both.
* **Absent/unsupported entries never silently succeed** — the whole slice; and the assertions are not
  count-only (the withdrawal leg asserts the *record*, the report legs assert `Vec<ChunkId>`
  contents, and (4c) blocks a pass that "fails everything").
* **Transactions / early return** — nothing is held open across an await: `marks` is a `WriteBatch`
  *value*, and the withdrawal's `?` early-returns exactly as the base's batch commit does (queued
  puts are dropped, unwritten; the pass is idempotent by design). No rollback obligation exists.
* **Await discipline** — the one new store await is a single-key commit; the bound belongs to the
  `MetadataStore` implementation (standing rejection (i), `crates/traits/src/lib.rs:1000-1012`), and
  `review-rejected.md`'s `bounded` row now names it explicitly.
* **Test fidelity — "a new destructive or concurrent path lands with seeded Tier-0 DST coverage".**
  Weighed for the withdrawal, and declined: it removes *evidence for* a deletion, so it makes the
  system strictly less destructive (the destructive path, `gc::reconcile`, is untouched and already
  carries DST coverage — `crates/dst/tests/custodian.rs`'s `gc_reclaims_only_true_orphans_q3` and
  `gc_over_a_segmented_map_never_reclaims_it_and_never_over_certifies`), the rule has no scheduling
  or concurrency dimension for a simulation to explore (it is a per-record decision over one scan),
  and the brief's Falsifiability and External-dependencies both state **no DST leg** for this slice.
  Flagging it here rather than silently: if a reviewer wants a seeded leg, it is a scoped follow-up,
  not a hole in this evidence.

## 6. Scratch

Nothing left behind: the only scratch file was
`$PDCA_SCRATCH/pdca-builder-651-ci.log` (the CI transcript), removed at the end. All target-source
edits were made in `$PDCA_WORKTREE` (`/home/eddie/wyrd/wyrd.pdca-wt-l0`); the primary checkout was
not touched.
