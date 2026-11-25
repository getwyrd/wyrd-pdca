# core,custodian: retirement drain — cursor, bounded ledger walk and deployed dispatch, delete-only mode (636.6a-1) — issue 1012 child-1

> Child 1 of 2 of #1012's split. #1012 is #659's child-1 (slice 6 of #636). Do reads ONLY this
> file. Keep the `- **Label:** value` lines. The design is normative in proposal 0016,
> `docs/design/proposals/draft/0016-multipart-commit-protocol.md`: read **decision 4** rule 1
> (`0016:1081`), the batch inventory (`0016:604-692`, above all the **Retirement drain step** row
> `:668`), "no cursor record class" (`:612-619`), and the record table (`:355-356`, mode in the
> key `:434-441`). Citations are verified on `db1a066` (2026-11-11), the fold of `main` @
> `d04e507` with #1008 and #1009 that #1012's first attempt built on; re-locate every anchor on the
> base this bundle is given.
>
> **This is a re-plan.** #1012's first attempt (2026-11-10, not published) built both modes in one
> 199 KB patch. Check found the step logic sound and three defects in the machinery this child
> owns; the Defect names them and the criterion's (L) and (W) legs test them.

- **Slug:** multipart-retirement-drain-records
- **Track:** alpha
- **Defect:** Every multipart slice installs `retire:` obligations and nothing on `main` drains
  them. The `retire:records:` ones, the root flip's published parts (`{parts}`, #1008) and a
  rolled-back attempt's segments (`{seg}`, #1009 and the restore fence), leave `part:`, `psum:` and
  `seg:` records for ever. #1012's first attempt built a drain, and Check found three defects in it:
  (1) a pass's budget counted only commits, so obligations that could not progress cost nothing,
  one pass walked the whole ledger (a probe read 4,096 held obligations in 65 pages in one pass),
  and the per-pass bounds the docs promised had no test; (2) writing the advanced cursor back could
  push an obligation past `MAX_VALUE_BYTES`, the store refuses that for good (FoundationDB 2103),
  and the error ended the whole leg on every pass, so every later obligation starved; (3) finding
  the next member of a sparse part set rescanned the runs from the first one each time (O(N²)).
