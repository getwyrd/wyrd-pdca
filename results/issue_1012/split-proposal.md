<!-- pdca:split-proposal v1 -->
# Split proposal — issue 1012

> **Intake-cap override (wyrd-pdca-P1).** The human (Eduard Ralph) explicitly overrode the rule
> in this Plan session on **2026-11-11** for **#1012's split only** ("split according to your
> suggestions"). `scripts/plan-cap --track alpha --need 2`, run while this proposal was drafted:
>
> ```
> planned 17/6 (cap, track alpha) — room for 0, need 2: Plan intake closed
>   PLANNED           11  1011 1013 1014 1015 1016 1017 1018 1019 668 669 670
>   BUILT              4  508 625 633 654
>   ITERATE_DO         2  1010 1030
>   other tracks (their own room): blackbox 1
>   rule wyrd-pdca-P1 (docs/INTEGRATION.md §11): no new brief in track alpha, no `pdca split --accept`, no split or reslicing proposal until its sign-offs make room.
> ```
>
> #1012 itself is UNPLANNED (iterate-to-Plan archived its brief), so it is not in that count.
> Accepting this proposal puts two children in, and the split parent itself sits at BUILT
> (counted) until its sign-off confirms the split: alpha went from 17 to **20** at acceptance
> (`plan-cap`, 2026-11-11), and drops to 19 once #1012 is signed off. The override covers these
> two children and nothing else. Re-run the script before `--accept`.
>
> **Authored by the planner, in the splitter's format.** `[leaves.splitter]` is an interactive
> leaf and cannot run inside the Plan session's shell; a split is brief authoring, so the
> planner wrote this file directly.

## Why this slice is oversized

The sign-off of #1012's first attempt (2026-11-10, `iteration-v1/SUMMARY.md` §9) chose
`iterate-plan` and a split. The attempt was a 199 KB patch over 13 files, twice the 100 KB
backstop: about 83 KB of production code (`retirement.rs` 54 KB, `multipart.rs` 23 KB), 79 KB of
tests (custodian 60 KB, DST 19 KB) and 36 KB from two edited lines in docs files whose lines are
very long. Check found the safety logic sound (the adversary reproduced three of the six required
reds; arm 0 and segments-before-parts held) and five defects in separate areas:

- **Pass budget** (3 of 4 T4 findings, reviewer F1, code-review P2): held obligations cost no
  budget, so one pass walked the whole ledger (4,096 held obligations in 65 page reads), and the
  per-pass bounds the docs promised had no test.
- **Value ceiling** (adversary): writing the cursor back could push an obligation past
  `MAX_VALUE_BYTES`; FoundationDB refuses that for good (2103), and the error ended the whole leg
  every pass.
- **Stale stamp** (code-review P1): marks used the pass's `now_millis`, cached before the
  reconstruction loop ran, so an obligation installed during that loop got marks dated before it
  existed.
- **Owned residue** (adversary): removing the hold for a `{session}` obligation with `sidx:`
  residue deleted the obligation, and no test failed.
- **Sparse part sets** (code-review P3): finding the next member rescanned from the first run
  each time (O(N²)).

## The split line, and why not the one §9 sketched

