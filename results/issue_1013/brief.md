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
- **Track:** alpha
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
  **(E) Untrusted records.** For an `Aborting` session, each of these is retired **by chunk id**
  on a pass whose fleet is the whole operator-configured fleet: (E1) a `part:` record whose
  placement has the wrong length under a supported scheme (an RS(4,2) chunk placed on 5 servers;
  the maintainer's case); (E2) an owned `sidx:` entry with the same wrong-length placement (it
  decodes: `StagedPlacement` admits a length mismatch, `crates/core/src/multipart.rs:4441` on
  `db1a066`); (E3) an owned `sidx:` value that will not decode (e.g. an unsupported scheme, which
  `StagedPlacement` refuses, `multipart.rs:4481`), its chunk id taken from its key
  (`parse_sidx_key`, `multipart.rs:1436`) exactly as GC's hold takes it
  (`crates/custodian/src/gc.rs:1392`). For each: every fragment bearing that chunk id that the
  whole-fleet listing finds is marked, and for E2 every position the entry itself names as well;
  then the record is deleted (a recording wrapper shows the order); after GC past grace those
  fragments are gone and the terminal delete has fired.
  **Late landing** (a wrapper forces listing → record deletion → fragment publication): for E2,
  a fragment that lands at a position the entry names finds this retirement's mark and is gone
  after grace; for E3, a fragment that lands after the listing stays on disk across GC passes,
  never reclaimed and never an error: the stated residual (Design).
  **Controls:** an `Open` session with the same kind of record keeps the record and every
  fragment of its chunk; a committed object's fragments on the same servers are untouched; a
  server left out at startup, and a server unreachable this pass (each leaving the fleet short of
  `operator_fleet_size`), each leave the record and every fragment of its chunk in place with a
  typed hold, and once the server is back and the fleet is whole the record retires and that
  server's fragments are gone after grace too; a `part:` value that will not decode (an
  unsupported scheme such as `RS(0,0)`, which `decode_part_record` refuses) is held with a typed
  outcome and an audit event saying it needs a human, and the session's terminal delete does not
  fire.
- **Falsifiability:** RED is reachable in-process on `cargo test -p wyrd-custodian`. On
  child-1's base, (D) is red: no walk, no terminal delete, `mpuctl.count` unchanged; (E) is red:
  child-1 holds the untrusted `part:` record and the session never ends. (D)'s double-decrement
  race, (E)'s ordering and (E)'s late landing are forced by wrappers, not timing. (E)'s fleet
  controls are red on a retirement that runs on a short fleet (the missing server's fragments are
  never marked, and GC keeps them for good once the record is gone).
- **Invariant to restore:** **C-1** (`docs/principles.md` §5 C-1 and its §6 row *storage
  lifecycle / reclamation*), the session's share: every session state the drain owns has an exit
  (terminal delete) that fires exactly once, and no record is deleted whose placement the drain
  cannot trust without first marking every fragment of its chunk that a **whole-fleet** listing
  finds and every position the record itself names. The one gap is stated, not hidden: a write
  still publishing at that listing, to a position the record does not name, lands with no
  evidence and is kept for good (never lost). **Sources:** the
  maintainer's rule 2026-07-25; decision 4 rule 2 (`0016:1123-1128`); decision 5's backstop
  (`0016:1654-1668`); `gc.rs:69` and its safety gate at `:654`. SELF-TEST: it binds the walk and
  terminal delete in the drain, their dispatch in the deployed custodian and GC's reclamation of
  the marks.
- **Repo + branch target:** getwyrd/wyrd @ main
- **Depends on:** 1062, 1007, 1008, 1009
- **Conflicts with:** 1014
- **Ordering note:** wave 2 or 3 of #659's split. Builds on child-1's drain step, cursor and
  dispatch leg. Conflicts with child-3 on the drain dispatch in `reconcile_step` / the deployed
  caller and on `docs/design/architecture/06-runtime-view.md:80-86`; neither needs the other's
  outcome. **External, to add to `Depends on` after acceptance:** `658, 634, 662` (carried
  through child-1; list them so the edge survives if #658's split re-points it). **Downstream:**
  #625 (`W_tombstone`) supplies the tombstone input this child defines, so #625's edge to #659
  becomes an edge to this child; #508 and #633 also need this child. **Intake-cap override
  (wyrd-pdca-P1):** the human's 2026-10-30 override for #659's split.
  **Re-pointed 2026-11-11:** #659's child-1 (#1012) was itself split by mode into #1061 (cursor,
  walk, dispatch leg, `retire:records:` drain) and #1062 (`retire:bytes:` drain: mark guard,
  placement hold, owned-residue hold). "child-1" in this brief means both; `Depends on` names
  #1062, which brings #1061.
