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
- **Track:** alpha
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
  **(F) The migration gate** (`0016:1251-1278`). Before a **valid** cleanup marker exists, the
  identity-keyed retirement paths are off: GC treats every `orphan:` mark as protecting, and
  `unlink` / supersede behave exactly as on the base. The cleanup pass walks `orphan:` page-wise
  and judges each mark by the rule in Design, shown leg by leg:
  (F1) **Committed reference:** a mark on a fragment a committed map names is dropped.
  (F2) **Staged, never published:** a stale mark (legacy, as an old custodian wrote them) on a
  fragment only an `sidx:` entry or an unpublished part's `part:` record names is dropped. Then
  the plan review's sequence: the part publishes (production Complete), the records drain retires
  its `part:` records, and the object is deleted with the routed `unlink`. GC-only passes with
  the clock past the stale stamp's grace, run **before** the drain marks, reclaim none of its
  fragments; passes inside the drain's fresh grace reclaim none; passes past it reclaim all.
  (F3) **The 2026-09-12 trap:** a mark on a fragment that only a **published** part's records
  name (its number in a pending `retire:records:` `{parts}` obligation) and no committed map names
  (the object was deleted by the inline `unlink`, which wrote the mark) is kept byte-identical;
  after the records drain retires the part records, GC reclaims those fragments after that mark's
  grace. No fragment is left with no mark and no reference.
  (F4) **Left alone:** a `reclaiming` mark, and a structured mark whose event's `retire:bytes:`
  obligation is present (a drain mid-retirement), are byte-identical after the pass, whatever
  names their fragment.
  (F5) **Race with unlink:** a wrapper pauses the cleanup between its read and its commit of an
  F1 drop, runs a production `unlink` of that object (inline, the gate being closed), then lets
  the cleanup commit. The drop fails, the mark the unlink wrote survives, and the fragments go only
  after its grace. The same with an overwrite in place of the unlink.
  (F6) **Completion:** a walk that meets an unreadable inode, an unreadable `seg:` record or an
  unreadable `part:` record (one fixture each) reaches the end of the range and writes **no**
  marker; it reports a typed blocked outcome naming the record, and the routing stays off. After
  the record is repaired, a later walk completes and writes the marker, and only then does (A)'s
  routing apply. A walk interrupted mid-way writes no marker either.
  (F7) **Restore:** a marker written under one restore-fence generation, followed by the
  production post-restore pass (`reconcile_after_restore`, which opens a new generation as its
  first write, `crates/custodian/src/restore.rs:517-522`), reads as **not** complete: with stale
  marks planted as a restored image would carry them, the routing and GC's keyed lookup stay off
  and GC treats every mark as protecting, until the cleanup walks again and writes a marker for
  the new generation.
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
  **And present while they must be:** in (A2) and (A3), GC-only passes run between the unlink (or
  overwrite) and the drain's marks, with the clock past every older mark's grace, reclaim none of
  the object's fragments; passes inside the grace the drain's marks start reclaim none; only
  passes past it leave them gone. An eventual-absence check alone also passes after a premature
  reclaim, so this half is required.
- **Falsifiability:** RED is reachable in-process on `cargo test -p wyrd-custodian` (and
  `-p wyrd-core`). (A2)/(A3) are red on any build where the routing flips but `reconcile_step`
  does not dispatch the drain (the fragments stay): the discontinued run's exact defect. (A1)
  alone is not, since a 274-operation unlink may stay inline; that is why A2 exists. (F) is red
  on a cleanup pass that drops the trap's mark (F3), keeps or re-stamps a stale mark on a staged
  part (F2: GC reclaims before the drain marks), touches a `reclaiming` or draining mark (F4),
  drops without preconditioning on the record it judged by (F5), writes the marker after a walk
  that met an unreadable record (F6), or reads a restored marker as valid (F7); and on a routing
  that does not consult the marker.
