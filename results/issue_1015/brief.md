# custodian: reap abandoned Open multipart sessions — idle arm, residency ceiling, profile check, operator one-shot (625.1)

> First of three slices #625 (multipart reaper) was split into on 2026-10-30, before any build.
> Do reads ONLY this file. **The design is normative and settled:** proposal 0016,
> `docs/design/proposals/draft/0016-multipart-commit-protocol.md` as it stands on `origin/main`
> (content tip `c35d39d`). Read it in the checkout, never at a commit. **Decision 6
> (`0016:1794-2277`) is this slice's design, so read it in full before writing code**, including
> its failure-mode table (`0016:2252-2276`) and its pass pseudocode (`0016:2163-2228`).
> Implement the parts of that pass this slice owns, in that order, with those preconditions.
> Citations were verified against `4bac538` / `12dd082` during #625's Plan (2026-10-15 to
> 2026-10-22). Re-verify line numbers on your base.
>
> **The teardown is #659's, not this slice's.** #659 lands the retirement drain, the three-arm
> `orphan:` mark guard, the owned-`sidx:` walk, the terminal session delete with its
> exactly-once `mpuctl` decrement, and the drain's dispatch from `reconcile_step`. This slice
> **fences**, and #659's drain tears down what was fenced. Do MUST NOT re-implement or copy any
> of that. A defect found in it is a Check §6 item against #659's layer.
> **Siblings:** 625.2 (child-2) wires the pass into every deployed startup shape. 625.3
> (child-3) adds the `Completing`/`Completed` exits, the tombstone window, `G_orphan` and the
> `B_ops` calibration. Leave the hooks this brief names for them, and do not implement them.

- **Slug:** multipart-reaper-open-sessions
- **Track:** alpha
- **Kind:** enhancement (design proposal)
- **Defect:** An `Open` multipart session whose client went silent (never aborted, never
  completed) is never fenced. Its staged parts stay protected on purpose (#637, 0016 decision
  2), its admission slot is never released, and `MAX_SESSIONS` such sessions make a permanent
  `503 SlowDown`. 0016 makes the reaper a hard ordering requirement (`0016:234-268`). Nothing in
  the custodian evaluates an `Open` session today, and no operator command runs such a pass.
- **Success criterion:** a NEW test file `crates/server/tests/multipart_reaper.rs` drives the
  reaper through the **operator one-shot** (see `Design § the C4 shape`) as a subprocess,
  against a redb store seeded with raw records. Every leg asserts on durable store state.
  **(A) Idle abandonment is reclaimed, with every byte evidenced.** An `Open` session whose
  latest progress instant is older than `W_open`, holding one committed `part:`/`psum:` pair
  and crashed in-flight `sidx:` residue with a lapsed lease, is fenced to `Aborting` and
  drained. **Only the idle arm may be able to reap the fixture:** `W_open < now -
  created_at_millis < W_session`; every progress stamp (`psum:` `committed_at_millis`, `slot:`
  `reserved_at_millis`) is older than `W_open`; every `sidx:` and `slot:` lease has already
  lapsed. Concretely, against the wall clock the test reads at seeding: `created_at_millis =
  now − 14 days`, progress stamps at `now − 10 days`, lease expiries at `now − 9 days`. The
  defaults are bounded (`Scope`) to leave at least three days of margin on each side. 0016 makes
  idle abandonment and the residency ceiling independent predicates (`0016:1910-1915`), and a
  fixture past `W_session` cannot tell a reaper that has no idle arm from a correct one. After
  repeated one-shots converge (bounded, at most N runs, where N is stated in the test):
  `mpu:<id>`, `part:<id>:`, `psum:<id>:`, `sidx:<id>:` and `slot:<id>:` are **all empty**;
  `mpuctl.count` is back to its pre-session value; and an `orphan:` mark exists for **every
  fragment position of both byte sets**, meaning the `sidx:` residue's recorded `staged`
  placement **and** every placement position of the committed `part:` record's chunks (the
  `retire:bytes:{session}` obligation the reap fence installs, `0016:664`). Assert the committed
  part's positions explicitly. Both session-scoped `retire:` ranges are empty
  (`retire_session_range(RetireMode::Bytes, id)` and `(RetireMode::Records, id)`,
  `crates/core/src/multipart.rs:1490`). GC's licence condition holds for every marked
  position: the mark decodes (`decode_orphan_mark`, `crates/core/src/metadata.rs:277`) as
  stamped, not `reclaiming`; its `retire_token()` (`:212`) names no `retire:bytes:` key that is
  present; and no `part:` or `sidx:` record names the position.
  **GC really reclaims them** (the tracker's own acceptance; `cargo xtask ci` evidence after the
  change, not C4's red→green): append a case to the existing
  `crates/custodian/tests/gc.rs` (in-memory metadata and chunk stores, `reconcile_step`
  in-process). Converge the reaper over leg A's fixture and put the fragments of both byte sets
  in the chunk store. Run one GC pass at `now = mark + grace − 1` and assert nothing is deleted,
  then run one at `now = mark + grace` and assert every fragment is deleted. **This case has no
  same-test red.** It calls `reconcile_step` with the reaper context, which does not compile
  against the base's signature (`crates/custodian/src/reconciliation.rs:126-134`). **The binding
  red→green is this file's subprocess test, over the durable marks of leg A.** The physical half's
  red is negation (15) under `Verification posture`. Never report it as a C4 red→green.
  **(B) The IDLE arm never reaps a progressing upload, for as long as the ceiling allows.**
  Three shapes, each of which MUST survive: (i) old parts but an **unexpired owned `sidx:`
  lease** (one `UploadPart` streaming for longer than `W_open`); (ii) a **reserved `slot:` with
  no chunk written yet** and a renewed slot lease (the pre-first-chunk window,
  `0016:1822-1835`); (iii) the newest `psum:` `committed_at_millis` inside `W_open`. Assert the
  records are untouched **and** the admission count is unchanged. Every fixture's
  `created_at_millis` MUST be **strictly inside `W_session`** (0016 bounds even a live session
  by the ceiling, `0016:1886-1888`, `:1908-1913`, `:2256`).
  **(C) The residency ceiling still bites.** A session with *fresh* progress but
  `created_at_millis` older than `W_session` is fenced to `Aborting` and drained (arm (ii),
  `0016:1908-1913`). The ceiling carries no slot preconditions.
  **(E-open) The clock guard suppresses judgments, not the pass.** A session whose
  `clock_source` the reaper does not own is never judged by `W_open` or by `W_session`: an
  over-age foreign-clocked `Open` session survives intact (one fixture per window). **The
  clock-free work still runs:** a foreign-clocked **`Aborting`** session has its `sidx:` range
  walked, its `retire:` obligation drained, and its terminal delete fired, all by #659's drain,
  which the reaper's guard MUST NOT suppress (`0016:1889-1907`, X71). The `Completing` and
  `Completed` halves of this leg are child-3's.
  **(H) A profile mismatch stops the reaper before it acts.** Seed `mpuctl` with a stored
  profile (`AdmissionRecord::profile`, `crates/core/src/multipart.rs:1858`) that differs from
  the custodian's local `Budget` (`:1567`). Assert that the pass **emits the operator signal and
  mutates nothing**: no fence, no mark, no deletion, no counter change. Meanwhile leg A's
  matching-profile fixture still reaps. The reason: the reap fence is an unsplittable batch
  sized by the admitting profile (`MAX_INFLIGHT_PARTS` slot preconditions,
  `MAX_PART_CHUNKS` chunk refs per part). **What profile equality does NOT prove: equal
  operation budgets.** The stored profile has five fields (`multipart.rs:1580-1586`).
  `B_ops` is `KnobSet::batch_ops` (`:5307-5308`), and `KnobSet::profile` leaves it out
  (`:5346-5355`). This slice MUST NOT add it (no format change). Do cites in `build-notes.md`
  the three things operation capacity rests on instead: (1) every reaper batch is sized from
  `MAX_BATCH_OPS` / `MAX_BATCH_BYTES` (`:5106`, `:5067`), and no other budget is defined here
  (`:5103-5105`); (2) `knob_clamps_hold` (`:5454-5457`) and
  `crates/core/tests/multipart_knobs.rs:167` stay green and unmodified; (3) the calibration,
  which is child-3's. **This leg does not cover 0016's staged-set memory guard**
  (`0016:2062-2070`). That is **#806**'s, at `gc.rs:1541-1546`. One comparison, not two: if
  #806 has landed, the reaper calls its comparison. If it has not, write the comparison as one
  `wyrd-custodian` function the reaper calls, with a doc comment naming #806 as the caller to
  add at `gc.rs:1541`.
- **Falsifiability:** RED is an in-process assertion failure with no deploy stack, fleet or
  container, **but only on a base that carries every prerequisite** (see `Depends on`). On that
  base the one-shot flag is unknown: `ParsedArgs` demands a value for any flag not in
  `VALUELESS_FLAGS` (`crates/server/src/cli.rs:2244`, parser `:2253-2275`), so the command
  errors at parse and reaps nothing. Legs A, C and E-open (Aborting half) then fail on their
  durable-state assertions. H fails on its signal assertion. B is **vacuously green** on the
  base (nothing reaps), so say so and evidence it through the negations under `Verification
  posture`. Do MUST record in `build-notes.md` the base commit it built on, confirm the base
  contains each prerequisite's code (in particular #659's drain is dispatched from
  `reconcile_step`), and record **how many tests ran and failed** on the RED leg. A test that
  does not compile on the base runs zero tests and is reported UNVERIFIABLE (exit 77,
  `engine/scripts/run-verify.sh:40-56`, `:207-229`, `:521-541`). **The added test file may
  reference only symbols that exist on the resolved base.** That is why it drives a
  stringly-typed operator command.