- **Scope:** the owned `sidx:` walk for `Aborting` and `Completed` sessions; retirement by chunk
  id of untrusted `part:` and `sidx:` records for `Aborting` sessions, on whole-fleet passes only
  (replacing child-1's hold for that case); the terminal session delete and its exactly-once
  `mpuctl` decrement; the tombstone input on the drain context, with the deployed caller passing
  "retain"; the docs. /
  out of scope:
  - the drain step itself, its cursor, and the segments-before-parts rule (child-1);
  - the migration gate and the routing switch (child-3);
  - the reaper's fences (idle `Open`, `W_completing` rollback, `W_session`) and the
    `W_tombstone` clock (#625); the operator abort (#633);
  - recovering a `part:` value that will not decode: its key names no chunk
    (`part:<id>:<n>`, `multipart.rs:1386`), so this child holds it (E's control);
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
  - an untrusted record deleted without marking by chunk id → (E) fails;
  - retirement by chunk id run on a fleet short of `operator_fleet_size` → (E)'s fleet controls
    fail;
  - an E2 entry deleted after marking only what the listing found (the positions it names left
    unmarked) → (E)'s late-landing leg fails.
- **Production reach:**
  - (a) The deployed custodian's drain leg gains the walk and the terminal delete. In production
    an `Aborting` session reaches its terminal delete; a `Completed` one keeps its tombstone
    record and its `mpuctl` slot until #625 supplies the window. Nothing in production creates a
    session until #508, which lands with or after #625 (`0016:236-268`).
  - (b) Retirement by chunk id reads the fleet's fragment listings, and only on a pass whose fleet
    is whole: state the cost per pass, and whether it reuses the listing GC already takes under
    the same gate (an open point of the 2026-09-18 comment). Any configured server down pauses
    these retirements, as it pauses GC; nothing is lost while paused.
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
    reason `untrusted-staged-record`; an undecodable `sidx:` held by its key's chunk `:1392`; an
    undecodable `part:` makes the set incomplete `:1414`); `RestoreReport::staged_untrusted`
    (`crates/custodian/src/restore.rs:179`). Lines on `db1a066`.
  - **Peer for the whole-fleet gate:** the deployed loop's GC gate
    (`crates/server/src/custodian.rs:636-680`, `fleet.len() == operator_fleet_size`), and the
    reason it gives: chunk-wide evidence retired on a partial fleet strands what a missing server
    holds. Mirror it; do not invent a weaker test.
  - **Late writes:** `crates/core/src/multipart/stage.rs:46-53` (an abandoned write may still
    land, position coverage); `gc.rs:265-276` ("`D` bounds acceptance, not effect"); GC keeps an
    unmarked, unreferenced fragment for good (`gc.rs:738`).
  - **Tombstone:** the `AlreadyCompleted` answer (`multipart.rs:4789-4795` on `12dd082`).
- **Prior-art check (triage cycles):** the parent's (2026-10-19): no terminal delete and no
  `mpuctl` decrement exist on `origin/main`. Rejected: the discontinued #636 patch decoded
  `RS(0,0)`, marked nothing and deleted the only record naming the bytes. The new test file does
  not exist on `376b106`.
- **Disposition hint:** new-feature

**Sources for claims taken from the tracker** (for the sign-off reader; Do need not open them):
the 2026-09-18 gap note with the untrusted-record rule,
https://github.com/getwyrd/wyrd/issues/659#issuecomment-5732403299; copy in
`results/issue_659/notes.json`. The parts this brief rests on, quoted:

> **Proposed rule.** For a session in `Aborting` only, retire an untrusted record by **chunk id**
> instead of by placement: 1. Mark `orphan:` on every fragment carrying that chunk id, found from
> the fleet's fragment listings (a `FragmentId` carries its chunk), not from the untrusted
> placement. 2. Then delete the record, in the same mark-then-delete order `retire:bytes` already
> means. For an `Open` session nothing changes: the record keeps holding its chunk.
>
> **Suggested acceptance legs.** An `Aborting` session holding a `part:` record with a
> wrong-length placement, and another holding an `sidx:` entry whose value will not decode. After
> the drain, and a GC pass past grace: every fragment of both chunks is gone; both records are
> gone; the terminal delete has fired […]. Control: an `Open` session with the same kind of
> record keeps every fragment of its chunk, and its record. Control: a committed object's
> fragments on the same servers are untouched.
>
> **Open points.** Cost: finding fragments by chunk id needs a fleet listing. Can the drain reuse
> the one GC already takes each pass?

The whole-fleet gate and the late-write rule are this brief's additions to that rule (plan review,
2026-11-11); the comment does not address either.

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

**Untrusted staged records** (the 2026-09-18 comment, excerpted below). For an `Aborting`
session only, a `part:` or `sidx:` record whose placement fails validation, or an `sidx:` value
that will not decode, is retired **by chunk id**: mark every fragment bearing that chunk id that
the fleet's fragment listings find (a `FragmentId` carries its chunk) rather than trusting the
placement, then delete the record. This replaces child-1's hold for `Aborting` sessions; for any
other state that hold stays. Three rules bound it:

- **Whole fleet only.** A listing finds only what the servers it reaches hold. Retirement by chunk
  id runs only on a pass whose fleet is the whole operator-configured fleet, the gate the deployed
  loop already puts on GC for this very reason (`crates/server/src/custodian.rs:636-680`). On any
  other pass the record is held as child-1 holds it, with a typed outcome. The drain context
  carries what the deployed caller knows (its fleet and `operator_fleet_size`, or the gate's
  answer); Do states which.
- **Late writes.** A listing shows what is on disk now. A `part:` record's chunk has no write in
  flight: the part commit follows every write's `Ok` (`stage.rs:42`), and the one later writer of
  a staged chunk, reconstruction's re-place, runs only for an `Open` upload (`gc.rs:1332`) and
  pre-marks its target. An `sidx:` entry's chunk can have one: a write authorized before the Abort
  may still land after any listing (`stage.rs:46-53`; `gc.rs:265-276`). So for an `sidx:` entry
  whose value decodes, the drain also marks every position the entry names
  (`(placement[i], (chunk, i))`), the position coverage a trusted entry gets (`stage.rs:49-51`).
  **Residual:** a write that lands after the listing at a position the entry does not name (its
  value undecodable, or its placement damaged where the write went) has no mark and no record, and
  GC keeps it for good (`gc.rs:738`): kept, never lost, never reclaimed automatically. The docs
  say so. The alternative, holding such entries for operator repair, keeps every fragment of the
  chunk and the session's admission slot instead; this brief takes the residual, and reversing
  that is the human's call.
- **Not recoverable here.** A `part:` value that will not decode. Its key names no chunk, so
  recovering its chunk ids would mean trusting the value the decoder refused; GC already reads it
  as making the staged set incomplete (`gc.rs:1414`) and keeps everything. The drain holds it,
  typed and audited as needing a human.

Gateway-minted chunk ids carry a random epoch and do not collide (`0016:830`, X36 `:2575`), and
GC still checks every fragment against the reference set and waits out the grace, so a fragment
another valid record names is kept. An `Open` session's record keeps protecting its chunk.

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
of a dead upload are retired by chunk id, only on passes that see the whole fleet; the late-write
residual for an untrusted `sidx:` entry; an undecodable `part:` record needs a human.

## Alternatives considered

- **Skip untrusted records.** The record stays for ever and the session's terminal delete never
  fires. Rejected (C-1: a state nothing exits).
- **Delete untrusted records without marking.** The discontinued patch's `RS(0,0)` failure.
  Rejected.
- **Terminal-delete `Completed` sessions now.** Breaks an identical Complete retry. Rejected;
  the tombstone input.
- **Retire by chunk id on whatever fleet the pass has.** A server left out at startup or
  unreachable this pass keeps fragments nothing marks, and they are kept for good once the record
  is gone. Rejected; the whole-fleet gate GC already uses.
- **Hold every untrusted `sidx:` entry for operator repair.** Sound, but keeps every fragment of
  the chunk, the session record and its admission slot until a human acts, to save at most one
  late write's bytes. Not taken; the human may choose it instead of the stated residual.

## Impact & compatibility

No record shape changes. Sessions now leave the store and release their `mpuctl` slot, in
production only for `Aborting` sessions until #625.

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a draft PR MAY
happen during the cycle (useful for CI feedback). The PR MUST NOT be marked ready before
sign-off accepts.

Plan-review response (2026-11-11, all three findings verified on `db1a066` and taken): (1) `RS(0,0)`
does not decode as a `part:` record, so (E) now uses a wrong-length placement under a supported
scheme, tests an undecodable `sidx:` through its key's chunk id, and holds an undecodable `part:`
(its key names no chunk) as out of scope; (2) a listing cannot see a late write, so an `sidx:`
entry's own positions are marked too, and the remaining gap is stated as a residual (kept, never
lost) with a wrapper-forced test; holding instead is offered to the human; (3) retirement by chunk
id now runs only on whole-fleet passes, mirroring the deployed GC gate, with startup-missing and
runtime-unreachable controls; the tracker rule is now quoted in the brief.

