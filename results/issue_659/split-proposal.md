<!-- pdca:split-proposal v1 -->
# Split proposal — issue 659

> **Intake-cap override (wyrd-pdca-P1).** The human (Eduard Ralph) explicitly overrode the rule
> in this session on **2026-10-30** for **#508, #625, #633, #658 and #659** — these splits only.
> `scripts/plan-cap --track alpha --need 3`, run while this proposal was drafted (2026-10-30):
>
> ```
> planned 7/6 (cap, track alpha) — room for 0, need 3: Plan intake closed
>   PLANNED            6  508 625 633 658 659 927
>   BUILT              1  654
>   other tracks (their own room): blackbox 4
>   rule wyrd-pdca-P1 (docs/INTEGRATION.md §11): no new brief in track alpha, no `pdca split --accept`, no split or reslicing proposal until its sign-offs make room.
> ```
>
> Accepting this proposal takes #659 out of PLANNED (split parent, close-disposition) and puts
> three children in: alpha goes from 7 to **9** on this split alone, before the four parallel
> splits add theirs. Re-run the script before `--accept`. The override stands on that
> arithmetic; the parent brief's own override (2026-10-19, "covers #659 only") does not cover
> the children, and this one does.
>
> **Three children, not two.** The parent brief's *Open questions* proposes two. Its first half
> (drain, walk, terminal delete, untrusted records, dispatch, cursor, ordering rule; legs B, C,
> D, E, G and six of the nine demonstrated reds) is on its own larger than #657's, #662's and
> #809's first attempts, which were 201 KB, 189 KB and 184 KB and were all split. Cutting that
> half once more, at the session's exit, is what brings each child near the 40–110 KB of the
> children that shipped cleanly. Rough patch estimates: child-1 **~110–145 KB** (it owns the
> `reclaiming` check, arm 0, and its seeded DST row, decided 2026-10-30), child-2 ~70–100 KB,
> child-3 ~70–100 KB. child-1 is the one at risk of the 100 KB backstop.

## Why this slice is oversized

#659 holds three outcomes that each ship alone, with their own defect, their own test file and
their own reds. The parent's success criterion has seven legs (A–G) and nine demonstrated reds,
and it binds four modules (core routing, the custodian drain, the deployed dispatch, GC). #636
died of exactly this shape.

**The hypothesis tested, and why it was not taken.** The hypothesis was: legs A–E (the drain
with the routing switch) in one child, leg F (the migration gate) in a later one. That ordering
breaks a normative rule. 0016 says the new retirement paths **MUST NOT** be enabled until the
cleanup pass has **completed** (`0016:1259-1278`, iteration-15 finding 5). Leg (A) is exactly
"the new retirement paths": `unlink` and supersede installing `retire:bytes:{generation}`, with
GC's keyed lookup deciding protection. Shipping (A) before (F) opens the window 0016 names: an
overwrite or delete removes the last reference to a fragment that still carries an expired mark
from an older event; the keyed lookup checks that old event's drained obligation, finds it
absent, and GC reclaims before the drain re-stamps. The brief's own text ties them too: "only
then do (A)'s routing and GC's keyed lookup apply" (leg F), and "until the marker is observed
… `unlink` / supersede keep today's inline path" (Design, *Migration gate*). So **the routing
switch travels with the gate**, not with the drain. The parent brief draws the same line.

**The seams, in build order:**

1. **The drain runs, deployed (legs B, C, C′ with the `reclaiming` arm 0, G, and the drain
   half of E).** Every multipart
   obligation installed so far (Abort's `{session, all}`, #657's per-part `{chunks}`, #658's
   `{parts}` and `{generation}`) is drained to empty by the deployed custodian through
   `reconcile_step`, in bounded batches, with the cursor inside the obligation, marking before
   deleting, and draining a fenced session's segments before its parts' records. The routing is
   unchanged, so in production this drains only multipart obligations, and nothing in production
   creates a session until #508. That makes it safe to ship before the gate. Its own defect is
   "obligations are installed and nothing drains them". Its own red is "dispatch removed ⇒
   (B) fails".
