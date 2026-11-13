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
  fixture uses a position **no record the drain retains names**: a `{chunks}` obligation (or a
  flat `{generation}`), whose installing batch removed the naming record. It plants a
  `reclaiming` mark carrying a different event there, runs a step, and asserts the stored value
  byte-identical and the obligation still present with its cursor at or before that position;
  then runs **real GC passes** (the deployed pass, not a hand-made key delete), which destroy the
  fragment and delete the key themselves (`gc.rs:673-678`, `:804-825`); runs the next step; and
  asserts a fresh structured mark of this event (arm c), and the obligation removed once its last
  position is marked. Then the three arms (`0016:1220-1226`): (a) same event ⇒ skipped,
  byte-identical; (b) different event ⇒ re-stamped under `require(== prior)` with this event and
  a fresh stamp; (c) absent ⇒ written under `require_absent`.
  **(H) `reclaiming` over a record the drain retains: a stated hold, not a wait on GC.** A
  `reclaiming` mark on a position that a `part:` record under this obligation still names (the
  drain keeps that record until the position carries this event's mark). No in-tree path makes
  this state: GC swaps a mark to `reclaiming` only on a position nothing named when it looked
  (`metadata.rs:165-168`), and a staged position is named by its `sidx:` entry before its
  fragment is written (`crates/core/src/multipart/stage.rs:22-24`). It comes only from damage or a
  store edit, and GC's own comment allows for it (`gc.rs:660-662`), so the fixture plants it. In
  that state GC never moves: the staged protection outranks the `reclaiming` mark
  (`gc.rs:654-668`) and the fragment-less sweep keeps a staged-protected key (`gc.rs:887-893`).
  Alternating drain steps with real GC passes over it, every pass: the fragment, the mark, the
  `part:` record and the obligation stay byte-identical; the step reports a typed hold and an audit
  event naming the position and the record and saying it needs a human; no pass returns `Err`; and
  a second obligation later in the ledger still drains (the hold counts against child-1's visit
  bound like any held obligation). The exit is operator repair, and the docs say so.
  **(D) Every remaining row drains to completion.** Each from a production writer on the base:
  `{chunks}` from a part re-upload (`RetireObligation::part_chunks`,
  `crates/core/src/multipart/commit.rs:1039`); flat `{generation}` from a Complete over an existing
  flat object (`superseded`, `crates/core/src/multipart/complete.rs:1627-1633`); `{session, all}`
  from a client Abort (`abort_session`, `crates/core/src/multipart/abort.rs:286`) of a session with
  committed parts and an **empty** `sidx:` range. For each: every position the row names carries
  this obligation's mark, before any record naming it is deleted (a recording wrapper shows the
  order where the row has records); the obligation is removed; after passes past the orphan grace
  every one of its fragments is gone from the chunk store. The `{session, all}` case is the
  counterpart of (R): the same shape, with no residue, completes.
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
  A `{generation}` over a segmented map (a Complete over an existing segmented object,
  `complete.rs:1631`) marks each `seg:` record's **current** placement, read at drain time
  (`0016:2432-2440`); its obligation is then removed, and after passes past the orphan grace every
  one of its fragments is gone.
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
  event" and re-stamps it. (H) is red on a drain that deletes the retained record under a
  `reclaiming` mark (GC then resumes the reclaim with no grace from this retirement,
  `gc.rs:673-678`) or that returns `Err` and stops the leg. (D) is red on any build whose drain
  leaves a `{chunks}`, flat `{generation}` or empty-residue `{session, all}` obligation in place. Its race with GC is an interleaving the record-level test cannot build
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
  empty in bounded batches, except the holds this brief names, each typed and audited: an untrusted
  placement and owned residue (both until #1013), and a `reclaiming` mark over a record the drain
  retains (operator repair, (H)). **Sources:** the maintainer's rule 2026-07-25 (via principles.md §5
  C-1); 0016 decision 4 rule 2 (`0016:1123`) and the three arms (`:1220-1226`); the custodian's
  "never reclaim a referenced fragment" (`gc.rs:66-70`, the safety gate `gc.rs:442-445`).
  SELF-TEST: it binds the drain in custodian, the mark codec in core, GC's reclamation of the
  marks, and restore's re-check, so guarding one module cannot satisfy it.