## Iteration 1 — carry-forward (from the previous attempt)
- Sign-off rationale: The design and the terminal delete with its exactly-once mpuctl decrement are sound; keep them. Fix the two confirmed stalls, both reproduced by the reviewer, the adversary and the code review: 1. marks.rs:520 — Walk::part restarts an untrusted part at position 0 (self.entry(&entry, 0)) and ignores the saved cursor `from`; parts with more than ~232 positions (e.g. 40 RS(4,2) chunks, one wrong-length) stall for ever as Partial with no hold or audit. Resume from the cursor safely across changing fleet listings (trusted positions in record order, then listed ones in BTreeMap order), or hold with a typed outcome. 2. sessions.rs:443 — the owned sidx: walk restarts each step at the head of the range and at each entry's first position; an entry with more than MARKS_PER_STEP (233) positions burns the budget re-reading its own marks, returns Walked::Stop with no hold, and blocks every later entry of the session. Make progress resumable within an entry, keeping mark-before-delete and late-write coverage. Add regressions through repeated reconcile_step calls that cross MARKS_PER_STEP (multi-chunk E1 leg, oversized E2 entry), SESSION_STEPS_PER_PASS and SESSION_PAGE (>16 sessions), to kill the surviving budget mutants (C5: 55 missed). Also: add a Completed-session (Tombstone::Elapsed) control holding a wrong-length and an undecodable sidx: entry, asserting both are held and their fragments kept (sessions.rs:377 aborting guard survived mutation); correct the cost claim at retirement.rs:145-146 (real bound is held sessions x fleet size) or charge listings against the step budget. Rebase onto current origin/main so diff coverage can be measured. Size is acceptable as one slice (most of the bulk is the test file and one-line context ripples); do not split.
- Sign-off session carry-forward (captured live, before §9 flattened it):
  The design and the terminal delete with its exactly-once mpuctl decrement are sound; keep them. Fix the two confirmed stalls, both reproduced by the reviewer, the adversary and the code review:
  1. marks.rs:520 — Walk::part restarts an untrusted part at position 0 (self.entry(&entry, 0)) and ignores the saved cursor `from`; parts with more than ~232 positions (e.g. 40 RS(4,2) chunks, one wrong-length) stall for ever as Partial with no hold or audit. Resume from the cursor safely across changing fleet listings (trusted positions in record order, then listed ones in BTreeMap order), or hold with a typed outcome.
  2. sessions.rs:443 — the owned sidx: walk restarts each step at the head of the range and at each entry's first position; an entry with more than MARKS_PER_STEP (233) positions burns the budget re-reading its own marks, returns Walked::Stop with no hold, and blocks every later entry of the session. Make progress resumable within an entry, keeping mark-before-delete and late-write coverage.
  Add regressions through repeated reconcile_step calls that cross MARKS_PER_STEP (multi-chunk E1 leg, oversized E2 entry), SESSION_STEPS_PER_PASS and SESSION_PAGE (>16 sessions), to kill the surviving budget mutants (C5: 55 missed).
  Also: add a Completed-session (Tombstone::Elapsed) control holding a wrong-length and an undecodable sidx: entry, asserting both are held and their fragments kept (sessions.rs:377 aborting guard survived mutation); correct the cost claim at retirement.rs:145-146 (real bound is held sessions x fleet size) or charge listings against the step budget.
  Rebase onto current origin/main so diff coverage can be measured. Size is acceptable as one slice (most of the bulk is the test file and one-line context ripples); do not split.
