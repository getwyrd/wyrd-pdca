# Brief — issue 813 / staged-scrub-and-keep

> Child 1 of 2 of #663's split (637.3). Do reads ONLY this file, plus the peers and prior art
> that `Citations expected` names. Keep the `- **Label:** value` lines.
> **This is round 4, re-planned after `iterate-plan`.** Three builds are archived in
> `iteration-v1/` … `iteration-v3/`. v3 passed every gate except the batched review. Do starts
> from v3's patch and changes only what "Round-4 delta" below lists — this is not a rebuild.
> Two sets of citations: **base** = `origin/main` @ `4ab2b28` (verified 2026-09-21; v3 was cut
> from the same commit and its patch still applies to it); **v3 tree** = base with
> `results/issue_813/iteration-v3/patch.diff` applied. A citation is on base unless it says
> "v3". 0016 = `docs/design/proposals/draft/0016-multipart-commit-protocol.md`.

- **Slug:** staged-scrub-and-keep
- **Kind:** enhancement
- **Defect:** two custodian loops ignore a multipart upload's staged bytes. **Scrub** walks only
  the committed reference set (`crates/custodian/src/scrub.rs:88`, grouped `:130-135`, fetched
  `:137-203`), so a fragment named by a committed `part:` record is never fetched or checked. Rot
  or loss during a staging window that can last hours never becomes a repair obligation, though
  0016 says scrub must check it with the scheme the part record carries (`0016:824`).
  **Reconstruction** resolves an obligation only against committed inodes (`read_committed`,
  `crates/custodian/src/reconstruction.rs:468`). A staged chunk has no committed site, so `assess`
  returns `Drain` (`:613`), the chunk joins `drain_only` (`:218`) and its obligation is deleted
  (`:333-339`). The only record that the chunk is short a fragment is thrown away, and the pass
  can answer `Satisfied`.
  **What is still wrong in v3 (the reason for round 4):** scrub answers `Satisfied` for a staged
  chunk it never checked because the chunk's `part:` placement is malformed (v3
  `scrub.rs:303`; the test at v3 `staged_scrub.rs:1091-1096` asserts that wrong answer). Three
  behaviours v3 has are pinned by no test, one audit line loses the record it is about, and one
  doc sentence is false. All are listed under "Round-4 delta". The round-4 plan review found one
  more: this brief's own invariant wording, which three v3 comments copy, forbade base's normal
  committed discharge. v3's code already discharges correctly; the words were wrong, and leg J
  pins the discharge only indirectly (J-discharge).
