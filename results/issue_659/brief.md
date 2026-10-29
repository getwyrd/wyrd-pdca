# core,custodian: retirement routing + drain + terminal delete with production dispatch (636.6) — issue 659

> The Plan artifact for slice 6 of #636. Do reads ONLY this file. The `- **Label:** value`
> lines are parsed by the driver; keep their shape. The design is normative in proposal 0016,
> `docs/design/proposals/draft/0016-multipart-commit-protocol.md`: read **decision 4** rules 1–2
> (`0016:1079-1290`, above all the three-arm mark rule `:1220-1226`, the keyed protection
> `:1228-1249`, and the migration gate `:1251-1278`), the **Retirement drain step**, **Owned-`sidx:`
> drain step**, **Object delete** and **Terminal session delete** rows of the batch inventory
> (`0016:668-674`), decision 5's backstop (`0016:1654-1668`), and the terminal-delete rule of §2
> (`0016:582-602`) before writing code. Citations verified against `getwyrd/wyrd` `origin/main` @
> `12dd082` on 2026-10-19. This slice builds on #658 (Complete), #657 and #653, which are not on
> `main` yet; build on the base this bundle is given.
>
> **The one rule this slice exists to keep:** the routing of `unlink` and supersede through
> `retire:bytes:{generation}` flips **in the same slice** that makes the deployed custodian run
> the drain. The discontinued #636 patch flipped the routing while the drain had callers only in
> tests: deleting a 30-chunk RS(6,3) object left all 270 fragments on disk after GC, for good,
> and its own classification oracle counted that leak as safe.