- Failing gate: C4 diff coverage: changed lines executed by the patch's tests (advisory) — diff coverage not measured — patch.diff does not apply on origin/main
- Failing gate: C5 surviving mutants on the bundle diff (cargo mutants --in-diff) (advisory) — 117 mutants tested in 42m: 55 missed, 43 caught, 17 unviable, 2 timeouts
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 7 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_1013/review-
- Full previous attempt preserved in `iteration-v1/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 2 — carry-forward (from the previous attempt)
- Sign-off rationale: Resume only: iteration 2 was never reviewed because gitleaks was missing; it is now installed (v8.30.1, ~/.local/bin/gitleaks). Keep iteration 2's patch and design (session leg, terminal delete with exactly-once mpuctl decrement, resumable part cursor and owned-walk bookmark); do not redesign. Run the target's commit hook (forbidden-file check + gitleaks) over the patch and record the result in build-notes.md, then let Check run the full review. Size: still one slice, as decided in iteration 1; do not split.
- Sign-off session carry-forward (captured live, before §9 flattened it):
  Resume only: iteration 2 was never reviewed because gitleaks was missing; it is now installed (v8.30.1, ~/.local/bin/gitleaks).
  Keep iteration 2's patch and design (session leg, terminal delete with exactly-once mpuctl decrement, resumable part cursor and owned-walk bookmark); do not redesign.
  Run the target's commit hook (forbidden-file check + gitleaks) over the patch and record the result in build-notes.md, then let Check run the full review.
  Size: still one slice, as decided in iteration 1; do not split.
- Full previous attempt preserved in `iteration-v2/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 3 — carry-forward (from the previous attempt)
- Sign-off rationale: Keep iteration 3's design and patch (session leg, terminal delete with exactly-once mpuctl decrement, resumable part cursor and owned-walk bookmark); the safety core held under adversarial probing. Fix only: 1. sessions.rs:619/626/673-679 — a held entry claims first_left with count 0, so a later wide entry's progress is never bookmarked and it restarts at position 0 every pass (stalls at 232-233/256 while any earlier entry of the session is held). Bookmark the first entry that actually advanced (covered > 0); resume already matches by key (:601-606). Add a repeated-reconcile_step regression: held first entry + RS(128,128) second entry; assert the eligible entry retires while the held one stays (Aborting and Completed/Tombstone::Elapsed cases). 2. Guard the deployed caller's new inputs (crates/server/src/custodian.rs:577 operator_fleet_size, :580 Tombstone::Retain): add a server test through run_reconstruction_until where the fleet is short from startup — an Aborting session with an untrusted entry expects a partial-fleet hold, a Completed session keeps its mpu: record. Mutating either input must turn it red. 3. Test lost-bookmark recovery (sessions.rs:704 `!moved` mutant survives) and assert the literal stored key `custodian:owned-walk:<id>` rather than building it with owned_walk_key. Size: one slice — no split as long as the pure logic code stays under 150 KB (test file and one-line context ripples don't count). Diff coverage stays unmeasurable while the patch is stacked on #1012; that is not a builder defect. Add no DST scenario (#660 owns it).
- Sign-off session carry-forward (captured live, before §9 flattened it):
  Keep iteration 3's design and patch (session leg, terminal delete with exactly-once mpuctl decrement, resumable part cursor and owned-walk bookmark); the safety core held under adversarial probing. Fix only:
  1. sessions.rs:619/626/673-679 — a held entry claims first_left with count 0, so a later wide entry's progress is never bookmarked and it restarts at position 0 every pass (stalls at 232-233/256 while any earlier entry of the session is held). Bookmark the first entry that actually advanced (covered > 0); resume already matches by key (:601-606). Add a repeated-reconcile_step regression: held first entry + RS(128,128) second entry; assert the eligible entry retires while the held one stays (Aborting and Completed/Tombstone::Elapsed cases).
  2. Guard the deployed caller's new inputs (crates/server/src/custodian.rs:577 operator_fleet_size, :580 Tombstone::Retain): add a server test through run_reconstruction_until where the fleet is short from startup — an Aborting session with an untrusted entry expects a partial-fleet hold, a Completed session keeps its mpu: record. Mutating either input must turn it red.
  3. Test lost-bookmark recovery (sessions.rs:704 `!moved` mutant survives) and assert the literal stored key `custodian:owned-walk:<id>` rather than building it with owned_walk_key.
  Size: one slice — no split as long as the pure logic code stays under 150 KB (test file and one-line context ripples don't count). Diff coverage stays unmeasurable while the patch is stacked on #1012; that is not a builder defect. Add no DST scenario (#660 owns it).
- Failing gate: C4 diff coverage: changed lines executed by the patch's tests (advisory) — diff coverage not measured — patch.diff does not apply on origin/main
- Failing gate: C5 surviving mutants on the bundle diff (cargo mutants --in-diff) (advisory) — 214 mutants tested in 70m: 59 missed, 133 caught, 20 unviable, 2 timeouts
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 3 blocking, 1 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_1013/review-
- Full previous attempt preserved in `iteration-v3/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 4 — carry-forward (from the previous attempt)
- Sign-off rationale: Keep iteration 4's design and patch (session leg, terminal delete with exactly-once mpuctl decrement, resumable part cursor, owned-walk bookmark, whole-fleet gate, deployed Tombstone::Retain); the safety core held again under 10 hand-mutations. Fix only: 1. sessions.rs:530 — the owned walk always calls scan_page with after = None and discards the continuation, so 16 held entries at the head of a session's sidx: range hide every later entry for good (reproduced by reviewer and adversary: trusted 17th entry left unmarked after 20 passes, Aborting on a short fleet and Completed with Tombstone::Elapsed). Carry bounded scan progress past a page where every entry is held (resume from the last held key, wrap to the start when the range ends), keeping intra-entry mark progress and mark-before-delete. Fix the module doc claim at sessions.rs:44-46 to match. Add repeated-reconcile_step regressions: 16 held entries + 1 trusted entry, for Aborting and for Completed/Tombstone::Elapsed; assert the trusted entry is marked and deleted while the held ones stay. 2. sessions.rs:697 — bookmark selection requires covered > 0, but covered counts only named positions, not the fleet-listed positions walked at :680, so an undecodable entry never gets a bookmark. Two held entries ahead of an undecodable entry with LISTED_PER_CHUNK (232) listed fragments leave it one fragment short every pass. Count listed-fragment progress for scheduling (without treating the listing offset as durable evidence) and add that three-entry regression. Out of scope for the builder: the owned walk's never-deleted marks over positions that never landed (adversary finding; cf. #825) — a tracking decision for the human, not a builder item. Size: still one slice under the iteration-3 rule (logic code ~38.6 KB, under 150 KB). Diff coverage stays unmeasurable while stacked; not a builder defect. Add no DST scenario (#660 owns it).
- Sign-off session carry-forward (captured live, before §9 flattened it):
  Keep iteration 4's design and patch (session leg, terminal delete with exactly-once mpuctl decrement, resumable part cursor, owned-walk bookmark, whole-fleet gate, deployed Tombstone::Retain); the safety core held again under 10 hand-mutations. Fix only:
  1. sessions.rs:530 — the owned walk always calls scan_page with after = None and discards the continuation, so 16 held entries at the head of a session's sidx: range hide every later entry for good (reproduced by reviewer and adversary: trusted 17th entry left unmarked after 20 passes, Aborting on a short fleet and Completed with Tombstone::Elapsed). Carry bounded scan progress past a page where every entry is held (resume from the last held key, wrap to the start when the range ends), keeping intra-entry mark progress and mark-before-delete. Fix the module doc claim at sessions.rs:44-46 to match. Add repeated-reconcile_step regressions: 16 held entries + 1 trusted entry, for Aborting and for Completed/Tombstone::Elapsed; assert the trusted entry is marked and deleted while the held ones stay.
  2. sessions.rs:697 — bookmark selection requires covered > 0, but covered counts only named positions, not the fleet-listed positions walked at :680, so an undecodable entry never gets a bookmark. Two held entries ahead of an undecodable entry with LISTED_PER_CHUNK (232) listed fragments leave it one fragment short every pass. Count listed-fragment progress for scheduling (without treating the listing offset as durable evidence) and add that three-entry regression.
  Out of scope for the builder: the owned walk's never-deleted marks over positions that never landed (adversary finding; cf. #825) — a tracking decision for the human, not a builder item. Size: still one slice under the iteration-3 rule (logic code ~38.6 KB, under 150 KB). Diff coverage stays unmeasurable while stacked; not a builder defect. Add no DST scenario (#660 owns it).
- Failing gate: C4 diff coverage: changed lines executed by the patch's tests (advisory) — diff coverage not measured — patch.diff does not apply on origin/main
- Failing gate: C5 surviving mutants on the bundle diff (cargo mutants --in-diff) (advisory) — 221 mutants tested in 73m: 51 missed, 148 caught, 20 unviable, 2 timeouts
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 4 blocking, 1 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_1013/review-
- Full previous attempt preserved in `iteration-v4/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 5 — carry-forward (from the previous attempt)
- Sign-off rationale: Keep iteration 5's design and patch (session leg, terminal delete with exactly-once mpuctl decrement, resumable part cursor, owned-walk bookmark, scan position past held pages, whole-fleet gate, deployed Tombstone::Retain); the safety core held again under hand-mutations. Fix only the two new bookmark bugs: 1. Starvation (adversary) — sessions.rs:769-787/831-840: an entry re-read from position 0 that marked nothing still takes the bookmark from a held entry's count, so held wide entries move the bookmark every pass and each commit costs a session step; 16 such Aborting sessions starve a 17th forever (no audit event). Do not charge a step for a bookmark-only move while the step holds something, or do not let a zero-progress re-read displace a held entry's count. Add a steady-state-quiet assertion to the three-held test (multipart_session_terminal.rs:2863, no trusted entry) and a 17-session starvation regression through repeated reconcile_step. 2. Recreated bookmark (code review) — sessions.rs:795: a bookmark-only commit guarded only by require_absent(bookmark) can recreate custodian:owned-walk:<id> after another drainer terminal-deleted the session. Add the exact session-record precondition to every bookmark-producing commit, as move_scan does at sessions.rs:1065. Add a wrapper test that delays drainer A's bookmark commit until B finishes the terminal delete, asserting no bookmark is left. Size: still one slice under the iteration-3 rule (logic code ~84 KB, under 150 KB). Diff coverage stays unmeasurable while stacked; not a builder defect. Add no DST scenario (#660 owns it), unchanged from prior rounds.
- Sign-off session carry-forward (captured live, before §9 flattened it):
  Keep iteration 5's design and patch (session leg, terminal delete with exactly-once mpuctl decrement, resumable part cursor, owned-walk bookmark, scan position past held pages, whole-fleet gate, deployed Tombstone::Retain); the safety core held again under hand-mutations. Fix only the two new bookmark bugs:
  1. Starvation (adversary) — sessions.rs:769-787/831-840: an entry re-read from position 0 that marked nothing still takes the bookmark from a held entry's count, so held wide entries move the bookmark every pass and each commit costs a session step; 16 such Aborting sessions starve a 17th forever (no audit event). Do not charge a step for a bookmark-only move while the step holds something, or do not let a zero-progress re-read displace a held entry's count. Add a steady-state-quiet assertion to the three-held test (multipart_session_terminal.rs:2863, no trusted entry) and a 17-session starvation regression through repeated reconcile_step.
  2. Recreated bookmark (code review) — sessions.rs:795: a bookmark-only commit guarded only by require_absent(bookmark) can recreate custodian:owned-walk:<id> after another drainer terminal-deleted the session. Add the exact session-record precondition to every bookmark-producing commit, as move_scan does at sessions.rs:1065. Add a wrapper test that delays drainer A's bookmark commit until B finishes the terminal delete, asserting no bookmark is left.
  Size: still one slice under the iteration-3 rule (logic code ~84 KB, under 150 KB). Diff coverage stays unmeasurable while stacked; not a builder defect. Add no DST scenario (#660 owns it), unchanged from prior rounds.
- Failing gate: C4 diff coverage: changed lines executed by the patch's tests (advisory) — diff coverage not measured — patch.diff does not apply on origin/main
- Failing gate: C5 surviving mutants on the bundle diff (cargo mutants --in-diff) (advisory) — 247 mutants tested in 81m: 50 missed, 175 caught, 20 unviable, 2 timeouts
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 2 blocking, 1 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_1013/review-
- Full previous attempt preserved in `iteration-v5/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).