- **Invariant to restore:** no state of an upload session is absorbing, and a session's
  residency is bounded (0016 invariants (2) and (3), `0016:236-262`; the custodian's reclaim
  mandate, `crates/custodian/src/gc.rs:14-25`). This slice restores it for `Open`. Catalogue
  invariant **C-1** applies (a permanent or data-losing failure mode is never an acceptable
  cost; `docs/principles.md` §6 storage-lifecycle; 0016 `:2802-2813`). It is a structural change
  under §1.2: the smallest change that restores the invariant, not the smallest diff.
- **Repo + branch target:** getwyrd/wyrd @ main
- **Depends on:** 1027, 1007, 1008, 1009, 1010, 1013
- **Conflicts with:**
- **Ordering note:** **#927 was split on 2026-11-01** into #1027, #1028, #1029 and #1030, and a split parent counts as met at once, so this field now names #1027. That slice lands the part commit and the composed `UploadPart` this bundle needs; the rest of the split only handles lost and unknown commits. Read #927 in this brief as #1027. wave 1 of #625's split. **The parent's prerequisites are all resolved, and nothing is left to
  add to `Depends on`.** Merged to `main`: #813, #814, #808, #810, #693, #894, #895, #932, #933.
  The split parents map to the field: #658 to 1007, 1008, 1009, 1010; #659 to 1013 and, through
  it, 1012; #927 to 1027. #1007 is COMPLETE with no published branch, so it must be published
  and carried or merged first (#1008's note says how). #625's history of re-points (657 → 926/927 → 932/933, 664 →
  808/810, 656 → 894/895, 663 → 813/814) is in the archived parent brief's `Ordering note`. The
  release order is normative (`0016:234-268`): every #625 child, and #633, merges with or before
  #508.
- **Surfaces:** data
- **Difficulty:** high
- **External dependencies:** `typos`, `docs-renderer`
- **Test file:** `crates/server/tests/multipart_reaper.rs`, a **NEW** file (C4-verify
  discriminates on an added `crates/<c>/tests/<t>.rs`, `engine/scripts/run-verify.sh:144`,
  `:392`). It MUST NOT be `#![cfg(...)]`-gated (`:155-182`, `:463`). The DST rows go into the
  EXISTING `crates/dst/tests/custodian.rs`. Do MUST NOT add a file under `crates/dst/tests/`.
