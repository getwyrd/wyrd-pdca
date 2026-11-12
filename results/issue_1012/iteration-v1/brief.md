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
- **Track:** alpha
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
- **Depends on:** 1007, 1008, 1009
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

## Iteration 1 — carry-forward (from the previous attempt)
- Sign-off rationale: Split the slice: 199 KB over 13 files (2x the size backstop), with real defects in several separate areas. Author the split in Plan (`pdca split 1012`). Each child must carry forward these findings from Check: - P1 (safety): the drain stamps marks with the pass's cached `now_millis` (retirement.rs:466; reconciliation.rs:165 runs reconstruction first). An obligation installed during the pass gets an old timestamp, and GC may reclaim it at once. Read `ctx.clock` per step, after the obligation is read, and add a regression for it. - Liveness: held / non-committing obligations don't consume the pass budget (retirement.rs:174-181, :227). Bound visits/reads as well as commits, resume past a held prefix, and test with more than 64 obligations (OBLIGATION_PAGE) for both held and productive backlogs. This is also the T4 batch-review blocker (3 of its 4 findings). - Wedge: writing the cursor back can push an obligation past MAX_VALUE_BYTES (retirement.rs:584-585); FDB 2103 then stops the whole leg every pass. Turn it into a typed hold and note the CURSOR_BYTES headroom against #1011. - Missing test: owned-residue hold (retirement.rs:607): a `{session}` obligation with `sidx:` residue must stay. - P3: `first_from` (multipart.rs:3477) is O(N^2) over sparse part sets; use partition_point. - The T4 finding asking for part/segment DST is out of scope (#660 owns it, per the brief); record-reject it. Open for the re-plan: whether the brief should allow the Cargo.lock dev-dependency line the redb test needs, and whether the compile-only red is accepted. The human did not decide these at this sign-off. Suggested split: (a) the drain step with the cursor plus the timestamp fix; (b) per-pass work bounds plus dispatch from reconcile_step.
- Sign-off session carry-forward (captured live, before §9 flattened it):
  Split the slice: 199 KB over 13 files (2x the size backstop), with real defects in several separate areas. Author the split in Plan (`pdca split 1012`).
  Each child must carry forward these findings from Check:
  - P1 (safety): the drain stamps marks with the pass's cached `now_millis` (retirement.rs:466; reconciliation.rs:165 runs reconstruction first). An obligation installed during the pass gets an old timestamp, and GC may reclaim it at once. Read `ctx.clock` per step, after the obligation is read, and add a regression for it.
  - Liveness: held / non-committing obligations don't consume the pass budget (retirement.rs:174-181, :227). Bound visits/reads as well as commits, resume past a held prefix, and test with more than 64 obligations (OBLIGATION_PAGE) for both held and productive backlogs. This is also the T4 batch-review blocker (3 of its 4 findings).
  - Wedge: writing the cursor back can push an obligation past MAX_VALUE_BYTES (retirement.rs:584-585); FDB 2103 then stops the whole leg every pass. Turn it into a typed hold and note the CURSOR_BYTES headroom against #1011.
  - Missing test: owned-residue hold (retirement.rs:607): a `{session}` obligation with `sidx:` residue must stay.
  - P3: `first_from` (multipart.rs:3477) is O(N^2) over sparse part sets; use partition_point.
  - The T4 finding asking for part/segment DST is out of scope (#660 owns it, per the brief); record-reject it.
  Open for the re-plan: whether the brief should allow the Cargo.lock dev-dependency line the redb test needs, and whether the compile-only red is accepted. The human did not decide these at this sign-off.
  Suggested split: (a) the drain step with the cursor plus the timestamp fix; (b) per-pass work bounds plus dispatch from reconcile_step.
- Failing gate: C4 diff coverage: changed lines executed by the patch's tests (advisory) — diff coverage not measured — patch.diff does not apply on origin/main
- Failing gate: C5 surviving mutants on the bundle diff (cargo mutants --in-diff) (advisory) — 208 mutants tested in 46m: 78 missed, 98 caught, 32 unviable
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 4 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_1012/review-
- Full previous attempt preserved in `iteration-v1/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).
