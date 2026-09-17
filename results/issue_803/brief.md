# custodian: staged protection class for GC and restore (662.1)

> Child 1 of 2 of #662's split (637.2), **re-planned 2026-09-16** after four attempts, on the
> human's `iterated-to-Plan` sign-off of v4 (`iteration-v4/SUMMARY.md` §9, Eduard Ralph,
> 2026-09-15) — see "Why this brief changed" at the end. The tracker issue's title and body are
> still the split-time brief ("in the shared reference set"); THIS file supersedes them. Do reads
> ONLY this file; keep the `- **Label:** value`
> lines. Citations are on `origin/main` @ `78f9859` (re-verified 2026-09-16). 0016 =
> `docs/design/proposals/draft/0016-multipart-commit-protocol.md`; background: its decision 2
> (`:765-893`).

- **Slug:** staged-protection-gc-restore
- **Kind:** enhancement
- **Defect:** staged bytes have no protection class. `ReferenceSet` holds committed placements
  only (`crates/custodian/src/gc.rs:383-413`), built from the `inode:` scan alone (`:478-573`).
  A committed part's fragments (`part:`) and an upload's in-flight owned fragments (`sidx:`,
  #772) are in no protected set, so GC reclaims one as soon as it carries an `orphan:` mark past
  grace (`:272`, `:277-326`). Restore gates on the same predicate
  (`crates/custodian/src/restore.rs:385`) and its pending skip (`:435-438`) no longer sees owned
  entries, so it marks a live upload's fragments stranded (`:440-443`) and the next GC pass
  deletes them.
- **Success criterion:** the NEW file `crates/custodian/tests/staged_protection.rs` passes over
  in-memory doubles. Records are seeded as raw JSON the base decoders accept (`SessionRecord`
  and `PartRecord` have no writer-side constructor, `crates/core/src/multipart.rs:2127`,
  `:2492`; shapes as the helpers in `crates/core/tests/multipart_session_records.rs:81-141`,
  with the `Completed` state spelled as at `:298-304`), each round-tripped through
  `decode_session_record` / `decode_part_record` / `decode_owned_entry` first. Every protection
  leg also seeds an unprotected control the pass does reclaim or mark. Legs:
  **(A) GC protects both staged classes, in every session state.** For each of an `Open`, a
  `Completing`, an `Aborting` and a `Completed` session: a committed `part:` record (fragment
  `F1`) and an owned `sidx:` entry (`F2`) on D-server doubles, each with an `orphan:` mark past
  grace. After `reconcile_step` with a `GcContext`, every one survives. (Unmarked, GC's
  conservative arm keeps any fragment, `gc.rs:307-310`, so the mark is what makes the leg bite.)
  Base: all reclaimed.
  **(B) Restore protects them through the same rule.** `reconcile_after_restore` over the same
  store, unmarked, writes no `orphan:` key for any staged fragment and `stranded_marked` counts
  the control alone; a GC pass past grace then keeps every staged fragment. Base: marked, then
  deleted. (Staged counters are #664's.)
  **(C) Source before destination, both handoffs (`0016:782-800`, X67 `:2596`).** A double
  acts right after the first of the two reads involved completes — the source range or the
  destination range/scan, whichever GC issues first: (i) a part commit, ONE atomic batch that
  deletes the chunk's `sidx:` entry and writes its `part:` record (`0016:782-784`; source
  `sidx:<id>:`, destination `part:<id>:`); (ii) a publication, which is TWO batches, not one
  (`0016:793-800`, `:941-944`, `:964-966`): first the root flip — the session goes to
  `Completing` before it, and the flip batch writes the committed inode naming the chunk and
  moves the session to `Completed`, leaving the `part:` record in place; then, as a separate later
  batch, the retirement drain deletes that `part:` record (source `part:<id>:`, destination the
  `inode:` scan). Leg C(ii) runs two schedules: flip AND drain both between the two reads (the
  only one a destination-first build sees in neither class), and flip between the reads with the
  drain after the second. The double omits the `retire:records:` obligation key itself: no pass
  in this slice reads `retire:` (#804's), and the drain batch is what deletes the record. In every
  schedule the fragment is marked past grace and is not reclaimed. Base: reclaimed.
  **(D) Bounded per-session reads (`0016:890`).** With the `scan` cap lowered, more
  sessions-with-parts than a global `scan("part:")` could return: `reconcile_step` (GC) and
  `reconcile_after_restore` both succeed, and the double records no `scan`/`scan_page` of the
  bare `part:` or `sidx:` prefix. A guard.
  **(E) What GC and restore cannot read or trust fails closed (ADR-0045 decision 3,
  `docs/design/adr/0045-metadata-validation-boundaries.md:55-59`).** (i) A `part:` value that
  will not decode; a key inside a listed session's `part:<id>:` range that `parse_part_key`
  rejects, seeded with a value `decode_part_record` accepts (e.g. an unpadded part number — key
  and value are validated separately, `crates/core/src/multipart.rs:1279`, `:2578`); an `sidx:`
  key naming no chunk; or an `mpu:` key naming no upload (one test each): GC reclaims
  nothing and answers `Reconciled::Blocked` (as `gc.rs:348-355`); restore marks nothing and names
  the record in `RestoreReport::unresolvable`. (ii) A staged placement of the wrong length, or an
  undecodable owned value under an `sidx:` key that names its chunk: that whole chunk is held in
  both passes and the record is named on each pass's audit seam, while unrelated fragments are
  still judged. (iii) A store fault: the metadata double fails the read of exactly one of `mpu:`,
  a session's `sidx:<id>:`, a session's `part:<id>:` (one test each). GC and restore both return
  `Err` whose text names the failed read, every staged fragment is still on disk and unmarked, and
  the double's read log shows the faulted read was issued. Healed, the same store runs a clean GC
  pass that still keeps them (the control). Base: reclaimed or marked; base restore returns `Ok`.
  **(F) Scrub and drain status do not see upload records.** One store holds a committed object
  whose only fragment is missing from its D server, a committed object with fragments on server
  `S` (draining, via `set_lifecycle(.., DServerLifecycle::Draining)`), and a listed session with a `part:` record and an owned
  `sidx:` entry whose fragments sit on servers other than `S`. Compared with the same store minus
  the upload records, `reconcile_step` with a `ScrubContext` gives the same `Reconciled` and
  leaves the same `repair:` key for the missing chunk (`wyrd_core::repair::repair_key`), and
  `reconciliation_status(S)` gives the same answer. That holds with the upload records healthy,
  with each E(i) damaged record in place (neither call answers `Blocked` or
  `PendingUnresolvable` on its account), and with each E(iii) store fault armed (neither call
  returns `Err`). The double logs no read under `mpu:`, `sidx:` or `part:` from either call. A
  guard: green on base, red against the rejected design where all four consumers share the
  staged read. Mark the leg `// deferred: #663, #664` — those slices add upload records to scrub
  and drain status and own changing it.
  **(G) Seeded DST**, appended to the EXISTING `crates/dst/tests/custodian.rs`
  (`#![cfg(madsim)]`, `:53`; no new DST file): a concurrent part commit, then the publication
  flip (session to `Completing` before, `Completed` within the batch that writes the inode), then
  the retirement drain's deletion of the `part:` record as its own batch — each at its own
  seed-chosen instant during GC's reads — never gets the chunk reclaimed; a coverage property
  proves landings between and outside those reads are both reached, as `prop_restore_two_readings_cover_the_divergence_window` (`:2139-2173`)
  does. Registered in the campaign as its neighbours are.
  **(H) `cargo xtask ci` green** (it runs the madsim DST suite and leg I).
  **(I) The operator verdict names staged records as staged.** A new `#[test]` in
  `crates/server/src/cli.rs`'s own test module, beside `:2978-3011` (`restore_verdict` is private
  to the server lib, so no integration test can reach it). A `RestoreReport` whose `unresolvable`
  is `["inode:7", "mpu:<id>", "part:<id>:000001", "sidx:<id>:000001:9"]` (keys as restore names
  them, `gc::object_name`, `gc.rs:588`): `restore_verdict(&report).needs_human` is `true`; the
  printed lines contain `INCOMPLETE`, `4 record(s) UNREADABLE`, `4 record(s) could not be READ`,
  each of the four names, `staged multipart record`, `action=unresolvable-chunk-map` and
  `action=unresolvable-staged-record` (the audit action restore's staged arm emits — use this
  exact string in `restore.rs` too); they contain neither `committed object(s) UNREADABLE` nor
  `committed object(s) could not be READ`. A second report holding only `part:<id>:000001`: the
  same, with `1 record(s)`. The existing test's total-count phrase (`:3004-3010`) moves to
  `record(s) could not be READ`; its other asserts stay. Base: red by assertion — base prints
  `4 committed object(s) could not be READ` for all four names (`cli.rs:1331-1341`).
- **Falsifiability:** RED in-process on `origin/main` @ `78f9859`, no container. A, B, C and E
  fail by assertion: every seeded record class exists on `main` (#691, #715, #716, #771, #772)
  and nothing in the maintenance plane reads it; E(iii)'s restore half fails because base restore
  returns `Ok` and marks. D and F are guards, green on base; F is red against v4's design, which
  is the failure it exists to catch. G edits an existing file that C4-ci runs under
  `--cfg madsim`. I sits in a modified file, so C4-verify cannot show its red (it earns red only
  from an added `*/tests/*.rs`, `run-verify.sh:141-144`) and C4-ci runs it green: Do runs leg I
  once against base's `restore_verdict` (production hunks of `cli.rs` reverted) and records the
  failing assertion in `build-notes.md`. The NEW file `staged_protection.rs` names NO symbol this
  slice adds — no new set member, reason string or report field (leg I's strings live in
  `cli.rs`, compiled with the fix); everything it uses exists on `main` today, e.g. `wyrd_custodian::{reconcile_step,
  reconcile_after_restore, reconciliation_status, set_lifecycle, mark_orphaned, GcContext,
  ScrubContext, ExpiredPendingPolicy, Custodian, FencedZone, Reconciled, RestoreReport,
  ReconciliationStatus, DServerLifecycle}`; `wyrd_core::multipart::{mpu_key, part_key, part_range, sidx_key,
  sidx_range, UploadId, PartNumber, OwnedEntry, StagedPlacement, decode_session_record,
  decode_part_record, decode_owned_entry}`; `wyrd_core::metadata::{orphan_key, inode_key, encode,
  decode, InodeRecord, PendingEntry, EcScheme, ORPHAN_PREFIX}`; `wyrd_core::repair::repair_key`;
  `wyrd_traits` store types. A red leg that fails to compile is UNVERIFIABLE
  (`engine/scripts/run-verify.sh:522-541`). Record in `build-notes.md` how many tests ran red,
  all by assertion.
- **Invariant to restore:** C-1 — no permanent or data-losing failure mode is an acceptable
  cost: every durable byte is, at every instant, protected by a record that names it or
  evidenced for reclamation (`docs/principles.md` §5 C-1, §6 storage-lifecycle row;
  `0016:2802-2813`; `gc.rs:30-33`; 0016 invariant (2), `:869-871`). Here: every pass that deletes
  or marks — GC's reclaim and the post-restore mark gate — protects a staged byte, as a class
  disjoint from committed placements so each consumer decides for itself (`0016:767-782`,
  `:881`); protection overlaps across handoffs — "no gaps", never a partition
  (`0016:2911-2922`). A consumer that does not yet act on staged bytes (scrub, drain status —
  until #663 and #664) must not inherit their reads: its answers, its failures and its read cost
  stay what they are on `main` (`desired_state.rs:178-180`: one damaged record never turns drain
  status into an `Err`). SELF-TEST: a filter inside GC alone passes A and fails B; one staged
  read shared by all four consumers passes A–E and fails F.
- **Repo + branch target:** getwyrd/wyrd @ main
- **Conflicts with:** 804, 722
- **Ordering note:** first of the pair: `Conflicts with` #804 (shared `gc.rs`, DST file,
  `06-runtime-view.md`; no build-on), and the scheduler builds the lower id — this one — first.
  Also conflicts with #722 (appends to `crates/dst/tests/custodian.rs`). #661 is merged (PR
  #802). Downstream: #664 and #663 depend on this slice; #805 (filed at this re-plan as a
  sub-issue of this one, not yet briefed) will depend on it and conflict with #664.
  `crates/server/src/cli.rs`: this slice edits the post-restore verdict (`:1256-1380`, test
  `:2978-3011`); #738, #774, #778 and #779 cite other regions (`:64`, `:492-560`, `:1039`,
  `:2113-2534`, `:2856`), so no conflict is declared with them; #664, which edits the same verdict,
  already builds after this slice. **Intake-cap overrides (wyrd-pdca-P1):** (1) granted by
  Eduard Ralph in #662's re-plan session, 2026-09-15, for #662's two-child split, at `planned
  22/6 (cap) — room for 0`; (2) granted by Eduard Ralph in this re-plan session, 2026-09-16, for
  #805, at `planned 23/6 (cap) — room for 0` — copy it into #805's brief when that is planned.
  This brief itself repairs an UNPLANNED id and is exempt. **Size:** v1–v4 ran 109–131 KB
  against the 100 KB backstop; the human accepted an oversize patch at this re-plan
  (2026-09-16), so size alone is not a reason to split again.
- **Surfaces:** data
- **Difficulty:** high — two destructive passes (`gc.rs`, `restore.rs`), the operator verdict
  (`cli.rs`), a DST property and two docs, plus a boundary with scrub (`scrub.rs:88`) and drain
  status (`desired_state.rs:188`) that must not move.
- **Do model:** opus-max
- **Scope:** the staged protection class, for the two passes that delete or mark: GC's reclaim
  and the post-restore mark gate. It covers the committed `part:` records and owned `sidx:`
  entries of every session listed under `mpu:`, whatever its state (0016 counts fewer; covering
  more only keeps more), read through each session's own bounded ranges and never a global
  `part:` or `sidx:` scan; within each pass's reading, `sidx:` before `part:` before the `inode:`
  scan. The class is disjoint from committed placements and has its own audit reasons. A staged
  record that cannot be read makes the set incomplete for GC and restore (E(i)); one that reads
  but cannot be trusted holds its chunk (E(ii)); a store fault fails the pass (E(iii)). The human
  accepted at sign-off that one unreadable upload record stalls GC and restore fleet-wide until it
  is repaired — keep that. **Scrub and drain status stay as on `main`:** they read no upload
  record, so an upload record's damage, a fault reading one, or the cost of reading them cannot
  reach their answers (F); the committed reference build they share (`gc.rs:478-573`) keeps its
  behaviour. **Restore** names each staged record it cannot read in `RestoreReport::unresolvable`
  (the existing field, so the report is not clean), and each record it holds as untrusted on its
  audit seam; whether a held record sets `needs_human()` is #664's, marked `// deferred: #664` at
  the site. Restore's docs claim only what the code does: its protection covers upload records
  already durable when the pass read them, and a write or upload that starts mid-pass is #805's,
  marked `// deferred: #805` where the pass reads them. The post-restore command's summary line
  and NEEDS-HUMAN paragraph (`crates/server/src/cli.rs:1263`, `:1331`) name staged records beside
  committed objects, with the text leg I pins; the runbook's UNREADABLE entry
  (`docs/design/architecture/m4-first-deployment-blueprint.md:609`) says the same. No change to the signatures of `reconcile_step`,
  `reconcile_after_restore` or `reconciliation_status`; no new field on a context struct or
  `RestoreReport`. Docs: one paragraph in `docs/design/architecture/06-runtime-view.md` §6.7 step
  2 (`:74`) — GC never reclaims, and restore never marks, a staged fragment; scrub and drain status
  read committed references only. / out of scope: mark shapes, reclaim intent, retirement
  protection (#804); drain status and rebalance reading upload records, restore's staged counters
  and fence (#664); scrub and reconstruction reading them (#663); restore's mid-pass window for
  writes and uploads (#805); the fragment-less sweep (#800); any edit to `desired_state.rs`,
  `rebalance.rs`, `scrub.rs`, `reconstruction.rs`, `crates/core/src/metadata.rs`,
  `crates/core/src/multipart.rs`; 0016 and the ADRs.
- **Repro instruction:** on `origin/main`, seed an `Open` session, one `part:` record and one
  owned `sidx:` entry whose fragments sit on a D server, mark each `orphan:` older than grace,
  run one GC pass: both fragments are deleted.
- **External dependencies:** `typos`, `docs-renderer`
- **Test file:** `crates/custodian/tests/staged_protection.rs` — **NEW**. C4-verify earns its
  red only from an added `*/tests/*.rs` (`engine/scripts/run-verify.sh:141-144`, `:389-392`);
  keep other test edits in existing files (`cli.rs`'s own test module, the DST file). No
  `Cargo.toml` change.
- **Production reach:** the production `reconcile_step` (GC and scrub), `reconcile_after_restore`
  and `reconciliation_status` over in-memory stores. No client creates a session before the S3
  verbs (#508), so the test seeds every staged record — the intended state.
- **Citations expected:** `path:line` on the target branch for every change. Peers Do MAY
  open: `gc.rs:383-452` (`ReferenceSet`, `protection()`, `protects()`); `gc.rs:478-573`
  (`referenced_fragments` — contain an unreadable record as `:496-533` does; this is the build
  scrub and drain status keep); `gc.rs:213-215` and `restore.rs:299-323` (where each pass takes
  its reading today); `scrub.rs:88` and `desired_state.rs:188` (the two callers that must not
  change); `multipart.rs:1132` (`MPU_PREFIX`), `:1212-1329` (`mpu:`/`part:`/`sidx:` keys, ranges
  and parsers), `:2250`, `:2578`, `:3544-3568`, `:3730-3757` (the decoders and
  `StagedPlacement`); `crates/dst/tests/custodian.rs:2116-2173` (the pattern for G);
  `cli.rs:1256-1380` (the post-restore verdict).
- **Prior-art check (triage cycles):** by path across merged, open and closed work
  (2026-09-16): no merged staged class (`git log -S'sidx' origin/main -- crates/custodian/src` is
  empty); no open PR on `gc.rs`, `restore.rs`, `scrub.rs`, `desired_state.rs`, `cli.rs`, the DST
  file or either doc; closed-unmerged #647 is segmented chunk maps, unrelated. Rejected: #508's
  4th attempt (a read-path-only resolver — restore stranded parts, GC deleted them; leg B); #637
  v1 (334 KB); #662 v1 (188 KB, with #804's work; restore held a malformed staged record
  silently — the audit-seam rule above); #803 v1–v4 (see below).
- **Disposition hint:** likely-fix

## Why this brief changed (re-plan 2026-09-16)

Attempts v1–v4 (preserved in `iteration-v1/` … `iteration-v4/`) built the protection correctly
for GC and restore and proved it red→green, but placed the staged read **inside the committed
reference build that all four consumers share**. The v4 sign-off (`iteration-v4/SUMMARY.md` §9,
outcome `iterated-to-Plan`, Eduard Ralph, 2026-09-15) traced three findings to that one placement
and asked for exactly this re-scope — GC/restore protection apart from any shared-builder change
that touches scrub or drain status, and restore's remaining window as its own slice (#805).
The three findings: scrub and drain status failed on a fault reading an upload record, though
the brief said they keep today's answers; every consumer paid up to `1 + 2 × 46` extra scans per
call for records two of them discard; and restore's protection was claimed to be complete while
an upload that starts mid-pass could still be marked. This brief keeps everything the earlier
sign-offs required, and moves the boundary:

- **Kept** from the carry-forwards: sessions in every state (A, B, C(ii), G — iteration 1);
  scrub not `Blocked` over a damaged upload record (now inside F — iteration 2); fail-closed
  retention accepted as-is, and store faults fail GC and restore (E(iii) — iteration 3). The
  `cli.rs` verdict and runbook edits were in all four attempts; leg I now pins them.
- **New:** leg F pins that scrub and drain status neither read upload records nor inherit their
  faults or cost. Restore's mid-pass window moved to #805; this slice narrows restore's docs to
  what the code does and marks the site.
- **Do not** re-attempt the shared-builder placement.

## Plan review (2026-09-16)

- Plan-review response: (1) tracker vs brief — revised: the header and the section above now cite
  the recorded authorization (`iteration-v4/SUMMARY.md` §9). The tracker's title/body are the
  split-time text and the driver does not rewrite them on a re-plan; updating #803's title/body
  before publish is the human's call, flagged at hand-off.
- Plan-review response: (2) publication handoff — revised: C(ii) and G now schedule the root flip
  (inode + `Completed`, `part:` record kept) and the retirement drain (deletes `part:`) as separate
  batches, per `0016:793-800`, `:941-944`.
- Plan-review response: (3) operator verdict had no red check — revised: new leg I pins the text
  and `needs_human`; its red is recorded by Do, since C4-verify only reds added test files.
- Plan-review response: (4) malformed `part:` key — revised: E(i) now includes a key in a
  session's `part:<id>:` range that `parse_part_key` rejects, with a valid value; F covers it
  through "each E(i) damaged record".

## Iteration 5 — carry-forward (from the previous attempt)
- Sign-off rationale: The slice converged; do NOT split (a split was vetoed at the 2026-09-16 re-plan, and the size backstop is overridden — brief.md:152-154). Three implementation-sized fixes, then the same brief applies unchanged: 1. Clear the blocking T4 finding (review-batch.md, gc.rs:810): the mpuctl budget-profile preflight (0016:348, X99 :2628) is deferred to getwyrd/wyrd#806, filed 2026-09-16. Add `// deferred: #806` at the staged_fragments site (gc.rs:809) as the patch already does for #663/#664/#805, and record the finding in review-rejected.md with that reference. Do not implement the check in this slice. 2. Adversary finding 1: add a paging test for one upload's OWN records — one Open upload, store scan cap 2, five part: records and five sidx: entries all marked past grace — for both GC (reconcile_step) and restore (reconcile_after_restore); the mutant "read one page of walk_staged_range then return" (gc.rs:839-851) must fail it. 3. Adversary finding 2: restore's staged-before-committed read order (restore.rs:340 before :350/:360) is claimed at restore.rs:239-241 but untested. Either add a restore version of C(ii) (hook the upload's part: range read and the inode: scan; flip+drain between the reads) or cut the sentence so the doc claims only what a test checks. Ignore: the four plan-advisory lines in §6 (already revised into the brief) and the size-backstop line. build-notes.md correctly says 15 of 17 ran red (D and F are guards, green on base); the gate's "17" is the gate's wording.
- Sign-off session carry-forward (captured live, before §9 flattened it):
  The slice converged; do NOT split (a split was vetoed at the 2026-09-16 re-plan, and the size backstop is overridden — brief.md:152-154). Three implementation-sized fixes, then the same brief applies unchanged:
  1. Clear the blocking T4 finding (review-batch.md, gc.rs:810): the mpuctl budget-profile preflight (0016:348, X99 :2628) is deferred to getwyrd/wyrd#806, filed 2026-09-16. Add `// deferred: #806` at the staged_fragments site (gc.rs:809) as the patch already does for #663/#664/#805, and record the finding in review-rejected.md with that reference. Do not implement the check in this slice.
  2. Adversary finding 1: add a paging test for one upload's OWN records — one Open upload, store scan cap 2, five part: records and five sidx: entries all marked past grace — for both GC (reconcile_step) and restore (reconcile_after_restore); the mutant "read one page of walk_staged_range then return" (gc.rs:839-851) must fail it.
  3. Adversary finding 2: restore's staged-before-committed read order (restore.rs:340 before :350/:360) is claimed at restore.rs:239-241 but untested. Either add a restore version of C(ii) (hook the upload's part: range read and the inode: scan; flip+drain between the reads) or cut the sentence so the doc claims only what a test checks.
  Ignore: the four plan-advisory lines in §6 (already revised into the brief) and the size-backstop line. build-notes.md correctly says 15 of 17 ran red (D and F are guards, green on base); the gate's "17" is the gate's wording.
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 1 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_803/review-b
- Full previous attempt preserved in `iteration-v5/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 6 — carry-forward (from the previous attempt)
- Sign-off rationale: Fix the five implementation-shaped findings from the adversary review, keeping the rest of the brief unchanged: 1. "Hold the chunk whole" test gap — the wrong-length placement tests at staged_protection.rs:1501 coincide with ChunkRef::fragments()'s identity fallback, so a build missing the `held` arm still passes. Move the held fragment to a server the fallback doesn't name (e.g. `(3, frag(held, 2))`) so all three wrong-length legs actually catch the mutant at gc.rs:693/754-756. 2. GC's audit line for an unreadable staged record (emit_unresolvable_staged, gc.rs:1344, called at :263) is never asserted. Add `named_on_audit_seam(GC_AUDIT, <damaged key>)` to the E(i) harness, as E(ii) already does. 3. docs/design/architecture/06-runtime-view.md:78 overclaims "page at a time... never one listing of a whole namespace" — the committed-inode scan is still one meta.scan(b"inode:") (gc.rs:549) and restore does it twice. Narrow the "page at a time" clause to the upload records only. 4. Leg C(ii)/G may test the wrong publication handoff shape — the double may be collapsing the root-flip batch and the retirement-drain batch into one, when 0016:793-800/941-965 and multipart.rs:3144-3150/3424-3432 describe them as two separate batches. Verify and, if the finding holds, revise C(ii) and G to schedule the flip and the drain-deletion of the `part:` record as separate batches. 5. No red test proves the operator-facing verdict actually says "staged record" vs "committed object" — cli.rs:1318-1341 hard-codes committed-object wording and the existing verdict test only seeds inode: names. Add a red assertion using mpu:/part:/sidx: report names, pinning the exact text and exit verdict per brief.md leg I. Explicitly NOT blocking, cleared this round — do not re-raise unchanged: - Validation fitness-to-purpose tradeoff (one unreadable staged record stalls GC/restore fleet-wide) — already accepted at the 2026-09-16 re-plan sign-off; a tracked bug covers the operational follow-up. - Tracker-record vs brief mismatch — already flagged as the human's call at hand-off (Plan review 2026-09-16, response (1)); not a build-blocking item. - Size backstop (153 KB vs 100 KB) — already accepted as an oversize patch at the 2026-09-16 re-plan; do not re-split. Stay iterate-do, not iterate-plan.
- Sign-off session carry-forward (captured live, before §9 flattened it):
  Fix the five implementation-shaped findings from the adversary review, keeping the rest of the brief unchanged:
  1. "Hold the chunk whole" test gap — the wrong-length placement tests at staged_protection.rs:1501 coincide with ChunkRef::fragments()'s identity fallback, so a build missing the `held` arm still passes. Move the held fragment to a server the fallback doesn't name (e.g. `(3, frag(held, 2))`) so all three wrong-length legs actually catch the mutant at gc.rs:693/754-756.
  2. GC's audit line for an unreadable staged record (emit_unresolvable_staged, gc.rs:1344, called at :263) is never asserted. Add `named_on_audit_seam(GC_AUDIT, <damaged key>)` to the E(i) harness, as E(ii) already does.
  3. docs/design/architecture/06-runtime-view.md:78 overclaims "page at a time... never one listing of a whole namespace" — the committed-inode scan is still one meta.scan(b"inode:") (gc.rs:549) and restore does it twice. Narrow the "page at a time" clause to the upload records only.
  4. Leg C(ii)/G may test the wrong publication handoff shape — the double may be collapsing the root-flip batch and the retirement-drain batch into one, when 0016:793-800/941-965 and multipart.rs:3144-3150/3424-3432 describe them as two separate batches. Verify and, if the finding holds, revise C(ii) and G to schedule the flip and the drain-deletion of the `part:` record as separate batches.
  5. No red test proves the operator-facing verdict actually says "staged record" vs "committed object" — cli.rs:1318-1341 hard-codes committed-object wording and the existing verdict test only seeds inode: names. Add a red assertion using mpu:/part:/sidx: report names, pinning the exact text and exit verdict per brief.md leg I.

  Explicitly NOT blocking, cleared this round — do not re-raise unchanged:
  - Validation fitness-to-purpose tradeoff (one unreadable staged record stalls GC/restore fleet-wide) — already accepted at the 2026-09-16 re-plan sign-off; a tracked bug covers the operational follow-up.
  - Tracker-record vs brief mismatch — already flagged as the human's call at hand-off (Plan review 2026-09-16, response (1)); not a build-blocking item.
  - Size backstop (153 KB vs 100 KB) — already accepted as an oversize patch at the 2026-09-16 re-plan; do not re-split. Stay iterate-do, not iterate-plan.
- Full previous attempt preserved in `iteration-v6/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 7 — carry-forward (from the previous attempt)
- Sign-off rationale: Rebuild to close two real implementation gaps found by adversarial review, not the size/plan concerns: - Item 2 (StagedSet::place mutants M1/M2 undetected): add a healthy multi-fragment ReedSolomon{k:2,m:1} upload with a real, non-identity placement (e.g. [2,0,1]) to legs A and B, fully placed and marked past grace, asserting all fragments survive GC/are unmarked by restore. Add an extra untracked copy on an unplaced server and assert GC reclaims it / restore marks it. This catches both M1 (index-zeroing bug) and M2 (place() short-circuit bug). - Item 6 (fail-closed key-validation hole): add a test case for a malformed key (not just a malformed value) inside a session's `part:<id>:` range, since key validation (`parse_part_key`) and value validation (`decode_part_record`) are separate code paths on the target branch; assert the pass fails closed the same way E(i) requires for value-level corruption. Human explicitly overrode the size backstop's `iterate-plan` recommendation (patch 157-160KB vs 100KB threshold, 2/2 rounds already spent) — proceeding with iterate-do on the basis that items 2 and 6 are genuine implementation gaps, not slicing/plan problems. Items 3 (re-plan/tracker mismatch) and 4 (leg C(ii) handoff mechanics) are brief/plan-level concerns, not addressed by this iteration; item 5 (CLI verdict red check) appears already covered by the current patch (`restore_verdict_names_unreadable_staged_records_as_staged` in the diff) and looks stale. Item 1 (fitness-to-purpose) remains open for the next sign-off.
- Sign-off session carry-forward (captured live, before §9 flattened it):
  Rebuild to close two real implementation gaps found by adversarial review, not the size/plan concerns:
  - Item 2 (StagedSet::place mutants M1/M2 undetected): add a healthy multi-fragment ReedSolomon{k:2,m:1} upload with a real, non-identity placement (e.g. [2,0,1]) to legs A and B, fully placed and marked past grace, asserting all fragments survive GC/are unmarked by restore. Add an extra untracked copy on an unplaced server and assert GC reclaims it / restore marks it. This catches both M1 (index-zeroing bug) and M2 (place() short-circuit bug).
  - Item 6 (fail-closed key-validation hole): add a test case for a malformed key (not just a malformed value) inside a session's `part:<id>:` range, since key validation (`parse_part_key`) and value validation (`decode_part_record`) are separate code paths on the target branch; assert the pass fails closed the same way E(i) requires for value-level corruption.

  Human explicitly overrode the size backstop's `iterate-plan` recommendation (patch 157-160KB vs 100KB threshold, 2/2 rounds already spent) — proceeding with iterate-do on the basis that items 2 and 6 are genuine implementation gaps, not slicing/plan problems. Items 3 (re-plan/tracker mismatch) and 4 (leg C(ii) handoff mechanics) are brief/plan-level concerns, not addressed by this iteration; item 5 (CLI verdict red check) appears already covered by the current patch (`restore_verdict_names_unreadable_staged_records_as_staged` in the diff) and looks stale. Item 1 (fitness-to-purpose) remains open for the next sign-off.
- Full previous attempt preserved in `iteration-v7/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 8 — carry-forward (from the previous attempt)
- Sign-off rationale: Human accepts the size/round overage (166 KB vs 100 KB threshold, 3 rounds) and chose iterate-do over iterate-plan despite the bundle's own recommendation. Scope the next round to the three verified implementation gaps from the adversary review: 1. gc.rs:756 — empty staged placement not covered by the exact-length rule (relaxing the check to match the committed-side rule leaves all tests green); add an empty-placement leg beside a_part_with_a_wrong_length_placement_holds_its_chunk and its sidx: twin. 2. gc.rs:719 + staged_protection.rs:740 — the stray-copy leak-detection fixture only injects a stray copy for the committed part's chunk, never the owned (sidx:) chunk; add the owned-side twin stray. 3. gc.rs:820 vs cli.rs:1339-1341 / runbook — the code never decodes a session's value (key-only read, deliberate), but operator text promises "key or value" is checked; narrow the CLI/runbook strings to "key" and add a leg pinning that a session with an undecodable value still has its ranges walked and fragments protected. Remaining §6 items (fitness-to-purpose tradeoff sign-off, tracker/brief authorization mismatch, leg C(ii) testing the wrong publication handoff, missing red check for the operator-verdict text, and the key-validation hole in the fail-closed rule) are deliberately left open — human will address these after this iteration, not folded into this round's scope.
- Sign-off session carry-forward (captured live, before §9 flattened it):
  Human accepts the size/round overage (166 KB vs 100 KB threshold, 3 rounds) and chose iterate-do
  over iterate-plan despite the bundle's own recommendation. Scope the next round to the three
  verified implementation gaps from the adversary review:
  1. gc.rs:756 — empty staged placement not covered by the exact-length rule (relaxing the check
     to match the committed-side rule leaves all tests green); add an empty-placement leg beside
     a_part_with_a_wrong_length_placement_holds_its_chunk and its sidx: twin.
  2. gc.rs:719 + staged_protection.rs:740 — the stray-copy leak-detection fixture only injects a
     stray copy for the committed part's chunk, never the owned (sidx:) chunk; add the owned-side
     twin stray.
  3. gc.rs:820 vs cli.rs:1339-1341 / runbook — the code never decodes a session's value (key-only
     read, deliberate), but operator text promises "key or value" is checked; narrow the CLI/runbook
     strings to "key" and add a leg pinning that a session with an undecodable value still has its
     ranges walked and fragments protected.
  Remaining §6 items (fitness-to-purpose tradeoff sign-off, tracker/brief authorization mismatch,
  leg C(ii) testing the wrong publication handoff, missing red check for the operator-verdict text,
  and the key-validation hole in the fail-closed rule) are deliberately left open — human will
  address these after this iteration, not folded into this round's scope.
- Full previous attempt preserved in `iteration-v8/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).