- **Repo + branch target:** getwyrd/wyrd @ main
- **Depends on:** 1061, 1008, 1009
- **Conflicts with:**
- **Ordering note:** wave 2 of #1012's split. Builds on child-1 (#1061): its cursor, walk, dispatch leg
  and records mode, which (B) and (G) need for the other half of a Complete and of a rolled-back
  attempt. **External dependencies** (added after acceptance): `1008, 1009` again, so the
  edge survives a re-point. **Downstream:** #1013 and #1014 depend on this child (their `Depends on:
  1012` was re-pointed here on 2026-11-11). **Intake-cap override (wyrd-pdca-P1):** the human's
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
  (B), (T), (C′), (D) and (S) fail. If that is impossible, say why in `build-notes.md`: C4-verify then
  reports UNVERIFIABLE, a sign-off item.
  **Demonstrated reds (required, recorded in `build-notes.md`).** Do makes each temporary edit,
  shows the named leg fail, then restores the file:
  - marks stamped with the pass's cached time → (T) fails;
  - arm 0 removed (a `reclaiming` mark re-stamped as "different event") → (C′) fails, and the DST
    row fails on its GC-wins interleaving;
  - an untrusted `part:` record deleted without marking → (E-hold) fails;
  - the owned-residue hold removed → (R) fails;
  - segments-before-parts removed → (G) fails;
  - a `part:` record deleted before its fragments are marked → (B) fails;
  - the retained record deleted under a `reclaiming` mark (arm 0 treated as "marked") → (H) fails;
  - the drain skipping the `{chunks}` row (left byte-identical) → (D) fails.
  **Seeded DST row (required; the human's decision 2026-10-30).** AGENTS.md asks a new concurrent
  destructive path to land with seeded Tier-0 DST coverage (`AGENTS.md:205`; `0016:2888`). One row:
  the drain's exact-value replacement (arm b) and GC's swap to `reclaiming` race on one stamped
  mark, at a position no record the drain retains names (a `{chunks}` or flat `{generation}`
  obligation: on a position a retained `part:` record names, GC never swaps, `gc.rs:654-668`, so
  the race cannot occur there). Assert exactly one commit wins; when GC wins, the drain's re-read finds `reclaiming` and
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
    lands, except over a record the drain retains, where GC never deletes it and the hold needs a
    human; a session's obligation waits for its owned residue.
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
obligation. Where no record the drain retains names the position, GC destroys the fragment and
deletes the key (`gc.rs:673-678`, `:804-825`), and a later step takes arm (c). Where a record the
drain retains does name it, GC never moves (staged protection outranks the mark,
`gc.rs:654-668`; the sweep keeps the key, `gc.rs:887-893`): the step holds there for good, typed
and audited as needing a human (H). It does not delete the record on the strength of the
`reclaiming` mark: GC would then resume the reclaim with no second grace test (`gc.rs:673-678`),
which is grace this retirement never gave. **(a)** same event ⇒ skip.
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

Plan-review response (2026-11-11, both findings verified on `db1a066` and taken): (1) GC's staged
protection outranks a `reclaiming` mark and its sweep keeps the key (`gc.rs:654-668`, `:887-893`),
so (C′) now runs on a position no retained record names and against real GC, and the
retained-record case is a new leg (H): a typed, audited hold whose exit is operator repair; the
invariant's "every obligation to empty" names its holds. (2) New leg (D) drains `{chunks}`, flat
`{generation}` and an empty-residue `{session, all}` to completion from production writers, and
(S) now ends with the segmented generation's obligation removed and its fragments gone.

## Iteration 1 — carry-forward (from the previous attempt)
- Sign-off rationale: Human decision: fix all the bugs the Check review found, including the GC mark leak. 1. Empty page with a partial cursor (all three reviewers): in `parts_all` (marks.rs:589-594) and `segments` (:838-843), when the page is empty and the stored cursor is (entry, item > 0), hold with `RecordUnreadable` (keep the obligation byte-identical, typed outcome plus audit event) instead of `Tail::Ended`. Add a regression test for each walker ({session, all}/parts, and segmented generation). 2. Drain marks over a position with no fragment are never deleted (adversary finding 2): the human authorizes the GC change. GC's fragment-less sweep must remove a retirement-event mark once its `retire:bytes:` obligation is gone (one keyed read, as `retirement_draining` does), so every mark the drain writes has a deleter (0016:1408). Test: the (C') arm-0 flow ends with no orphan mark left after grace. Update 06-runtime-view.md to say who deletes the fresh mark. 3. Paging paths untested: add tests that abort a session with more than 16 parts, and that drain a segmented generation with more than 16 segments (this kills the surviving mutant at marks.rs:481). Move the (C') `reclaiming` mark to position 0 and assert `Reconciled::Partial` with no audit event (kills mutants at :379 and :484). 4. DST race row (crates/dst/tests/custodian.rs:6721, :6793): require a real race using `RaceEvent::Commit` outcomes (both stale-value attempts, exactly one `Committed` and one `Conflict`). In the GC-wins case, assert the drain's re-read sees `reclaiming` before GC deletes the key.
- Sign-off session carry-forward (captured live, before §9 flattened it):
  Human decision: fix all the bugs the Check review found, including the GC mark leak.
  1. Empty page with a partial cursor (all three reviewers): in `parts_all` (marks.rs:589-594) and `segments` (:838-843), when the page is empty and the stored cursor is (entry, item > 0), hold with `RecordUnreadable` (keep the obligation byte-identical, typed outcome plus audit event) instead of `Tail::Ended`. Add a regression test for each walker ({session, all}/parts, and segmented generation).
  2. Drain marks over a position with no fragment are never deleted (adversary finding 2): the human authorizes the GC change. GC's fragment-less sweep must remove a retirement-event mark once its `retire:bytes:` obligation is gone (one keyed read, as `retirement_draining` does), so every mark the drain writes has a deleter (0016:1408). Test: the (C') arm-0 flow ends with no orphan mark left after grace. Update 06-runtime-view.md to say who deletes the fresh mark.
  3. Paging paths untested: add tests that abort a session with more than 16 parts, and that drain a segmented generation with more than 16 segments (this kills the surviving mutant at marks.rs:481). Move the (C') `reclaiming` mark to position 0 and assert `Reconciled::Partial` with no audit event (kills mutants at :379 and :484).
  4. DST race row (crates/dst/tests/custodian.rs:6721, :6793): require a real race using `RaceEvent::Commit` outcomes (both stale-value attempts, exactly one `Committed` and one `Conflict`). In the GC-wins case, assert the drain's re-read sees `reclaiming` before GC deletes the key.
- Failing gate: C4 diff coverage: changed lines executed by the patch's tests (advisory) — diff coverage not measured — patch.diff does not apply on origin/main
- Failing gate: C5 surviving mutants on the bundle diff (cargo mutants --in-diff) (advisory) — 164 mutants tested in 44m: 51 missed, 85 caught, 26 unviable, 2 timeouts
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 3 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_1062/review-
- Full previous attempt preserved in `iteration-v1/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 2 — carry-forward (from the previous attempt)
- Sign-off rationale: Human decision: iterate-do on a narrow list; the size backstop (258 KB) is overridden. Do not re-split; keep the production code as is unless a test below exposes a bug. 1. Untrusted placements outside `part:` records are untested. Add a `{chunks}` (or flat `{generation}`) case for the hold at marks.rs:774-777, and a segmented `{generation}` case for the `seg:` hold at :857-861. Each asserts the typed hold, the audit event, a byte-identical obligation and record, and no marks. Both must fail if the `Err(_)` arm is turned into a silent skip. 2. `Hold::UnreadableMark` (marks.rs:259, :400-408) has no test. Plant a value that will not decode at a drained position. Assert the hold, the value byte-identical, the obligation byte-identical, and the audit naming the position. It must fail if that mark is re-stamped via arm (b). 3. Carry-forward item 3 (attempt 2) is not met: the surviving mutants at marks.rs:886 (`Tail::At(index + 1)` vs `Tail::Ended`). Kill them with a planted group of 17 or more one-chunk `seg:` records under a `{generation}` obligation, asserting every segment is marked and its record drained before the obligation is removed. 4. Arm-(0) "wait on GC" (marks.rs:396-399, :494) has no time limit or audit; a reclaiming mark whose D server left the fleet waits forever (gc.rs:904-905). Policy: PENDING human choice (see below). Not in scope: anything else in the adversary's notes; the other 26 mutants are accepted as equivalent.
- Sign-off session carry-forward (captured live, before §9 flattened it):
  Human decision: iterate-do on a narrow list; the size backstop (258 KB) is overridden. Do not re-split; keep the production code as is unless a test below exposes a bug.
  1. Untrusted placements outside `part:` records are untested. Add a `{chunks}` (or flat `{generation}`) case for the hold at marks.rs:774-777, and a segmented `{generation}` case for the `seg:` hold at :857-861. Each asserts the typed hold, the audit event, a byte-identical obligation and record, and no marks. Both must fail if the `Err(_)` arm is turned into a silent skip.
  2. `Hold::UnreadableMark` (marks.rs:259, :400-408) has no test. Plant a value that will not decode at a drained position. Assert the hold, the value byte-identical, the obligation byte-identical, and the audit naming the position. It must fail if that mark is re-stamped via arm (b).
  3. Carry-forward item 3 (attempt 2) is not met: the surviving mutants at marks.rs:886 (`Tail::At(index + 1)` vs `Tail::Ended`). Kill them with a planted group of 17 or more one-chunk `seg:` records under a `{generation}` obligation, asserting every segment is marked and its record drained before the obligation is removed.
  4. Arm-(0) "wait on GC" (marks.rs:396-399, :494) has no time limit or audit; a reclaiming mark whose D server left the fleet waits forever (gc.rs:904-905). Policy: PENDING human choice (see below).
  Not in scope: anything else in the adversary's notes; the other 26 mutants are accepted as equivalent.
- Failing gate: C4 diff coverage: changed lines executed by the patch's tests (advisory) — diff coverage not measured — patch.diff does not apply on origin/main
- Failing gate: C5 surviving mutants on the bundle diff (cargo mutants --in-diff) (advisory) — 167 mutants tested in 43m: 26 missed, 108 caught, 31 unviable, 2 timeouts
- Full previous attempt preserved in `iteration-v2/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).