- **Success criterion:** the NEW file `crates/custodian/tests/staged_scrub.rs` passes, the legs
  added to the existing `crates/custodian/tests/staged_protection.rs` pass, the seeded DST legs
  pass, and `cargo xtask ci` is green (it is the only command that compiles `crates/dst`, which
  is `--cfg madsim`). All run over in-memory doubles. Records are seeded as raw JSON the base
  decoders accept (shapes as `crates/core/tests/multipart_session_records.rs:81-141`), each
  round-tripped through `decode_session_record` / `decode_part_record` / `decode_owned_entry`
  first. Leg names are v3's, so each maps to a test that already exists.
  **Legs v3 already has — they must stay green and keep their meaning:**
  **(A) Scrub checks committed-part fragments.** An `Open` session has one committed `part:`
  record. One fragment carries a flipped bit (`corrupt_fragment`,
  `crates/custodian/tests/scrub.rs:157`). One `reconcile_step` with a `ScrubContext` answers
  `Changed` and leaves the chunk in `wyrd_core::repair::queued_repairs`
  (`crates/core/src/repair.rs:151`). The same for a missing fragment, and for an intact fragment
  whose header names a different EC scheme from the part record's `ChunkRef`. Control: all
  intact, nothing queued, `Satisfied`.
  **(A′) A committed map wins over a leftover part record.** A chunk that a committed map names
  is checked where the committed map places it, never where the part record does.
  **(B) Scrub leaves in-flight chunks alone.** A chunk named only by an owned `sidx:` entry with
  a fragment missing queues nothing (`0016:776-781`). Green on base by design; a guard, not a
  red leg.
  **(C) Scrub fails closed on what it cannot read.** One `part:` record that will not decode:
  every other fragment is still checked, the record is named on the audit seam, the pass answers
  `Blocked` (scrub's rule for an unreadable committed map, `scrub.rs:99-116`, `:205-215`). A
  store fault reading a `part:` range fails the pass with `Err`. **(C‴)** the same for a session
  key the parser rejects.
  **(C′) Scrub reads source before destination.** `part:` before `inode:`. A publication that
  lands between the two reads leaves the chunk checked (both hook timings v3 tests).
  **(F, rewritten) Scrub reads committed parts and no owned entry.**
  **(G) Reconstruction keeps a staged chunk's obligation.** A committed part's fragment is lost
  and its chunk enqueued. One `reconcile_step` with a `ReconstructionContext`: the obligation is
  still queued; the pass answers `Blocked`, as for a refused `seg:` repair
  (`reconstruction.rs:249-256`, `:341-358`); no D server received a write; the `part:` record is
  byte-identical. The same for an `sidx:`-only chunk. Control: an obligation no committed map and
  no staged record names still drains, `Satisfied`.
  **(H) Source before destination, for reconstruction.** Staged classes are read before the
  committed namespace, `sidx:` → `part:` → `inode:` (normative, `0016:782-800`; GC's order,
  `gc.rs:419` then `:434`). A store hook (`Meta::hook`, `staged_protection.rs:202`) publishes the
  chunk right after the pass's first `inode:` read. The obligation must not drain. The chunk is
  seeded with one fragment lost, so the leg stays green after #814.
  **(I) An unreadable staged record holds back every drain**, and is named on the audit seam
  even when the committed read then faults.
  **(J) After publication, scrub and reconstruction settle on the committed placement.**
  (v3 `staged_protection.rs:2663`; round 4 adds assertions to it, see J-discharge below.)
  **DST property 13** (`crates/dst/tests/custodian.rs`, v3): seeded part-commit / publication /
  retirement handoffs never drain the obligation, for the GC driver and the reconstruction driver.
  **Round-4 delta — the legs that are new or change:**
  **(C″, CHANGED) A staged chunk whose `part:` placement is malformed is never certified.** The
  placement is empty or the wrong length; the chunk is named by no committed map. The pass queues
  no repair and invents no placement (as v3), names the record on the audit seam, and now answers
  **`Blocked`**, not `Satisfied`: `Satisfied` claims every referenced fragment was checked
  (`crates/custodian/src/reconciliation.rs:32`), and this one was not. Flip the assertion at v3
  `staged_scrub.rs:1091-1096`. Add the counter-case: the same damaged `part:` record, but a
  committed map with a valid placement also names the chunk. That chunk IS checked (leg A′), so
  the damaged leftover record does not make the pass `Blocked`. Whenever a committed map names
  the chunk, the committed map's rule alone decides the answer, whatever the part record holds.
  **(C″-audit, NEW) The audit line names the damaged record.** For a malformed `part:`
  placement the audit seam carries the `part:` key and says it is a staged record. v3 keeps only
  the chunk id (v3 `gc.rs:1650`) and prints the committed-map wording (v3 `scrub.rs:398-407`), so
  an operator goes looking for an inode that does not exist. GC's staged reader already keeps
  the key for this kind of damage (`StagedSet::hold`, `gc.rs:1423`). Assert on the key.
  **(A′-malformed, NEW)** a chunk that a committed map names with a *malformed* placement, and a
  part record also names: scrub does not check the part placement. Deleting the
  `referenced.malformed` half of v3 `scrub.rs:220-221` must turn this leg red.
  **(G-held, NEW) Reconstruction keeps an obligation for a chunk a staged record holds.** An
  `Open` session's `part:` record places an RS(2,1) chunk on only `[0, 1]`: readable, wrong
  length, so the chunk is in `StagedSet::held`, not `placed`. Queue its obligation, run one
  pass: still queued, `Blocked`. Add the `sidx:` twin. Deleting v3 `reconstruction.rs:242` must
  turn this leg red; v3's adversary review found that all 46 staged tests pass without that line.
  **(Empty queue, NEW assertion) An empty queue reads nothing at all.** With nothing queued, a
  reconstruction pass reads no `mpu:`, `sidx:` or `part:` key as well as no `inode:` key, and
  answers `Satisfied` even over a store whose staged read would fault. Extend
  `an_empty_queue_reads_nothing_and_answers_satisfied`
  (`crates/custodian/tests/segmented_map_reconstruction.rs:697-717`); if that file's double cannot
  see reads by prefix, put the leg beside G in `staged_protection.rs`, whose `Meta` can. Calling
  the staged reader in the empty-queue branch (v3 `reconstruction.rs:214-215`) must turn it red.
  Green on base by design; a guard.
  **(J-discharge, NEW assertions in leg J) A committed chunk's obligation is discharged as on
  base, whatever a leftover part record names.** v3's J proves the repair's discharge only
  indirectly (round 2's scrub sees no `repair:` key) and never runs reconstruction over a queued
  obligation for a whole committed chunk. In the same test, with the leftover `part:` record
  still in place and byte-identical at every step: (i) right after round 1's reconstruction
  answers `Changed`, `wyrd_core::repair::queued_repairs` is empty — the repair discharged the
  obligation in its repoint commit although a staged record still names the chunk; (ii) after
  the settle rounds, enqueue the chunk again (`repair::enqueue_repair`,
  `crates/core/src/repair.rs:138`, a duplicate as a health report makes) and run one
  reconstruction pass: it answers `Satisfied`, not `Blocked`, the queue is empty, and no
  fragment was written. Green on base by design (base discharges committed obligations and reads
  no staged record); a guard against the fix reaching too far. Its proof is a mutation: dropping
  from `drain_only` every chunk in `staged_chunks` (v3 `reconstruction.rs:238`) — the literal
  reading of the old wording, "removed only when no record names it" — must turn (ii) red.
- **Falsifiability:** RED in-process on `origin/main` @ `4ab2b28`, no container. v3's C4-verify
  run is the proof for the new file: with production reverted, 9 of its 13 tests failed by
  assertion and 4 passed (`iteration-v3/gate-logs/C4-verify.log`; the 4 are B, the intact
  control and the two A′ tests, all green on base by design). C″ joins the red set in round 4:
  base scrub reads no `part:` record and answers `Satisfied`, and the flipped assertion demands
  `Blocked`. G, G-held, H and I go red by hand as `Verification posture` says: base reconstruction
  deletes the obligation. J-discharge is green on base by design; the mutation named in its
  entry is how it goes red.
- **Verification posture:** the new-file legs (A, A′, B, C, C′, C″, C‴ and the round-4 scrub
  legs) get their red→green from C4-verify, which earns a red only from an ADDED `*/tests/*.rs`
  (`engine/scripts/run-verify.sh:144`, `:391-392`). Legs in `staged_protection.rs`,
  `segmented_map_reconstruction.rs` and `crates/dst/tests/custodian.rs` are in MODIFIED files,
  which C4-verify reverts with the production change, so they are green-only there (C4-ci runs
  them). They cannot move to the new file: they build a `ReconstructionContext`, whose two new
  fields do not exist on the red leg's base, and one added test that does not compile makes the
  whole C4-verify run UNVERIFIABLE (`run-verify.sh:522-533`). So **the new file must not build
  a `ReconstructionContext`.** Do shows the red for G, G-held, H and I by hand, as v3 did: base
  production code, plus the patched `staged_protection.rs` with only the two field initialisers
  removed, each leg failing by assertion. Record the command and the pass/fail counts in
  `build-notes.md`. Also record, per round-4 leg, the one-line mutation that turns it red (the
  lines named above) and that it did.
- **Invariant to restore:** two ways an obligation leaves the queue, kept apart. **Discharge**
  (the obligation is resolved): a chunk a committed map names is assessed against that map
  alone, exactly as on base — a verified full-redundancy chunk drains (`reconstruction.rs:705-707`)
  and a successful repair deletes the obligation in its repoint commit (`:938-941`, pinned by
  `crates/custodian/tests/reconstruction.rs:384-388`), whatever a leftover `part:` record names.
  **Discard** (deleted with nothing resolved): allowed only when no record, committed or staged,
  names or holds the chunk. So while no committed map names a chunk, its staged records are its
  only reference: every fragment they place is present and intact or a durable repair
  obligation, and that obligation is never discarded. While any record, staged or committed,
  cannot be read, no drain batch commits at all (base's own rule for an unreadable committed
  object, `:324-339`, extended to the staged read; a repair still discharges its own
  obligation). A pass answers `Satisfied` only if it checked every fragment it was answerable
  for. "I could not read a record" and "I could not use a record's placement" never count as
  "no record names it" or as "checked". Sources: C-1, "a permanent or data-losing
  failure mode is never an acceptable cost" (the harness catalogue `wyrd-pdca/docs/principles.md`
  §5 and its §6 storage-lifecycle row, not a target file; its target sources are 0016's
  refutation standard, `0016:2802-2803`, and GC's "never reclaim a referenced fragment",
  `crates/custodian/src/gc.rs:69`); `0016:824-825`, the read order `:782-800`; scrub's own
  invariant (`scrub.rs:21-25`); the outcome contract (`reconciliation.rs:28-36`); proposal
  0005's repair contract (`docs/design/proposals/accepted/0005-milestone-3-custodians.md:269-286`);
  ADR-0040 decision 4 (strict maintenance: classify, skip, audit — it does not require a
  successful answer, `docs/design/adr/0040-mixed-era-placement-expansion.md:82`); ADR-0045.
  SELF-TEST: fixing scrub alone queues obligations that reconstruction then deletes (G goes
  red); fixing reconstruction alone leaves rot undetected (A goes red); over-fixing
  reconstruction — keeping every obligation a staged record names, committed or not — leaves a
  whole committed chunk's obligation queued, and the pass `Blocked`, for as long as a leftover
  part record lives (J-discharge goes red).
- **Repo + branch target:** getwyrd/wyrd @ main
- **Conflicts with:** 777
- **Ordering note:** first of the pair (#663's split, accepted 2026-09-19 under the human's
  intake-cap override, recorded in `results/issue_663/split-proposal.md`); #814 depends on this
  one. `Conflicts with: 777`: #777 edits `reconstruction.rs`. #809 and #810 name this id in their
  own `Conflicts with`; #508 and #625 depend on it. #800 and #808 have merged (PRs #821/#823 and
  #822), so the base already has `W_WRITE_MILLIS` and drain status already reads the staged
  class. **Re-plan decision, 2026-09-21 (Eduard Ralph):** v3's sign-off asked for a re-split;
  at re-plan the intake cap stood at 22/6 with no room (`scripts/plan-cap`), and a two-way cut
  would still leave the reconstruction child at an estimated 110 KB (sized from v3's per-file
  bytes, not from a build). The human chose to keep one slice and
  override the size backstop (100 KB) again. No cap override was used: this is a repair of a
  brief the flow returned to Plan. **Stop rule:** if round 4's Check finds a new correctness bug
  in production code (not a test gap, a doc line or a style point), the next sign-off is
  `iterate-plan` and the slice is split then.
- **Surfaces:** data
- **Difficulty:** high — it touches scrub, reconstruction's drain decision and the staged reader
  GC and restore share, and changes a struct that 9 files in 4 crates build.
- **Do model:** opus
- **Scope:** the slice as v3 built it, plus the round-4 delta. **Start from
  `results/issue_813/iteration-v3/patch.diff` applied to base, and change only what the delta
  needs. Do not restructure, rename or re-flow what passed review; every round so far grew the
  patch (123 KB → 196 KB), and it must not grow by more than the delta.** If the base Do is
  given has moved and the patch no longer applies cleanly, carry v3's hunks over by hand onto
  that base and say so in `build-notes.md`; that is still not a rebuild. The slice: (1) scrub
  fetches and checks every fragment a committed `part:` record places, for a chunk no committed
  map names (a committed map's placement wins, leg A′), with the scheme that record carries,
  inside the loop it already runs (`scrub.rs:137-203`), reads `part:` before `inode:`, and
  reads no `sidx:` entry. (2) Reconstruction reads the staged classes before the committed
  namespace and never discards an obligation for a chunk that no committed map names while a
  staged record names or holds it: it keeps it, keeps it off the repairable-backlog gauge
  (`reconstruction.rs:175-199`) and names it on the audit seam, like the `seg:` refusal
  (`emit_refused`, `:1107`). A chunk a committed map names is assessed, repaired and discharged
  exactly as on base, whatever a part record says (leg J-discharge; v3's `assess` already
  consults the staged set only when the committed reading has no site, v3
  `reconstruction.rs:734`). Staged faults are named before any later fallible read. An empty queue still reads nothing
  (`:165-169`). (3) The seam #814 reads: `ReconstructionContext` (`reconstruction.rs:72-95`)
  has `clock: &'a (dyn wyrd_testkit::Clock + Sync)` (ADR-0024,
  `docs/design/adr/0024-clock-and-time-source-trust.md`; `crates/testkit/src/lib.rs:23`) and
  `staged_write_window_millis: u64`. Use these names; #814's brief names them. The deployed loop
  (`crates/server/src/custodian.rs:488`, context at `:511`) feeds the context's clock and the
  pass's `now_millis` from one source, and passes `wyrd_custodian::gc::W_WRITE_MILLIS`
  (`gc.rs:201`) as the window — never a second definition. `wyrd-testkit` is a normal dependency
  where production code names `Clock` (as `crates/chunkstore-fs/Cargo.toml:17-19`). Every
  construction site is updated (9 files). Nothing in this slice reads either field. (4) **The
  round-4 delta:** the behaviours of legs C″, C″-audit, A′-malformed, G-held, the empty-queue
  assertion and J-discharge. (5) Keep the prose true: the three v3 comments that state the old
  wording, "removed / discarded only when no record, committed or staged, names its chunk" (v3
  `reconstruction.rs:237`, `:293`; v3 `crates/dst/tests/custodian.rs:2597`), reworded to the
  discharge / discard split in `Invariant to restore`, with no change to the code around them;
  the module-doc sentence at `gc.rs:89-90` ("Scrub and the
  drain-status surface do not read the class at all" — false for drain status since #808, and
  for scrub and reconstruction after this slice); `gc.rs:396`; `staged_protection.rs:34` and the
  `deferred: #663` markers at `staged_protection.rs:2159` and `gc.rs:1316`, narrowed to the
  rebuild that #814 owns; the last sentence of `06-runtime-view.md:80` ("Scrub reads committed
  references only."). (6) **State the re-queue bound, do not fix it.** One case remains after
  v3: an object is deleted or overwritten before its `part:` records are retired, after
  reconstruction had moved one of its fragments. The leftover part record then names the empty
  old position; scrub re-queues the chunk each pass and reconstruction keeps it and answers
  `Blocked` each pass, until the `retire:records:` drain deletes the record. Say so, and name
  that drain as the bound, in scrub's module doc and in `06-runtime-view.md:80`. No production
  code on main writes a `part:` record yet (`part_key` has no caller outside
  `crates/core/src/multipart.rs`), so this is forward-looking. What GC and restore conclude must
  not change: `staged_protection.rs` legs A–E stay green unedited.
  / out of scope: rebuilding or re-placing anything (#814); closing the re-queue case in (6);
  the rule for a malformed COMMITTED placement, which keeps answering as it does today
  (`scrub.rs:95-96`, pinned by `short_placement_is_malformed_scrub_fails_safe`,
  `crates/custodian/tests/scrub.rs:788`) — the asymmetry with C″ is a known, human-accepted
  decision for this slice; servers absent from the live fleet (scrub has only ever visited
  `ctx.fleet`, `scrub.rs:138`, and the deployed loop drops unreachable peers and reads around
  them, `crates/server/src/custodian.rs:249-265` — the same for committed chunks today); a
  scrub driver in the DST sweep (decided at v1's sign-off: scripted hook tests for scrub, seeded
  coverage for reconstruction); `crates/core/src/multipart.rs`; edits to 0016 or an ADR.
- **Review findings Do must close:** the batched review blocks until each finding is fixed or
  recorded in `$PDCA_BUNDLE/review-rejected.md` as `<file:line> | <CLASS> | <MATCH> | <reason>`,
  MATCH being a phrase from the finding's text. (a) "A committed-part fragment placed on a D
  server absent from `ctx.fleet` is silently skipped" (v3 `scrub.rs:233`, BUG): **record it
  rejected** — out of scope by this brief, the human confirmed it at v3's sign-off and again on
  2026-09-21, and committed chunks behave the same today. Use the line number the file has after
  round 4. (b) "A malformed committed-part placement … does not make the pass `Blocked`" (v3
  `scrub.rs:300`): **fix it**, leg C″. (c) The code-review note that scrub copies every fragment
  into a `fragments` map only to regroup it into `by_dserver` (v3 `scrub.rs:200`, low priority):
  fix it if the change is small and leaves legs A′ green, otherwise record it rejected with the
  reason.
- **Repro instruction:** on `origin/main`, seed an `Open` session with one committed `part:`
  record, flip one bit in one of its fragments and run a scrub pass: nothing is queued. Enqueue
  the chunk by hand and run a reconstruction pass: the obligation is gone. For the round-4 bug,
  on the v3 tree: seed a `part:` record whose chunk has `placement: []` and run a scrub pass:
  it answers `Satisfied`.
- **External dependencies:** `typos`, `docs-renderer`
- **Test file:** `crates/custodian/tests/staged_scrub.rs` — **NEW** (it is new relative to base;
  v3's copy is the starting point). `run-verify.sh --classify` on v3's patch returns
  `ADDED_TEST crates/custodian/tests/staged_scrub.rs` and crates `chunkstore-grpc`, `custodian`,
  `dst`, `server` (run 2026-09-21). Reconstruction legs go in the existing
  `crates/custodian/tests/staged_protection.rs`, which has the staged fixtures and the store hook.
- **Production reach:** every leg runs through the production `reconcile_step`. The two new
  context fields are a seam ahead of their reader: (a) the deployed loop fills them, nothing
  reads them; (b) #814, the next wave of the same run, reads them; (c) they sit here so #814's
  new test compiles on its base and C4-verify can prove #814's red.
- **Citations expected:** `path:line` on the target branch for every change. Prior art Do MAY
  read: `results/issue_813/iteration-v3/patch.diff` (the starting point) and
  `results/issue_813/iteration-v3/build-notes.md` (how v3 ran the by-hand red and the gates).
  Peers Do MAY open: `scrub.rs:137-203` (the loop v3 extended); `gc.rs:419-434` (read order),
  `:1320-1457` (`StagedSet`, `place` `:1402`, `hold` `:1423`, `staged_fragments` `:1457`,
  `walk_staged_range` `:1493`) — mirror `hold` for the C″-audit key; `reconstruction.rs:249-256`,
  `:322-358`, `:1107` (the `seg:` refusal and the incomplete-reading rule); `staged_protection.rs:147-215`
  (the `Meta` double and hook).
- **Prior-art check (triage cycles):** by path, 2026-09-21. Merged history: on `origin/main`
  neither `scrub.rs` nor `reconstruction.rs` reads `part:` or `sidx:` (no `staged_fragments`
  call, no prefix constant; `git log -S staged_fragments` over both files is empty). Open PRs:
  none touches `scrub.rs`, `reconstruction.rs`, `gc.rs`, `staged_scrub.rs` or
  `staged_protection.rs`. Closed-unmerged PRs touching `scrub.rs` / `reconstruction.rs`: only
  #647 (segmented chunk maps), unrelated. Rejected prior art: #663 v1–v3 and #637 v1
  (whole-slice builds), and this bundle's own v1–v3.
- **Disposition hint:** likely-fix

## Sign-off items declared ahead (so they are not a surprise at Check)

- **Fitness to purpose:** this slice detects and keeps; it does not rebuild. Rebuilding a staged
  chunk is #814, the next wave. The human accepted that interim state when #663 was split.
- **Size:** the patch will be over the 100 KB backstop again (v3: 196 KB, about 60% tests the
  earlier sign-offs asked for). The human has decided to override it for this round; the stop
  rule is in `Ordering note`.
- **Prior art:** the reviewer's disposable target has one synthetic commit and no remote, so it
  cannot re-run the search. The result above was run against the real checkout and GitHub.

## Plan-review response (#301 revision pass, 2026-09-21 — `plan-advisory-plan-reviewer.md`)

Plan-review response: finding 1 accepted, brief revised in place. The reviewer is right, and
every citation holds on `4ab2b28`: base drains a full-redundancy committed chunk
(`reconstruction.rs:705-707`) and deletes the obligation with a successful repoint (`:938-941`),
and `crates/custodian/tests/reconstruction.rs:384-388` requires it; the old invariant forbade
both. `Invariant to restore` now separates **discharge** (committed map decides, as on base,
whatever a part record names) from **discard** (only when no record names or holds the chunk).
Scope (1) and (2) say the same; leg J gains J-discharge, (i) the queue is empty after the
committed repair and (ii) a duplicate obligation for the whole committed chunk drains and the
pass answers `Satisfied`, both with the leftover part record in place; Scope (5) rewords the
three v3 comments that copied the old wording. No production behaviour changes: v3's `assess`
already consults the staged set only when there is no committed site (v3
`reconstruction.rs:734`), and v3's J already shows the discharge indirectly. No scope, size
or ordering decision changed, so nothing new needs the human beyond clearing this item.

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a
draft PR MAY happen during the cycle (useful for CI feedback). The PR MUST NOT be
marked ready before sign-off accepts.

## Iteration 4 — carry-forward (from the previous attempt)
- Sign-off rationale: Round 5 scope: close the two test-gap findings from the round-4 adversary review only. (1) Add a scrub test for staged-record paging past the first page of upload sessions (STAGED_PAGE = 512; gc.rs:1600-1618's session loop is not shared with GC's paging, unlike walk_staged_range). (2) Add a scrub test pinning that staged damage is named on the audit seam before a later committed-read fault (scrub.rs:147-163's ordering promise; reconstruction already has this test, scrub does not). Both are test-only gaps — no production code changed. Everything else in this bundle stands: the size backstop (213 KB vs 100 KB) is overridden again by the human, reaffirming the 2026-09-21 decision; do not re-split. Fitness-to-purpose and the T4 batch-review staleness question were left open by the human as not needing a decision at this point — do not treat their absence as a rejection.
- Sign-off session carry-forward (captured live, before §9 flattened it):
  Round 5 scope: close the two test-gap findings from the round-4 adversary review only.
  (1) Add a scrub test for staged-record paging past the first page of upload sessions
  (STAGED_PAGE = 512; gc.rs:1600-1618's session loop is not shared with GC's paging, unlike
  walk_staged_range). (2) Add a scrub test pinning that staged damage is named on the audit
  seam before a later committed-read fault (scrub.rs:147-163's ordering promise; reconstruction
  already has this test, scrub does not). Both are test-only gaps — no production code changed.
  Everything else in this bundle stands: the size backstop (213 KB vs 100 KB) is overridden again
  by the human, reaffirming the 2026-09-21 decision; do not re-split. Fitness-to-purpose and the
  T4 batch-review staleness question were left open by the human as not needing a decision at this
  point — do not treat their absence as a rejection.
- Full previous attempt preserved in `iteration-v4/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).