2. **Sessions exit (legs D and the by-chunk-id half of E).** After child-1 a session's
   obligations drain, but its owned `sidx:` residue stays, its record stays and its `mpuctl`
   slot is never released, so the count only rises until every Create answers `503`. This child
   adds the owned walk, the by-chunk-id retirement of untrusted records, the terminal delete with
   its exactly-once decrement, and the tombstone input ("retain" from the deployed caller until
   #625). Its own red is "`sidx:`-empty gate removed ⇒ (D) fails".
3. **Deletes and overwrites retire through the ledger, behind the gate (legs F and A).** The
   orphan-identity cleanup pass with its durable completion marker, GC's fallback to "every mark
   protects" before the marker, and only after the marker the switch of `unlink` and
   `commit_chunk_map_superseding{,_leased}` to one `retire:bytes:{generation}` obligation above
   one batch's budget. This child holds **the one rule the parent exists to keep**: the routing
   flips only on a base where child-1's dispatch is already deployed (its `Depends on: child-1`),
   and its leg (A2) is still red against a build with that dispatch removed.

**What stays whole.** The drain step's two modes, its cursor and its dispatch are one child: the
cursor exists only for the step, and a step without its dispatch is the discontinued run's
defect. The gate and the routing are one child: the gate protects nothing without the routing
(the 2026-09-12 comment: "a gate … would have protected nothing"), and the routing is unsound
without the gate. The segments-before-parts rule (G) stays with the drain step because it
orders two of the step's obligations, and restore's check needs no terminal delete: once the
`seg:` range is empty, `check_attempt` returns early (`crates/custodian/src/restore.rs:1201-1203`
on `origin/main` @ `376b106`), and `NoDeleter` fires only when the records obligation is gone
**while** segments survive (`restore.rs:1172-1177`), which the drain never allows because it
deletes an obligation only in its finishing step.

## Wave sketch

Three waves, one child each.

```
wave 1: child-1  — the drain step, cursor, dispatch from reconcile_step      (B, C, C′, G, E-hold)
wave 2: child-2  — owned sidx: walk, untrusted retirement, terminal delete    (D, E)
wave 3: child-3  — orphan-identity gate + unlink/supersede routing switch     (F, A)
        (child-2 and child-3 may run in either order; never in one wave)
```

- **child-2 depends on child-1.** The owned walk and the terminal delete are new legs of the
  drain dispatch child-1 adds, and the terminal delete's precondition is "no session-scoped
  `retire:` obligation is left", which only child-1's drain ever makes true. Real build-on.
- **child-3 depends on child-1.** This is the parent's rule as an ordering field: the routing
  installs `retire:bytes:g:` obligations, and only child-1's dispatched drain empties them. A
  `{generation}` obligation over a segmented map re-reads each `seg:` record's current placement
  at drain time; that is child-1's code. Without child-1 on its base, (A2) is the discontinued
  run's permanent leak.
- **child-2 conflicts with child-3, but needs nothing from it.** Their logic is disjoint, their
  files are not. Both add a leg to the same drain dispatch in `reconcile_step` / its context
  (`crates/custodian/src/reconciliation.rs:126-168`, and the deployed caller
  `crates/server/src/custodian.rs:397`): child-2 the owned walk, terminal delete and tombstone
  input; child-3 the cleanup pass and the marker check. And both add to the one GC paragraph of
  `docs/design/architecture/06-runtime-view.md:80-86`. Adjacent hunks do not fold cleanly.
- **Either order works for waves 2 and 3.** child-2 first is slightly better: it is the one
  #625 builds on, and child-3 changes production behaviour for every DELETE, so it is the one to
  land on the most settled base. Nothing breaks if the scheduler picks child-3 first.
- **The cost, stated plainly:** no parallel wave. That is the price of one dispatch and one doc
  paragraph, and it is cheaper than a fold conflict at integration.
- **Outside this proposal** (ordering fields can name only siblings,
  `src/pdca_harness/split.py:_validate_ordering`, so these go into the materialised briefs after
  acceptance, by hand):
  - **External `Depends on`** — child-1 carries the parent's `658, 634, 662`; child-2 and
    child-3 carry them through child-1 and list them again in their `Ordering note`. If #658's
    parallel split is accepted first, re-point `658` at whichever of its children publishes a
    Complete with `{parts}` and `{generation}` obligations and a segmented object; this proposal
    does not invent those ids.
  - **Downstream edges that today name #659:** **#625** (the `W_tombstone` clock) supplies the
    tombstone input child-2 defines, so its edge moves to **child-2**. **#508** needs sessions to
    exit (child-2) **and** the gate (child-3): it is the first thing in production that creates
    staged fragments, so it is the first thing that makes the multipart drain's identity marks
    meet a stale-mark population (0016's "rollout-skew population (an old custodian marking live
    staged fragments)"). Until child-3's marker exists, GC treats every mark as protecting, and
    that is what makes the multipart drain safe too. So #508 should depend on **child-2 and
    child-3**, not on child-1 alone. **#633** (operator abort) installs an Abort obligation and
    needs child-2 for the session to end.
  - **#660** owns the "two concurrent drainers" DST case; it should depend on child-2 (whose
    terminal-delete race it seeds) or on all three, at the human's choice. The one seeded DST
    row child-1 adds (the drain racing GC's swap to `reclaiming`, decided 2026-10-30) is not one
    of #660's three cases.
  - **The `reclaiming` check (arm 0) is child-1's (decided by the human 2026-10-30).** child-1's
    drain is the mark writer that reads before it writes, which `crates/core/src/metadata.rs:109-140`
    already names as inheriting the rule. #625's split (its child-3) keeps only a composed-path
    fixture through the reaper and does not implement the check.

<!-- pdca:child child-1 -->
# core,custodian: retirement drain step with cursor and production dispatch (636.6a) — issue 659 child-1

> Child 1 of 3 of #659's split (slice 6 of #636). Do reads ONLY this file. Keep the
> `- **Label:** value` lines. The design is normative in proposal 0016,
> `docs/design/proposals/draft/0016-multipart-commit-protocol.md`: read **decision 4** rules 1–2
> (`0016:1079-1290`, above all the three-arm mark rule `:1220-1226` and the keyed protection
> `:1228-1249`), the **Retirement drain step** row of the batch inventory (`0016:668`), and the
> "no cursor record class" rule (`0016:613-619`) before writing code. Citations are the parent
> brief's, verified against `getwyrd/wyrd` `origin/main` @ `12dd082` on 2026-10-19; the
> custodian anchors (`reconciliation.rs:126`, `server/src/custodian.rs:397`,
> `restore.rs:597`, `:1156`) were re-checked on `origin/main` @ `376b106` on 2026-10-30.
> `crates/core/src/multipart.rs` has drifted about 60 lines since: on `376b106`,
> `RetirePayload` is at `:3683`, `decode_retire_obligation` at `:3954`, `MAX_BATCH_BYTES` at
> `:5132`, `MAX_BATCH_OPS` at `:5171`. Re-locate every `multipart.rs` anchor on the base this
> bundle is given. This child builds on #658, #657 and #653; build on the base this bundle is
> given.
>
> **What this child does not do:** it does not change `unlink` or any superseding commit (that
> is child-3, behind the migration gate), and it does not walk `sidx:` or delete a session
> record (child-2). In production it drains only multipart obligations.
>
> **What this child does own** (the human's decision, 2026-10-30): the `reclaiming` check
> (arm 0) of the mark guard. This child's drain is the first mark writer that reads before it
> writes, and `crates/core/src/metadata.rs:109-140` on `main` already says "No writer overwrites
> a `reclaiming` mark … the three-arm drain (#659) … inherits this rule". #625's split (its
> child-3) keeps only a composed-path fixture through the reaper and does **not** implement the
> check; it builds on this one.

- **Slug:** multipart-retirement-drain-step
- **Track:** <leave as is — `pdca split --accept` writes the parent's track in>
- **Kind:** enhancement (design proposal)
- **Defect:** Every multipart slice so far installs `retire:` obligations (Abort's
  `{session, all}`, #657's per-part `{chunks}`, #658's `{parts}` and `{generation}`), and nothing
  drains them: staged bytes stay protected for ever and Complete's superseded records never
  leave. #636's attempts at this step failed in two named ways: one re-derived the same first
  keys forever (no cursor); another truncated derivation while marking the obligation drained
  (silent part loss). This child adds a byte- and operation-budgeted, cursor-keyed drain step for
  `retire:bytes:` (mark, then delete the naming records) and `retire:records:` (delete only,
  never mark), and dispatches it from `reconcile_step` in the deployed custodian.
- **Success criterion:** A NEW test file `crates/custodian/tests/multipart_retirement_drain.rs`
  (core-level codec tests MAY go in another NEW `crates/core/tests/*.rs` file), over
  `RedbMetadataStore::in_memory()` and a real chunk store or placement-aware double, driving the
  drain **through `reconcile_step`** with the contexts the deployed loop passes
  (`crates/server/src/custodian.rs:397`), not through a test-only entry. It shows all of:
  **(B) Complete, then drain.** After a Complete (#658) and reconcile steps: no `part:` or
  `psum:` record of a published part remains; **no** `orphan:` mark names a chunk of the
  published map (`0016:1503`); a subset Complete's unnamed part has every fragment marked
  **before** its `part:` record is deleted (a recording wrapper shows the order) and, after
  reconcile steps past the orphan grace, its fragments are gone from the chunk store.
  **(C) Convergence at scale.** A `{parts}` obligation over **at least 4,001 parts** (planted as
  canonical records is fine) drains to empty across many steps, each commit within
  `MAX_BATCH_BYTES` and `MAX_BATCH_OPS` (preconditions counted); running the whole drain a second
  time is a no-op. A single entry whose own deletes exceed one batch's budget still makes
  progress every step and finishes. The obligation's stored cursor advanced every step; a drain
  restarted from a fresh process state (nothing in memory) resumes from the stored cursor; the
  obligation is deleted only in its last batch.
  **(C') Mark guard, after the `reclaiming` check.** For a position that already carries a
  mark, first: **(0) a mark that decodes as `reclaiming` is never replaced, whatever event it
  carries.** That shape is GC's recorded decision to reclaim
  (`crates/core/src/metadata.rs:109-140`: "No writer overwrites a `reclaiming` mark … Every mark
  writer that reads before it writes — the three-arm drain (#659) … inherits this rule"). GC
  deletes the key blind after destroying the fragment (`crates/custodian/src/gc.rs:804-824`), so
  a value written over it vanishes with it and leaves the new event with no evidence. The step
  leaves it **byte-identical** (assert the stored value before and after), keeps the obligation
  and its cursor (at or before that position) so the work resumes, and on a later pass, once
  GC has deleted the key, writes the position under `require_absent` (arm c). Then the three
  arms (`0016:1220-1226`): (a) same event ⇒ skipped, byte-for-byte unchanged; (b) different
  event ⇒ re-stamped under `require(== prior)` with this event and a fresh `orphaned_at`;
  (c) absent ⇒ written under `require_absent`. The fixture for arm 0 plants a `reclaiming` mark
  carrying a different event on a position the obligation names, runs a step, asserts it
  unchanged and the obligation still present, deletes the key as GC would, runs the next step,
  and asserts a fresh structured mark of this event.
  **(E-hold) Validated placements.** A `part:` record under a `retire:bytes:` obligation whose
  placement fails validation (length not the scheme's fragment count, e.g. a decoded `RS(0,0)`)
  is **never deleted unmarked**: the step neither marks from that placement nor deletes the
  record; it stops at that entry with a typed outcome and an audit event, leaves the obligation
  and its cursor in place, and every fragment of that chunk stays (GC's `StagedSet` keeps
  holding it, `crates/custodian/src/gc.rs:1337-1360`). Child-2 replaces the hold with retirement
  by chunk id.
  **(G) Restore stays accurate between drain batches** (the `deferred: #659` at
  `crates/custodian/src/restore.rs:1156`). An `Aborting@E+1` session fenced from `Completing@E`
  with segments written and parts staged (its two obligations, `retire:bytes:s:<id>:<E>` and
  `retire:records:s:<id>:<E>` owing `{seg}`, seeded as `restore_completing_fence.rs` seeds them)
  is drained **one step at a time**. After **every** step, the production
  `reconcile_after_restore` (`restore.rs:597`) runs and does not name the session (no
  `ChunkInNoPart`, `restore.rs:1225-1234`, and no `NoDeleter`, `:1172-1177`), until both
  obligations are gone.
- **Falsifiability:** RED is reachable in-process on `cargo test -p wyrd-custodian` (and
  `-p wyrd-core`). (B) is red on any build where `reconcile_step` does not dispatch the drain.
  (C) is red on a drain with no stored cursor (never finishes) or one that deletes the obligation
  after a truncated derivation (the obligation vanishes with parts left). (C′) arm 0 is red on
  a guard that treats a `reclaiming` mark as "different event" and re-stamps it. Its race with
  GC is an interleaving the record-level test cannot build: the drain's exact-value replacement
  (arm b) and GC's swap to `reclaiming` both read the same stamped bytes; the seeded DST row
  (Verification posture) forces it. (E-hold)'s ordering is
  forced by a wrapper, not timing. (G) is red against independent draining of the two
  obligations: the bytes drain deletes a `part:` record a surviving segment still needs, and the
  next restore run reports `ChunkInNoPart`.
- **Invariant to restore:** **C-1** (`docs/principles.md` §5 C-1 and its §6 row *storage
  lifecycle / reclamation*), the drain's share of it: the drain marks before it deletes the
  record that named the bytes, never marks a published chunk, never overwrites a mark GC has
  committed to reclaim (`reclaiming`), never deletes a record whose
  placement it cannot trust, and every installed multipart obligation is driven to empty by the
  deployed custodian in bounded batches. **Sources:** the maintainer's rule 2026-07-25 (via
  principles.md §5); 0016's refutation standard (`0016:2802-2813`); decision 4 rule 2
  (`0016:1123-1128`); the custodian's "never reclaim a referenced fragment"
  (`crates/custodian/src/gc.rs:69`, the safety gate at `:654`). SELF-TEST: it binds the drain in
  core or custodian, the dispatch in the deployed custodian, and GC's reclamation of the marks,
  so guarding one module cannot satisfy it.
- **Repo + branch target:** getwyrd/wyrd @ main
- **Depends on:**
- **Conflicts with:**
- **Ordering note:** wave 1 of #659's split. **External, to add to `Depends on` after
  acceptance** (the field takes sibling labels only): `658, 634, 662`, as the parent had them.
  `658`: Complete's obligations and published objects are what (B) drains, and through it this
  child builds on #657 and #653; if #658's parallel split lands first, re-point this at the
  child of #658 that installs `{parts}` / `{generation}`. `634`: the drain walks `retire:` with
  `scan_page` (merged). `662`: GC's staged reference set and keyed `retire:bytes:` protection
  (merged). child-2 and child-3 both build on this child. **Intake-cap override
  (wyrd-pdca-P1):** the human's 2026-10-30 override for #659's split, recorded at the top of
  `results/issue_659/split-proposal.md`.
- **Scope:** the retirement drain step for both modes; the cursor component inside
  `RetirePayload`; the mark guard with the `reclaiming` check (arm 0) ahead of the three arms,
  and its one seeded DST row; validation of placements before marking, with the hold for an untrusted
  `part:` record; the segments-before-parts rule between a session token's two obligations; the
  drain's dispatch from `reconcile_step` in the deployed custodian; replacing the
  `deferred: #659` comment at `restore.rs:1156`; the docs. / out of scope:
  - the owned `sidx:` walk, the terminal session delete, the `mpuctl` decrement, the tombstone
    input, and retirement by chunk id of untrusted records (child-2);
  - the orphan-identity cleanup pass, its marker, GC's gate on it, and any change to `unlink`
    or `commit_chunk_map_superseding{,_leased}` (child-3);
  - the reaper's fences and the `W_tombstone` clock (#625); the operator abort (#633);
  - any seeded DST scenario other than the one drain-vs-GC `reclaiming` row (#660 owns "two
    concurrent drainers" and its other two cases);
  - the bulk `DeleteObjects` batching (#509); the gateway / S3 verbs (#668–#670 under #508);
  - any edit to proposal 0016 or to any ADR; `Cargo.lock` and `deny*.toml`.
- **Repro instruction:** n/a (new functionality). Today nothing on `main` reads a `retire:` key
  except GC's keyed protection (`crates/custodian/src/gc.rs:743-761`) and restore's re-check;
  an obligation installed by Abort stays for ever.
- **Difficulty:** high
- **Do model:** opus
- **External dependencies:** `typos`, `docs-renderer`
- **Test file:** `crates/custodian/tests/multipart_retirement_drain.rs`, a **NEW** file
  (C4-verify treats an added `crates/*/tests/*.rs` as the per-fix discriminator).
- **Verification posture:** (a) net-new; the test cannot compile on C4-verify's reverted base,
  so C4-verify reports **UNVERIFIABLE** (exit 77), a pre-declared sign-off item. **Built and
  exercised at Check:** the drain step through `reconcile_step` as deployed.
  **Demonstrated reds (required, recorded in `build-notes.md`).** Do makes each temporary edit,
  shows the named leg fail, then restores the file:
  - the drain's dispatch removed from the deployed loop's path → (B) fails;
  - the cursor not persisted (each step re-derives from the start) → (C) fails;
  - the obligation deleted after the first batch → (C) fails;
  - an untrusted `part:` record deleted without marking → (E-hold) fails;
  - arm 0 removed (a `reclaiming` mark re-stamped as "different event") → (C′) fails, and the
    DST row below fails on its GC-wins interleaving;
  - the segments-before-parts rule removed → (G) fails.
  **Seeded DST row (required; the human's decision 2026-10-30).** `AGENTS.md` asks a new
  concurrent destructive path to land with seeded DST coverage (`0016:2878-2905`). One row: the
  drain's exact-value replacement (arm b) and GC's swap to `reclaiming` race on one stamped mark
  (both read the same stamped bytes). Assert exactly one commit wins; when GC wins, the drain's
  re-read finds `reclaiming` and preserves it, byte-identical, rather than retrying arm (b)
  against it; and the obligation still drains to empty once GC's key delete lands. **Append it
  to an existing `crates/dst/tests/*.rs` file** (`crates/dst/tests/custodian.rs` is the peer):
  C4-verify discriminates on an added test file and applies an added file's crate-level cfgs
  (`--cfg madsim`) to the whole invocation, so Do MUST NOT add a new file under
  `crates/dst/tests/`. Verify with `cargo xtask ci`, which is the only gate that builds
  `crates/dst`.
- **Production reach:**
  - (a) The deployed custodian now runs the drain every reconcile pass
    (`crates/server/src/custodian.rs:397`). In production it finds no multipart obligation
    until #508 creates sessions; the routing is unchanged, so no DELETE or overwrite changes
    behaviour in this child.
  - (b) For multipart bytes, grace starts when the drain writes each mark (`0016:1098-1104`).
  - (c) One existing record shape grows: `RetirePayload` gains the optional `cursor` component,
    omitted when absent (Design), so every obligation written before this child decodes
    unchanged.
- **Citations expected:** Do must cite `path:line` on the target branch for every change.
  - **Peer for the deployed dispatch:** `reconcile_step`
    (`crates/custodian/src/reconciliation.rs:126-168`) and its one production caller,
    `crates/server/src/custodian.rs:397`. The drain must run on that path. A new parameter on
    `reconcile_step` reaches about 25 test files that call it; dispatching the drain as a named
    leg through an existing context avoids that, and either is acceptable if the deployed
    caller runs it. Leave room in that leg for child-2's owned walk and child-3's cleanup pass:
    both add to it.
  - **Peer for page-wise ledger walks and mark guards:** GC's walk (#661) and its keyed
    protection `retirement_draining` (`crates/custodian/src/gc.rs:756`); the mark codec
    `OrphanMark` (`crates/core/src/metadata.rs:142`, `encode_orphan_mark` `:257`,
    `decode_orphan_mark` `:277`) and its three value shapes
    (`docs/design/architecture/08-crosscutting-concepts.md:87`).
  - **Untrusted staged records today:** GC and restore hold every fragment of such a chunk
    (`StagedSet`, `crates/custodian/src/gc.rs:1337-1360`, reason `untrusted-staged-record`);
    restore reports them (`RestoreReport::staged_untrusted`, `restore.rs:179`). A malformed
    committed placement already blocks drains (`crates/custodian/src/desired_state.rs:234-246`).
  - **Records and budgets** (parent's lines on `12dd082`; re-locate): `RetireMode` /
    `retire_key` / `retire_session_range` (`multipart.rs:1366`, `:1492`, `:1502`),
    `decode_retire_obligation` (`:3891`), `RetirePayload` (`:3620`, wire shape `:3549-3562`),
    `PartScope` (`:3173`), `StagedPlacement` (`:4045`), `MAX_BATCH_BYTES` (`:5067`),
    `MAX_BATCH_OPS` (`:5106`, value 500), `scan_page` (`crates/traits/src/lib.rs:1438`).
  - **Marker to settle:** `deferred: #659` at `restore.rs:1156`: replace it with what the drain
    now guarantees between two restore runs (the segments-before-parts rule).
- **Prior-art check (triage cycles):** the parent's (2026-10-19): nothing drains `retire:` on
  `origin/main`; merged prerequisites `scan_page` (#634), GC's paged ledger walk (#661), the
  staged reference set and keyed protection (#662), the restore fences (#664, #809, #810).
  Rejected: #636's single-slice attempt (discontinued 2026-10-10), whose drain re-derived the
  same first keys forever in one attempt and truncated derivation while marking the obligation
  drained in another. The new test file does not exist on `376b106`.
- **Disposition hint:** new-feature

**Sources for claims taken from the tracker** (for the sign-off reader; Do need not open them):
#659's body (the cursor and convergence requirements carried forward from #508's attempts) and
the 2026-09-18 gap note with the untrusted-record rule,
https://github.com/getwyrd/wyrd/issues/659#issuecomment-5732403299; copies in
`results/issue_659/notes.json`.

## Motivation

Without a drain every obligation is a promise nobody keeps: staged bytes are protected for ever
and Complete's superseded `part:` / `psum:` records never leave. And the order of #636's slices
requires the drain to be **deployed** before anything routes ordinary deletes through it
(child-3): this child is that deployment, with nothing yet depending on it in production.

## Design

**Drain step** (`0016:668`). One obligation, one step: read it, derive the next batch of work
from its **cursor**, commit `require(retire:… == prior)` + the batch + the cursor advance, and
delete the obligation in the step that finishes it. Batches are bounded by encoded bytes **and**
by operations (`MAX_BATCH_BYTES`, `MAX_BATCH_OPS`), preconditions counted. `retire:bytes:` writes
`orphan:` marks under the guard below, then deletes the records that named the bytes; `retire:records:` only deletes records and never marks.
A `{generation}` obligation re-reads each `seg:` record's **current** placement at drain time
(`0016:2432-2440`); child-3's routing relies on this for a segmented object, so build and test it
here (a planted `{generation}` obligation over a segmented map is enough). An entry too large for
one batch on its own is split across steps with the cursor inside it.

**Mark guard: the `reclaiming` check, then three arms.** Read each position's mark (through
`decode_orphan_mark`, the one codec), then: **(0)** it decodes as `reclaiming` ⇒ leave it
byte-identical, whatever event it carries; stop the step at that position with the cursor at or
before it, keeping the obligation; a later pass finds the key gone (GC deletes it blind after
destroying the fragment, `gc.rs:804-824`) and takes arm (c). **(a)** same event ⇒ skip.
**(b)** different event ⇒ re-stamp under `require(== prior)`. **(c)** absent ⇒ write under
`require_absent` (`0016:1220-1226`). When arm (b)'s commit fails its precondition, re-read: a
`reclaiming` value now is arm 0, never a retry of (b). The records that named a position held by
arm 0 are not deleted until that position carries this event's mark, and the obligation is never
deleted with such a position unmarked.

**Where the cursor lives: inside the obligation** (normative, `0016:613-619`: "no cursor record
class"). Extend the `RetirePayload` codec with one optional `cursor` component; no new key or
record class.
- **Shape:** one position in the payload's own ordered work, the next entry and the next mark
  inside it (so an entry too large for one batch resumes mid-entry). Do chooses the spelling;
  state it in the type's doc.
- **Absent means "start"**, and is **omitted** when absent, as every other component is, so a
  fresh obligation's bytes are unchanged and no installer (Abort, #657, #658) changes.
- **Decoded against its payload** through `decode_retire_obligation`: a cursor past the
  payload's end, or on a component that has no ordered work, is a typed error.
- **Updated only by the drain step**, in the step's own batch, which requires the obligation's
  prior bytes and puts it back with the advanced cursor (the "1 cursor put" of `0016:668`). The
  finishing step deletes both. After a crash or restart the next step resumes from the stored
  cursor.
- **Compatibility:** a binary from before this child cannot decode a cursor-bearing obligation
  (`deny_unknown_fields`). Its GC does not decode obligations (presence protects,
  `gc.rs:743-761`), so no bytes are at risk; its restore pass reports such a `retire:records:`
  value as undecodable (`restore.rs:1180-1188`). Say this in the docs paragraph: downgrading
  past this child is safe for data and noisy in restore reports.

**Validated placements.** Placements are validated before marking. A `part:` record whose
placement fails validation is held, never deleted unmarked and never marked from its placement:
the step stops at it with a typed outcome and an audit event; obligation and cursor stay. This is
the safe direction (GC already holds every fragment of such a chunk). Child-2 turns the hold into
retirement by chunk id for `Aborting` sessions.

**Segments drain before parts' records** (G). For a session token `s:<id>:<E>` whose
`retire:records:` obligation owes `{seg}`, the drain deletes no `part:` record under the same
token's `retire:bytes:` obligation until that records obligation is gone; the bytes obligation
may still write marks. Restore's `check_attempt` requires every surviving segment's chunks to be
held by a surviving part (`restore.rs:1195-1234`), so independent draining would make an honest
half-drained state read as damage. The check is one keyed read. It is about the restore report's
accuracy, not safety (each obligation already marks before it deletes).

**Dispatch.** `reconcile_step` drives the drain each pass, a bounded number of steps per pass,
from the contexts the deployed caller already builds. Name the leg so child-2 (owned walk,
terminal delete) and child-3 (cleanup pass) can add to it without a new parameter.

**Docs currency** (`AGENTS.md:154-157`): `docs/design/architecture/06-runtime-view.md` (the GC
paragraphs at `:80-86`): the custodian drains `retire:` obligations in bounded batches, marking
before deleting; grace for drained bytes starts at the drain; a `reclaiming` mark is never
overwritten and the drain resumes once GC's key delete lands; the cursor and the downgrade note.

## Alternatives considered

- **A separate cursor record.** 0016 forbids a cursor record class (`0016:613-619`). Rejected.
- **Drain without dispatch, dispatch later.** The discontinued run's shape. Rejected.
- **Retire untrusted `part:` records by chunk id here.** Needs the fleet's fragment listings in
  the drain; it belongs with the owned walk, which meets the other untrusted kind (an `sidx:`
  value that will not decode). Deferred to child-2; this child holds.

## Impact & compatibility

The deployed custodian gains a drain leg. One existing record shape grows (`RetirePayload`'s
optional cursor, omitted when absent). No DELETE or overwrite changes behaviour.

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a draft PR MAY
happen during the cycle (useful for CI feedback). The PR MUST NOT be marked ready before
sign-off accepts.
<!-- pdca:end child-1 -->

<!-- pdca:child child-2 -->
# core,custodian: owned sidx walk, untrusted-record retirement and terminal session delete (636.6b) — issue 659 child-2

> Child 2 of 3 of #659's split (slice 6 of #636). Do reads ONLY this file. Keep the
> `- **Label:** value` lines. The design is normative in proposal 0016,
> `docs/design/proposals/draft/0016-multipart-commit-protocol.md`: read the **Owned-`sidx:` drain
> step** and **Terminal session delete** rows of the batch inventory (`0016:672-674`), decision
> 5's backstop (`0016:1654-1668`), the terminal-delete rule of §2 (`0016:582-602`) and the
> tombstone rule (`0016:574`) before writing code. Citations are the parent brief's, verified
> against `getwyrd/wyrd` `origin/main` @ `12dd082` on 2026-10-19; `crates/core/src/multipart.rs`
> anchors have drifted about 60 lines on `376b106` (2026-10-30), so re-locate them. This child
> builds on child-1 (the drain step and its dispatch), which lands in the wave before.

- **Slug:** multipart-session-terminal-delete
- **Track:** <leave as is — `pdca split --accept` writes the parent's track in>
- **Kind:** enhancement (design proposal)
- **Defect:** After child-1 a session's `retire:` obligations drain, but nothing ends the
  session: its owned `sidx:` residue (crashed in-flight intents from #657's staging) stays and
  keeps its fragments protected; its `mpu:` and `slot:` records stay; and `mpuctl.count` only
  rises, until every Create answers `503`. And an untrusted staged record on a dead upload (a
  placement that fails validation, or an `sidx:` value that will not decode) is held for ever:
  child-1 holds it, GC holds every fragment of its chunk, and the session can never reach its
  exit. This child adds the owned `sidx:` walk, retirement of untrusted records by chunk id for
  `Aborting` sessions, and the terminal session delete with its exactly-once `mpuctl` decrement,
  dispatched from the drain leg child-1 added to `reconcile_step`.
- **Success criterion:** A NEW test file
  `crates/custodian/tests/multipart_session_terminal.rs`, over `RedbMetadataStore::in_memory()`
  and a real chunk store or placement-aware double, driving everything **through
  `reconcile_step`** with the contexts the deployed loop passes
  (`crates/server/src/custodian.rs:397`). It shows all of:
  **(D) Owned walk and terminal delete.** An `Aborting` session holding committed parts and
  crashed in-flight `sidx:` residue, and a `Completed` session holding in-flight residue, each
  drain to: every owned fragment marked from its `sidx:` placement, then the entry deleted; no
  `sidx:` record of the session left. **`Aborting`:** the terminal delete fires **once**,
  `mpuctl.count` down by exactly one (two drainers racing the terminal delete through a wrapper
  still decrement once); no `slot:` record left; the `seggrp:` marker deleted when no `seg:`
  record names the nonce and **kept** when one does (`0016:674`). **`Completed`:** with the
  contexts the deployed loop passes, the `mpu:` record (the tombstone) is **kept** after its
  obligations and residue drain, and an identical Complete retry still answers
  `AlreadyCompleted` (`crates/core/src/multipart.rs:4789-4795` on `12dd082`); with the drain
  context's tombstone input saying the window has passed, the terminal delete fires once, as for
  `Aborting`. The terminal delete does not fire while the session's `sidx:` range is non-empty,
  nor while a session-scoped `retire:` obligation is left.
  **(E) Untrusted records.** For an `Aborting` session, a `part:` record or an owned `sidx:`
  record whose placement fails validation (length not the scheme's fragment count, e.g. a
  decoded `RS(0,0)`), and an owned `sidx:` value that will not decode, are retired **by chunk
  id**: every fragment bearing that chunk id is marked, then the record is deleted (a recording
  wrapper shows the order); after GC past grace those fragments are gone and the terminal delete
  has fired. Controls: an `Open` session with the same kind of record keeps the record and every
  fragment of its chunk; a committed object's fragments on the same servers are untouched.
- **Falsifiability:** RED is reachable in-process on `cargo test -p wyrd-custodian`. On
  child-1's base, (D) is red: no walk, no terminal delete, `mpuctl.count` unchanged; (E) is red:
  child-1 holds the untrusted `part:` record and the session never ends. (D)'s double-decrement
  race and (E)'s ordering are forced by wrappers, not timing.
- **Invariant to restore:** **C-1** (`docs/principles.md` §5 C-1 and its §6 row *storage
  lifecycle / reclamation*), the session's share: every session state the drain owns has an exit
  (terminal delete) that fires exactly once, and no record is deleted whose placement the drain
  cannot trust without marking every position its chunk could occupy. **Sources:** the
  maintainer's rule 2026-07-25; decision 4 rule 2 (`0016:1123-1128`); decision 5's backstop
  (`0016:1654-1668`); `gc.rs:69` and its safety gate at `:654`. SELF-TEST: it binds the walk and
  terminal delete in the drain, their dispatch in the deployed custodian and GC's reclamation of
  the marks.
- **Repo + branch target:** getwyrd/wyrd @ main
- **Depends on:** child-1
- **Conflicts with:** child-3
- **Ordering note:** wave 2 or 3 of #659's split. Builds on child-1's drain step, cursor and
  dispatch leg. Conflicts with child-3 on the drain dispatch in `reconcile_step` / the deployed
  caller and on `docs/design/architecture/06-runtime-view.md:80-86`; neither needs the other's
  outcome. **External, to add to `Depends on` after acceptance:** `658, 634, 662` (carried
  through child-1; list them so the edge survives if #658's split re-points it). **Downstream:**
  #625 (`W_tombstone`) supplies the tombstone input this child defines, so #625's edge to #659
  becomes an edge to this child; #508 and #633 also need this child. **Intake-cap override
  (wyrd-pdca-P1):** the human's 2026-10-30 override for #659's split.
- **Scope:** the owned `sidx:` walk for `Aborting` and `Completed` sessions; retirement by chunk
  id of untrusted `part:` and `sidx:` records for `Aborting` sessions (replacing child-1's hold
  for that case); the terminal session delete and its exactly-once `mpuctl` decrement; the
  tombstone input on the drain context, with the deployed caller passing "retain"; the docs. /
  out of scope:
  - the drain step itself, its cursor, and the segments-before-parts rule (child-1);
  - the migration gate and the routing switch (child-3);
  - the reaper's fences (idle `Open`, `W_completing` rollback, `W_session`) and the
    `W_tombstone` clock (#625); the operator abort (#633);
  - seeded DST (#660 owns "two concurrent drainers"; add no DST scenario);
  - the gateway / S3 verbs (#668–#670 under #508);
  - any edit to proposal 0016 or to any ADR; `Cargo.lock` and `deny*.toml`.
- **Repro instruction:** n/a (new functionality). On child-1's base, an `Aborting` session with
  drained obligations keeps its `mpu:` record, its `slot:` records and its `mpuctl` slot; an
  `sidx:` entry it owned stays with its fragments protected.
- **Difficulty:** high
- **Do model:** opus
- **External dependencies:** `typos`, `docs-renderer`
- **Test file:** `crates/custodian/tests/multipart_session_terminal.rs`, a **NEW** file
  (C4-verify treats an added `crates/*/tests/*.rs` as the per-fix discriminator).
- **Verification posture:** (a) net-new; C4-verify reports **UNVERIFIABLE** (exit 77), a
  pre-declared sign-off item. **Built and exercised at Check:** the walk, the untrusted-record
  retirement and the terminal delete, through `reconcile_step` as deployed.
  **Demonstrated reds (required, recorded in `build-notes.md`):**
  - the `sidx:`-empty gate removed from the terminal delete → (D) fails;
  - a `Completed` session terminal-deleted without the tombstone input → (D)'s `Completed` leg
    fails;
  - the `mpuctl` decrement taken outside the terminal delete's batch (or not under its
    precondition) → (D)'s race leg fails (two decrements);
  - an untrusted record deleted without marking by chunk id → (E) fails.
- **Production reach:**
  - (a) The deployed custodian's drain leg gains the walk and the terminal delete. In production
    an `Aborting` session reaches its terminal delete; a `Completed` one keeps its tombstone
    record and its `mpuctl` slot until #625 supplies the window. Nothing in production creates a
    session until #508, which lands with or after #625 (`0016:236-268`).
  - (b) Retirement by chunk id reads the fleet's fragment listings: state the cost per pass, and
    whether it reuses the listing GC already takes (an open point of the 2026-09-18 comment).
- **Citations expected:** Do must cite `path:line` on the target branch for every change.
  - **The dispatch leg:** child-1's drain leg in `reconcile_step`
    (`crates/custodian/src/reconciliation.rs:126-168`) and the deployed caller
    (`crates/server/src/custodian.rs:397`); add to the leg, do not add a second parameter.
  - **Records** (parent's lines on `12dd082`; re-locate): `retire_session_range`
    (`multipart.rs:1502`), `StagedPlacement` (`:4045`), `decode_owned_entry` (`:4231`),
    `MPUCTL_KEY` (`:1144`), `seggrp_key` / `seg_group_prefix`
    (`crates/core/src/metadata.rs:1521`, `:1516`), `scan_page`
    (`crates/traits/src/lib.rs:1438`).
  - **Untrusted staged records today:** `StagedSet` (`crates/custodian/src/gc.rs:1337-1360`,
    reason `untrusted-staged-record`); `RestoreReport::staged_untrusted`
    (`crates/custodian/src/restore.rs:179`).
  - **Tombstone:** the `AlreadyCompleted` answer (`multipart.rs:4789-4795` on `12dd082`).
- **Prior-art check (triage cycles):** the parent's (2026-10-19): no terminal delete and no
  `mpuctl` decrement exist on `origin/main`. Rejected: the discontinued #636 patch decoded
  `RS(0,0)`, marked nothing and deleted the only record naming the bytes. The new test file does
  not exist on `376b106`.
- **Disposition hint:** new-feature

**Sources for claims taken from the tracker** (for the sign-off reader; Do need not open them):
the 2026-09-18 gap note with the untrusted-record rule,
https://github.com/getwyrd/wyrd/issues/659#issuecomment-5732403299; copy in
`results/issue_659/notes.json`.

## Motivation

A session that never ends holds an admission slot for ever, and `mpuctl.count` only rises until
every Create answers `503`. An untrusted record on a dead upload is a state nothing exits:
skipping it keeps the record and its fragments for ever; deleting it unmarked leaves bytes
unreferenced and unevidenced (the discontinued patch's `RS(0,0)` failure).

## Design

**Owned walk** (`0016:672`, `:1654-1668`). For a session in `Aborting` or `Completed`, walk its
bounded `sidx:<id>:` range: mark every position of each entry's `staged` placement, then delete
the entry, in budgeted batches (`MAX_BATCH_BYTES`, `MAX_BATCH_OPS`, preconditions counted), marks
under the three-arm guard child-1 implements. Nothing refills the range once the session left
`Open` (#657's fenced intents). The walk needs no cursor: each batch deletes the entries it
marked, so what is left is the progress.

**Untrusted staged records** (the 2026-09-18 comment). For an `Aborting` session only, a `part:`
or `sidx:` record whose placement fails validation, or an `sidx:` value that will not decode, is
retired **by chunk id**: mark every fragment bearing that chunk id, found from the fleet's
fragment listings (a `FragmentId` carries its chunk) rather than from the untrusted placement,
then delete the record. This replaces child-1's hold for `Aborting` sessions; for any other
state child-1's hold stays. Gateway-minted chunk ids carry a random epoch and do not collide
(`0016:830`, X36 `:2575`), and GC still checks every fragment against the reference set and
waits out the grace, so a fragment another valid record names is kept. An `Open` session's
record keeps protecting its chunk.

**Terminal delete** (`0016:674`, `:582-602`). Only when the session's `sidx:` range was observed
empty in this pass and no session-scoped `retire:` obligation is left (two one-row `scan_page`
reads): one batch that `require`s the session record's exact bytes, deletes it, deletes surviving
`slot:` records, deletes the `seggrp:` marker iff no `seg:` record names the nonce, and CASes
`mpuctl` down by one. Exactly one of two racing drainers wins.

**A `Completed` session also needs its tombstone window to have elapsed** (`0016:574`); deleting
it early turns an identical Complete retry from `AlreadyCompleted` into `NoSuchUpload`. Who
measures the window is #625's (`W_tombstone`). The drain's context carries an explicit tombstone
input, and the terminal delete fires for a `Completed` session only when that input says the
window has passed. **The deployed caller passes "retain" until #625 lands.** An `Aborting`
session needs no such input.

**Docs currency** (`AGENTS.md:154-157`): `docs/design/architecture/06-runtime-view.md` (the GC
paragraphs at `:80-86`): multipart sessions end at a terminal delete that releases the admission
slot once; a `Completed` session's tombstone is kept until its window passes; untrusted records
of a dead upload are retired by chunk id.

## Alternatives considered

- **Skip untrusted records.** The record stays for ever and the session's terminal delete never
  fires. Rejected (C-1: a state nothing exits).
- **Delete untrusted records without marking.** The discontinued patch's `RS(0,0)` failure.
  Rejected.
- **Terminal-delete `Completed` sessions now.** Breaks an identical Complete retry. Rejected;
  the tombstone input.

## Impact & compatibility

No record shape changes. Sessions now leave the store and release their `mpuctl` slot, in
production only for `Aborting` sessions until #625.

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a draft PR MAY
happen during the cycle (useful for CI feedback). The PR MUST NOT be marked ready before
sign-off accepts.
<!-- pdca:end child-2 -->

<!-- pdca:child child-3 -->
# core,custodian: orphan-identity migration gate and unlink/supersede retirement routing (636.6c) — issue 659 child-3

> Child 3 of 3 of #659's split (slice 6 of #636). Do reads ONLY this file. Keep the
> `- **Label:** value` lines. The design is normative in proposal 0016,
> `docs/design/proposals/draft/0016-multipart-commit-protocol.md`: read **decision 4** rules 1–2
> (`0016:1079-1290`: the routing `:1081-1106`, the three-arm mark rule `:1220-1226`, the keyed
> protection `:1228-1249`, the cleanup pass and the migration gate `:1251-1278`) before writing
> code. Citations are the parent brief's, verified against `getwyrd/wyrd` `origin/main` @
> `12dd082` on 2026-10-19; `crates/core/src/multipart.rs` anchors have drifted about 60 lines on
> `376b106` (2026-10-30), so re-locate them. This child builds on child-1 (the deployed drain).
>
> **The one rule this child exists to keep** (the parent's): the routing of `unlink` and
> supersede through `retire:bytes:{generation}` flips only where the deployed custodian already
> runs the drain. That is child-1, on this child's base. The discontinued #636 patch flipped the
> routing while the drain had callers only in tests: deleting a 30-chunk RS(6,3) object left all
> 270 fragments on disk after GC, for good, and its own classification oracle counted that leak
> as safe. **And the second rule** (0016's): the routing flips only after the orphan-identity
> cleanup pass has **completed**, observed through a durable marker.

- **Slug:** retirement-routing-migration-gate
- **Track:** <leave as is — `pdca split --accept` writes the parent's track in>
- **Kind:** enhancement (design proposal)
- **Defect:** `unlink` writes every orphan mark inline in its delete batch
  (`crates/core/src/metadata.rs:2141-2164`) and refuses a segmented object (`:2148-2151`);
  `commit_chunk_map_superseding` does the same for the prior map (`:2235`; a segmented prior is
  refused at `:2188-2193` for `commit_chunk_map`). Past 55 chunks at RS(6,3) that batch exceeds
  the protocol's `MAX_BATCH_OPS` (500): a 100-chunk object's unlink is 904 operations, a maximal
  object's marks exceed the transaction envelope outright (`0016:1091-1096`), and a segmented
  object cannot be deleted at all. 0016 moves this fan-out to the retirement ledger
  (`0016:1081-1106`), but only after a one-time cleanup of stale marks has completed
  (`0016:1251-1278`): until then GC's keyed lookup can find an old event's obligation drained
  and reclaim a just-unreferenced fragment before the drain re-stamps its expired mark. This
  child adds the cleanup pass, its durable completion marker, the gate on it, and the routing
  switch behind the gate.
- **Success criterion:** A NEW test file `crates/custodian/tests/retirement_routing.rs` (core-level
  tests MAY also go in another NEW `crates/core/tests/*.rs` file), over
  `RedbMetadataStore::in_memory()` and a real chunk store or placement-aware double, driving the
  cleanup pass and the drain **through `reconcile_step`** with the contexts the deployed loop
  passes (`crates/server/src/custodian.rs:397`). It shows all of:
  **(F) The migration gate** (`0016:1251-1278`). Before the cleanup marker exists, the
  identity-keyed retirement paths are off: GC treats every `orphan:` mark as protecting, and
  `unlink` / supersede behave exactly as on the base. The cleanup pass walks `orphan:`
  page-wise, and a mark found on a still-referenced fragment is **re-stamped, not dropped**, when
  the reference is a `part:` record (the 2026-09-12 trap: an upload publishes; the object is
  deleted by `unlink`, which writes legacy marks; a GC pass drops those marks as stale; the part
  record retires; the fragments are left with no mark and no reference). When the pass finishes
  it writes the durable marker, and only then do (A)'s routing and GC's keyed lookup apply. A
  pass interrupted mid-walk writes no marker, and the routing stays off.
  **(A) Positive reclamation after `unlink` and overwrite**, all with the marker present.
  (A1) The tracker's historical case: a committed 30-chunk RS(6,3) object (270 fragments, 274
  operations, which fits one batch, so the inline path may still serve it) is deleted with
  `metadata::unlink`. After reconcile steps past the orphan grace, every one of its 270 fragments
  is gone. (A2) **Past the batch budget:** a committed flat 100-chunk RS(6,3) object (900
  fragments, 904 operations inline) is deleted with `metadata::unlink`. **Before any reconcile
  step**, the unlink's own batch has installed exactly one `retire:bytes:g:<inode>:<version>`
  obligation and written no `orphan:` mark. After reconcile steps past the orphan grace,
  **every one of its 900 fragments is gone from the chunk store** and no `retire:` obligation or
  `orphan:` mark of it remains. **The boundary, preconditions counted:** an 83-chunk RS(4,2)
  object (498 marks + 2 deletes + 2 preconditions = 502 operations; 500 if the preconditions
  were ignored) also takes the obligation path, with no inline mark. (A3) The same as A2 for an
  ordinary overwrite (`write::commit_overwrite`) of such an object, for the prior generation's
  fragments, and for an `unlink` of a **segmented** object published by #658's Complete (no
  inline path exists for one). Asserted as fragments absent, never as "no failure observed".
- **Falsifiability:** RED is reachable in-process on `cargo test -p wyrd-custodian` (and
  `-p wyrd-core`). (A2)/(A3) are red on any build where the routing flips but `reconcile_step`
  does not dispatch the drain (the fragments stay): the discontinued run's exact defect. (A1)
  alone is not, since a 274-operation unlink may stay inline; that is why A2 exists. (F) is red
  on a cleanup pass that drops a `part:`-referenced mark, and on a routing that does not consult
  the marker.
- **Invariant to restore:** **C-1** (`docs/principles.md` §5 C-1 and its §6 row *storage
  lifecycle / reclamation*): every durable fragment is, at every instant, named by a record or
  by an `orphan:` mark whose grace the reader can rely on. Here: a delete or overwrite never
  leaves a fragment unnamed (the obligation names it until the drain marks it), the keyed lookup
  is relied on only after the invariant "a fragment's mark names its most recent unreference
  event" holds (the marker), and every installed `{generation}` obligation is drained by the
  deployed custodian. **Sources:** the maintainer's rule 2026-07-25; 0016's refutation standard
  (`0016:2802-2813`); decision 4 rules 1–2 (`0016:1079-1290`); `gc.rs:69` and its safety gate
  at `:654`. SELF-TEST: it binds the routing in core, the cleanup pass and the gate in the
  custodian, the drain's dispatch and GC.
- **Repo + branch target:** getwyrd/wyrd @ main
- **Depends on:** child-1
- **Conflicts with:** child-2
- **Ordering note:** wave 2 or 3 of #659's split. `Depends on child-1` is the parent's rule as
  an ordering field: the drain that empties this child's `{generation}` obligations must already
  be dispatched on this child's base. Conflicts with child-2 on the drain dispatch in
  `reconcile_step` / the deployed caller and on `docs/design/architecture/06-runtime-view.md:80-86`.
  **External, to add to `Depends on` after acceptance:** `658, 634, 662` (carried through
  child-1; `658` is also where (A3)'s segmented object comes from, and `662` is GC's keyed
  lookup that this child gates). **Downstream:** #508 is the first thing in production that
  creates staged fragments; until this child's marker exists GC treats every mark as protecting,
  which is what makes the multipart drain safe against the rollout-skew mark population too, so
  #508 should depend on this child as well as child-2. **Intake-cap override (wyrd-pdca-P1):**
  the human's 2026-10-30 override for #659's split.
- **Scope:** the orphan-identity cleanup pass and its durable completion marker; GC's fallback
  to "every mark protects" until the marker is observed; the switch of `unlink` and
  `commit_chunk_map_superseding{,_leased}` to one `retire:bytes:{generation}` obligation above
  one batch's budget, behind the marker; the docs. / out of scope:
  - the drain step, its cursor and its dispatch (child-1); the owned walk and the terminal
    delete (child-2);
  - the bulk `DeleteObjects` per-request batching (#509's, inside the per-transaction bound
    `0016:1107-1122`);
  - seeded DST (#660); the gateway / S3 verbs (#668–#670 under #508);
  - any edit to proposal 0016 or to any ADR (ADR-0045's refinement is #811); `Cargo.lock` and
    `deny*.toml`.
- **Repro instruction:** today `unlink` writes every orphan mark inline in its delete batch
  (`crates/core/src/metadata.rs:2141-2164`) and refuses a segmented object (`:2148-2151`).
  Deleting a 270-fragment object puts 274 operations in one batch, inside `MAX_BATCH_OPS` = 500;
  a 100-chunk RS(6,3) object's unlink is 904 operations. No migration marker exists on
  `origin/main`.
- **Difficulty:** high
- **Do model:** opus
- **External dependencies:** `typos`, `docs-renderer`
- **Test file:** `crates/custodian/tests/retirement_routing.rs`, a **NEW** file (C4-verify treats
  an added `crates/*/tests/*.rs` as the per-fix discriminator).
- **Verification posture:** (b) a real red on the base for (A2)'s "installs one obligation, no
  inline mark" assertion; the file as a whole needs this child's marker type, so C4-verify may
  still report **UNVERIFIABLE** (exit 77): say which in `build-notes.md`. **Built and exercised
  at Check:** the cleanup pass and the gate through `reconcile_step` as deployed; the routing
  through the production `unlink` and `commit_overwrite`.
  **Demonstrated reds (required, recorded in `build-notes.md`):**
  - the drain's dispatch removed from the deployed loop's path (routing left switched) → (A2)
    fails;
  - the inline-path threshold counting mutations only (preconditions ignored) → (A2)'s 83-chunk
    RS(4,2) boundary case fails (it goes inline);
  - a mark on a `part:`-referenced fragment dropped instead of re-stamped → (F) fails;
  - the routing enabled without consulting the marker → (F) fails.
- **Production reach:**
  - (a) **This child changes production behaviour**, deliberately, once the marker is written:
    every DELETE and every overwrite above one batch's worth of fragments installs an obligation,
    and the deployed custodian drains it (`crates/server/src/custodian.rs:397`). (A) is the guard
    that the live path reclaims, asserted as fragments gone.
  - (b) Grace now starts when the drain writes each mark, slightly later than today's inline
    mark: reclamation is never earlier than today, only later (`0016:1098-1104`), an accepted
    cost.
  - (c) Until the marker is written, the gate keeps the old protective behaviour, so a
    deployment upgrades without a window where GC could reclaim against a stale mark.
- **Citations expected:** Do must cite `path:line` on the target branch for every change.
  - **Routing:** `metadata::unlink` (`crates/core/src/metadata.rs:2133-2165`),
    `commit_chunk_map_superseding{,_leased}` (`:2188-2235`), `write::commit_overwrite`.
  - **Peer for page-wise ledger walks and mark guards:** GC's walk (#661) and its keyed
    protection `retirement_draining` (`crates/custodian/src/gc.rs:756`); `OrphanMark`
    (`crates/core/src/metadata.rs:142`, `encode_orphan_mark` `:257`, `decode_orphan_mark`
    `:277`) and its three value shapes
    (`docs/design/architecture/08-crosscutting-concepts.md:87`).
  - **Records and budgets** (re-locate on the base): `retire_key` (`multipart.rs:1492`),
    `MAX_BATCH_BYTES` (`:5067`), `MAX_BATCH_OPS` (`:5106`), `scan_page`
    (`crates/traits/src/lib.rs:1438`).
  - **The dispatch leg:** child-1's drain leg in `reconcile_step`
    (`crates/custodian/src/reconciliation.rs:126-168`).
  - **New record:** the marker is a new singleton record: state its key shape, writer, deleter
    and which scans see it (ADR-0046).
- **Prior-art check (triage cycles):** the parent's (2026-10-19): no migration marker exists;
  #662 decodes all three mark shapes and adds keyed protection (merged). Rejected: #636's
  single-slice attempt, whose routing flip shipped without dispatch; #637's first attempt, whose
  cleanup dropped a lapsed legacy mark whenever any `part:` record named the fragment. The new
  test file does not exist on `376b106`.
- **Disposition hint:** new-feature

**Sources for claims taken from the tracker** (for the sign-off reader; Do need not open them):
the 2026-09-12 scope note moving the migration gate to #659,
https://github.com/getwyrd/wyrd/issues/659#issuecomment-5648454954; copy in
`results/issue_659/notes.json`.

## Motivation

Deleting or overwriting any object past 55 chunks at RS(6,3) puts more operations in one batch
than the protocol's per-batch budget, a maximal object's marks exceed the transaction envelope,
and a segmented object cannot be deleted at all. The ledger fixes that, but only on a store where
every mark names its most recent unreference event; the gate makes that a checked precondition
rather than an operational note.

## Design

**Migration gate** (`0016:1251-1278`, the 2026-09-12 comment). A page-wise pass over `orphan:`,
dispatched from child-1's drain leg in `reconcile_step`, re-stamps or drops a mark found on a
still-referenced fragment; when the reference is a `part:` record, re-stamp (or drop only
against **committed** references), never drop. On finishing it writes a durable completion
marker: a new singleton record, so state its key shape, writer, deleter and which scans see it
(ADR-0046). Until the marker is observed, the identity-keyed paths stay off: GC treats every mark
as protecting (its existing `Defer` arm), and `unlink` / supersede behave exactly as on the base.

**Routing switch** (`0016:1081-1106`). With the marker observed, `unlink` and
`commit_chunk_map_superseding{,_leased}` remove the references and install **one**
`retire:bytes:{generation}` (the removed map's chunks, and its `seg:` range if segmented) in the
same batch, instead of inline marks, under `require_absent` with the `g:<inode>:<version>` token.
A small flat object may keep the inline path when its **whole** batch fits one batch's budget,
counted as preconditions plus deletes plus mark puts against `MAX_BATCH_OPS` (500), and its
bytes against `MAX_BATCH_BYTES`; above that, and for every segmented object, the obligation is
the only path. child-1's drain re-reads each `seg:` record's current placement at drain time.

**Docs currency** (`AGENTS.md:154-157`): `docs/design/architecture/06-runtime-view.md` (the GC
paragraphs at `:80-86`): deletes and overwrites install a retirement obligation that the
custodian drains, marking before deleting; grace starts at the drain; the migration marker and
what it gates.

## Alternatives considered

- **Flip the routing first, dispatch later.** The discontinued run's permanent leak. Rejected;
  `Depends on child-1`.
- **Flip the routing with the drain, gate later.** Opens the stale-mark window 0016 forbids
  (`0016:1259-1278`). Rejected; the gate travels with the routing.
- **Expand pending obligations into GC's reference set.** Unbounded memory for one segmented
  generation (`0016:1236-1242`). Rejected; keyed lookup.

## Impact & compatibility

Production behaviour changes for DELETE and overwrite of objects past one batch of fragments,
once the marker is written: reclamation moves from the request to the custodian, a little later
than today. A new singleton record (the marker) is added.

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a draft PR MAY
happen during the cycle (useful for CI feedback). The PR MUST NOT be marked ready before
sign-off accepts.
<!-- pdca:end child-3 -->