- **Verification posture:** red→green at Check for A, C, E-open (Aborting half) and H. B is
  vacuous on the base. Do runs these **negations** and records each one in `build-notes.md`,
  naming the broken variant and the leg that catches it: (1) derive progress from `created_at` +
  `psum:` only, ignoring `slot:` ⇒ B(ii); (6) skip a foreign-clocked session wholesale ⇒
  E-open's `Aborting` half; (10) disable the idle arm and keep only the ceiling ⇒ A; (14)
  compare no profile ⇒ H; (15) dispatch no reaper from `reconcile_step` ⇒ the GC case in
  `gc.rs`, which then finds no mark and deletes nothing at `mark + grace`. A negation that does NOT fail its leg means the leg is inert, so say
  so. **Interleaving negations (2) and (3) cannot be built at record level.** They belong to the
  DST rows, and Do must not claim the integration test catches them: (2) read `psum:` before
  `slot:` ⇒ X60, the commit landing between the reaper's two progress reads (`0016:1808-1821`);
  (3) omit the fence's slot preconditions ⇒ X62, the renewal committing after the expired read
  (`0016:1841-1884`). **Seeded Tier-0 DST, appended to the existing
  `crates/dst/tests/custodian.rs`** (`AGENTS.md`: a new destructive or concurrent path lands
  with seeded DST; `0016:2878-2905`): X60; X62; the **stale-snapshot mid-pass creation (X19)**;
  and the **pre-first-chunk liveness window**, both halves (live and renewing MUST survive,
  crashed MUST become reapable). These are `cargo xtask ci` evidence, not C4.