- **Invariant to restore:** **C-1** (`docs/principles.md` §5 C-1 and its §6 row *storage
  lifecycle / reclamation*): every durable fragment is, at every instant, named by a record or
  by an `orphan:` mark whose grace the reader can rely on. Here: a delete or overwrite never
  leaves a fragment unnamed (the obligation names it until the drain marks it), the keyed lookup
  is relied on only after the invariant "a fragment's mark names its most recent unreference
  event" holds (the marker, valid only for the restore-fence generation it was completed under,
  and only after a walk that judged every mark against a complete reference set), no mark that
  a later routed unreference could be judged by survives the walk, and every installed
  `{generation}` obligation is drained by the deployed custodian. **Sources:** the maintainer's rule 2026-07-25; 0016's refutation standard
  (`0016:2802-2813`); decision 4 rules 1–2 (`0016:1079-1290`); `gc.rs:69` and its safety gate
  at `:654`. SELF-TEST: it binds the routing in core, the cleanup pass and the gate in the
  custodian, the drain's dispatch and GC.
- **Repo + branch target:** getwyrd/wyrd @ main
- **Depends on:** 1062, 1007, 1008, 1009
- **Conflicts with:** 1013
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
  **Re-pointed 2026-11-11:** #659's child-1 (#1012) was itself split by mode into #1061 (cursor,
  walk, dispatch leg, `retire:records:` drain) and #1062 (`retire:bytes:` drain: mark guard,
  placement hold, owned-residue hold). "child-1" in this brief means both; `Depends on` names
  #1062, which brings #1061.
- **Scope:** the orphan-identity cleanup pass, its conditional drops, and its durable completion
  marker, including its validity across a metadata restore (a marker an image brings back never
  reads as complete once the post-restore pass has run); GC's fallback to "every mark protects"
  until a valid marker is observed; the switch of `unlink` and
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
  - the trap's mark (published part, no committed map) dropped → (F3) fails;
  - a stale mark on a staged part's fragment kept → (F2) fails (GC reclaims before the drain
    marks);
  - a drop committed without requiring the judged record's bytes → (F5) fails;
  - the marker written when the walk reaches the end, whatever it met → (F6) fails;
  - the marker read without its generation → (F7) fails;
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
  - (d) After a metadata restore, the routing turns off again until the cleanup has walked the
    restored ledger. This relies on the post-restore pass running, as the runbook already
    requires before gateways return (`cli.rs:1482-1494`); a restore whose pass never runs is
    outside the marker's reach, and the docs say so.
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
  - **The cleanup's race and completion:** the inline `unlink` batch (deletes the inode, blind-puts
    each mark: `crates/core/src/metadata.rs:2161-2191`); the `reclaiming` rule
    (`metadata.rs:154-168`); GC's refusal to certify on an incomplete reference set
    (`crates/custodian/src/gc.rs:524-533`, the walk going on past an unreadable record
    `:1184-1188`), on `d04e507`.
  - **Restore:** `MPUFENCE_KEY` (`crates/core/src/multipart.rs:1243` on `db1a066`), the
    post-restore pass's generation opened first (`crates/custodian/src/restore.rs:517-522`,
    `open_generation` `:917`), and the runbook's warning that an image brings back an old record
    (`crates/server/src/cli.rs:1482-1494`).
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
dispatched from child-1's drain leg in `reconcile_step`, judges each mark against the records
that name its fragment. What it must achieve: once the marker is written, no mark sits on a
fragment that a later routed `unlink` or supersede could unreference, because GC's keyed lookup
would judge that fragment by the old mark (a legacy mark, or one whose obligation has drained,
reads as "not draining", `gc.rs:756-762`) before the drain marks it. Per mark:

- `reclaiming` ⇒ leave it (no writer touches one, `metadata.rs:154-168`);
- structured, its event's `retire:bytes:` obligation present ⇒ leave it (a drain mid-retirement,
  mark-then-delete; GC's keyed lookup already protects it);
