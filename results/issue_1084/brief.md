# custodian: committed repair and evacuation pre-mark their destinations (0016 X47) (636.6c-2) — issue 1014 child-2

> Child 2 of 3 of #1014's split. Do reads ONLY this file. Keep the `- **Label:** value` lines.
> The rule is normative in proposal 0016, `docs/design/proposals/draft/0016-multipart-commit-protocol.md`:
> read the pre-mark rule (`0016:1287-1312`), the write gate (`0016:1341-1360`), the per-move
> nonce (`0016:1179-1189`), the repoint row (`0016:670`), X47 (`0016:2587`) and X29
> (`0016:2568`) first. Citations are verified on `dd9fa05` (#1013's PR head, getwyrd/wyrd
> PR #1082); re-locate them on the base this bundle is given.
>
> **Base.** Do builds on `main` plus #1013's published head (`dd9fa05`). `main` alone
> (`e839e76`) lacks what this child's tests use: the retirement drain and its leg in
> `reconcile_step` (D3 needs the drain), and the custodian's `wyrd-metadata-redb`
> dev-dependency (`crates/custodian/Cargo.toml:65`). #1013 reaches the base only if PR #1082
> has merged into `main`, or if `1013` is named in the same `pdca flow` command (the run then
> carries it onto its integration line before wave 0); otherwise the driver holds this bundle
> rather than build it on `main`. **Check the base before writing code:** `reconcile_step`
> calls `retirement::reconcile` (`crates/custodian/src/reconciliation.rs:168`) and
> `crates/custodian/Cargo.toml` lists `wyrd-metadata-redb` as a dev-dependency. If either is
> missing, stop and say so in `build-notes.md`; do not add the dependency or the drain yourself.
>
> **This is a re-plan (attempt 4).** Three builds kept one design for fresh destinations, and
> this brief keeps it: one pre-mark module shared by both movers (pre-mark, a write gated by
> `W_repoint` and sent with a deadline the D server enforces, an adoption that consumes the
> pre-mark), and the D1–D7 legs. What changed is at the end, under *Re-plan 2026-11-23*. The
> previous attempt is in `iteration-v3/` (patch, review, gates); reuse what it got right.

- **Slug:** committed-move-destination-premark
- **Track:** alpha
- **Kind:** enhancement (design proposal 0016, X47) / defect
- **Defect:** The committed-path moves — reconstruction's `repair_chunk`
  (`crates/custodian/src/reconstruction.rs:1103`) and rebalance's `evacuate_chunk`
  (`crates/custodian/src/rebalance.rs:516`) — write the destination fragment, then commit the
  repoint with a fresh legacy mark on each **vacated** position only
  (`reconstruction.rs:1226-1240`, `rebalance.rs:634-649`). They never read or touch the mark at
  the **destination** position. 0016 X47 requires the committed repoint to pre-mark its
  destination before writing the fragment and to adopt it in the repoint's commit under a
  precondition on that pre-mark (`0016:1287-1294`, `:2587`); the staged path does exactly that
  (`crates/custodian/src/reconstruction/staged.rs:557-576`, `:634-656`, `:721-751`), the
  committed paths do not. Four failures follow on the base: (1) a fragment moved back onto a
  server within grace (evacuate 5→2 marks `orphan:5:c:i`; a later move 2→5) stays referenced
  under that old mark, and once an unreference that writes no fresh mark comes (multipart
  Complete's supersede today; #1014's routed `unlink` next), GC reclaims it on the old stamp
  before the drain marks it; (2) a destination already carrying an expired mark can have the
  move's freshly written fragment reclaimed by GC before the move commits, after which the commit
  publishes a placement over missing bytes (0016 refutation outcome (c)); (3) a move that loses
  its commit leaves its written fragment unreferenced and unmarked, which GC keeps for ever
  (`gc.rs:742`); (4) both movers send their destination writes with no deadline
  (`put_fragment(.., None)`, `reconstruction.rs:1229`, `rebalance.rs:638`), so a pre-mark and an
  adoption precondition alone would still let a write land after GC had reclaimed its evidence:
  0016 also gates the write (`0016:1341-1360`), and the staged path does both
  (`staged.rs:680-687`).
- **Success criterion:** both movers, each on a flat and on a segmented committed object, driven
  **through `reconcile_step`** over `RedbMetadataStore::in_memory()` and a placement-aware
  chunk-store double that enforces `put_fragment`'s deadline on the test's clock, as a D server
  does (`WriteDeadlineExpired::if_elapsed`, `crates/traits/src/lib.rs:925`, called by
  `crates/chunkstore-fs/src/lib.rs:286-288`). Repair runs with a reconstruction context built as
  the deployed loop builds it (`crates/server/src/custodian.rs:564-580`). Evacuation runs with a
  **library-level `Some(RebalanceContext)`**: the deployed loop passes `rebalance: None`
  (`custodian.rs:591-599`, `:613-621`), and wiring it is out of scope. **Topology:** four D
  servers in four distinct failure domains, the RS(2,1) object's fragments on servers 3, 4 and
  5, server 2 free. Evacuation excludes the draining server and the survivors' domains
  (`rebalance.rs:528-535`) and refuses when no domain is free (`crates/core/src/placement.rs:283-287`), so
  server 2 is the only legal destination for server 5's fragment. **Repair is not forced by the
  domains alone:** it excludes only the survivors' domains (`reconstruction.rs:1122-1126`), so
  the lost server's own domain stays free, and the selector orders free domains by
  utilization, then label (`placement.rs:290-301`). Each repair pass whose destination a leg
  depends on is therefore given a `topology` in which the intended destination's domain is the
  only free one (Do may add a second free server where a leg needs "picks another server", D4):
  servers 2, 3, 4 for a move to server 2 (D1, D2's first pass after the loss on 5); servers 3,
  4, 5 for a move to server 5 (D2's second pass after the loss on 2) and for an in-place
  rebuild on 5 (D10).
  The fleet keeps all four stores (a destination outside it aborts the repair,
  `reconstruction.rs:1142-1146`). After every pass a leg relies on, the test asserts that the
  placement names the intended server **before** any mark assertion, so no leg passes or fails
  on the wrong move. One `wyrd_testkit::ManualClock` backs every context's clock and every
  pass's `now_millis`. **Where the legs live:** the repair legs in the NEW file
  `crates/custodian/tests/move_destination_marks.rs`; the evacuation legs in the `#[cfg(test)]`
  module of `crates/custodian/src/rebalance.rs`, still through `crate::reconcile_step`. They
  must build a `RebalanceContext`, whose literal this child changes (Scope), and C4-verify
  compiles every added test file in one red-leg run, so no added file could hold them and still
  compile on the base. It shows all of, for each mover and both map shapes (D9 and D10 are
  repair's alone: evacuation already answers `Blocked`, and never lands on the server it drains):
  (D1) A stale legacy mark at the destination position before the move: after the move commits,
  no mark exists at any position the new placement names (rebuilt-in-place positions included),
  and the vacated positions carry their mark as today. (D2) The adversary's round trip: a
  fragment moved from server 5 to server 2 and, within grace, back to 5 (evacuation: drain 5,
  then drain 2 and un-drain 5; repair: lose it on 5 under topology {2, 3, 4}, then on 2 under
  topology {3, 4, 5}; the placement asserted after each pass): after the second commit no mark
  exists at server 5's position. (D3) The consequence: after (D1) or (D2), a
  production multipart Complete supersedes the object; GC-only passes with the clock past the old
  stamp's grace reclaim none of its fragments before the drain marks them, and all are gone after
  those marks' grace. (D4) A destination whose mark is `reclaiming`, or undecodable, is never
  written: the move picks another server or aborts, and names it on the audit seam; a
  **vacated** position whose mark is `reclaiming` or undecodable still holds those exact bytes
  after the adoption. (D5) GC records its reclaim intent on the destination's mark while the
  move is in flight, and the adoption is released **while the key still holds `reclaiming`**
  (a wrapper holds GC after its intent commit; a second arm holds it after the fragment delete
  and before the key cleanup): the adoption fails, no placement names the reclaimed position,
  and a later pass repairs the chunk. (D6) A move that loses its commit (a wrapper changes the
  root first): its written destination fragment is reclaimed by GC after grace, not kept for
  ever. (D7) **The write is gated, not only the adoption.** Server 2 already holds an
  unreferenced copy of the fragment under an aged mark (an earlier move that lost its commit).
  (D7a) The store wrapper holds the move right after its pre-mark commit lands; the clock moves
  past `W_REPOINT_MILLIS` (`gc.rs:218`) and past the pre-mark's grace; GC-only passes reclaim
  the old copy and delete the pre-mark's key; the move is released and sends **no** write to
  server 2 (the double records none) and aborts. (D7b) The double holds the destination
  `put_fragment` in flight; GC-only passes past the pre-mark's grace reclaim the old copy and the
  pre-mark; the double then refuses the write, its deadline (the pre-mark's stamp plus the write
  window) having passed, and the move aborts. (D7c) The deadline passes while the pre-mark still
  stands (clock past `W_WRITE` but inside grace) and the destination holds nothing: no
  adoption, the obligation kept. (D7d) **The gate's exact boundary.** The store wrapper holds
  the move right after its pre-mark commit; the clock is set to exactly the pre-mark's stamp
  (the `ManualClock` reading the pre-mark batch was built at) plus `W_REPOINT_MILLIS`, still
  inside `W_WRITE_MILLIS` (`gc.rs:202`, `:218`: 30,000 vs 10,000 ms), so the D server would
  still take the write: the double records **no** write to the destination, the move aborts,
  and the move is still owed (repair: a non-`Satisfied` verdict, the obligation kept;
  evacuation: the placement still names the draining server). A twin arm, its clock one
  millisecond short of that instant, sends the write and commits. The peer refuses at equality
  (`staged.rs:680`). After any of D7a–d, no fragment at server 2 lacks both a reference and a
  mark, no placement names a missing fragment, and a later pass moves the chunk. (D8) **Each
  move's identity is its own.** Two moves of one chunk stamp their pre-marks in the same
  millisecond (one `ManualClock` reading): B is held before its pre-mark commit; A
  pre-marks, writes, and is held before its adoption; GC ages and reclaims A's destination and
  its mark; B is released and pre-marks the now-empty key; A is released: A's adoption fails,
  and no placement names a missing fragment. (D9) **A refusal never certifies.** A pass whose
  only committed repair is refused — every usable destination `reclaiming` or undecodable, the
  pre-mark too old to authorize the write, the write refused at its deadline, or its landing
  unverified — returns a non-`Satisfied` verdict, keeps the obligation, and names the refusal
  under its own reason on the audit seam, never `unplaced`; the D4 and D7 legs assert the
  pass's verdict. (D10) **A fragment rebuilt in place is never left under a mark.** (a) An
  in-place repair stopped after its write lands and before its adoption (a wrapper ends the
  pass there): the next pass drains the obligation, no mark exists at the position, and after a
  production multipart Complete supersedes the object, GC-only passes reclaim none of its
  fragments before the drain marks them. (b) A supersede lands between the in-place repair's
  pre-mark batch and its write, and the write lands before its deadline: GC reclaims the
  fragment only after the drain's mark's grace.
- **Falsifiability:** RED is reachable in-process on `cargo test -p wyrd-custodian`. **On the
  base**, with existing APIs: (D1)/(D2) the destination mark survives, (D3) GC reclaims early,
  (D4) the move writes onto a `reclaiming` destination, (D5) the commit lands over a reclaimed
  position, (D6) the fragment is held for ever; the repair legs in the new file earn
  C4-verify's red (its classifier names this file `ADDED_TEST`). (D7)–(D10), the evacuation
  legs and the nonce's replay test are red on the mutations named under Verification posture
  (the replay test only under `--cfg madsim`, which C4-verify does not build). (D8) is red on iteration 3's
  identity (`repoint:<chunk>:<stamp>`), which the iteration-3 reviewer reproduced on both shapes.
- **Invariant to restore:** **C-1** (`docs/principles.md` §5 C-1, §6 row *storage lifecycle /
  reclamation*): every durable fragment is, at every instant, named by a record or evidenced for
  reclamation by a mark whose grace a reader can rely on; a mark never outlives, on a position a
  committed reference names, the event it evidenced; no commit publishes a placement over a
  position GC has decided to reclaim (0016 outcome (c), `0016:2530-2535`); no fragment is
  written after its evidence may have been reclaimed (`0016:1341-1360`); and no pass reports
  redundancy restored while a repair it owes is still undone (`Satisfied`,
  `crates/custodian/src/reconciliation.rs:22-23`). **One named exception, deferred:** a fragment
  rebuilt in place can be left unreferenced and unmarked when a competing move's adoption
  commits late (#1100); closing it needs a fence 0016 does not define. **Sources:** C-1
  (maintainer's rule 2026-07-25); 0016 X47/X29, the pre-mark rule, the write gate and the
  per-move nonce; `crates/core/src/metadata.rs:154-168` ("once every move that re-places a chunk
  adopts under its mark's precondition, nothing can reference that position again before its
  key is gone"). SELF-TEST: it binds both movers, the reconstruction verdict and GC's reclaim
  intent; guarding one module cannot satisfy it.
- **Repo + branch target:** getwyrd/wyrd @ main
- **Depends on:** 1013
- **Ordering note:** wave 1 of #1014's split, **first**: in this split child-1 = #1083, child-2 =
  #1084, child-3 = #1085, and both other children depend on this one. Child-1 does because its
  cleanup marker stays valid only while no move can recreate a stale mark on a referenced
  position (D1, D2, D10 keep that true); child-3 because routing removes the inline re-stamp
  that today hides a stale destination mark. This child's docs edit is confined to
  `06-runtime-view.md:40-41`; its `gc.rs` edits are doc comments only (`:255-263`, `:862-865`),
  and child-1, which edits GC's behaviour, builds after it. `1013` is in `Depends on` for the
  drain (D3), the redb dev-dependency, and the `ReconstructionContext` fields of #1013's stack
  (`crates/custodian/src/reconstruction.rs:119-155`) this child's tests construct; see *Base*
  at the top. **Intake-cap override (wyrd-pdca-P1):** the human's 2026-11-18 override for
  #1014's split; this re-plan repairs that brief and adds no bundle. No further split
  (2026-11-23): the track's cap is full and this bundle is two splits deep.
- **Scope:** the committed-path repair and evacuation moves keep 0016 X47's destination rule and
  its write gate (`0016:1287-1312`, `:1341-1360`), as the staged re-place does: a pre-mark
  committed before any write; writes authorized only while the pre-mark is younger than
  `W_REPOINT_MILLIS` and sent with a deadline, the pre-mark's stamp plus the write window, that
  the D server enforces; an adoption that requires and consumes the pre-mark. So no mark older
  than the move survives on a position the move's new placement names, the move never publishes
  over a position GC has decided to reclaim, no write lands after its evidence may be gone, and
  a move that does not commit leaves what it wrote reclaimable. Within that:
  - **Destination choice.** A mark rules out one position (server, fragment index), never its
    server, and the choice is the one the staged peer makes (`staged.rs:406-460`), shared rather
    than copied a third time; a multi-fragment move can swap servers between fragments.
  - **Per-move identity.** Each pre-mark carries a fresh per-move nonce (`0016:1179-1189`):
    unique across moves, stable only across the retries of one move, and never derived from
    coordinates two moves share (the chunk, the stamp, the root generation, the `seg:` record).
    Three constraints: no new crate (`Cargo.lock` untouched); no new field on
    `ReconstructionContext` (the new test file must compile on the base for C4-verify's red);
    and a madsim seed still replays to the same run. `cargo xtask ci` does **not** check that:
    its DST leg sets only `MADSIM_TEST_NUM` (`xtask/src/main.rs:1816`), never
    `MADSIM_TEST_CHECK_DETERMINISM` (`crates/dst/src/lib.rs:91`), and D8 compares identities
    within one run. So add a **replay test** to `crates/dst/tests/custodian.rs`: a plain
    `#[test]` that runs one fixed seed twice, each run on a fresh `std::thread` with its own
    `madsim::runtime::Runtime::with_seed_and_config(seed, madsim::Config::default())` (madsim's
    own runner gives each seed a fresh thread, and std's hash keys are per thread; the in-repo
    peer for one runtime per seed is `run_pinned_race`, `crates/dst/tests/concurrency.rs:160-169`).
    Each run drives one committed repair onto a fresh destination through `reconcile_step` and
    returns the pre-mark value(s) the move committed, the pass's verdict and the chunk's final
    placement; the test asserts the two runs return equal values. Capture the pre-mark from the
    batches the move commits (the adoption deletes it), with a tap shaped like `RecordingMeta`
    (`crates/dst/tests/custodian.rs:1835-1848`: instance state only, ADR-0035). The test runs
    in `cargo xtask ci`'s DST leg. Std's `std::hash::RandomState` is one candidate source; the
    replay test decides whether it qualifies, not an argument. If no source meets the three
    constraints and passes the replay test, stop and say so in `build-notes.md`.
  - **Rebuilt in place** (the destination is the server the placement already names): no mark
    is ever written at that position; a stale mark there is deleted under its exact bytes and
    the repoint's pins, in the batch committed before the write; the write is gated and carries
    its deadline from that batch's stamp, like any other.
  - **The vacated position's mark** is written under a precondition on the mark the move read
    there, never blind: absent ⇒ `require_absent` and write (`0016:670`); a legacy or structured
    mark of another event ⇒ re-stamped under its exact bytes (`0016:1224-1226`); `reclaiming` or
    undecodable ⇒ left byte for byte (`metadata.rs:154-164`, ADR-0045 decision 3).
  - **Refusals.** Reconstruction's verdict counts each refusal above as a repair still owed
    (non-`Satisfied`, obligation kept, its own audit reason), as evacuation already answers
    `Blocked` (`rebalance.rs:49-56`, `:219`); today an aborted repair only emits `unplaced`
    (`reconstruction.rs:457`) and never reaches the verdict (`:504-519`).
  - **The clock.** Both movers read a move's time from the context's `wyrd_testkit::Clock`
    (ADR-0009, ADR-0024; `ReconstructionContext::clock`, `reconstruction.rs:119-133`) and take
    the window from the context (`staged_write_window_millis`, `:134-141`, deployed as
    `gc::W_WRITE_MILLIS`, `crates/server/src/custodian.rs:132`, `:572`). `RebalanceContext`
    (`rebalance.rs:91-99`) gains the same two fields, documented the same way, and every struct
    literal of it is updated:
    `crates/custodian/tests/{placement_ceiling,rebalance,segmented_map_evacuate,segmented_map_rebalance,staged_drain_status}.rs`
    and `crates/dst/tests/custodian.rs` (built only under `--cfg madsim`; check it with
    `cargo xtask ci`).
  - **Tests that already exist stay honest.** The committed-repair fixtures —
    `crates/custodian/tests/reconstruction.rs` (15 context literals),
    `segmented_map_repoint.rs`, `segmented_map_reconstruction.rs`, `staged_protection.rs` and
    `crates/server/tests/custodian_day_one.rs` (7 literals, and the fixed `|| 500` loop clocks at
    `:1190`, `:1250`, `:1360`) — pass `staged_write_window_millis: 0` and doubles that drop the
    deadline. They move to `W_WRITE_MILLIS` and to doubles that refuse at or after the deadline
    on the context clock. A race hook that fires on a move's first commit
    (`segmented_map_repoint.rs`'s `Race::IntoCommit`, `:66`, `:142`, armed `:625`, and the
    segmented supersede regression among the reconstruction tests) now fires at the pre-mark;
    it must fire at the adoption, so each still shows an adoption losing to a supersede after
    the write. The DST rival in `crates/dst/tests/custodian.rs` pins what `repoint_chunk` pins
    (the root and the `seg:` record), not the segment alone.
  - **Docs and doc comments this change makes false:** `06-runtime-view.md:40-41`; GC's
    statement of which writers mark before they write and why the sweep is sound
    (`gc.rs:255-263`, and the premise at `:862-865` that a fragment lands at a committed
    record's position before that record commits, which an in-place write breaks), each naming
    #1100 where it is the exception; and no doc or audit text may say an in-place write sits
    under a pre-mark.
  / out of scope:
  - **#1100**: a fragment rebuilt in place, left unreferenced and unmarked when a competing
    move's adoption commits late. The code that writes in place carries `deferred: #1100`; a
    review finding that restates it is recorded-rejected in `review-rejected.md` naming #1100;
  - the staged re-place (it already keeps the rule; its own `move_event`, `staged.rs:805`,
    stays as it is) and #825's residuals (settling an aborted move's pre-marks; a write the D
    server answered `WriteEffect::Unknown`), which apply here equally and stay #825's;
  - production wiring of rebalance: the deployed loop passes `rebalance: None`
    (`crates/server/src/custodian.rs:591-599`, `:613-621`), and the evacuation legs use a
    library-level context;
  - backfill and every other caller of `commit_chunk_map` that does not move a fragment;
  - GC's behaviour, the cleanup and the marker (child-1); routing (child-3);
  - seeded DST for these interleavings (#682; the nonce's same-seed replay test is not an
    interleaving property and stays in scope); `Cargo.lock`, `deny*.toml`, 0016, any ADR;
  - the Tier-1/Tier-2 container scenarios (`tier1_disk_faults.rs`, the docker tiers): keep them
    compiling, list them under manual validation in `build-notes.md`, and declare neither
    `root + device-mapper` nor `docker` as a dependency (pdca-harness #969).
- **Repro instruction:** on `dd9fa05`: a committed RS(2,1) object over **four** D servers in four
  distinct failure domains, its fragments on servers 3, 4 and 5, server 2 free; put the legacy
  value `1000` at `orphan_key` for that fragment's position on server 2; mark server 5 draining
  and run `reconcile_step` with a library-level `Some(RebalanceContext)`; after the pass the
  inode's placement names server 2 and `orphan:2:<chunk>:<index>` still holds `1000`. (With
  only three servers the evacuation aborts: no free domain is left, `rebalance.rs:528-535`.)
- **Difficulty:** high
- **Do model:** opus
- **External dependencies:** `typos`, `docs-renderer`
- **Test file:** `crates/custodian/tests/move_destination_marks.rs`, a **NEW** file holding the
  repair legs (dry-run of `run-verify.sh --classify` on the expected file set, 2026-11-23:
  `ADDED_TEST crates/custodian/tests/move_destination_marks.rs`; crates `custodian`, `dst`,
  `server`). The evacuation legs are inline in `crates/custodian/src/rebalance.rs` (Success
  criterion says why); the nonce's replay test is in `crates/dst/tests/custodian.rs` (Scope,
  *Per-move identity*).
- **Verification posture:** default. The new file builds no `RebalanceContext` and uses nothing
  this child adds, so it compiles on the base and earns C4-verify's red there; the inline
  evacuation legs and D7–D10 get their red from the mutations below. **Demonstrated reds
  (recorded in `build-notes.md`, each mover):** destination mark left in place → (D1)/(D2); the
  destination's mark not required at the commit → (D5); a `reclaiming` destination accepted →
  (D4); a vacated mark written blind → (D4); writes authorized with no `W_REPOINT_MILLIS` check
  → (D7a); the refusal moved one millisecond late (`>=` → `>`, the peer's form at
  `staged.rs:680`) → (D7d); writes sent with no deadline (`None`) → (D7b); a refused write
  treated as landed → (D7c); the identity built from chunk and stamp alone → (D8); a refusal
  answered `Satisfied` → (D9); a pre-mark written at an in-place position → (D10a); the nonce
  drawn from a source that does not replay (a process-global counter, or the wall clock) → the
  replay test (Scope, *Per-move identity*; under `--cfg madsim`, run with `cargo xtask ci`).
- **Citations expected:** Do must cite `path:line` on the target branch for every change.
  - **The peer to mirror:** the staged re-place — choosing destinations
    (`choose_destinations`, `crates/custodian/src/reconstruction/staged.rs:406-460`); reading the
    destination's mark and refusing a `reclaiming` or undecodable one (`position`, `:557-576`);
    the pre-mark commit before any fragment write (`:634-656`, its event `move_event` `:805`);
    the write gate and store-enforced deadline (`:680-687`; `W_REPOINT_MILLIS` `gc.rs:218`,
    `W_WRITE_MILLIS` `gc.rs:202`); and the adoption that requires and deletes the pre-mark and
    marks the vacated position (`:721-751`).
  - **The two committed movers:** `repair_chunk` (`crates/custodian/src/reconstruction.rs:1103`,
    writes `:1226-1230`, vacated marks `:1232-1238`, commit `:1240`); `evacuate_chunk`
    (`crates/custodian/src/rebalance.rs:516`, copies `:634-640`, vacated marks `:642-647`,
    commit `:649`); the repoint they share (`metadata::repoint_chunk`,
    `crates/core/src/metadata.rs:3561`, its three pins `:3521-3532`).
  - **Reconstruction's verdict:** `reconstruction.rs:440-519` (the `unplaced` arm `:457`, the
    hole and verdict `:504-519`).
  - **The nonce's peer:** the caller's randomness behind a seam, as multipart Create takes it
    (`CreateRuntime::mint`, `crates/core/src/multipart/create.rs:72-76`); the constraints in
    Scope decide whether this child can take the same shape.
  - **GC's side:** reclaim intent before deletion (`record_intents`,
    `crates/custodian/src/gc.rs:768-800`); the no-mark arm that keeps an unmarked fragment
    (`:742`); the fragment-less sweep and the structured marks it keeps (`:891-940`,
    `event-may-await-write` `:928`); the late-write deadline (`:281`).
  - **Docs:** `docs/design/architecture/06-runtime-view.md:40` (Reconstruct) and `:41`
    (Rebalance) only.
- **Prior-art check (triage cycles):** by path, on `main` and #1013's head: `reconstruction.rs`
  (659ed31, 9599aad, 19cec1a, 5377850) and `rebalance.rs` (f481cd3, 0538720, c40948a): none
  touches a destination mark on the committed path; the staged path got the rule in 5377850
  (#826). Open: #825 (staged pre-mark settling, related, not overlapping); #1100 (the in-place
  race this child defers, filed 2026-11-23). Tracker searches by title and body for "in-place
  repair", "vacated mark", "rebuilt in place", "per-move nonce", "destination mark": no other
  open or closed issue covers this. Raised by #1014's iteration-2 adversary and reproduced
  through `metadata::repoint_chunk` by its iteration-3 reviewer.
- **Disposition hint:** likely-fix

## Docs currency

`AGENTS.md:169-172`: `06-runtime-view.md` steps 2 and 3 (`:40-41`) say the move marks its
destination before writing it (a fragment rebuilt in place excepted: its position stays named and
is never marked), sends the write only while that mark is young and with a deadline the D server
enforces, and clears the mark in the commit that adopts the fragment, so a mark never outlives
its event on a referenced position.

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a draft PR MAY
happen during the cycle (useful for CI feedback). The PR MUST NOT be marked ready before
sign-off accepts.

## Re-plan 2026-11-23 — what changed and why

The iteration-3 sign-off (iterate-plan) kept the fresh-destination design and asked Plan to
decide one question and state five answers. Decided with the human:

1. **In-place repair: option A.** It keeps iteration 3's handling (no mark at a named position,
   a stale one deleted, the write gated), and the late-adoption race is deferred to #1100. The
   two alternatives failed or cost too much. Round 2's pre-mark at a named position outlives a
   crash. Stamping the vacated mark at commit time is impossible: `MetadataStore` has no commit
   stamp, and 0016 rules out caller-side timing for when a commit applies (`0016:1304-1312`).
   Making the competing adoption conflict needs a key an in-place move would change: none exists
   on a segmented object, and `repair:<chunk>` is overwritten blindly by `enqueue_repair`. The
   race is older than this child (on `main` an in-place write has no deadline at all), and its
   worst case is bytes kept, never bytes lost.
2. **Per-move nonce** (Scope, D8). The recorded rejection in `review-rejected.md` (the two
   `same millisecond` entries, `premark.rs:515` and `:510`) is refuted by the iteration-3
   reviewer's probe: a delayed pre-mark committed under `require_absent` after GC deleted the
   first move's mark recreates its bytes. Delete both entries.
3. **Refusals never certify** (Scope, D9).
4. **Test fidelity:** the zero-window fixtures, the D5 release while `reclaiming` is stored,
   the race hooks at the adoption, and the DST rival's pins (Scope, D5).
5. **GC's comments** that this change makes false are corrected and name #1100 (Scope).
6. **The vacated mark** is written under a precondition (Scope, D4): iteration 3's batch review
   found it written blind.

Not split: the `alpha` intake cap is full (15/6), and the bundle is two splits deep.

**Plan review (2026-11-23), three findings, all taken into the brief.** (1) Repair's round trip
was not forced: committed repair leaves the lost server's domain eligible, so each repair pass
now gets a topology that leaves one free domain, and the placement is asserted before the marks
(Success criterion, D2). (2) D7a's clock jump could not tell `>=` from `>`: the new D7d tests the
exact `W_REPOINT_MILLIS` boundary and its one-millisecond-early twin, and D7a keeps the
missing-gate mutation (Verification posture). (3) `cargo xtask ci` does not check seed replay: a
same-seed replay test in `crates/dst/tests/custodian.rs` now decides whether a nonce source
qualifies (Scope, *Per-move identity*).

## Iteration 4 — carry-forward (from the previous attempt)
- Sign-off rationale: Keep the design (destination pre-mark + adoption + write gate; nonce and deadline held). Production logic is ~67 KB; the size backstop fires on test volume, so this is not a split. Fix only: - F1: a pre-mark or adoption that loses on the destination mark while the root is still the planned generation (`still_current`) must not answer `Satisfied` (`reconstruction.rs:1306/1336/482`). Match rebalance (non-certifying → `Blocked`). Capture and assert the verdict in repair D5 and D6 (`move_destination_marks.rs:1491/1532`), and add the pre-mark-conflict probe (GC swap between assessment and pre-mark). - F2: for both movers and both shapes, change the vacated mark between the adoption's read (`premark.rs:538`) and commit (`:559`), absent→reclaiming and stamped→reclaiming. Assert that the adoption loses and the bytes survive. - C5 survivors (`premark.rs:373`, `reconstruction.rs:379`): one leg with no free domain and no marks, asserting `Blocked` and no `repair-refused`, plus a `repair_blocked` gauge assertion. - DST rival (`crates/dst/tests/custodian.rs:6174-6181`): read and pin the vacated mark, as `PreMarked::adopt` does. No new features. Don't grow the patch beyond these.
- Sign-off session carry-forward (captured live, before §9 flattened it):
  Keep the design (destination pre-mark + adoption + write gate; nonce and deadline held). Production logic is ~67 KB; the size backstop fires on test volume, so this is not a split. Fix only:
  - F1: a pre-mark or adoption that loses on the destination mark while the root is still the planned generation (`still_current`) must not answer `Satisfied` (`reconstruction.rs:1306/1336/482`). Match rebalance (non-certifying → `Blocked`). Capture and assert the verdict in repair D5 and D6 (`move_destination_marks.rs:1491/1532`), and add the pre-mark-conflict probe (GC swap between assessment and pre-mark).
  - F2: for both movers and both shapes, change the vacated mark between the adoption's read (`premark.rs:538`) and commit (`:559`), absent→reclaiming and stamped→reclaiming. Assert that the adoption loses and the bytes survive.
  - C5 survivors (`premark.rs:373`, `reconstruction.rs:379`): one leg with no free domain and no marks, asserting `Blocked` and no `repair-refused`, plus a `repair_blocked` gauge assertion.
  - DST rival (`crates/dst/tests/custodian.rs:6174-6181`): read and pin the vacated mark, as `PreMarked::adopt` does.
  No new features. Don't grow the patch beyond these.
- Failing gate: C4 diff coverage: changed lines executed by the patch's tests (advisory) — diff coverage not measured — patch.diff does not apply on origin/main
- Failing gate: C5 surviving mutants on the bundle diff (cargo mutants --in-diff) (advisory) — 68 mutants tested in 12m: 2 missed, 49 caught, 14 unviable, 3 timeouts
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 10 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_1084/review
- Full previous attempt preserved in `iteration-v4/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 5 — carry-forward (from the previous attempt)
- Sign-off rationale: Keep the design (destination pre-mark + adoption + write gate; nonce and deadline). Not a split. Attempt 5's carry-forward fixes landed; fix only these, and don't grow the patch beyond them: - No-free-domain verdict: "asserting Blocked" meant the pass verdict. Make `Assessment::Blocked` non-certifying (`reconstruction.rs:377`: `repair_blocked += 1; refused = true`) so a capacity-blocked repair answers `Reconciled::Blocked`. Keep the capacity class (gauge reads 1, no `repair-refused` audit line). Change both `d4_no_free_domain` legs (`move_destination_marks.rs:1420`) to require `Blocked`, and update any existing test that asserted `Satisfied` for this case. - Evacuation doubles must enforce the write deadline: `MemDServer::put_fragment` in `crates/custodian/tests/rebalance.rs:132`, `segmented_map_evacuate.rs:181`, `segmented_map_rebalance.rs:105`. Share the context's ManualClock and refuse at or after the deadline, as the updated reconstruction double does (`tests/reconstruction.rs:114`). - Pre-mark pins test: one leg per shape where a supersede lands between the assessment and the pre-mark; assert no `put_fragment` is sent (`Fleet::sent_to` empty) and no pre-mark key is written. It must kill the `.take(0)` mutation on the pin fold (`premark.rs:461-467`). - DELETE/overwrite trigger for the #1100 class: a gateway unlink or flat overwrite stamped before the in-place batch and committed after it can strand an in-place fragment (bytes kept, never lost). Handle it as docs, not code: correct `gc.rs:263-269`, `premark.rs:37`, `:50-56`, `:60` and `06-runtime-view.md:40` so they name "any unreference whose mark is stamped before the in-place batch and commits after it" as the #1100 exception, and widen #1100 to match (note it in build-notes for the human to update the tracker).
- Sign-off session carry-forward (captured live, before §9 flattened it):
  Keep the design (destination pre-mark + adoption + write gate; nonce and deadline). Not a split. Attempt 5's carry-forward fixes landed; fix only these, and don't grow the patch beyond them:
  - No-free-domain verdict: "asserting Blocked" meant the pass verdict. Make `Assessment::Blocked` non-certifying (`reconstruction.rs:377`: `repair_blocked += 1; refused = true`) so a capacity-blocked repair answers `Reconciled::Blocked`. Keep the capacity class (gauge reads 1, no `repair-refused` audit line). Change both `d4_no_free_domain` legs (`move_destination_marks.rs:1420`) to require `Blocked`, and update any existing test that asserted `Satisfied` for this case.
  - Evacuation doubles must enforce the write deadline: `MemDServer::put_fragment` in `crates/custodian/tests/rebalance.rs:132`, `segmented_map_evacuate.rs:181`, `segmented_map_rebalance.rs:105`. Share the context's ManualClock and refuse at or after the deadline, as the updated reconstruction double does (`tests/reconstruction.rs:114`).
  - Pre-mark pins test: one leg per shape where a supersede lands between the assessment and the pre-mark; assert no `put_fragment` is sent (`Fleet::sent_to` empty) and no pre-mark key is written. It must kill the `.take(0)` mutation on the pin fold (`premark.rs:461-467`).
  - DELETE/overwrite trigger for the #1100 class: a gateway unlink or flat overwrite stamped before the in-place batch and committed after it can strand an in-place fragment (bytes kept, never lost). Handle it as docs, not code: correct `gc.rs:263-269`, `premark.rs:37`, `:50-56`, `:60` and `06-runtime-view.md:40` so they name "any unreference whose mark is stamped before the in-place batch and commits after it" as the #1100 exception, and widen #1100 to match (note it in build-notes for the human to update the tracker).
- Failing gate: C4 diff coverage: changed lines executed by the patch's tests (advisory) — diff coverage not measured — patch.diff does not apply on origin/main
- Failing gate: C5 surviving mutants on the bundle diff (cargo mutants --in-diff) (advisory) — 69 mutants tested in 12m: 51 caught, 15 unviable, 3 timeouts
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 9 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_1084/review-
- Full previous attempt preserved in `iteration-v5/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 6 — carry-forward (from the previous attempt)
- Sign-off rationale: Keep the design (destination pre-mark + gated write with deadline + adoption consuming the pre-mark; per-move nonce). Not a split: production logic is ~67 KB, the size backstop fires on test volume (as ruled in rounds 4 and 5). Fix only these, and don't grow the patch beyond them: - F1 (all three reviewers): a lost move must not certify when the root moved but this chunk did not. `lost()` (`reconstruction.rs:1368-1378`) maps any root change to `RepairOutcome::Conflict`, which leaves `refused` false and lets the pass answer `Satisfied` with the fragment still missing and the obligation queued. Make that case non-certifying (answer `Blocked`, as evacuation does for the same race, `rebalance.rs:203`), or re-resolve the chunk and certify only if the current reference no longer owes the repair. A changed root byte string alone is not enough. Change the D6 test (`move_destination_marks.rs:1704-1708`) to require the non-`Satisfied` verdict, and cover both phases (before pre-mark, before adoption) and both map shapes, as the reviewer probes do. - `seg:` pin regression guard (adversary): no test fails if the pre-mark batch pins only the root (`premark.rs:465-471`). On the segmented shape, add one leg per mover that arms `premarks_free` with `Act::PlantFirst` of a `seg:`-only rewrite (root unchanged), then asserts server 2 was sent no write and holds no mark. - Doc fix only: `Assessment::Refused`'s comment "It clears when GC finishes the reclaim" (`reconstruction.rs:802-803`) is false for a `reclaiming` mark on a still-referenced position (GC never finishes it there). Correct the comment to say so and point at #1083; do not change the refusal behaviour. - Optional, if cheap: one mixed-result leg per mover (one write lands, one refused) asserting the whole move refuses (`premark.rs:600`).
- Sign-off session carry-forward (captured live, before §9 flattened it):
  Keep the design (destination pre-mark + gated write with deadline + adoption consuming the pre-mark; per-move nonce). Not a split: production logic is ~67 KB, the size backstop fires on test volume (as ruled in rounds 4 and 5). Fix only these, and don't grow the patch beyond them:
  - F1 (all three reviewers): a lost move must not certify when the root moved but this chunk did not. `lost()` (`reconstruction.rs:1368-1378`) maps any root change to `RepairOutcome::Conflict`, which leaves `refused` false and lets the pass answer `Satisfied` with the fragment still missing and the obligation queued. Make that case non-certifying (answer `Blocked`, as evacuation does for the same race, `rebalance.rs:203`), or re-resolve the chunk and certify only if the current reference no longer owes the repair. A changed root byte string alone is not enough. Change the D6 test (`move_destination_marks.rs:1704-1708`) to require the non-`Satisfied` verdict, and cover both phases (before pre-mark, before adoption) and both map shapes, as the reviewer probes do.
  - `seg:` pin regression guard (adversary): no test fails if the pre-mark batch pins only the root (`premark.rs:465-471`). On the segmented shape, add one leg per mover that arms `premarks_free` with `Act::PlantFirst` of a `seg:`-only rewrite (root unchanged), then asserts server 2 was sent no write and holds no mark.
  - Doc fix only: `Assessment::Refused`'s comment "It clears when GC finishes the reclaim" (`reconstruction.rs:802-803`) is false for a `reclaiming` mark on a still-referenced position (GC never finishes it there). Correct the comment to say so and point at #1083; do not change the refusal behaviour.
  - Optional, if cheap: one mixed-result leg per mover (one write lands, one refused) asserting the whole move refuses (`premark.rs:600`).
- Failing gate: C4 diff coverage: changed lines executed by the patch's tests (advisory) — diff coverage not measured — patch.diff does not apply on origin/main
- Failing gate: C5 surviving mutants on the bundle diff (cargo mutants --in-diff) (advisory) — 71 mutants tested in 12m: 53 caught, 15 unviable, 3 timeouts
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 3 blocking, 4 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_1084/review-
- Full previous attempt preserved in `iteration-v6/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).