- **Production reach:** the one-shot and the deployed loop dispatch the **same** pass through
  the **same** fenced control point, `reconcile_step`
  (`crates/custodian/src/reconciliation.rs:65-115`; the anti-#141 rule at `gc.rs:124-128`).
  In this slice the deployed fleet loop's existing `reconcile_step` call passes the reaper
  context, as the signature widening forces it to. A healthy fleet therefore reaps every
  interval. Do records that call path in `build-notes.md`. **Not in this slice: the
  no-endpoints role, a degraded fleet behind GC's whole-fleet gate, and the empty-fleet
  startup.** Those are child-2's. Do MUST NOT add a test-only reaper entry point.
- **Scope:** decision 6's `Open`-session half as a custodian loop dispatched from
  `reconcile_step`:
  - the liveness observable `max(created_at, max psum committed_at, max slot reserved_at)`,
    plus "live while any owned `sidx:` **or** `slot:` lease is unexpired", read
    **source-first** (`slot:` then `psum:`, normative);
  - the idle arm at `W_open`, fencing `Open@E → Aborting@E+1` with the fence's slot
    preconditions pinned, **reusing #895's one fence batch builder** and not copying it;
  - the administrative `W_session` ceiling, without slot preconditions;
  - the `clock_source` guard, which suppresses judgments only and never #659's clock-free
    drain;
  - the profile check, which fails closed with an operator signal before any mutation (H);
  - the pass's byte and operation budget (`MAX_BATCH_OPS` / `MAX_BATCH_BYTES`, never
    re-derived). **This slice adds no batch splitter.** Each fence it sends is one unsplittable
    batch (`0016:647-649`) from #895's builder (`crates/core/src/multipart/abort.rs:77-92`) plus
    the idle arm's slot pins. The splittable work is the drain's (`0016:668`, `:672`), and its
    batching and tests stay with #1012 and #1013. A unit test shows the largest fence this slice
    can build (`MAX_INFLIGHT_PARTS` slot pins, at the profile (H) checks) fits both budgets;
  - the pass's outcome counts on the existing `DurabilityTelemetry` seam (no alerting surface:
    that is #630).

  `reconcile_step` gains the reaper (a `reaper: Option<&ReaperContext<'_>>` parameter, or a
  field on an existing context). Keep the ripple through every caller mechanical, including
  `crates/custodian/tests/gc.rs`'s calls. **Give `ReaperContext` and a public `ReaperPolicy` a
  stable shape** (#633 calls them, and so do child-2 and child-3).

  - The server builds the policy from `ReaperPolicy::default()`, with **no** window flags on
    the CLI. Child-3 then adds its windows inside `crates/custodian` without touching a server
    file.
  - **Route #659's tombstone input through `ReaperPolicy`.** It still says "retain" in this
    slice, and child-3 flips it there.

  Knob values owned here: `W_open` and `W_session`, as days-scale defaults **bounded `W_open ≤
  7 days` and `W_session ≥ 21 days`**, each cited at its definition; and the pass cadence.

  **The operator one-shot:** a valueless flag on `wyrd custodian` that joins `VALUELESS_FLAGS`.
  - It runs **exactly one** `reconcile_step` pass covering the reaper and #659's record-only
    drain (not GC, scrub or reconstruction, which need a fleet), then exits.
  - It does **not** require `--endpoints`.
  - It follows `--reconcile-after-restore`'s shape (`crates/server/src/cli.rs:1166-1230`)
    without the whole-fleet requirement.
  - Use one spelling in the test, the runbook and the docs (Open question 2).

  **Docs** (a merge requirement, `AGENTS.md` "Docs currency"): add the reaper loop and the new
  flag to `docs/design/architecture/06-runtime-view.md` and `08-crosscutting-concepts.md`.
  Child-2 later edits only `06`, and child-3 only `08`, so write each paragraph so that a later
  sentence can be appended.

  **Out of scope:**
  - `Completing` rollback and the over-age edge, tombstone expiry, the foreign-clock
    `Completing`/`Completed` cases, `G_orphan` and the calibration (all child-3);
  - the no-endpoints role, the degraded-fleet driver and the empty-fleet pass (child-2);
  - the drain, the mark guard, the owned walk and the terminal delete (#659);
  - the staged-set preflight (#806);
  - the operator verbs and the foreign-clock alarm (#633);
  - telemetry alerting (#630);
  - FU-2 and FU-4;
  - any file under `docs/design/adr/` or `docs/design/specs/`, and any edit to 0016;
  - any change to the records or state machine of #693, #894, #895, #932, #933, #927, #658 or
    #659 (raise it as a §6 item).
- **Design § the C4 shape (normative).**
  - The production change widens `reconcile_step`, so a test calling it directly compiles
    against only one of the two trees. The binding test therefore runs the **real binary**
    (`Command::new(env!("CARGO_BIN_EXE_wyrd"))`) with the flag **last** in argv, as a
    **subprocess**. It is stringly typed, so it compiles identically on both trees, and the
    difference shows up in the store.
  - Use a plain `#[test]` that seeds and asserts inside its own short-lived runtimes, never
    `#[tokio::test]`: the role `block_on`s its own runtime (`cli.rs:981-982`).
  - **Drop the store before spawning** (redb's exclusive lock), and reopen it to assert.
  - Wait for the subprocess with a **bound**. The one-shot MUST return after exactly one pass.
  - Annotate every wall-clock read as `clippy.toml` demands (wyrd#619), stating which clock
    owns the read.
  - Assert on store state, not on the exit status alone.
- **Repro instruction:** on the resolved base:
  1. Create a temp dir and open `RedbMetadataStore::open(dir.join("meta.redb"))`.
  2. Seed leg A's session as canonical raw JSON, or through the public helpers on the base
     (`mpu_key`, `retire_session_range`, `decode_*` in `crates/core/src/multipart.rs`, and the
     prerequisites' `pub` writers), using leg A's idle-only timestamps, plus `mpuctl` at
     `{count: 1, …}`.
  3. Drop the store and run the one-shot as a subprocess.
  4. Reopen the store and scan the prefixes. On the base, the records are exactly as seeded.
- **Citations expected:** `path:line` on the target branch for every change. Peer callsites
  to mirror:
  - `reconcile_step` (`crates/custodian/src/reconciliation.rs:65-115`) and its
    fenced-control-point rule (`gc.rs:124-128`);
  - `MARK_BATCH` and its FDB rationale (`crates/custodian/src/restore.rs:92-100`, `:278-286`),
    which 0016 tightens to bytes **and** ops (`0016:621-652`);
  - the cross-clock hazard (`ExpiredPendingPolicy`, `gc.rs:77-104`; #557; `NOW_MILLIS` in
    `cli.rs`);
  - the renewal loop whose liveness the reaper reads (`crates/core/src/write.rs:460-520`, and
    the same-batch `slot:` renewal from #933);
  - `cmd_custodian` (`cli.rs:913-1010`);
  - telemetry (`crates/telemetry/src/lib.rs:79-84`, `:170-180`) and the audit pattern
    (`restore.rs:433-470`).
- **Prior-art check (triage cycles):** #625's Plan searched on 2026-07-25 at `22d71b4` and
  found no reaper, no multipart-aware pass, no `crates/custodian/src/reaper.rs` and no
  multipart flag in `cli.rs`. Net-new. #659 (planned) adds the drain this slice consumes.
  Re-run the search on your base.
- **Disposition hint:** likely-fix

Plan-review response (2026-11-01, revision pass): both findings hold. Physical reclamation is now stated as post-change evidence with its own negation, and the subprocess test is the only red→green. The batch splitter is removed: the fence is unsplittable and the drain's batching belongs to #1012 and #1013, so this slice only proves its largest fence fits the budgets. The stale `Depends on` instructions were replaced with the actual mapping.

## Iteration 1 — carry-forward (from the previous attempt)
- Sign-off rationale: Rebuild inside this slice (not a re-plan): the logic is ~69 KB; the size backstop tripped on test volume, not on two outcomes. Fix the reproduced defects: 1. Slot ABA race (reaper.rs:579): the idle fence's require_absent(slot) pins pass again after a reserve -> part-commit -> release cycle lands between the reads and the fence, so a progressing upload is aborted. Make the fence detect a completed slot cycle and add a seeded DST row for reserve -> commit between the reads and the fence. Fix the doc claims that say this case is covered (reaper.rs:40-43, 06-runtime-view.md:94). If it truly cannot be closed without a record-format or 0016 change, stop and say so in build-notes rather than expanding scope. 2. Profile refusal must mutate nothing: a Blocked profile result must also suppress the dependent retirement leg (reconciliation.rs:183-187, :191-200). Keep a global refusal separate from per-session holds, keep the foreign-clock teardown, and extend leg H with an already-Aborting session plus a retire: obligation. 3. A per-session read fault in the reaper must not skip #659's drain: hold it like decode faults are held (or run the drain first). Add a DST row. Also: rebase onto current main (the patch no longer applies, so diff coverage was not measured); add leg B's "slot reserved 1h ago, lease lapsed" case; assert fence cost against an independent count; test the non-zero exit on a held session; move WALL_CLOCK_SOURCE to wyrd_core::multipart and fix its doc.
- Sign-off session carry-forward (captured live, before §9 flattened it):
  Rebuild inside this slice (not a re-plan): the logic is ~69 KB; the size backstop tripped on test volume, not on two outcomes. Fix the reproduced defects:
  1. Slot ABA race (reaper.rs:579): the idle fence's require_absent(slot) pins pass again after a reserve -> part-commit -> release cycle lands between the reads and the fence, so a progressing upload is aborted. Make the fence detect a completed slot cycle and add a seeded DST row for reserve -> commit between the reads and the fence. Fix the doc claims that say this case is covered (reaper.rs:40-43, 06-runtime-view.md:94). If it truly cannot be closed without a record-format or 0016 change, stop and say so in build-notes rather than expanding scope.
  2. Profile refusal must mutate nothing: a Blocked profile result must also suppress the dependent retirement leg (reconciliation.rs:183-187, :191-200). Keep a global refusal separate from per-session holds, keep the foreign-clock teardown, and extend leg H with an already-Aborting session plus a retire: obligation.
  3. A per-session read fault in the reaper must not skip #659's drain: hold it like decode faults are held (or run the drain first). Add a DST row.
  Also: rebase onto current main (the patch no longer applies, so diff coverage was not measured); add leg B's "slot reserved 1h ago, lease lapsed" case; assert fence cost against an independent count; test the non-zero exit on a held session; move WALL_CLOCK_SOURCE to wyrd_core::multipart and fix its doc.
- Failing gate: C4 diff coverage: changed lines executed by the patch's tests (advisory) — diff coverage not measured — patch.diff does not apply on origin/main
- Failing gate: C5 surviving mutants on the bundle diff (cargo mutants --in-diff) (advisory) — 142 mutants tested in 39m: 85 missed, 40 caught, 17 unviable
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 4 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_1015/review-
- Full previous attempt preserved in `iteration-v1/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 2 — carry-forward (from the previous attempt)
- Sign-off rationale: Rebuild inside this slice (logic ~54 KB, below the 150 KB split line). Last round's profile-refusal and read-fault fixes landed; red->green passes. Accepted gap: the slot reserve->commit->release race (reaper.rs:644) stays open in this slice. Do not try to close it and do not change 0016. Keep the DST row ignored, but give its #[ignore] a reason naming the follow-up (recorded in SUMMARY §10; tracker issue to be filed), and make the docs/module comments state the gap plainly instead of claiming it is covered. Fix, each with a test that fails today: 1. `latest_psum` must parse the key (`parse_psum_key`, multipart.rs:1480) and hold + signal on failure, like `read_slots` (reaper.rs:569-579). 2. Foreign-clocked Open sessions must not make the one-shot report "nothing to reap" with exit 0 (reaper.rs:426-430, :315-325; cli.rs:1109): count them as holds or report them truthfully; adjust leg E-open. 3. A `created_at_millis` ahead of the custodian clock beyond a set tolerance must hold with a signal, not be silently kept via saturating_sub (reaper.rs:442, :460-466). 4. The session cap must be a resumable work budget, not a fixed cutoff from the first ID (reaper.rs:381); churn between pages must not be read as damage. 5. An absent ledger must not authorize the drain under an unchecked profile when a concurrent first Create installs one (reaper.rs:345, reconciliation.rs:231); test that a newly installed mismatch mutates nothing. 6. Test the deployed-loop branch with both reconstruction and reaper passed (reconciliation.rs:177-183): mismatched profile leaves an Aborting session and its retire: obligation untouched; matching profile drains a fenced session in the same step. 7. Pre-first-chunk DST row (dst/tests/custodian.rs:7457-7500): drive the production renewal (stage.rs:1620-1632) before lease expiry and show the part commits after the reaper; use a separate crashed request for the lapse/reap half. Also: rebase onto current main so diff coverage is measured (asked for last round, still missing). Note in build-notes who must plumb a non-shipped profile into ReaperPolicy (reaper.rs:178-186; likely #508).
- Sign-off session carry-forward (captured live, before §9 flattened it):
  Rebuild inside this slice (logic ~54 KB, below the 150 KB split line). Last round's profile-refusal and read-fault fixes landed; red->green passes.
  Accepted gap: the slot reserve->commit->release race (reaper.rs:644) stays open in this slice. Do not try to close it and do not change 0016. Keep the DST row ignored, but give its #[ignore] a reason naming the follow-up (recorded in SUMMARY §10; tracker issue to be filed), and make the docs/module comments state the gap plainly instead of claiming it is covered.
  Fix, each with a test that fails today:
  1. `latest_psum` must parse the key (`parse_psum_key`, multipart.rs:1480) and hold + signal on failure, like `read_slots` (reaper.rs:569-579).
  2. Foreign-clocked Open sessions must not make the one-shot report "nothing to reap" with exit 0 (reaper.rs:426-430, :315-325; cli.rs:1109): count them as holds or report them truthfully; adjust leg E-open.
  3. A `created_at_millis` ahead of the custodian clock beyond a set tolerance must hold with a signal, not be silently kept via saturating_sub (reaper.rs:442, :460-466).
  4. The session cap must be a resumable work budget, not a fixed cutoff from the first ID (reaper.rs:381); churn between pages must not be read as damage.
  5. An absent ledger must not authorize the drain under an unchecked profile when a concurrent first Create installs one (reaper.rs:345, reconciliation.rs:231); test that a newly installed mismatch mutates nothing.
  6. Test the deployed-loop branch with both reconstruction and reaper passed (reconciliation.rs:177-183): mismatched profile leaves an Aborting session and its retire: obligation untouched; matching profile drains a fenced session in the same step.
  7. Pre-first-chunk DST row (dst/tests/custodian.rs:7457-7500): drive the production renewal (stage.rs:1620-1632) before lease expiry and show the part commits after the reaper; use a separate crashed request for the lapse/reap half.
  Also: rebase onto current main so diff coverage is measured (asked for last round, still missing). Note in build-notes who must plumb a non-shipped profile into ReaperPolicy (reaper.rs:178-186; likely #508).
- Failing gate: C4 diff coverage: changed lines executed by the patch's tests (advisory) — diff coverage not measured — patch.diff does not apply on origin/main
- Failing gate: C5 surviving mutants on the bundle diff (cargo mutants --in-diff) (advisory) — 144 mutants tested in 11m: 4 missed, 122 caught, 18 unviable
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 15 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_1015/review
- Full previous attempt preserved in `iteration-v2/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 3 — carry-forward (from the previous attempt)
- Sign-off rationale: Rebuild inside this slice (attempt 4). Findings are narrowing; fix these, each with a test that fails today: 1. F1 — an absent `mpuctl` ledger with a non-empty listing must answer `Reaped::Refused` (store-wide), not per-session holds (reaper.rs:425-435), so the dependent retirement leg does not run (reconciliation.rs:236). Add a step-level `reconcile_step` test: Aborting session + committed part + residue + retire: obligation, no `mpuctl` -> store byte-identical. Keep foreign-clock teardown working when a matching ledger exists. 2. F2 — a reconstruction error must not starve the reaper: keep the reconstruction error but let the reaper/drain run in the same step (reconciliation.rs:168-170); regression covering both inputs with a failing `repair:` scan. 3. F3 — rewrite DST X60/X62 with reachable production interleavings: X60 keeps its slot lease live; X62 pauses the production renewal after its live check, crosses expiry, releases it between the reaper's read and fence, then shows the conflicting fence AND the eventual part commit (0016:2275; see multipart_upload_part_stage.rs:4978 for the schedule). 4. Bound the reaper cursor (reaper.rs:488): reset oversized/damaged values like GC does (gc.rs:1919, :2035); test with a backend rejecting oversized scan keys. Out of scope (filed as #659 follow-ups, SUMMARY §10): the stray-obligation-beside-Open drain case and the post-check first-Create/drain race — do not change #659's drain. Keep the slot-cycle gap accepted and ignored as before. The C4-ci timeout looked environmental (custodian_gc passes in isolation); make sure the full `cargo xtask ci` completes.
- Sign-off session carry-forward (captured live, before §9 flattened it):
  Rebuild inside this slice (attempt 4). Findings are narrowing; fix these, each with a test that fails today:
  1. F1 — an absent `mpuctl` ledger with a non-empty listing must answer `Reaped::Refused` (store-wide), not per-session holds (reaper.rs:425-435), so the dependent retirement leg does not run (reconciliation.rs:236). Add a step-level `reconcile_step` test: Aborting session + committed part + residue + retire: obligation, no `mpuctl` -> store byte-identical. Keep foreign-clock teardown working when a matching ledger exists.
  2. F2 — a reconstruction error must not starve the reaper: keep the reconstruction error but let the reaper/drain run in the same step (reconciliation.rs:168-170); regression covering both inputs with a failing `repair:` scan.
  3. F3 — rewrite DST X60/X62 with reachable production interleavings: X60 keeps its slot lease live; X62 pauses the production renewal after its live check, crosses expiry, releases it between the reaper's read and fence, then shows the conflicting fence AND the eventual part commit (0016:2275; see multipart_upload_part_stage.rs:4978 for the schedule).
  4. Bound the reaper cursor (reaper.rs:488): reset oversized/damaged values like GC does (gc.rs:1919, :2035); test with a backend rejecting oversized scan keys.
  Out of scope (filed as #659 follow-ups, SUMMARY §10): the stray-obligation-beside-Open drain case and the post-check first-Create/drain race — do not change #659's drain.
  Keep the slot-cycle gap accepted and ignored as before. The C4-ci timeout looked environmental (custodian_gc passes in isolation); make sure the full `cargo xtask ci` completes.
- Failing gate: C4 diff coverage: changed lines executed by the patch's tests (advisory) — diff coverage not measured — patch.diff does not apply on origin/main
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 14 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_1015/review
- Full previous attempt preserved in `iteration-v3/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 4 — carry-forward (from the previous attempt)
- Sign-off rationale: Rebuild inside this slice (attempt 5, the last auto round). Product code is ~94 KB (reaper.rs 55 KB); the size backstop trips on tests/docs, as in rounds 1-3. Findings are narrow; fix these, each with a test that fails today: 1. F1 — ledger lost mid-pass: when the first `mpuctl` read matches and the final read finds it Absent, `refusal()` accepts it and the early return at reaper.rs:443 lets the record-only drain run (reconciliation.rs:200). Treat matching->absent as a store-wide Refused. Add a step-level `reconcile_step` regression that removes `mpuctl` just before the second read and asserts no drain mutation (reviewer probe: pdca-reviewer-1015-probes/run.py). The existing lost-ledger test (reaper/tests.rs:1056) only covers absence at the first read. 2. F2 — in crates/custodian/tests/gc.rs:1425/:1444, run ONE GC pass at mark+grace-1 and ONE at mark+grace (brief.md:60), not three each; assert reclamation after the single pass. 3. No-fleet hold test: the `None` arm of `ByChunk::whole` (retirement.rs:429-435) has no test; a mutant removing it survives all suites. Add a record-only step test (or one-shot leg): Aborting session with its {session, all} obligation, a `part:` with wrong-length placement, an undecodable `sidx:` -> records byte-identical, step Blocked naming partial-fleet, one-shot exits non-zero. 4. Usage text: add `--reap-abandoned-uploads` to the custodian usage at cli.rs:484, with one line: metadata-only, needs no --endpoints, exits non-zero when a session is held or the profile check refuses. 5. Exact-budget boundary (reaper.rs:607, cli.rs:1111): a full final page must not leave the one-shot reporting Partial for ever; handle the terminal boundary so "rerun until Satisfied" is reachable, with regressions at 64 and 65 retained sessions. Do NOT rebase again: the patch stacks on unmerged #1027 and #1007-#1013, so it cannot apply on origin/main; diff coverage stays unmeasured until they land — accepted. Keep the slot-cycle gap accepted/ignored and the #659 follow-ups out of scope, as before. The batch-splitter and binding-red->green plan findings were already resolved in the brief (brief.md:264); do not reopen them.
- Sign-off session carry-forward (captured live, before §9 flattened it):
  Rebuild inside this slice (attempt 5, the last auto round). Product code is ~94 KB (reaper.rs 55 KB); the size backstop trips on tests/docs, as in rounds 1-3. Findings are narrow; fix these, each with a test that fails today:
  1. F1 — ledger lost mid-pass: when the first `mpuctl` read matches and the final read finds it Absent, `refusal()` accepts it and the early return at reaper.rs:443 lets the record-only drain run (reconciliation.rs:200). Treat matching->absent as a store-wide Refused. Add a step-level `reconcile_step` regression that removes `mpuctl` just before the second read and asserts no drain mutation (reviewer probe: pdca-reviewer-1015-probes/run.py). The existing lost-ledger test (reaper/tests.rs:1056) only covers absence at the first read.
  2. F2 — in crates/custodian/tests/gc.rs:1425/:1444, run ONE GC pass at mark+grace-1 and ONE at mark+grace (brief.md:60), not three each; assert reclamation after the single pass.
  3. No-fleet hold test: the `None` arm of `ByChunk::whole` (retirement.rs:429-435) has no test; a mutant removing it survives all suites. Add a record-only step test (or one-shot leg): Aborting session with its {session, all} obligation, a `part:` with wrong-length placement, an undecodable `sidx:` -> records byte-identical, step Blocked naming partial-fleet, one-shot exits non-zero.
  4. Usage text: add `--reap-abandoned-uploads` to the custodian usage at cli.rs:484, with one line: metadata-only, needs no --endpoints, exits non-zero when a session is held or the profile check refuses.
  5. Exact-budget boundary (reaper.rs:607, cli.rs:1111): a full final page must not leave the one-shot reporting Partial for ever; handle the terminal boundary so "rerun until Satisfied" is reachable, with regressions at 64 and 65 retained sessions.
  Do NOT rebase again: the patch stacks on unmerged #1027 and #1007-#1013, so it cannot apply on origin/main; diff coverage stays unmeasured until they land — accepted. Keep the slot-cycle gap accepted/ignored and the #659 follow-ups out of scope, as before. The batch-splitter and binding-red->green plan findings were already resolved in the brief (brief.md:264); do not reopen them.
- Failing gate: C4 diff coverage: changed lines executed by the patch's tests (advisory) — diff coverage not measured — patch.diff does not apply on origin/main
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 11 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_1015/review
- Full previous attempt preserved in `iteration-v4/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 5 — carry-forward (from the previous attempt)
- Sign-off rationale: Rebuild inside this slice (attempt 6, narrow). Red->green and full CI pass; findings dropped 15 -> 14 -> 11 and are implementation-shaped, so no re-plan. Fix these, each with a test that fails today: 1. F1 — persistent listing fault starves the readable prefix forever (crates/custodian/src/reaper.rs:654-658, :692-695): after a page fault the cursor is saved at the failing boundary and every later pass resumes there. Keep the hold/signal, but start the next pass from the head after a listing fault. Regression: fault a later page persistently, advance the clock past W_open, assert an earlier session is fenced. Update the test at reaper/tests.rs:1003-1033 that currently asserts the stuck behaviour. This should also kill the surviving C5 mutant at reaper.rs:679. 2. Deployed-loop wiring test: crates/server/src/custodian.rs:680 can be changed from Some(&reaper) to None with all suites green. Add one case to crates/server/tests/custodian_gc.rs that seeds an idle Open session, runs drive_passes (:1341), and asserts it is fenced and torn down. 3. F3 — late profile refusal (reaper.rs:476-493, :1187-1189): emit the pass tallies (emit_pass) on every exit after walk, including a transient read error and a final refusal; distinguish "refused before acting" from "stopped after fencing N" in the message. Assert message and counters in the regression at reaper/tests.rs:1400. Deferred / not to fix: F2 (stale clean-lap, reaper.rs:714) needs >64 sessions; the only accepted profile caps at 46 — defer to #508 and say so in build-notes and a doc comment. Record as rejected with reasons the T4 items reachable only through outside damage: no mpuctl precondition on the fence (reaper.rs:1123) and the reconciliation.rs:187 separate-context convention (one store and clock passed to both, custodian.rs:611-639). The psum-read cost per interval is low priority; do not change cadence in this round. Keep as before: do NOT rebase (stacked on unmerged #1027, #1007-#1013; diff coverage unmeasured is accepted); the slot-cycle gap stays accepted/ignored; #659 follow-ups stay out of scope; the batch-splitter and binding-red->green plan findings stay resolved (brief.md:264).
- Sign-off session carry-forward (captured live, before §9 flattened it):
  Rebuild inside this slice (attempt 6, narrow). Red->green and full CI pass; findings dropped 15 -> 14 -> 11 and are implementation-shaped, so no re-plan. Fix these, each with a test that fails today:
  1. F1 — persistent listing fault starves the readable prefix forever (crates/custodian/src/reaper.rs:654-658, :692-695): after a page fault the cursor is saved at the failing boundary and every later pass resumes there. Keep the hold/signal, but start the next pass from the head after a listing fault. Regression: fault a later page persistently, advance the clock past W_open, assert an earlier session is fenced. Update the test at reaper/tests.rs:1003-1033 that currently asserts the stuck behaviour. This should also kill the surviving C5 mutant at reaper.rs:679.
  2. Deployed-loop wiring test: crates/server/src/custodian.rs:680 can be changed from Some(&reaper) to None with all suites green. Add one case to crates/server/tests/custodian_gc.rs that seeds an idle Open session, runs drive_passes (:1341), and asserts it is fenced and torn down.
  3. F3 — late profile refusal (reaper.rs:476-493, :1187-1189): emit the pass tallies (emit_pass) on every exit after walk, including a transient read error and a final refusal; distinguish "refused before acting" from "stopped after fencing N" in the message. Assert message and counters in the regression at reaper/tests.rs:1400.
  Deferred / not to fix: F2 (stale clean-lap, reaper.rs:714) needs >64 sessions; the only accepted profile caps at 46 — defer to #508 and say so in build-notes and a doc comment. Record as rejected with reasons the T4 items reachable only through outside damage: no mpuctl precondition on the fence (reaper.rs:1123) and the reconciliation.rs:187 separate-context convention (one store and clock passed to both, custodian.rs:611-639). The psum-read cost per interval is low priority; do not change cadence in this round.
  Keep as before: do NOT rebase (stacked on unmerged #1027, #1007-#1013; diff coverage unmeasured is accepted); the slot-cycle gap stays accepted/ignored; #659 follow-ups stay out of scope; the batch-splitter and binding-red->green plan findings stay resolved (brief.md:264).
- Failing gate: C4 diff coverage: changed lines executed by the patch's tests (advisory) — diff coverage not measured — patch.diff does not apply on origin/main
- Failing gate: C5 surviving mutants on the bundle diff (cargo mutants --in-diff) (advisory) — 191 mutants tested in 11m: 1 missed, 168 caught, 22 unviable
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 11 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_1015/review
- Full previous attempt preserved in `iteration-v5/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 6 — carry-forward (from the previous attempt)
- Sign-off rationale: Rebuild inside this slice (attempt 7, narrow). Red->green and full CI pass; the remaining findings are implementation bugs, so no re-plan. Fix these, each with a test that fails today: 1. F2 — uncertain fence commit reported as "left in place" (crates/custodian/src/reaper.rs:1216-1235): when the fence commit returns an error that may have applied (e.g. CommitUnknownResult), do not call hold()'s "left a session in place" audit and do not leave the fence tally silent. Classify the error; report the outcome as unknown with its own audit action and counter. Regression: a commit double that applies the batch and then returns the unknown result (the current double at reaper/tests.rs:215 errors before applying). 2. F1 — largest-fence test is not the largest legal session (crates/custodian/src/reaper/tests.rs:1314): widest_open_session() uses a 1,024-byte object and 256-byte content type. Build a session at the real admission ceiling Create allows (crates/core/src/multipart/create.rs:345, including JSON escaping/headroom) with all maximum-width slot pins, and keep the independent cost assertion against MAX_BATCH_OPS / MAX_BATCH_BYTES. 3. One-shot silently ignores flags (crates/server/src/cli.rs:1007-1029, :1103-1126): refuse --reap-abandoned-uploads combined with --gc-expired-pending, and with --endpoints/--ids/--failure-domains, the same way :1007 refuses --reconcile-after-restore. Test each refused combination exits non-zero with a clear message. 4. Wrong --data-dir gives a false "satisfied" (cli.rs:1671 -> :2019-2022): the one-shot must not report "nothing to reap or drain" on a store it just created. Refuse when meta.redb did not exist before the open (or at minimum name the opened store path in the outcome line). Test with a non-existent data dir. Keep as before: do NOT rebase (stacked on unmerged #1027, #1007-#1013; diff coverage unmeasured is accepted); the slot-cycle gap stays accepted/ignored (its tracker number is a human item for sign-off); #659 follow-ups stay out of scope; the batch-splitter and binding-red->green plan findings stay resolved; F2-of-iteration-5 (stale clean-lap) stays deferred to #508; the recorded rejections in review-rejected.md stand.
- Sign-off session carry-forward (captured live, before §9 flattened it):
  Rebuild inside this slice (attempt 7, narrow). Red->green and full CI pass; the remaining findings are implementation bugs, so no re-plan. Fix these, each with a test that fails today:
  1. F2 — uncertain fence commit reported as "left in place" (crates/custodian/src/reaper.rs:1216-1235): when the fence commit returns an error that may have applied (e.g. CommitUnknownResult), do not call hold()'s "left a session in place" audit and do not leave the fence tally silent. Classify the error; report the outcome as unknown with its own audit action and counter. Regression: a commit double that applies the batch and then returns the unknown result (the current double at reaper/tests.rs:215 errors before applying).
  2. F1 — largest-fence test is not the largest legal session (crates/custodian/src/reaper/tests.rs:1314): widest_open_session() uses a 1,024-byte object and 256-byte content type. Build a session at the real admission ceiling Create allows (crates/core/src/multipart/create.rs:345, including JSON escaping/headroom) with all maximum-width slot pins, and keep the independent cost assertion against MAX_BATCH_OPS / MAX_BATCH_BYTES.
  3. One-shot silently ignores flags (crates/server/src/cli.rs:1007-1029, :1103-1126): refuse --reap-abandoned-uploads combined with --gc-expired-pending, and with --endpoints/--ids/--failure-domains, the same way :1007 refuses --reconcile-after-restore. Test each refused combination exits non-zero with a clear message.
  4. Wrong --data-dir gives a false "satisfied" (cli.rs:1671 -> :2019-2022): the one-shot must not report "nothing to reap or drain" on a store it just created. Refuse when meta.redb did not exist before the open (or at minimum name the opened store path in the outcome line). Test with a non-existent data dir.
  Keep as before: do NOT rebase (stacked on unmerged #1027, #1007-#1013; diff coverage unmeasured is accepted); the slot-cycle gap stays accepted/ignored (its tracker number is a human item for sign-off); #659 follow-ups stay out of scope; the batch-splitter and binding-red->green plan findings stay resolved; F2-of-iteration-5 (stale clean-lap) stays deferred to #508; the recorded rejections in review-rejected.md stand.
- Failing gate: C4 diff coverage: changed lines executed by the patch's tests (advisory) — diff coverage not measured — patch.diff does not apply on origin/main
- Failing gate: C5 surviving mutants on the bundle diff (cargo mutants --in-diff) (advisory) — 200 mutants tested in 16m: 177 caught, 22 unviable, 1 timeouts
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 8 blocking, 2 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_1015/review-
- Full previous attempt preserved in `iteration-v6/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 7 — carry-forward (from the previous attempt)
- Sign-off rationale: Rebuild inside this slice (attempt 8, narrow). The logic is ~100 KB source / ~1,200 code lines; the size backstop trips on test volume, not two outcomes, so no re-plan. Red->green, full CI and mutants pass. Fix only these two, each with a test that fails today: 1. F1 — zero-byte meta.redb is initialized and reported "satisfied" (crates/server/src/cli.rs:2118-2127 -> RedbMetadataStore::open = Database::create, crates/metadata-redb/src/lib.rs:67). Add a non-creating open (e.g. RedbMetadataStore::open_existing over redb::Database::open) and use it in the one-shot, which also closes the check-then-open race. Extend the_reaper_one_shot_opens_only_a_store_that_exists with a zero-length file: non-zero exit, file left at 0 bytes. 2. F2 — TiKV/FDB outcome line names only the backend (cli.rs:1722, :1724). Carry the resolved target identity (WYRD_TIKV_PD_ENDPOINTS; WYRD_FDB_CLUSTER_FILE or the default cluster file) into the [store: ...] tail, with a test showing two configurations give different identifiers. Keep as before: do NOT rebase (stacked on unmerged #1027, #1007-#1013; diff coverage unmeasured is accepted); the slot-cycle gap stays accepted/ignored (its tracker number is a human item at sign-off); #659 follow-ups stay out of scope; the batch-splitter and binding-red->green plan findings stay resolved; F2-of-iteration-5 stays deferred to #508; recorded rejections in review-rejected.md stand. Do not widen scope beyond F1/F2.
- Sign-off session carry-forward (captured live, before §9 flattened it):
  Rebuild inside this slice (attempt 8, narrow). The logic is ~100 KB source / ~1,200 code lines; the size backstop trips on test volume, not two outcomes, so no re-plan. Red->green, full CI and mutants pass. Fix only these two, each with a test that fails today:
  1. F1 — zero-byte meta.redb is initialized and reported "satisfied" (crates/server/src/cli.rs:2118-2127 -> RedbMetadataStore::open = Database::create, crates/metadata-redb/src/lib.rs:67). Add a non-creating open (e.g. RedbMetadataStore::open_existing over redb::Database::open) and use it in the one-shot, which also closes the check-then-open race. Extend the_reaper_one_shot_opens_only_a_store_that_exists with a zero-length file: non-zero exit, file left at 0 bytes.
  2. F2 — TiKV/FDB outcome line names only the backend (cli.rs:1722, :1724). Carry the resolved target identity (WYRD_TIKV_PD_ENDPOINTS; WYRD_FDB_CLUSTER_FILE or the default cluster file) into the [store: ...] tail, with a test showing two configurations give different identifiers.
  Keep as before: do NOT rebase (stacked on unmerged #1027, #1007-#1013; diff coverage unmeasured is accepted); the slot-cycle gap stays accepted/ignored (its tracker number is a human item at sign-off); #659 follow-ups stay out of scope; the batch-splitter and binding-red->green plan findings stay resolved; F2-of-iteration-5 stays deferred to #508; recorded rejections in review-rejected.md stand. Do not widen scope beyond F1/F2.
- Failing gate: C4 diff coverage: changed lines executed by the patch's tests (advisory) — diff coverage not measured — patch.diff does not apply on origin/main
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 8 blocking, 4 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_1015/review-
- Full previous attempt preserved in `iteration-v7/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).