- its fragment named by a committed map ⇒ drop;
- named only by staged records (an `sidx:` entry, or the `part:` record of a part no Complete
  published) ⇒ drop: nothing has unreferenced that fragment yet, so the mark evidences nothing,
  and its own retirement (the bytes drain's arm c) or its publication names it next;
- named only by a **published** part's records (its number in a pending `retire:records:`
  `{parts}` obligation) and by no committed map ⇒ keep it byte-identical: it is the evidence of
  the delete or overwrite that unreferenced the object, and the records drain deletes those
  records without marking (the 2026-09-12 trap);
- named by nothing ⇒ not this pass's (GC's ordinary input).

**No re-stamping.** No event identity a re-stamp could give is sure to be the one that later
unreferences the fragment, so a re-stamped mark is the stale mark again with a fresher date. The
2026-09-12 comment allowed "re-stamp in that case, or drop only against committed references";
keeping the trap's mark byte-identical meets its point (the fragment never ends with no mark and
no reference), and dropping against an unpublished staged record is safe for the reason above.

**Every drop is conditional.** One batch `require`s the mark's exact bytes and the exact bytes of
every record the judgement read: the inode (and `seg:` record) for a committed reference; the
`part:` or `sidx:` record and the session's `mpu:` record for a staged one (a Complete rewrites
the session record, so a publication racing the drop fails it). The inline `unlink` deletes the
inode and blind-puts its mark in one batch (`metadata.rs:2161-2191`), so the unlink and the drop
cannot both commit on the same observation.