§9 sketched (a) the drain with the cursor and the timestamp fix, then (b) per-pass bounds and the
dispatch. That puts the dispatch in the second child, so the first child's tests could not run
through `reconcile_step`. That is the "drain now, dispatch later" shape #659's brief rejected
(the discontinued #636 run's shape), and #1013 and #1014 both build on the dispatch leg, so they
would wait for (b) anyway.

This proposal splits **by mode**. 0016 puts the mode in the key (`0016:434-441`): `retire:records:`
only deletes records, `retire:bytes:` marks and then deletes. So:

- **child-1: the drain itself, delete-only mode.** The cursor, the bounded ledger walk, the
  dispatch from `reconcile_step`, and the `retire:records:` drain. It closes the pass-budget,
  value-ceiling and sparse-set findings.
- **child-2: mark-then-delete mode.** The `retire:bytes:` drain: the mark guard with the
  `reclaiming` check, the per-step stamp, validated placements, the owned-residue hold,
  segments-before-parts, and the one DST row. It closes the stale-stamp and owned-residue
  findings.

Each Check finding lands in exactly one child. T4's request for part/segment DST stays out of
scope (#660 owns concurrent-drainer DST); child-2 record-rejects it if it recurs.

Rough patch estimates: child-1 **~80–100 KB** (including the long building-block docs line),
child-2 **~100–130 KB** (its DST row alone was 19 KB). child-2 is the one at risk of the backstop.

## Wave sketch

```
wave 1: child-1 — cursor, bounded walk, deployed dispatch, retire:records: drain   (B-rec, C, L, W, G-rec, K)
wave 2: child-2 — retire:bytes: drain, mark guard, stamp, holds, DST row            (B, T, C′, E-hold, R, S, G)
```

- **child-2 depends on child-1.** Its drain is a second mode of child-1's step, on child-1's
  walk and dispatch leg, and its (B) and (G) legs need the records mode to drain the other half of
  a Complete and of a rolled-back attempt.
- **Outside this proposal** (ordering fields name siblings only, `split.py:_validate_ordering`;
  added by hand after acceptance):
  - **child-1 `Depends on`:** `1008, 1009` (as #1012 had; `1007` is merged, PR #1032) and **`1011`**
    (new): the cursor's room comes out of the value ceiling #1011 enforces on the obligations a
    Complete installs, and both edit the core multipart records.
  - **child-2 `Depends on`:** child-1, plus `1008, 1009` again so the edge survives a re-point.
  - **#1013 and #1014:** `Depends on: 1012` becomes child-2's id (child-2 brings child-1). Their
    prose says "child-1" for #659's child-1, which is now both of these children; one line in
    each Ordering note says so.

## Decisions recorded this session (2026-11-11, the human)

- Split by mode, as above.
- `Cargo.lock`: the `wyrd-metadata-redb` dev-dependency line(s) the redb-backed custodian tests
  need are in scope (child-1 adds it; child-2 inherits it).
- child-1's compile-only red (C4-verify UNVERIFIABLE, the test needs a dev-dependency the red leg
  reverts) is accepted in advance as a sign-off item. child-2 aims for a real red instead.

<!-- pdca:child child-1 -->
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
  installer's bytes change); a cursor round-trips exactly; a cursor past its payload's end, or on a
  component with no ordered work, is a typed decode error.
- **Falsifiability:** RED is reachable in-process on `cargo test -p wyrd-custodian` (and
  `-p wyrd-core`). (B-rec) and (C) are red on any build where `reconcile_step` does not dispatch
  the drain. (C) is red on a drain that does not store its cursor (it never finishes) or deletes the
  obligation after a truncated derivation (records left). (L1) is red when the per-pass step count
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
- **Depends on:**
- **Conflicts with:**
- **Ordering note:** wave 1 of #1012's split. **External, to add to `Depends on` after
  acceptance** (the field takes sibling labels only here): `1008, 1009, 1011`. `1008`/`1009`:
  Complete's obligations and the fence release's `{seg}` are what (B-rec), (C) and (G-rec) drain
  (`1007` is merged, PR #1032). `1011`: it enforces the value ceiling on the obligations a Complete
  installs (`crates/core/src/multipart/complete.rs:598`, `deferred: #1011`), and this child takes
  the cursor's room out of that ceiling. child-2 builds on this child; #1013 and #1014 are
  re-pointed to child-2. **Intake-cap override (wyrd-pdca-P1):** the human's 2026-11-11 override
  for #1012's split, recorded at the top of `results/issue_1012/split-proposal.md` (alpha 17 → 19).
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
the group's `seg:` records page by page, the cursor holding the next segment index. Never write an
`orphan:` record. Next-member lookup over a part set is a binary search over its sorted runs, not a
scan from the first run.

**Where the cursor lives: inside the obligation** (normative, `0016:612-619`). One optional
component of `RetirePayload`; no new key for it.
- **Shape:** one position in the payload's own ordered work: the next component, the next entry,
  and the next item inside it (child-2 needs the third for an entry whose marks exceed a batch). Do
  chooses the spelling and states it in the type's doc.
- **Absent means "start"**, and is omitted when absent, so no installer's bytes change.
- **Decoded against its payload** through `decode_retire_obligation`: a cursor past the payload's
  end, or on a component with no ordered work, is a typed error.
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
<!-- pdca:end child-1 -->

<!-- pdca:child child-2 -->
# core,custodian: retirement drain — mark-then-delete mode with the reclaiming-safe mark guard (636.6a-2) — issue 1012 child-2

> Child 2 of 2 of #1012's split. #1012 is #659's child-1 (slice 6 of #636). Do reads ONLY this
> file. Keep the `- **Label:** value` lines. The design is normative in proposal 0016,
> `docs/design/proposals/draft/0016-multipart-commit-protocol.md`: read **decision 4** rule 2
> (`0016:1123`), the three-arm mark rule (`:1220-1226`) and the keyed protection after it
> (`:1228-1249`), grace starting at the drain (`:1098-1104`), the segmented generation re-read
> (`:2432-2440`), and the tests the slices owe (`:2886-2949`). Citations are verified on `db1a066`
> (2026-11-11); this child builds on child-1, so re-locate every anchor on the base it is given.
>
> **This is a re-plan.** #1012's first attempt (2026-11-10, unpublished) built this mode. Check
> found its mark guard, its placement hold and its segments-before-parts rule sound under
> adversarial review, and two defects this child owns; the Defect names them and the criterion's
> (T) and (R) legs test them.

- **Slug:** multipart-retirement-drain-marks
- **Track:** alpha
- **Defect:** After child-1, `retire:records:` obligations drain, but `retire:bytes:` ones never
  do: Abort's `{session}` / `{session, all}` / `{session, parts}`, the root flip's unnamed parts
  `{parts}`, a losing writer's `{chunks}`, a superseded generation's `{generation}`. Their bytes
  stay protected for ever (GC's keyed protection, `crates/custodian/src/gc.rs:743-762`) and their
  records never leave. #1012's first attempt drained them, and Check found two defects: (1) every
  mark carried the pass's `now_millis`, read before the reconstruction loop ran, so an obligation
  installed during that loop got marks dated before it existed, and GC could reclaim them with less
  than the full grace, tearing a reader that overlapped the supersede; (2) the hold for a
  `{session}` obligation whose session still owns `sidx:` entries had no test: removing it deleted
  the obligation, left the `sidx:` entry with nothing to drain it, and every test still passed.
- **Success criterion:** A NEW test file `crates/custodian/tests/multipart_retirement_marks.rs`,
  over `RedbMetadataStore::in_memory()` and a real chunk store or placement-aware double, driving
  the drain **only through `reconcile_step`** with the contexts the deployed loop passes
  (`crates/server/src/custodian.rs:397`). It shows all of:
  **(B) Complete, then drain.** After a production subset Complete (#1008) and reconcile passes:
  **no** `orphan:` mark names a chunk of the published map (`0016:1503`); every fragment of the
  unnamed part is marked **before** its `part:` record is deleted (a recording wrapper shows the
  order); after passes past the orphan grace its fragments are gone from the chunk store; no
  `part:` or `psum:` record of the session remains.
  **(T) Stamped no earlier than the retirement.** With a test clock: a store wrapper that, while
  the pass's reconstruction loop runs, advances the clock past the orphan grace and installs a
  `retire:bytes:` obligation. Every mark the drain writes for it carries `orphaned_at_millis` at
  or after the clock reading at installation, and a GC pass straight after reclaims none of its
  fragments.
  **(C′) Mark guard, the `reclaiming` check first.** For a position the obligation names that
  already carries a mark: **(0)** a mark that decodes as `reclaiming` is never replaced, whatever
  event it carries (`crates/core/src/metadata.rs:154-168`: "No writer overwrites a `reclaiming`
  mark … the three-arm drain (#659) … inherits this rule"). GC deletes the key blind after
  destroying the fragment (`gc.rs:804-825`), so a value written over it would vanish with it. The
  fixture plants a `reclaiming` mark carrying a different event, runs a step, and asserts the stored
  value byte-identical and the obligation still present with its cursor at or before that position;
  deletes the key as GC would; runs the next step; and asserts a fresh structured mark of this event
  (arm c). Then the three arms (`0016:1220-1226`): (a) same event ⇒ skipped, byte-identical;
  (b) different event ⇒ re-stamped under `require(== prior)` with this event and a fresh stamp;
  (c) absent ⇒ written under `require_absent`.
  **(E-hold) Validated placements.** A `part:` record under a `retire:bytes:` obligation whose
  placement length is not its scheme's fragment count (a decodable case: an RS(4,2) chunk placed on
  5 servers; `RS(0,0)` does not decode as a `part:` record) is never deleted unmarked: no mark from
  that placement, no delete, a typed outcome and an audit event, the obligation and its cursor stay,
  and every fragment of that chunk stays (GC's `StagedSet` holds it, `gc.rs:1337-1360`).
  **(R) Owned residue.** A `{session}` or `{session, all}` obligation whose session still has a
  `sidx:` entry (`sidx_range`, `crates/core/src/multipart.rs:1429`) stays byte-identical across
  passes, the `sidx:` entry stays, and the step reports a typed hold. (#1013's owned walk empties
  the range.)
  **(S) An entry larger than a batch.** A part or generation with more fragment positions than one
  batch can mark makes progress every step, its cursor advancing inside the entry, and finishes.
  A `{generation}` over a segmented map marks each `seg:` record's **current** placement, read at
  drain time (`0016:2432-2440`).
  **(G) Restore stays accurate between drain steps** (the `deferred: #659` at
  `crates/custodian/src/restore.rs:1156`). An `Aborting@E+1` session fenced from `Completing@E`
  with segments written and parts staged (both obligations, `retire:bytes:s:<id>:<E>` and
  `retire:records:s:<id>:<E>` owing `{seg}`, installed by the production restore fence as
  `crates/custodian/tests/restore_completing_fence.rs` does) is drained **one step at a time**.
  After every step the production `reconcile_after_restore` (`restore.rs:597`) does not name the
  session (no `ChunkInNoPart`, `restore.rs:1234`; no `NoDeleter`, `:1176`) until both obligations
  are gone.
- **Falsifiability:** RED is reachable in-process on `cargo test -p wyrd-custodian`. (T) is red on
  the first attempt's shape (marks carry the pass's cached time: the stamp predates installation,
  and GC reclaims at once). (C′) arm 0 is red on a guard that treats `reclaiming` as "different
  event" and re-stamps it. Its race with GC is an interleaving the record-level test cannot build
  (arm b's exact-value replacement and GC's swap to `reclaiming` read the same stamped bytes); the
  seeded DST row forces it on `cargo xtask ci`'s madsim tier. (E-hold) is red when an untrusted
  `part:` record is deleted without marking. (R) is red when the hold is removed (the obligation
  vanishes with the `sidx:` entry left). (G) is red when the two obligations drain independently:
  the bytes drain deletes a `part:` record a surviving segment still needs, and the next restore
  run reports `ChunkInNoPart`. (B)'s order is forced by a wrapper, not timing.
- **Invariant to restore:** **C-1**, its *storage lifecycle / reclamation* row
  (`docs/principles.md` §6), this child's share: the drain marks before it deletes the record that
  named the bytes; never marks a published chunk; stamps each mark no earlier than the retirement
  it evidences, so grace runs in full from the drain (`0016:1098-1104`); never overwrites a mark GC
  has committed to reclaim; never deletes a record whose placement it cannot trust; never drops a
  session obligation while the session owns residue; and drives every `retire:bytes:` obligation to
  empty in bounded batches. **Sources:** the maintainer's rule 2026-07-25 (via principles.md §5
  C-1); 0016 decision 4 rule 2 (`0016:1123`) and the three arms (`:1220-1226`); the custodian's
  "never reclaim a referenced fragment" (`gc.rs:66-70`, the safety gate `gc.rs:442-445`).
  SELF-TEST: it binds the drain in custodian, the mark codec in core, GC's reclamation of the
  marks, and restore's re-check, so guarding one module cannot satisfy it.
- **Repo + branch target:** getwyrd/wyrd @ main
- **Depends on:** child-1
- **Conflicts with:**
- **Ordering note:** wave 2 of #1012's split. Builds on child-1: its cursor, walk, dispatch leg
  and records mode, which (B) and (G) need for the other half of a Complete and of a rolled-back
  attempt. **External, to add to `Depends on` after acceptance:** `1008, 1009` again, so the edge
  survives a re-point. **Downstream:** #1013 and #1014 depend on this child (their `Depends on:
  1012` is re-pointed here after acceptance). **Intake-cap override (wyrd-pdca-P1):** the human's
  2026-11-11 override, recorded at the top of `results/issue_1012/split-proposal.md`.
- **Scope:** the `retire:bytes:` drain in every shape of the row table
  (`crates/core/src/multipart.rs:3857-3863`): `{session}`, `{session, all}`, `{session, parts}`,
  `{parts}`, `{chunks}`, and `{generation}` flat and segmented; the mark guard (the `reclaiming`
  check, then the three arms) and its one seeded DST row; the time each mark is stamped with;
  validation of placements before marking, with the hold for an untrusted `part:` record; the
  owned-residue hold for a `session` component; segments-before-parts between a session token's two
  obligations; replacing the `deferred: #659` comment at `restore.rs:1156`; the docs. / out of
  scope:
  - child-1's work (the cursor codec, the walk and its bounds, the dispatch, the records mode),
    except what this mode adds to them;
  - the owned `sidx:` walk, the terminal session delete, and retirement by chunk id of untrusted
    records (#1013); the orphan-identity gate and unlink/supersede routing (#1014); the reaper
    (#625); the operator abort (#633);
  - any seeded DST scenario other than the one drain-versus-GC `reclaiming` row. #660 owns
    concurrent drainers; a review request for part/segment DST is out of scope and is
    record-rejected;
  - the S3 verbs (#668–#670); any edit to proposal 0016 or an ADR; `deny*.toml`; `Cargo.lock`
    (child-1 added the redb line; if this child needs another dev-dependency line, the human's
    2026-11-11 approval covers that and nothing else).
- **Repro instruction:** n/a (new functionality). On child-1's base, a `retire:bytes:` obligation
  (Abort's, or a Complete's unnamed parts) stays for ever, and GC keeps its bytes
  (`gc.rs:743-762`).
- **Difficulty:** high
- **Do model:** opus
- **External dependencies:** `typos`, `docs-renderer`
- **Test file:** `crates/custodian/tests/multipart_retirement_marks.rs`, a **NEW** file
  (C4-verify treats an added `crates/*/tests/*.rs` as the per-fix discriminator).
- **Verification posture:** default, aiming for a **real red**. C4-verify runs this bundle on
  child-1's folded base, which already has the redb dev-dependency and the drain leg. Write the test
  against what that base has (the store, `reconcile_step`, the deployed contexts, the core codecs)
  and assert holds through stored records and audit output, not through a type this child adds.
  Then its red leg compiles, child-1's drain leaves the `retire:bytes:` obligations untouched, and
  (B), (T), (C′) and (S) fail. If that is impossible, say why in `build-notes.md`: C4-verify then
  reports UNVERIFIABLE, a sign-off item.
  **Demonstrated reds (required, recorded in `build-notes.md`).** Do makes each temporary edit,
  shows the named leg fail, then restores the file:
  - marks stamped with the pass's cached time → (T) fails;
  - arm 0 removed (a `reclaiming` mark re-stamped as "different event") → (C′) fails, and the DST
    row fails on its GC-wins interleaving;
  - an untrusted `part:` record deleted without marking → (E-hold) fails;
  - the owned-residue hold removed → (R) fails;
  - segments-before-parts removed → (G) fails;
  - a `part:` record deleted before its fragments are marked → (B) fails.
  **Seeded DST row (required; the human's decision 2026-10-30).** AGENTS.md asks a new concurrent
  destructive path to land with seeded Tier-0 DST coverage (`AGENTS.md:205`; `0016:2888`). One row:
  the drain's exact-value replacement (arm b) and GC's swap to `reclaiming` race on one stamped
  mark. Assert exactly one commit wins; when GC wins, the drain's re-read finds `reclaiming` and
  preserves it byte-identical rather than retrying arm (b); and the obligation still drains to empty
  once GC's key delete lands. **Append it to `crates/dst/tests/custodian.rs`** (its campaigns are
  registered with `dst_campaign_test!`, `:6601-6689`). C4-verify discriminates on an added test
  file and applies an added file's crate-level cfgs (`--cfg madsim`) to the whole invocation, so Do
  MUST NOT add a new file under `crates/dst/tests/`. Verify with `cargo xtask ci`, the only gate
  that builds `crates/dst`.
- **Production reach:**
  - (a) The deployed custodian drains `retire:bytes:` obligations every pass. Production installs
    none until #508 creates sessions, and no DELETE changes behaviour here (#1014 routes unlink).
  - (b) For multipart bytes, grace starts when the drain writes each mark (`0016:1098-1104`).
- **Citations expected:** Do must cite `path:line` on the target branch for every change.
  - **The leg:** child-1's retirement leg in `reconcile_step` (on this bundle's base); add this mode
    to it.
  - **Peer for the stamp:** the deployed loop's one time source, `LoopClock`
    (`crates/server/src/custodian.rs:134-167`), reached through `ReconstructionContext::clock`
    (`crates/custodian/src/reconstruction.rs:133`). Read it for each step, after the step has read
    its obligation, never a value cached before the pass's earlier legs ran.
  - **Mark codec:** `OrphanMark` (`crates/core/src/metadata.rs:170`), `encode_orphan_mark`
    (`:285`), `decode_orphan_mark` (`:305`), the `reclaiming` rule (`:154-168`), the three value
    shapes (`docs/design/architecture/08-crosscutting-concepts.md:87`).
  - **GC:** keyed protection `retirement_draining` (`gc.rs:743-762`), the blind key delete
    (`destroy`, `gc.rs:804-825`), the grace test (`:682-688`), `staged_placement` (`:1471`,
    validates placement length), `StagedSet` (`:1337-1360`, reason `untrusted-staged-record`).
  - **Restore:** `recheck_fenced` (`restore.rs:1152-1190`), `check_attempt` (`:1195`),
    `segment_fault` (`:1225-1236`), `RestoreReport::staged_untrusted` (`:179`). Replace the
    `deferred: #659` marker (`:1156-1158`) with what the drain now guarantees between two restore
    runs (segments before parts; a records obligation goes only with its last record).
  - **Records:** `sidx_range` (`multipart.rs:1429`); the row table's `retire:bytes:` rows (`:3857-3863`).
  - **Docs currency** (`AGENTS.md:169`): `docs/design/architecture/06-runtime-view.md` (the GC
    paragraphs `:78-86`): the drain marks before it deletes; grace for drained bytes starts at the
    drain; a `reclaiming` mark is never overwritten and the drain resumes once GC's key delete
    lands; a session's obligation waits for its owned residue.
- **Prior-art check (triage cycles):** by path. The test file has no history. #1012's first
  attempt (2026-11-10, unpublished) drained this mode: its arm 0, placement hold and
  segments-before-parts reds were reproduced by the adversary, its DST row reached both winners, and
  its marks carried the pass's cached time (the defect above). #636 (closed not planned
  2026-10-10). Merged prerequisites: GC's keyed `retire:bytes:` protection (#662) and reclaim
  intent, the restore fences (#664, #809, #810).
- **Disposition hint:** new-feature

## Design

**Mark guard: the `reclaiming` check, then three arms.** Read each position's mark through
`decode_orphan_mark`, then: **(0)** it decodes as `reclaiming` ⇒ leave it byte-identical whatever
event it carries; stop the step at that position with the cursor at or before it and keep the
obligation; a later pass finds the key gone and takes arm (c). **(a)** same event ⇒ skip.
**(b)** different event ⇒ re-stamp under `require(== prior)`. **(c)** absent ⇒ write under
`require_absent` (`0016:1220-1226`). When arm (b)'s commit fails its precondition, re-read: a
`reclaiming` value now is arm 0, never a retry of (b). The records that named a position held by
arm 0 are not deleted until it carries this event's mark, and the obligation is never deleted with
such a position unmarked.

**The stamp.** Each step reads the time from the loop's clock after it has read its obligation.

**Validated placements.** Placements are validated before marking. A `part:` record whose placement
fails is held: never deleted unmarked, never marked from its placement. The step stops at it with
a typed outcome and an audit event; obligation and cursor stay. This is the safe direction: GC
already holds every fragment of such a chunk. #1013 turns the hold into retirement by chunk id.

**Owned residue.** A `session` component is not finished while the session's `sidx:` range is
non-empty: the obligation is held. #1013's owned walk empties the range.

**Segments drain before parts' records** (G). For a session token `s:<id>:<E>` whose
`retire:records:` obligation owes `{seg}`, the drain deletes no `part:` record under the same
token's `retire:bytes:` obligation until that records obligation is gone; the bytes obligation may
still write marks. Restore's `check_attempt` requires every surviving segment's chunks to be held by
a surviving part, so independent draining would make an honest half-drained state read as damage.
The check is one keyed read.

## Alternatives considered

- **Stamp with the pass's `now_millis`.** The first attempt's shape. Rejected: an obligation
  installed during the pass's earlier legs gets marks older than itself.
- **Retire untrusted `part:` records by chunk id here.** Needs the fleet's fragment listings; it
  belongs with #1013's owned walk. Deferred; this child holds.

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a draft PR MAY
happen during the cycle (useful for CI feedback). The PR MUST NOT be marked ready before
sign-off accepts.
<!-- pdca:end child-2 -->