- **Success criterion:** A NEW test file `crates/custodian/tests/multipart_retirement_drain.rs`
  (the core-level parts, (K) and the Complete refusal in (W), MAY go inline in
  `crates/core/src/multipart.rs` or in a NEW `crates/core/tests/*.rs`), over `RedbMetadataStore::in_memory()`, driving the drain **only
  through `reconcile_step`** with the contexts the deployed loop passes
  (`crates/server/src/custodian.rs:397`), never a test-only entry. It shows all of:
  **(B-rec) Complete, then drain.** After a production subset Complete (#1008) and reconcile
  passes: no `part:` or `psum:` record of a published part remains; the drain wrote **no**
  `orphan:` record at all (`0016:1503`); the published object reads back byte-identical; the
  unnamed parts' records and their `retire:bytes:` obligation are byte-identical to before (that
  mode is child-2's).
  **(C) Convergence at scale.** A `retire:records:` `{parts}` obligation over **at least 4,001
  part numbers, no two adjacent** (one run per part; planted canonical `part:`/`psum:` records are
  fine), and a `{seg}` obligation over more `seg:` records than `MAX_BATCH_OPS`, drain to empty
  across steps. Every commit stays within `MAX_BATCH_BYTES` and `MAX_BATCH_OPS`, preconditions
  counted. The obligation's stored value changes every step (its cursor advanced); a drain resumed
  from fresh contexts (nothing in memory) continues from the stored cursor; the obligation is
  deleted only in the batch that deletes its last record; a second full drain is a no-op.
  **Combined payload:** a `retire:records:` `{parts, seg}` obligation (the row the decoder accepts,
  `crates/core/src/multipart.rs:3866`; no production writer installs it yet, so the fixture plants
  its canonical encoding), sized so its drain crosses from one component to the other **between
  two commits**. A drain resumed from fresh contexts exactly at that boundary continues into the
  second component; after the first component's last record is gone the obligation is still
  present, its cursor in the second component; it is deleted only with the second component's
  last record.
  **(L) Per-pass bounds, no starvation.** With a recording store wrapper counting reads and
  commits per pass, over a ledger at least three times the per-pass visit bound Do chooses:
  (L1) a productive backlog: every pass stays within fixed bounds on obligations visited, store
  reads and commits, whatever the ledger size; (L2) a held prefix: obligations that cannot progress
  (values that will not decode) sorting before one productive obligation: every pass stays within
  the same bounds, each held obligation stays byte-identical with a typed outcome and an audit
  event, and the productive obligation drains within a number of passes the test states from the
  bounds.
  **(W) Value ceiling.** Under a store wrapper that refuses any put over `MAX_VALUE_BYTES`, as
  FoundationDB does: an obligation installed within the installer ceiling (which now leaves the
  cursor's room) drains to empty; one planted just under `MAX_VALUE_BYTES` (e.g. 6,249 non-adjacent
  six-digit part numbers, 99,995 bytes) is held byte-identical with a typed outcome and an audit
  event, its over-ceiling put is never submitted, no pass returns `Err`, and a second obligation
  later in key order drains in the same passes. A Complete whose obligation would leave less than
  the cursor's room under `MAX_VALUE_BYTES` is refused where #1011 refuses one over the ceiling.
  **(G-rec) Restore between records-drain steps.** An `Aborting@E+1` session fenced from
  `Completing@E` with segments written (installed by the production restore fence, as
  `crates/custodian/tests/restore_completing_fence.rs` does) has its `retire:records:` `{seg}`
  obligation drained **one step at a time**. After every step the production
  `reconcile_after_restore` (`crates/custodian/src/restore.rs:597`) does not name the session (in
  particular no `NoDeleter`, `restore.rs:1176`) until that obligation is gone.
  **(K) Cursor codec.** An obligation without a cursor encodes byte-identical to today (no
  installer's bytes change); a cursor round-trips exactly. Decode refuses, with a typed error,
  every cursor it can judge from the key and value it is given: one on a component the payload
  does not carry; one past the end of work the payload itself lists (a part number past the
  `{parts}` set's last member; an index past a `{chunks}` list or a flat `{generation}` list); a
  segment index past the key grammar's bound `MAX_SEGMENT_INDEX`
  (`crates/core/src/metadata.rs:536`). A `{seg}` payload names only the group's nonce and epoch
  (`SegmentGroup`, `metadata.rs:1052-1055`), so decode **cannot** know where the group's `seg:`
  range ends, and (K) does not ask it to. That end is the drain's to find, and (C) tests it: a
  `{seg}` component is finished only when a read of the group's **whole** `seg:` range, from its
  start, finds it empty. A planted `{seg}` cursor past the last record, with records still below
  it, is a typed hold (obligation byte-identical, audit event), never a finish; this half of (K)
  is asserted through `reconcile_step`, in the custodian test file.
- **Falsifiability:** RED is reachable in-process on `cargo test -p wyrd-custodian` (and
  `-p wyrd-core`). (B-rec) and (C) are red on any build where `reconcile_step` does not dispatch
  the drain. (C) is red on a drain that does not store its cursor (it never finishes) or deletes the
  obligation after a truncated derivation (records left), and its combined leg is red on a drain
  that deletes the obligation when the first component finishes (the second component's records
  left with nothing to drain them). (K)'s `{seg}` hold is red on a drain that trusts the stored
  cursor as the range's end. (L1) is red when the per-pass step count
  is not decremented (the first attempt's surviving mutant `steps_left -= 1` → `+=`); (L2) is red on
  the first attempt's shape (held obligations cost nothing, so reads grow with the ledger) and on a
  walk that always restarts at the ledger head under a bounded visit budget (the productive
  obligation is never reached). (W) is red on the first attempt's shape (the put is submitted, the
  wrapper refuses it, every pass returns `Err`, the later obligation never drains). The wrapper
  stands in for FoundationDB's refusal by enforcing the same constant FDB does; no live FDB is
  needed. (G-rec) is red when the obligation is deleted before its last `seg:` record.
- **Invariant to restore:** **C-1**, its *storage lifecycle / reclamation* row
  (`docs/principles.md` §6: "every durable byte is … protected by a record that names it **or**
  evidenced for reclamation, and every state has an actor that exits it in bounded time"), this
  child's share: every installed `retire:records:` obligation is driven to empty by the deployed
  custodian in bounded batches, and its records leave without any `orphan:` mark; a pass's work
  (obligations visited, reads, commits) is bounded by constants independent of the ledger's size,
  and no obligation is starved by others that cannot progress; one obligation's fault (it will not
  decode, its cursor will not fit) holds that obligation only and never stops the rest of the
  ledger. **Sources:** the maintainer's rule 2026-07-25 (via principles.md §5 C-1); 0016 decision
  4 rule 1 (`0016:1081`) and the batch inventory (`:604-692`, `:668`); mode in the key
  (`:434-441`). SELF-TEST: it binds the codec in core, the walk and step in custodian, and the
  dispatch in the deployed server loop, so guarding one module cannot satisfy it.
- **Repo + branch target:** getwyrd/wyrd @ main
- **Depends on:** 1008, 1009, 1011
- **Conflicts with:**
- **Ordering note:** wave 1 of #1012's split. **External dependencies** (added after acceptance):
  `1008, 1009, 1011`. `1008`/`1009`:
  Complete's obligations and the fence release's `{seg}` are what (B-rec), (C) and (G-rec) drain
  (`1007` is merged, PR #1032). `1011`: it enforces the value ceiling on the obligations a Complete
  installs (`crates/core/src/multipart/complete.rs:598`, `deferred: #1011`), and this child takes
  the cursor's room out of that ceiling. child-2 (#1062) builds on this child; #1013 and #1014
  are re-pointed to #1062. **Intake-cap override (wyrd-pdca-P1):** the human's 2026-11-11 override
  for #1012's split, recorded at the top of `results/issue_1012/split-proposal.md` (alpha 17 → 20 at
  acceptance, 19 once #1012's split is signed off).
- **Scope:** the optional `cursor` component inside `RetirePayload`, decoded against its payload,
  with its maximum encoded size reserved under `MAX_VALUE_BYTES` in the ceiling installed
  obligations are held to; the retirement leg dispatched from `reconcile_step` in the deployed
  custodian; the `retire:` ledger walk with per-pass bounds on obligations visited, reads and
  commits, resuming past obligations that cannot progress; the drain step for `retire:records:`
  (`{parts}`: each member's `part:` and `psum:`; `{seg}`: the group's `seg:` records; both
  together), delete only, never mark; typed holds with audit events for an obligation that will not
  decode and one whose advanced value would cross the ceiling; next-member lookup over a part set
  that does not rescan from the first run; the `wyrd-metadata-redb` dev-dependency of
  `wyrd-custodian` and its `Cargo.lock` line (approved 2026-11-11); the docs. / out of scope:
  - every `retire:bytes:` obligation, in any shape: this child leaves each byte-identical. Marking,
    the mark guard and its `reclaiming` check, the stamp, validated placements and their hold, the
    owned-residue hold, segments-before-parts, the DST row and the `deferred: #659` marker at
    `restore.rs:1156` are child-2's;
  - the owned `sidx:` walk and the terminal session delete (#1013); the orphan-identity gate and
    unlink/supersede routing (#1014); the reaper (#625); the operator abort (#633); concurrent
    drainers (#660); the S3 verbs (#668–#670);
  - any edit to proposal 0016 or an ADR; `deny*.toml`; any `Cargo.lock` change beyond the
    dev-dependency line.
- **Repro instruction:** n/a (new functionality). On the base, nothing reads `retire:records:`
  except restore's re-check (`restore.rs:1152-1190`); after a Complete, its `{parts}` obligation
  and every published part's `part:`/`psum:` records stay for ever.
- **Difficulty:** high
- **Do model:** opus
- **External dependencies:** `typos`, `docs-renderer`
- **Test file:** `crates/custodian/tests/multipart_retirement_drain.rs`, a **NEW** file
  (C4-verify treats an added `crates/*/tests/*.rs` as the per-fix discriminator).
- **Verification posture:** (a) net-new. The test cannot compile on C4-verify's red leg: it runs
  over redb, and the red leg reverts the new dev-dependency with the rest of production, so
  C4-verify reports **UNVERIFIABLE** (exit 77). The human accepted this in advance on 2026-11-11 as
  a sign-off item. **Built and exercised at Check:** the drain through `reconcile_step` as deployed.
  **Demonstrated reds (required, recorded in `build-notes.md`).** Do makes each temporary edit,
  shows the named leg fail, then restores the file:
  - the dispatch made unreachable in `reconcile_step` → (B-rec) and (C) fail;
  - the cursor not stored (each step derives from the start) → (C) fails;
  - the obligation deleted after its first batch → (C) and (G-rec) fail;
  - the obligation deleted when its first component finishes → (C)'s combined leg fails;
  - the `{seg}` finish judged from the cursor alone, not a read of the whole range → (K)'s
    `{seg}` hold fails;
  - the per-pass step count not decremented → (L1) fails;
  - obligations that cannot progress not counted against the visit bound → (L2) fails;
  - the walk always restarting at the ledger head → (L2) fails;
  - the ceiling check removed → (W) fails.
  **No DST row in this child.** It writes no mark, and every record it deletes has its bytes owned
  by another record (the published inode for `{parts}`; the session's `part:` records, which
  nothing deletes until child-2, for `{seg}`). The drain-versus-GC row is child-2's, and #660 owns
  concurrent drainers. Verify with `cargo xtask ci`.
- **Production reach:**
  - (a) The deployed custodian runs the leg every reconstruction pass
    (`crates/server/src/custodian.rs:397`). Production installs no multipart obligation until #508
    creates sessions, so no request changes behaviour.
  - (b) `RetirePayload` gains an optional `cursor`, omitted when absent, so every obligation
    written before this child decodes unchanged. A binary from before this child cannot decode a
    cursor-bearing obligation (`deny_unknown_fields`, `multipart.rs:3831`). Its GC never decodes
    obligations (presence protects, `crates/custodian/src/gc.rs:743-762`), so no bytes are at risk;
    its restore pass reports such a `retire:records:` value as undecodable (`restore.rs:1185`). Say
    so in the docs: downgrading past this child is safe for data and noisy in restore reports.
- **Citations expected:** Do must cite `path:line` on the target branch for every change.
  - **Peer for the dispatch:** `reconcile_step` (`crates/custodian/src/reconciliation.rs:126-168`)
    and its one production caller (`crates/server/src/custodian.rs:397`, `reconcile_pass`). Run the
    leg on the **reconstruction** input, as the first attempt did, not GC's: a drain on GC's input
    reads `retire:` by range and breaks the merged GC test
    `crates/custodian/tests/gc_reclaim_intent.rs:1207-1277` (`no_retire_scan`, obligations that
    must outlive GC-only passes). A new `reconcile_step` parameter reaches about 25 test files;
    avoid it. Name the leg so child-2, #1013 and #1014 can add to it.
  - **Peer for the cross-pass walk position:** GC's orphan walk bookmark, `ORPHAN_CURSOR_KEY`
    (`gc.rs:279-291`; resume and advance `gc.rs:1858-1958`). It is a position, never evidence, and
    it lives in the store because the deployed loop builds fresh contexts every pass and shared
    mutable globals are ruled out (ADR-0035). 0016's "no cursor record class" (`:612-619`) binds the
    obligation's own drain cursor, which lives inside the obligation; a ledger bookmark outside
    `retire:`, like GC's, is not that. Do may use it or any other mechanism that meets (L2), and
    states the choice.
  - **Paged reads:** `scan_page` (`crates/traits/src/lib.rs:1438`), GC's `checked_page`
    (`gc.rs:2066`).
  - **Records and budgets:** `RetirePayload` (`multipart.rs:3904`; wire `:3831-3843`; its row
    table `:3857-3866`, the records rows `:3864-3866`), `decode_retire_obligation` (`:4175`), `RetireMode` / `retire_key` /
    `retire_session_range` (`:1456`, `:1582`, `:1592`), `PartScope` (`:3454`) and the part set's
    `runs()` (`:3428-3436`, "the drain … decides how to page a run"), `MAX_BATCH_BYTES` (`:5482`),
    `MAX_BATCH_OPS` (`:5521`), `MAX_VALUE_BYTES` (`crates/core/src/metadata.rs:577`).
  - **Ceiling:** `crates/core/src/multipart/complete.rs:598` (`deferred: #1011`) and whatever #1011
    lands there. FoundationDB's 2103 is permanent (`crates/metadata-fdb/src/lib.rs:303-306`).
  - **Docs currency** (`AGENTS.md:169`): `docs/design/architecture/06-runtime-view.md` (the GC
    paragraphs `:78-86`: the custodian drains `retire:` obligations in bounded batches per pass, the
    records mode deletes without marking, the cursor, the downgrade note) and
    `05-building-block-view.md:246` (it says the drain has not arrived).
- **Prior-art check (triage cycles):** by path. `crates/custodian/src/retirement.rs` and the test
  file have no history on `main` or any branch. #636 (closed not planned 2026-10-10): its drain
  re-derived the same first keys forever in one attempt and truncated derivation while marking the
  obligation drained in another. #1012's first attempt (2026-11-10, unpublished): sound step logic;
  the three defects named above. Merged prerequisites: `scan_page` (#634), GC's paged ledger walk
  (#661), the staged reference set and keyed protection (#662), the restore fences (#664, #809,
  #810).
- **Disposition hint:** new-feature

## Design

**One leg, many steps, bounded per pass.** `reconcile_step` drives the retirement leg on the
reconstruction input. Each pass visits at most a fixed number of obligations, reads at most a fixed
number of pages, and commits at most a fixed number of steps. An obligation that cannot progress
still counts as a visit. The next pass continues after the last obligation visited and laps back to
the head at the end, so a held prefix delays the rest by a bounded number of passes and never
blocks them.

**One step.** Read the obligation, decode it with its cursor, derive the next batch from the
cursor, and commit `require(retire:… == prior)` + the deletes + the obligation put back with the
advanced cursor. The step that deletes the last record deletes the obligation instead. Batches stay
within `MAX_BATCH_BYTES` and `MAX_BATCH_OPS`, preconditions counted. A precondition failure
re-reads and re-plans. A value the store would refuse for its size is never submitted: the step
computes the advanced value's size first, and over the ceiling it holds.

**Records mode.** `{parts}`: per member, delete `part:<id>:<n>` and `psum:<id>:<n>`. `{seg}`: delete
the group's `seg:` records page by page, the cursor holding the next segment index; the component
is finished only when a read of the group's whole range from its start finds it empty (one
one-row `scan_page`), because nothing in the payload says where the range ends. `{parts, seg}`:
both components in the payload's order; the obligation goes only with the last record of the last
component. Never write an `orphan:` record. Next-member lookup over a part set is a binary search
over its sorted runs, not a scan from the first run.

**Where the cursor lives: inside the obligation** (normative, `0016:612-619`). One optional
component of `RetirePayload`; no new key for it.
- **Shape:** one position in the payload's own ordered work: the next component, the next entry,
  and the next item inside it (child-2 needs the third for an entry whose marks exceed a batch). Do
  chooses the spelling and states it in the type's doc.
- **Absent means "start"**, and is omitted when absent, so no installer's bytes change.
- **Decoded against its payload** through `decode_retire_obligation`, as far as the payload can
  judge it: a cursor on a component the payload does not carry, past the end of a list the payload
  carries, or past `MAX_SEGMENT_INDEX`, is a typed error. Work the payload names only by a store
  range (a `{seg}` group's records; child-2's `{parts: all}` and segmented `{generation}`) has no
  end decode can check: the drain finds it by reading the range, as (K) says.
- **Written only by the drain**, in the step's own batch. The finishing step deletes both.
- **Its room is reserved.** Name the cursor's largest encoded size as a constant, and make the
  ceiling installed obligations are held to `MAX_VALUE_BYTES` minus that. The hold in (W) is the
  backstop for an obligation installed without the reservation.

**`retire:bytes:` obligations** are left byte-identical. If the walk visits them, each visit
counts against the visit bound.

## Alternatives considered

- **Dispatch later.** Rejected: the discontinued #636 shape, and #1013/#1014 need the leg.
- **The drain on GC's input.** Rejected: it breaks `gc_reclaim_intent.rs`'s GC-only legs.
- **Held obligations free of budget, restart at the head every pass.** The first attempt's shape.
  Rejected: unbounded work per pass.
- **A separate cursor record per obligation.** 0016 forbids it (`:612-619`).

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a draft PR MAY
happen during the cycle (useful for CI feedback). The PR MUST NOT be marked ready before
sign-off accepts.

Plan-review response (2026-11-11, both findings verified on `db1a066` and taken): (1) the `{seg}`
payload carries no extent (`metadata.rs:1052-1055`), so (K) now splits what decode can check from
what the drain must read, and the `{seg}` finish needs the whole range empty; (2) (C) gains a
combined `{parts, seg}` leg that resumes at the component boundary, with its own demonstrated red.

## Iteration 1 — carry-forward (from the previous attempt)
- Sign-off rationale: Rejected for three reproduced defects (the step logic and the cursor codec held up); keep the slice, fix these: 1. seg_step (retirement.rs:496) discards the backend's `next` continuation and finishes a `{seg}` component on a page the store clamped. Honor `next`: never delete the obligation while `next` is Some; resume after the page's last record. Add a (C)-style leg under `RedbMetadataStore::in_memory().with_scan_cap(..)`. 2. A pass that resumed mid-ledger and reached the end answers `Satisfied` without seeing the head (retirement.rs:240-273), leaving undrained obligations certified done. Choose one rule: answer Partial unless the walk started at the head, or whenever a step this pass left an advanced cursor. The same rule must cover retry-budget exhaustion at retirement.rs:300 (the T4 blocking finding). Add regressions for both: a wrap with unfinished obligations, and repeated conflicts on one obligation. 3. Tests check the code against itself: assert literal hold-reason strings, not `Hold::X.as_str()`; add a `{seg}` backlog and a page holding more records obligations than STEPS_PER_PASS so the L1 read/visit bounds and page cap are actually pinned (C5 survivors at retirement.rs:121, :222, :233). Size: the 186 KB patch is about two-thirds tests; the human judged this a fix round, not a re-split.
- Sign-off session carry-forward (captured live, before §9 flattened it):
  Rejected for three reproduced defects (the step logic and the cursor codec held up); keep the slice, fix these:
  1. seg_step (retirement.rs:496) discards the backend's `next` continuation and finishes a `{seg}` component on a page the store clamped. Honor `next`: never delete the obligation while `next` is Some; resume after the page's last record. Add a (C)-style leg under `RedbMetadataStore::in_memory().with_scan_cap(..)`.
  2. A pass that resumed mid-ledger and reached the end answers `Satisfied` without seeing the head (retirement.rs:240-273), leaving undrained obligations certified done. Choose one rule: answer Partial unless the walk started at the head, or whenever a step this pass left an advanced cursor. The same rule must cover retry-budget exhaustion at retirement.rs:300 (the T4 blocking finding). Add regressions for both: a wrap with unfinished obligations, and repeated conflicts on one obligation.
  3. Tests check the code against itself: assert literal hold-reason strings, not `Hold::X.as_str()`; add a `{seg}` backlog and a page holding more records obligations than STEPS_PER_PASS so the L1 read/visit bounds and page cap are actually pinned (C5 survivors at retirement.rs:121, :222, :233).
  Size: the 186 KB patch is about two-thirds tests; the human judged this a fix round, not a re-split.
- Failing gate: C5 surviving mutants on the bundle diff (cargo mutants --in-diff) (advisory) — 172 mutants tested in 32m: 24 missed, 121 caught, 27 unviable
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 1 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_1061/review-
- Full previous attempt preserved in `iteration-v1/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).