- **Slug:** multipart-retirement-drain
- **Track:** alpha
- **Kind:** enhancement (design proposal)
- **Goal:** Every multipart slice so far installs `retire:` obligations (Abort's `{session, all}`,
  #657's per-part `{chunks}`, #658's `{parts}` and `{generation}`), and nothing drains them; a
  session's records and its admission slot are never released. This slice adds the drain and
  dispatches it in production: a byte- and operation-budgeted, cursor-keyed drain step for
  `retire:bytes:` (mark, then delete the naming records) and `retire:records:` (delete only, never
  mark); the walk of an `Aborting` or `Completed` session's owned `sidx:` entries; the terminal
  session delete with its exactly-once `mpuctl` decrement; the drain's dispatch from
  `reconcile_step` in the deployed custodian; the orphan-identity migration gate; and, last, the
  switch of `unlink` and the superseding commits from inline orphan marks to one
  `retire:bytes:{generation}` obligation.
- **Success criterion:** A NEW test file `crates/custodian/tests/multipart_retirement_drain.rs`
  (core-level tests MAY also go in another NEW `crates/core/tests/*.rs` file), over
  `RedbMetadataStore::in_memory()` and a real chunk store or placement-aware double, driving the
  drain **through `reconcile_step`** with the contexts the deployed loop passes
  (`crates/server/src/custodian.rs:397`), not through a test-only entry. It shows all of the
  following.
  **(A) Positive reclamation after `unlink`** (the discontinued run's leak). `MAX_BATCH_OPS` is
  **500** (`crates/core/src/multipart.rs:5093-5106`), and today's unlink batch spends 2
  preconditions + 2 deletes + one mark put per fragment (`crates/core/src/metadata.rs:2133-2165`).
  (A1) The tracker's historical case: a committed 30-chunk RS(6,3) object (270 fragments, 274
  operations, which fits one batch, so the inline path may still serve it) is deleted with
  `metadata::unlink`. After reconcile steps past the orphan grace, every one of its 270 fragments
  is gone. (A2) **Past the batch budget:** a committed flat 100-chunk RS(6,3) object (900
  fragments, 904 operations inline) is deleted with `metadata::unlink`. **Before any reconcile
  step**, the unlink's own batch has installed exactly one `retire:bytes:g:<inode>:<version>`
  obligation and written no `orphan:` mark. After reconcile steps past the orphan grace, **every
  one of its 900 fragments is gone from the chunk store** and no `retire:` obligation or
  `orphan:` mark of it remains. **The boundary, preconditions counted:** an 83-chunk RS(4,2)
  object (498 marks + 2 deletes + 2 preconditions = 502 operations; 500 if the preconditions were
  ignored) also takes the obligation path, with no inline mark. (A3) The same as A2 for an
  ordinary overwrite (`write::commit_overwrite`) of such an object, for the prior generation's fragments, and for an
  `unlink` of a **segmented** object published by #658's Complete (no inline path exists for
  one). Asserted as fragments absent, never as "no failure observed".
  **(B) Complete, then drain.** After a Complete (#658) and the drain: no `part:` or `psum:` record
  of a published part remains; **no** `orphan:` mark names a chunk of the published map
  (`0016:1503`); a subset Complete's unnamed part has every fragment marked **before** its `part:`
  record is deleted (a recording wrapper shows the order) and is reclaimed after grace.
  **(C) Convergence at scale.** A `{parts}` obligation over **at least 4,001 parts** (planted as
  canonical records is fine) drains to empty across many steps, each commit within the byte and
  operations budgets (preconditions counted); running the whole drain a second time is a no-op. A
  single entry whose own deletes exceed one batch's budget still makes progress every step and
  finishes. Assert that the obligation's stored cursor (see Design) advanced every step, that a
  drain restarted from a fresh process state (nothing in memory) resumes from the stored cursor,
  and that the obligation is deleted only after its last batch.
  **(D) Owned walk and terminal delete.** An `Aborting` session holding committed parts and
  crashed in-flight `sidx:` residue (from #657's staging), and a `Completed` session holding
  in-flight residue, each drain to: every owned fragment marked from its `sidx:` placement then the
  entry deleted; no `sidx:` record of the session left. **`Aborting`:** the terminal delete fires
  **once**, `mpuctl.count` down by exactly one (two drainers racing the terminal delete through a
  wrapper still decrement once); no `slot:` record left; the `seggrp:` marker deleted when no
  `seg:` record names the nonce and **kept** when one does (`0016:674`). **`Completed`:** with
  the contexts the deployed loop passes, the `mpu:` record (the tombstone) is **kept** after its
  obligations and residue drain, and an identical Complete retry still answers
  `AlreadyCompleted` (`crates/core/src/multipart.rs:4789-4795`); with the drain context's
  tombstone input saying the window has passed (see Design), the terminal delete fires once, as
  for `Aborting`. The terminal delete does not fire while the session's `sidx:` range is
  non-empty.
  **(E) Validated placements and untrusted records.** A `part:` or `sidx:` record whose placement
  fails validation (length not the scheme's fragment count, e.g. a decoded `RS(0,0)`) is never
  deleted unmarked. For an `Aborting` session, such a record and an owned `sidx:` value that will
  not decode are retired **by chunk id** per the 2026-09-18 tracker comment's rule: every fragment
  bearing that chunk id is marked, then the record is deleted, and after GC past grace those
  fragments are gone and the terminal delete has fired. Controls: an `Open` session with the same
  kind of record keeps the record and every fragment of its chunk; a committed object's fragments
  on the same servers are untouched.
  **(F) The migration gate** (`0016:1251-1278`). Before the cleanup marker exists, the
  identity-keyed retirement paths are off and GC treats every `orphan:` mark as protecting. The
  cleanup pass walks `orphan:` page-wise, and a mark found on a still-referenced fragment is
  **re-stamped, not dropped**, when the reference is a `part:` record (the trap the 2026-09-12
  comment records: dropping it there strands the fragments after a later `unlink`). When the pass
  finishes it writes the durable marker, and only then do (A)'s routing and GC's keyed lookup
  apply.
  **(G) Restore stays accurate between drain batches** (the `deferred: #659` at
  `crates/custodian/src/restore.rs:1156`). An `Aborting@E+1` session fenced from `Completing@E`
  with segments written and parts staged (its two obligations: `retire:bytes:s:<id>:<E>` and
  `retire:records:s:<id>:<E>` owing `{seg}`, seeded as `restore_completing_fence.rs` seeds them)
  is drained **one step at a time**. After **every** step, the production
  `reconcile_after_restore` (`restore.rs:597`) runs and does not name the session (no
  `ChunkInNoPart`, `restore.rs:1225-1234`, and no `NoDeleter`), until the terminal delete.
- **Falsifiability:** RED is reachable in-process on `cargo test -p wyrd-custodian` (and
  `-p wyrd-core`). (A2)/(A3) are red on any build where the routing flips but `reconcile_step`
  does not dispatch the drain (the fragments stay): the discontinued run's exact defect. (A1)
  alone is not, since a 274-operation unlink may stay inline; that is why A2 exists. (C) is red on
  a drain with no stored cursor (never finishes) or one that marks the obligation drained after a
  truncated derivation (the obligation vanishes with parts left). (D)'s double-decrement race and
  (E)'s ordering are forced by wrappers, not timing. (G) is red against independent draining of
  the two obligations: the bytes drain deletes a `part:` record a surviving segment still needs,
  and the next restore run reports `ChunkInNoPart`.
- **Invariant to restore:** **C-1** (`docs/principles.md` §5 C-1 and its §6 row *storage
  lifecycle / reclamation*). Every durable fragment is, at every instant, named by a record or by
  an `orphan:` mark whose grace the reader can rely on: the drain marks before it deletes the
  record that named the bytes, never marks a published chunk, and never deletes a record whose
  placement it cannot trust without marking every position its chunk could occupy; every installed
  obligation is driven to empty by the deployed custodian in bounded batches; and every session
  state that the drain owns has an exit (terminal delete) that fires exactly once. **Sources:** the
  maintainer's rule 2026-07-25 (via principles.md §5); 0016's refutation standard
  (`0016:2802-2813`); decision 4 rule 2 (`0016:1123-1128`); the custodian's "never reclaim a
  referenced fragment" (`crates/custodian/src/gc.rs:69`, the safety gate at `:654`). SELF-TEST: it binds the routing in
  core, the drain, the dispatch in the deployed custodian and GC, so guarding one module cannot
  satisfy it.
- **Repo + branch target:** getwyrd/wyrd @ main
- **Depends on:** 658, 634, 662
- **Conflicts with:**
- **Ordering note:** `Depends on 658`: Complete's obligations and published objects are what (B)
  drains; through it this bundle also builds on #657 and #653. `Depends on 634`: the drain walks
  `retire:` with `scan_page` (merged). `Depends on 662`: GC's staged reference set and keyed
  `retire:bytes:` protection (merged). Wave 3 of the 2026-10-19 batch. **Intake-cap override
  (wyrd-pdca-P1):** the human allowed this id in over the alpha cap in the 2026-10-19 Plan session
  ("override the alpha cap"); count when written 7/6, room 0, need 1. **Likely oversized** (see
  Open questions): if the sizer says so, the split point is recorded there; a split's children need
  their own override, since this one covers #659 only.
- **Scope:** the retirement drain step for both modes, the owned-`sidx:` walk, the terminal session
  delete, the drain's dispatch from `reconcile_step` in the deployed custodian, the orphan-identity
  cleanup pass with its durable completion marker and the gate on it, the retirement of untrusted
  staged records for `Aborting` sessions, the switch of `unlink` and the superseding commits to
  `retire:bytes:{generation}` above one batch's worth of fragments, and the docs. / out of scope:
  - the reaper's fences (idle `Open`, `W_completing` rollback, `W_session`) and the `W_tombstone`
    clock (#625): the terminal delete here fires for `Aborting` sessions and, for `Completed`
    sessions, only when the drain context's tombstone input says the window has passed; the
    deployed caller passes "retain" until #625 supplies that input;
  - the operator abort (#633);
  - seeded DST: #660 owns exactly three cases, one of them "two concurrent drainers"; do not add a
    DST scenario here (the discontinued run's review flagged extra scenarios as structure creep);
  - the bulk `DeleteObjects` per-request batching (#509's, inside the per-transaction bound
    `0016:1107-1122` settles);
  - the gateway / S3 verbs (#668–#670 under #508);
  - any edit to proposal 0016 or to any ADR (ADR-0045's refinement is #811); `Cargo.lock` and
    `deny*.toml`.
- **Repro instruction:** n/a for the drain (new functionality). For the routing: today `unlink`
  writes every orphan mark inline in its delete batch (`crates/core/src/metadata.rs:2141-2164`)
  and refuses a segmented object (`:2148-2151`); `commit_chunk_map_superseding` does the same for
  the prior map (`:2235`, refusing a segmented prior at `:2188-2193` for `commit_chunk_map`).
  Deleting a 270-fragment object today puts 274 operations in one batch (2 preconditions, 2
  deletes, 270 mark puts), inside `MAX_BATCH_OPS` = 500. Past 55 chunks at RS(6,3) the batch
  exceeds it: a 100-chunk object's unlink is 904 operations.
- **Difficulty:** high
- **Do model:** opus
- **External dependencies:** `typos`, `docs-renderer`
- **Test file:** `crates/custodian/tests/multipart_retirement_drain.rs`, a **NEW** file (C4-verify
  treats an added `crates/*/tests/*.rs` as the per-fix discriminator).
- **Verification posture:** (a) net-new for the drain; the test cannot compile on C4-verify's
  reverted base, so C4-verify reports **UNVERIFIABLE** (exit 77), a pre-declared sign-off item.
  **Built and exercised at Check:** the drain, the walk, the terminal delete and the gate, through
  `reconcile_step` as deployed; the routing change through the production `unlink` and
  `commit_overwrite`.
  **Demonstrated reds (required, recorded in `build-notes.md`).** Do makes each temporary edit,
  shows the named leg fail, then restores the file:
  - the drain's dispatch removed from the deployed loop's path (routing left switched) → (A2)
    fails;
  - the inline-path threshold counting mutations only (preconditions ignored) → (A2)'s 83-chunk
    RS(4,2) boundary case fails (it goes inline);
  - the drain cursor not persisted (each step re-derives from the start) → (C) fails;
  - the obligation deleted after the first batch → (C) fails;
  - the `sidx:`-empty gate removed from the terminal delete → (D) fails;
  - a `Completed` session terminal-deleted without the tombstone input → (D)'s `Completed` leg
    fails;
  - a mark on a `part:`-referenced fragment dropped instead of re-stamped → (F) fails;
  - an untrusted record deleted without marking → (E) fails;
  - the segments-before-parts ordering rule removed → (G) fails.
- **Production reach:**
  - (a) **This slice changes production behaviour**, deliberately: every DELETE and every
    overwrite above one batch's worth of fragments installs an obligation, and the deployed
    custodian drains it (`crates/server/src/custodian.rs:397`). (A) is the guard that the live path
    reclaims, asserted as fragments gone.
  - (b) Grace now starts when the drain writes each mark, slightly later than today's inline mark:
    reclamation is never earlier than today, only later (`0016:1098-1104`), an accepted cost.
  - (c) Until the migration marker is written, the gate keeps the old protective behaviour, so a
    deployment upgrades without a window where GC could reclaim against a stale mark.
  - (d) Multipart obligations are drained too, but nothing in production creates a session until
    #508. In production an `Aborting` session reaches its terminal delete; a `Completed` one keeps
    its tombstone record until #625 supplies the window (Design), which lands with or before #508.
- **Citations expected:** Do must cite `path:line` on the target branch for every change.
  - **Peer for the deployed dispatch:** `reconcile_step` (`crates/custodian/src/reconciliation.rs:126-168`)
    and its one production caller, `crates/server/src/custodian.rs:397`. The drain must run on that
    path. A new parameter on `reconcile_step` reaches 25 test files that call it; dispatching the
    drain as a named leg through an existing context avoids that, and either is acceptable if the
    deployed caller runs it.
  - **Peer for page-wise ledger walks and mark guards:** GC's walk (#661) and its keyed protection
    `retirement_draining` (`crates/custodian/src/gc.rs:756`); the mark codec `OrphanMark`
    (`crates/core/src/metadata.rs:142`, `encode_orphan_mark` `:257`, `decode_orphan_mark` `:277`)
    and its three value shapes (`docs/design/architecture/08-crosscutting-concepts.md:87`).
  - **Untrusted staged records today:** GC and restore hold every fragment of such a chunk
    (`StagedSet`, `crates/custodian/src/gc.rs:1337-1360`, reason `untrusted-staged-record`);
    restore reports them (`RestoreReport::staged_untrusted`, `crates/custodian/src/restore.rs:179`)
    and promises no cleanup until this issue. A malformed committed placement already blocks
    drains (`crates/custodian/src/desired_state.rs:234-246`).
  - **Markers to settle:** `deferred: #659` at `crates/custodian/src/restore.rs:1156`: update it
    to say what the drain now guarantees between two restore runs.
  - **Records and budgets:** `RetireMode` / `retire_key` / `retire_session_range`
    (`crates/core/src/multipart.rs:1366`, `:1492`, `:1502`), `decode_retire_obligation` (`:3891`),
    `RetirePayload` (`:3620`), `PartScope` (`:3173`), `StagedPlacement` (`:4045`),
    `decode_owned_entry` (`:4231`), `MAX_BATCH_BYTES` (`:5067`), `MAX_BATCH_OPS` (`:5106`),
    `MPUCTL_KEY` (`:1144`), `seggrp_key` / `seg_group_prefix` (`crates/core/src/metadata.rs:1521`,
    `:1516`), `scan_page` (`crates/traits/src/lib.rs:1438`).
- **Prior-art check (triage cycles):** searched by path `crates/core/src/metadata.rs`,
  `crates/core/src/multipart.rs`, `crates/custodian/src/{gc,reconciliation,restore,desired_state}.rs`,
  `crates/server/src/custodian.rs` and `docs/design/architecture/06-runtime-view.md` on
  `origin/main`. Merged: `scan_page` (#634), GC's paged ledger walk (#661), the staged reference
  set, reclaim-intent ordering and keyed protection (#662, PRs #807 and later), the restore fences
  (#664, #809, #810), the three mark shapes (#662). Nothing drains `retire:` today and no
  migration marker exists. Rejected: #636's single-slice attempt (discontinued 2026-10-10), whose
  drain re-derived the same first keys forever in one attempt and truncated derivation while
  marking the obligation drained in another, and whose routing flip shipped without dispatch. The
  new test file does not exist. Open PRs on getwyrd/wyrd: none.
- **Disposition hint:** new-feature

**Sources for claims taken from the tracker** (for the sign-off reader; Do need not open them):
the 2026-09-12 scope note moving the migration gate here,
https://github.com/getwyrd/wyrd/issues/659#issuecomment-5648454954, and the 2026-09-18 gap note
with the untrusted-record rule,
https://github.com/getwyrd/wyrd/issues/659#issuecomment-5732403299; copies in
`results/issue_659/notes.json`.

## Motivation

Without a drain every obligation is a promise nobody keeps: sessions never leave the store,
`mpuctl.count` only rises until every Create answers `503`, and staged bytes are protected for
ever. And without the routing switch, deleting or overwriting any object past 55 chunks at
RS(6,3) puts more operations in one batch than the protocol's per-batch budget (`MAX_BATCH_OPS` =
500, half the backend's deadline-derived limit), a maximal object's marks exceed the transaction
envelope outright (`0016:1091-1096`), and a segmented object cannot be deleted at all.

## Design

**Drain step** (`0016:668`). One obligation, one step: read it, derive the next batch of work from
its **cursor**, commit `require(retire:… == prior)` + the batch + the cursor advance, and delete the
obligation in the step that finishes it. Batches are bounded by encoded bytes **and** by operations
(`MAX_BATCH_BYTES`, `MAX_BATCH_OPS`). `retire:bytes:` writes `orphan:` marks under the three-arm
guard (`0016:1220-1226`: absent ⇒ write under `require_absent`; same event ⇒ skip; different event
⇒ re-stamp under `require(== prior)`), then deletes the records that named the bytes;
`retire:records:` only deletes records and never marks. A `{generation}` obligation re-reads each
`seg:` record's **current** placement at drain time (`0016:2432-2440`). An entry too large for one
batch on its own is split across steps with the cursor inside it, so it never re-derives the same
shape forever. Placements are validated before marking; see (E) for what an untrusted one gets.

**Where the cursor lives: inside the obligation** (normative, `0016:613-619`: "no cursor record
class"). This slice **extends the `RetirePayload` codec** (`crates/core/src/multipart.rs:3549-3562`
for the wire shape, `:3620-3632` for the payload) with one optional `cursor` component; no new key
or record class. Its rules:
- **Shape:** one position in the payload's own ordered work, the next entry and the next mark
  inside it (so an entry too large for one batch resumes mid-entry). Do chooses the spelling;
  state it in the type's doc.
- **Absent means "start"**, and is **omitted** when absent, as every other component is
  (`:3621-3622`), so a fresh obligation's bytes are unchanged and no installer on `main` (Abort),
  in #657 or in #658 changes.
- **Decoded against its payload** through `decode_retire_obligation`, like every component: a
  cursor past the payload's end, or on a component that has no ordered work, is a typed error.
- **Updated only by the drain step**, in the step's own batch, which requires the obligation's
  prior bytes and puts it back with the advanced cursor (the "1 cursor put" of `0016:668`). Its
  lifetime is the obligation's: the finishing step deletes both. After a crash or restart the
  next step resumes from the stored cursor.
- **The owned `sidx:` walk needs no cursor:** each of its batches deletes the entries it marked,
  so what is left is the progress.
- **Compatibility:** a binary from before this slice cannot decode a cursor-bearing obligation
  (`deny_unknown_fields`). Its GC does not decode obligations at all (presence protects,
  `crates/custodian/src/gc.rs:743-761`), so no bytes are put at risk; its restore pass reports
  such a `retire:records:` value as undecodable (`restore.rs:1180-1188`). Say this in the docs
  paragraph: downgrading past this slice is safe for data and noisy in restore reports.

**Segments drain before parts' records** (the restore check, (G)). For a session token
`s:<id>:<E>` whose `retire:records:` obligation owes `{seg}`, the drain deletes no `part:` record
under the same token's `retire:bytes:` obligation until that records obligation is gone; the
bytes obligation may still write marks. Restore's `check_attempt` requires every surviving
segment's chunks to be held by a surviving part (`restore.rs:1195-1234`), so independent draining
would make an honest half-drained state read as damage. The check is one keyed read. This is the
one ordering rule between two obligations in this slice; it is about the restore report's
accuracy, not about safety (each obligation already marks before it deletes). Replace the
`deferred: #659` comment at `restore.rs:1156` with this guarantee.

**Owned walk** (`0016:672`, `:1654-1668`). For a session in `Aborting` or `Completed`, walk its
bounded `sidx:<id>:` range: mark every position of each entry's `staged` placement, then delete the
entry, in budgeted batches. Nothing refills the range once the session left `Open` (#657's fenced
intents).

**Terminal delete** (`0016:674`, `:582-602`). Only when the session's `sidx:` range was observed
empty in this pass and no session-scoped `retire:` obligation is left (two one-row `scan_page`
reads): one batch that `require`s the session record's exact bytes, deletes it, deletes surviving
`slot:` records, deletes the `seggrp:` marker iff no `seg:` record names the nonce, and CASes
`mpuctl` down by one. Exactly one of two racing drainers wins.

**A `Completed` session also needs its tombstone window to have elapsed** (`0016:574`); deleting
it early turns an identical Complete retry from `AlreadyCompleted` into `NoSuchUpload`
(`multipart.rs:4789-4795`). Who measures the window is #625's (`W_tombstone`). So the drain's
context carries an explicit tombstone input, and the terminal delete fires for a `Completed`
session only when that input says the window has passed. **The deployed caller passes "retain"
until #625 lands**: in production a `Completed` session's obligations and owned residue drain,
but its record and its `mpuctl` slot stay. That is harmless today, because nothing in production
creates a session until #508, and #508 lands with or after #625 (`0016:236-268`). An `Aborting`
session needs no such input.

**Untrusted staged records** (the 2026-09-18 comment). For an `Aborting` session only, a `part:` or
`sidx:` record whose placement fails validation, or an `sidx:` value that will not decode, is
retired **by chunk id**: mark every fragment bearing that chunk id, found from the fleet's fragment
listings (a `FragmentId` carries its chunk) rather than from the untrusted placement, then delete
the record. Gateway-minted chunk ids carry a random epoch and do not collide (`0016:830`, X36
`:2575`), and GC still checks every
fragment against the reference set and waits out the grace, so a fragment another valid record
names is kept. An `Open` session's record keeps protecting its chunk.

**Migration gate** (`0016:1251-1278`, the 2026-09-12 comment). A page-wise pass over `orphan:`
re-stamps or drops a mark found on a still-referenced fragment; when the reference is a `part:`
record, re-stamp (or drop only against **committed** references), never drop. On finishing it writes
a durable completion marker: a new singleton record, so state its key shape, writer, deleter and
which scans see it (ADR-0046). Until the marker is observed, the identity-keyed paths stay off: GC
treats every mark as protecting, and `unlink` / supersede keep today's inline path where it fits.

**Routing switch** (`0016:1081-1106`). `unlink` and `commit_chunk_map_superseding{,_leased}` remove
the references and install **one** `retire:bytes:{generation}` (the removed map's chunks, and its
`seg:` range if segmented) in the same batch, instead of inline marks. A small flat object may keep
the inline path when its **whole** batch fits one batch's budget, counted as preconditions plus
deletes plus mark puts against `MAX_BATCH_OPS` (500), and its bytes against `MAX_BATCH_BYTES`;
above that, and for every segmented object, the obligation is the only path. Installed under `require_absent` with the `g:<inode>:<version>` token.

**Docs currency** (`AGENTS.md:154-157`): `docs/design/architecture/06-runtime-view.md` (the GC
paragraphs at `:80-86`): deletes and overwrites install a retirement obligation that the custodian
drains, marking before deleting; grace starts at the drain; multipart sessions end at a terminal
delete that releases the admission slot once; the migration marker and what it gates.

## Alternatives considered

- **Flip the routing first, dispatch later.** The discontinued run's permanent leak. Rejected.
- **Skip untrusted records.** The record stays for ever, its chunk's fragments stay, and the
  session's terminal delete never fires. Rejected (C-1: a state nothing exits).
- **Delete untrusted records without marking.** Bytes unreferenced and unevidenced: the
  discontinued patch's `RS(0,0)` failure. Rejected.
- **Expand pending obligations into GC's reference set.** Unbounded memory for one segmented
  generation (`0016:1236-1242`). Rejected; keyed lookup.

## Impact & compatibility

Production behaviour changes for DELETE and overwrite of objects past one batch of fragments:
reclamation moves from the request to the custodian, a little later than today. A new singleton
record (the migration marker) is added. **One existing record shape grows:** `RetirePayload` gains
the optional `cursor` component (Design), omitted when absent, so every obligation written before
this slice decodes unchanged; a pre-slice binary cannot decode one that carries a cursor (safe for
data, noisy in its restore report). In production a `Completed` session is not terminal-deleted
until #625 supplies the tombstone window.

## Open questions

- **Size.** This is the largest of #636's slices, and #636 died of size. If the sizer flags it, the
  natural split, keeping the issue's rule intact, is: **(1)** drain step, owned walk, terminal
  delete, untrusted-record retirement and the deployed dispatch (the routing unchanged, so the
  dispatch drains only multipart obligations); **(2)** the migration gate and the routing switch,
  depending on (1), so the flip still never precedes a dispatched drain. Children need their own
  cap override. The 2026-10-19 plan-review revision made the slice larger (the cursor codec, the
  tombstone input, the segments-before-parts rule, legs (A2)/(A3) and (G)); under that split the
  cursor, the tombstone input, the ordering rule and (G) go to (1), and (A) goes to (2).
- **The untrusted-record rule's open points** (from the 2026-09-18 comment): whether the drain can
  reuse the fragment listing GC already takes each pass, and whether any feature (copy, dedup) can
  make two objects share a chunk id. GC's reference check covers the second either way.

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a
draft PR MAY happen during the cycle (useful for CI feedback). The PR MUST NOT be
marked ready before sign-off accepts.

Plan-review response (2026-10-19, revision pass): all four findings accepted and the brief
revised. (1) `MAX_BATCH_OPS` is 500, not 250: (A) keeps the 30-chunk case but adds a 100-chunk
object that must install a `retire:bytes:g:` obligation with no inline mark, an 83-chunk RS(4,2)
boundary case that counts preconditions, and overwrite and segmented-delete cases; the Repro and
Motivation numbers are corrected. (2) Cursor: it is an optional component inside the obligation,
as 0016:613-619 requires; the brief states its rules, its lifetime and restart, and the
compatibility cost, and no longer claims "no existing record shape changes". (3) `Completed`
terminal delete: it needs an explicit tombstone input; the deployed caller passes "retain" until
#625, and (D) tests both retention and deletion. (4) Restore: segments drain before parts'
records, and the new (G) re-runs the production restore pass after every drain step.