**Completion.** The walk is complete only when it has covered the whole range, judged every mark
against a complete reference set (committed and staged: GC's own test, `gc.rs:524-533`), and every
write it made has settled (committed, or re-read and found moot). A walk that met an unreadable
record ends with no marker and a typed blocked outcome naming it; a later walk starts again. An
interrupted walk writes no marker.

**The marker** is a new singleton record: state its key shape, writer, deleter and which scans
see it (ADR-0046). It names the restore-fence generation current when its walk began
(`MPUFENCE_KEY`; an absent record is a value too), its write `require`s that generation record's
bytes, and it reads as complete only while that generation is still current. Every post-restore
pass opens a new generation as its first write (`restore.rs:517-522`), so a marker an image brings
back reads as not complete, and the cleanup walks the restored ledger again: 0016 names a metadata
restore among the sources of stale marks (`0016:1254-1256`). Do may choose another mechanism with
the same property (the post-restore pass deleting the marker, say) and says why in
`build-notes.md`.

Until a valid marker is observed, the identity-keyed paths stay off: GC treats every mark as
protecting (its existing `Defer` arm), and `unlink` / supersede behave exactly as on the base.

**Routing switch** (`0016:1081-1106`). With a valid marker observed, `unlink` and
`commit_chunk_map_superseding{,_leased}` remove the references and install **one**
`retire:bytes:{generation}` (the removed map's chunks, and its `seg:` range if segmented) in the
same batch, instead of inline marks, under `require_absent` with the `g:<inode>:<version>` token.
A small flat object may keep the inline path when its **whole** batch fits one batch's budget,
counted as preconditions plus deletes plus mark puts against `MAX_BATCH_OPS` (500), and its
bytes against `MAX_BATCH_BYTES`; above that, and for every segmented object, the obligation is
the only path. child-1's drain re-reads each `seg:` record's current placement at drain time.

**Docs currency** (`AGENTS.md:154-157`): `docs/design/architecture/06-runtime-view.md` (the GC
paragraphs at `:80-86`): deletes and overwrites install a retirement obligation that the
custodian drains, marking before deleting; grace starts at the drain; the migration marker, what
it gates, and that a metadata restore turns the routing off until the cleanup has walked again
(relying on the post-restore pass having run).

## Alternatives considered

- **Flip the routing first, dispatch later.** The discontinued run's permanent leak. Rejected;
  `Depends on child-1`.
- **Flip the routing with the drain, gate later.** Opens the stale-mark window 0016 forbids
  (`0016:1259-1278`). Rejected; the gate travels with the routing.
- **Expand pending obligations into GC's reference set.** Unbounded memory for one segmented
  generation (`0016:1236-1242`). Rejected; keyed lookup.
- **Re-stamp a mark found on a `part:`-referenced fragment** (this brief's first draft). The
  re-stamped mark survives into a later publication and routed unlink and is judged by GC before
  the drain marks (plan review, 2026-11-11). Rejected; drop for an unpublished part, keep for the
  trap.
- **A marker valid for ever once written.** A restore brings back an old marker with the stale
  marks 0016 says a restore produces. Rejected; the marker is bound to the restore-fence
  generation.

## Impact & compatibility

Production behaviour changes for DELETE and overwrite of objects past one batch of fragments,
once the marker is written: reclamation moves from the request to the custodian, a little later
than today. A new singleton record (the marker) is added.

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a draft PR MAY
happen during the cycle (useful for CI feedback). The PR MUST NOT be marked ready before
sign-off accepts.

Plan-review response (2026-11-11, all four findings verified on `d04e507`/`db1a066` and taken):
(1) re-stamping gave a mark no sure identity, so the cleanup now drops a stale mark on an
unpublished part, keeps the trap's mark byte-identical, and never re-stamps; (F2) runs the
review's publish-then-unlink sequence and (A) now asserts the bytes stay during the pending
retirement and the fresh grace; (2) every drop requires the exact bytes of the mark and of the
records it judged by, with a wrapper-forced race against `unlink` (F5); (3) completion needs a
complete reference set and settled writes, with an unreadable inode/segment/part blocking the
marker (F6); (4) the marker is bound to the restore-fence generation, now in Scope, tested by
(F7), with the reliance on the post-restore pass stated.

## Iteration 1 — carry-forward (from the previous attempt)
- Sign-off rationale: Rebuild inside this slice; the routing change itself is red->green (reviewer-confirmed). Fix the reproduced safety defects, each with a regression test that fails today: 1. Premature reclaim (P1): a preserved `reclaiming` mark on a live fragment survives cleanup (orphan_identity.rs:934), the marker still certifies (:266), and after a routed unlink GC resumes it with no grace (gc.rs:696). Do not let such marks authorize routing or reclaim before the new obligation's grace. Extend F4 through restore -> cleanup -> routed delete -> GC before drain. 2. Permanent leak (P1): an undecodable session (Phase::Unknown, orphan_identity.rs:540/:563/:1053) is treated as staged/live and its marks are dropped. Unknown must Leave the mark or block certification, never drop. Cover F3 with a damaged Completed session. 3. Read-budget underflow (P2): READS_PER_MARK=7 is one short (orphan_identity.rs:142, :920, :938). Charge the real worst case (or hoist the token lookup out of the retry loop) and use checked/saturating subtraction; add the two-conflicts-at-the-limit regression. 4. Marker never written on a busy store: a "namer-moved" mark from an inline unlink/overwrite between the index read and the page read discards the whole lap (orphan_identity.rs:902-907). Stop the window before that key, or settle it as moot under the closed gate; test that the marker is still written within a bounded number of passes with steady deletes. Also fix: the pending-lease bypass (metadata.rs:2311 / gc.rs:732; add a test with release omitted and ExpiredPendingPolicy::Reclaim); bound memory in the retirement-ledger index walk (orphan_identity.rs:769, :809); add stale-mark cases on committed segmented objects (C5 survivors at :744, :981). Keep the brief's closed-gate design ("every mark protects"), but correct the 06-runtime-view.md wording that it "only delays reclamation". Rebase onto current main (the patch does not apply; conflicts with #1013's drain dispatch in reconcile_step) and re-run CI.
- Sign-off session carry-forward (captured live, before §9 flattened it):
  Rebuild inside this slice; the routing change itself is red->green (reviewer-confirmed). Fix the reproduced safety defects, each with a regression test that fails today:
  1. Premature reclaim (P1): a preserved `reclaiming` mark on a live fragment survives cleanup (orphan_identity.rs:934), the marker still certifies (:266), and after a routed unlink GC resumes it with no grace (gc.rs:696). Do not let such marks authorize routing or reclaim before the new obligation's grace. Extend F4 through restore -> cleanup -> routed delete -> GC before drain.
  2. Permanent leak (P1): an undecodable session (Phase::Unknown, orphan_identity.rs:540/:563/:1053) is treated as staged/live and its marks are dropped. Unknown must Leave the mark or block certification, never drop. Cover F3 with a damaged Completed session.
  3. Read-budget underflow (P2): READS_PER_MARK=7 is one short (orphan_identity.rs:142, :920, :938). Charge the real worst case (or hoist the token lookup out of the retry loop) and use checked/saturating subtraction; add the two-conflicts-at-the-limit regression.
  4. Marker never written on a busy store: a "namer-moved" mark from an inline unlink/overwrite between the index read and the page read discards the whole lap (orphan_identity.rs:902-907). Stop the window before that key, or settle it as moot under the closed gate; test that the marker is still written within a bounded number of passes with steady deletes.
  Also fix: the pending-lease bypass (metadata.rs:2311 / gc.rs:732; add a test with release omitted and ExpiredPendingPolicy::Reclaim); bound memory in the retirement-ledger index walk (orphan_identity.rs:769, :809); add stale-mark cases on committed segmented objects (C5 survivors at :744, :981).
  Keep the brief's closed-gate design ("every mark protects"), but correct the 06-runtime-view.md wording that it "only delays reclamation".
  Rebase onto current main (the patch does not apply; conflicts with #1013's drain dispatch in reconcile_step) and re-run CI.
- Failing gate: C4 diff coverage: changed lines executed by the patch's tests (advisory) — diff coverage not measured — patch.diff does not apply on origin/main
- Failing gate: C5 surviving mutants on the bundle diff (cargo mutants --in-diff) (advisory) — 243 mutants tested in 57m: 100 missed, 118 caught, 25 unviable
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 5 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_1014/review-
- Full previous attempt preserved in `iteration-v1/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 2 — carry-forward (from the previous attempt)
- Sign-off rationale: Rebuild inside this slice (logic ~102 KB, below the 150 KB split line). Round 2 closed round 1's list; fix the new reproduced P1s, each with a regression test that fails today: 1. Damaged walk cursor (`{"after":"ff"}`) certifies the marker without judging marks: validate the `orphan:` prefix and length at decode and restart the lap (core/src/orphan_identity.rs:235), like GC's cursor check (gc.rs:2123). 2. A mark naming an unrelated live obligation is left alone *and* certified (custodian/src/orphan_identity.rs:1025): keep the mark but don't certify unless the obligation names that fragment. Extend F4 through drain -> routed unlink -> GC before drain. 3. An untrusted placement or undecodable `sidx:` entry names the chunk whole, and the sweep then drops marks (:454-461, :1038, :1123): whole-chunk namers give Leave or Block, never Drop. 4. A restored `reclaiming` mark whose only namer is a flat generation obligation deadlocks sweep, drain and GC (:1016, retirement/marks.rs:477, gc.rs:717): tell this drain-owned case apart from a live reference and give it a safe way to finish. Test the closed-gate cycle. 5. P2: keep only a bounded set of blocker diagnostics (:534). Also add X3 (segmented-obligation drop path, :1090-1098) as a test. Steady-state stale marks from repair (adversary finding: repair/rebalance can rebuild a fragment onto a server still holding an old mark, after the marker exists): not decided at this sign-off; do not resolve it silently — record it in build-notes.md.
- Sign-off session carry-forward (captured live, before §9 flattened it):
  Rebuild inside this slice (logic ~102 KB, below the 150 KB split line). Round 2 closed round 1's list; fix the new reproduced P1s, each with a regression test that fails today:
  1. Damaged walk cursor (`{"after":"ff"}`) certifies the marker without judging marks: validate the `orphan:` prefix and length at decode and restart the lap (core/src/orphan_identity.rs:235), like GC's cursor check (gc.rs:2123).
  2. A mark naming an unrelated live obligation is left alone *and* certified (custodian/src/orphan_identity.rs:1025): keep the mark but don't certify unless the obligation names that fragment. Extend F4 through drain -> routed unlink -> GC before drain.
  3. An untrusted placement or undecodable `sidx:` entry names the chunk whole, and the sweep then drops marks (:454-461, :1038, :1123): whole-chunk namers give Leave or Block, never Drop.
  4. A restored `reclaiming` mark whose only namer is a flat generation obligation deadlocks sweep, drain and GC (:1016, retirement/marks.rs:477, gc.rs:717): tell this drain-owned case apart from a live reference and give it a safe way to finish. Test the closed-gate cycle.
  5. P2: keep only a bounded set of blocker diagnostics (:534).
  Also add X3 (segmented-obligation drop path, :1090-1098) as a test.
  Steady-state stale marks from repair (adversary finding: repair/rebalance can rebuild a fragment onto a server still holding an old mark, after the marker exists): not decided at this sign-off; do not resolve it silently — record it in build-notes.md.
- Failing gate: C4 diff coverage: changed lines executed by the patch's tests (advisory) — diff coverage not measured — patch.diff does not apply on origin/main
- Failing gate: C5 surviving mutants on the bundle diff (cargo mutants --in-diff) (advisory) — 242 mutants tested in 50m: 80 missed, 134 caught, 28 unviable
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 15 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_1014/review
- Full previous attempt preserved in `iteration-v2/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 3 — carry-forward (from the previous attempt)
- Sign-off rationale: Re-plan (and likely split) this slice instead of a 4th rebuild. Three rounds each reproduced NEW data-safety defects, logic grew to ~130 KB (was ~102 KB), and the brief leaves design questions open. The re-plan must turn the round-3 builder-level problems into explicit brief requirements, each with a test that fails today: 1. Damaged sweep cursor (P1): `{"after":"6f727068616e3a7e"}` (orphan:~) or orphan:\xff decodes, the lap sees an empty window and certifies the marker; a routed unlink then lets GC reclaim a live fragment before the drain marks it (core/src/orphan_identity.rs:248, custodian/src/orphan_identity.rs:278-313). Accept only canonical orphan keys and record the last canonical key judged (mind gc.rs:2114-2119); regression through routed unlink -> GC before drain. 2. Segmented delete with a missing seg: row (P1): routing keeps only map.group() and drops the segment table (metadata.rs:2277); unlink commits and the row's fragments leak forever. Check the rows against segment_count before routing and refuse with a typed error; cover unlink and overwrite, missing middle and final segment. 3. Malformed flat placement becomes an undrainable obligation (metadata.rs:2316 vs retirement/marks.rs:224): refuse or handle before removing the live inode; cover supersede. 4. Truncated cursor never advances (core/src/orphan_identity.rs:196-198): use a strictly advancing resume position or report a blocked outcome; multi-window malformed-key regression. 5. Inode read uses a whole-prefix scan (custodian/src/orphan_identity.rs:645, :909) and fails past SCAN_CAP: page it; test with a lowered backend cap. 6. Unbounded namers (custodian/src/orphan_identity.rs:691, :720, :1060): keep only the winning evidence plus class flags; test overlapping obligations. 7. Tests that cannot fail: the foreign/live-session/undecodable-session blocking rules (:1210-1214) need one F4 case each; the gate pin on routed writes (metadata.rs:2163, OpenGate::pinned) needs a Conflict race test or the claim removed; the read-budget test must count reads and assert the cap (:1111, :1371). Also resolve in the brief the open plan-advisory items: what a re-stamped mark's identity is, conditional (commit-time-validated) cleanup deletes vs a concurrent unlink, what "sweep complete" requires when references are unreadable, and how the marker is invalidated after a metadata restore. The repair/rebalance steady-state stale-mark question (reconstruction.rs:1232, rebalance.rs:642) was not decided at this sign-off; the re-plan must decide it explicitly (fix in scope or a prerequisite issue), not leave it to Do. Suggested split: (a) sweep + marker (cursor, paging, memory bound, completion, restore invalidation); (b) routed delete for flat and segmented objects, depending on (a).
- Sign-off session carry-forward (captured live, before §9 flattened it):
  Re-plan (and likely split) this slice instead of a 4th rebuild. Three rounds each reproduced NEW data-safety defects, logic grew to ~130 KB (was ~102 KB), and the brief leaves design questions open. The re-plan must turn the round-3 builder-level problems into explicit brief requirements, each with a test that fails today:
  1. Damaged sweep cursor (P1): `{"after":"6f727068616e3a7e"}` (orphan:~) or orphan:\xff decodes, the lap sees an empty window and certifies the marker; a routed unlink then lets GC reclaim a live fragment before the drain marks it (core/src/orphan_identity.rs:248, custodian/src/orphan_identity.rs:278-313). Accept only canonical orphan keys and record the last canonical key judged (mind gc.rs:2114-2119); regression through routed unlink -> GC before drain.
  2. Segmented delete with a missing seg: row (P1): routing keeps only map.group() and drops the segment table (metadata.rs:2277); unlink commits and the row's fragments leak forever. Check the rows against segment_count before routing and refuse with a typed error; cover unlink and overwrite, missing middle and final segment.
  3. Malformed flat placement becomes an undrainable obligation (metadata.rs:2316 vs retirement/marks.rs:224): refuse or handle before removing the live inode; cover supersede.
  4. Truncated cursor never advances (core/src/orphan_identity.rs:196-198): use a strictly advancing resume position or report a blocked outcome; multi-window malformed-key regression.
  5. Inode read uses a whole-prefix scan (custodian/src/orphan_identity.rs:645, :909) and fails past SCAN_CAP: page it; test with a lowered backend cap.
  6. Unbounded namers (custodian/src/orphan_identity.rs:691, :720, :1060): keep only the winning evidence plus class flags; test overlapping obligations.
  7. Tests that cannot fail: the foreign/live-session/undecodable-session blocking rules (:1210-1214) need one F4 case each; the gate pin on routed writes (metadata.rs:2163, OpenGate::pinned) needs a Conflict race test or the claim removed; the read-budget test must count reads and assert the cap (:1111, :1371).
  Also resolve in the brief the open plan-advisory items: what a re-stamped mark's identity is, conditional (commit-time-validated) cleanup deletes vs a concurrent unlink, what "sweep complete" requires when references are unreadable, and how the marker is invalidated after a metadata restore. The repair/rebalance steady-state stale-mark question (reconstruction.rs:1232, rebalance.rs:642) was not decided at this sign-off; the re-plan must decide it explicitly (fix in scope or a prerequisite issue), not leave it to Do.
  Suggested split: (a) sweep + marker (cursor, paging, memory bound, completion, restore invalidation); (b) routed delete for flat and segmented objects, depending on (a).
- Failing gate: C4 diff coverage: changed lines executed by the patch's tests (advisory) — diff coverage not measured — patch.diff does not apply on origin/main
- Failing gate: C5 surviving mutants on the bundle diff (cargo mutants --in-diff) (advisory) — 323 mutants tested in 69m: 103 missed, 188 caught, 32 unviable
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 17 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_1014/review
- Full previous attempt preserved in `iteration-v3/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).
