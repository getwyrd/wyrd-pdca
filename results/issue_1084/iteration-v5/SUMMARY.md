# Result — issue 1084 / committed-move-destination-premark

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect: The committed-path moves — reconstruction's `repair_chunk`
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
- Success criterion: both movers, each on a flat and on a segmented committed object, driven
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
- Repo + branch target: getwyrd/wyrd @ main
- Scope: the committed-path repair and evacuation moves keep 0016 X47's destination rule and
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

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: likely-fix
- Confidence: medium
- Recommendation: (set by Do)

## 3. Correctness (Check — chain)
- C1 Spec: none — brief.md
- C2 Reproduction (red pre-fix): none — (no gate configured)
- C3 Change: none — patch.diff
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): pass — xtask ci: all checks passed
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass — run-verify.sh: PASS — red without the fix, green with it (48 test(s) ran red).
- C4 diff coverage: changed lines executed by the patch's tests: fail — diff coverage not measured — patch.diff does not apply on origin/main
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): fail — 69 mutants tested in 12m: 51 caught, 15 unviable, 3 timeouts

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 9 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_1084/review-
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 13.72s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Review #1084: make committed repair and evacuation preserve destination orphan evidence, bound fragment writes, and avoid certifying unfinished repairs.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The brief defines observable flat/segmented outcomes, the prerequisite retirement drain, and explicit exclusions; the supplied target contains the prerequisite (`brief.md:79`, `brief.md:240`, `crates/custodian/src/reconciliation.rs:187`, `crates/custodian/Cargo.toml:65`). |
| C2 Reproduction (red pre-fix) | PASS | Independently stashing the tracked fix leaves the added discriminator compiling: 44 assertion failures and 4 passes, including stale marks and early reclamation (`review-red.log:124`, `review-red.log:140`, `review-red.log:353`). |
| C3 Change | PASS | Both committed movers share the destination protocol and update their context users and living runtime description within the approved scope (`crates/custodian/src/reconstruction.rs:1310`, `crates/custodian/src/rebalance.rs:677`, `docs/design/architecture/06-runtime-view.md:40`). |
| C4 Verification (red→green) | PASS | Restoring the identical patch yields 48/48 discriminator passes, 520 custodian test passes, and a passing actual madsim nonce replay; full CI/TiKV are additionally supported by frozen logs, while diff coverage remains unmeasured (`review-green.log:196`, `review-custodian.log:83`, `review-dst-replay.log:243`, `gate-logs/C4-diff-cov.log:11`). |
| C5 Causal adequacy | NEEDS-HUMAN [impl] | Correct capacity-blocked certification as the carry-forward requires — both shapes still answer `Satisfied` with a missing fragment and queued repair, and the new test enshrines that answer (`brief.md:379`, `crates/custodian/tests/move_destination_marks.rs:1420`, `crates/custodian/src/reconstruction.rs:539`, `review-probes.log:6`). |
| T1 Structure | PASS | The shared protocol stays behind trait seams with instance-owned state; the independent statics scan passes and no new dependency or crate is introduced (`crates/custodian/src/premark.rs:182`, `crates/custodian/src/premark.rs:342`, `review-statics.log:3`). |
| T2 Shape | PASS | Position admission, matching, pre-marking, authorization and adoption have distinct responsibilities; staged placement consumes the same matcher and runtime documentation accompanies the changed context contract (`crates/custodian/src/premark.rs:222`, `crates/custodian/src/reconstruction/staged.rs:317`, `crates/custodian/src/rebalance.rs:98`). |
| T3 Runtime | PASS | The normal protocol uses one context clock for stamps and deadlines, finite destination search, and joined writes without spawned helpers; in-process deadline and GC races pass, with the remaining certification defect recorded under C5 (`crates/custodian/src/premark.rs:219`, `crates/custodian/src/premark.rs:459`, `crates/custodian/src/premark.rs:505`, `crates/custodian/src/reconstruction.rs:1327`, `review-custodian.log:46`). |
| T4 Contribution | FAIL | Two batch-review test-fidelity findings remain grounded and fixable (F2 below); prior-art was checked by mover path, while the contribution-artifact gate is separately N/A until its mandatory publish rerun (`gate-logs/T4-batch-review.log:10`, `crates/custodian/tests/rebalance.rs:128`, `crates/custodian/tests/segmented_map_evacuate.rs:181`, `gate-logs/T4-contribution.log:10`). |
| T5 Judgment | NEEDS-HUMAN [impl] | Make the touched evacuation doubles enforce the destination deadline — both accept and store a write with deadline zero, weakening the suite's production-seam fidelity despite correct dedicated D7 coverage (`AGENTS.md:203`, `crates/custodian/tests/rebalance.rs:132`, `crates/custodian/tests/segmented_map_evacuate.rs:181`, `review-probes.log:54`, `review-probes.log:75`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether the demonstrated in-process repair/GC behavior is sufficient for this slice and schedule the privileged durability follow-up — real disk-fault and Docker kill/reconstruct outcomes were not exercised, and deployed rebalance wiring remains explicitly excluded (`brief.md:247`, `brief.md:255`, `review-custodian.log:730`, `AGENTS.md:97`). |

The destination pre-mark fix has a reproduced red→green, but two implementation concerns remain: the explicitly requested no-capacity verdict is inverted, and two updated evacuation fixtures still discard deadlines. This report is advisory; it does not change the deterministic gates or authorize acceptance.

Source citations beginning `crates/`, `docs/`, `.github/`, or `AGENTS.md` resolve under `$PDCA_TARGET`, the supplied `target/` directory. Evidence logs and `brief.md` resolve in the reviewer cwd. No builder notes were read. The target's tracked diff was restored byte-for-byte after the red leg; reviewer probes live separately in `pdca-reviewer-1084-probes/` and do not alter the judged source.

**F1 — unfinished capacity-blocked repairs still certify.** The carry-forward explicitly requests a no-free-domain leg asserting `Blocked` and no `repair-refused` (`brief.md:379`). Instead, the new regression asserts `Reconciled::Satisfied` (`crates/custodian/tests/move_destination_marks.rs:1420`). `Assessment::Blocked` increments only the gauge (`crates/custodian/src/reconstruction.rs:377`); it does not affect the final certification decision at `:539`. An independent probe using the supplied real-redb fixture and `reconcile_step` observed, for both flat and segmented objects, `verdict=Satisfied, owed=true, blocked=Some("1"), placement=[3, 4, 5]` after fragment 2 on server 5 was lost and only servers 3 and 4 remained (`review-probes.log:6`). The behavior predates this patch, but the approved carry-forward expressly includes correcting/asserting this case. Preserve its capacity classification and absence of `repair-refused`, make the pass non-certifying, and change both new regressions to require `Blocked`.

**F2 — two evacuation fixtures accept writes a D server must refuse.** The modified callers now supply a clock and write window, but their stores still discard the deadline: `crates/custodian/tests/rebalance.rs:128` and `crates/custodian/tests/segmented_map_evacuate.rs:181`. Independent copies of each fixture accepted `put_fragment(..., Some(0))` and stored the bytes (`review-probes.log:54`, `review-probes.log:75`). That deadline is expired at every legal unsigned clock reading: the production seam compares `now >= deadline` (`crates/traits/src/lib.rs:925`), and the filesystem adapter calls it (`crates/chunkstore-fs/src/lib.rs:286`). Give these doubles the context's clock and the same refusal/unverified-publication semantics already used in the updated repair fixtures (`crates/custodian/tests/reconstruction.rs:114`). This is a test-model defect, not evidence that the production deadline implementation failed. The new inline evacuation D7 tests do enforce deadlines and passed independently (`crates/custodian/src/rebalance.rs:1524`, `review-custodian.log:52`).

The frozen gate evidence supports the following bounded conclusions:

| Gate | Adjudication | Evidence |
|------|--------------|----------|
| C4-ci | PASS, log-backed for the full workspace; relevant tests and scanners independently rerun | The log includes typos, successful docs rendering/link audit, fmt, workspace clippy/build/tests, dependency checks and madsim, ending in success (`gate-logs/C4-ci.log:11`, `:18`, `:4507`, `:4780`, `:5170`). Independent custodian tests, clippy, fmt, typos, docs lint/render and statics all passed. |
| C4-verify | PASS, independently reproduced | 48 tests ran on each leg; **44**, not 48, failed without the fix. All 48 pass with it (`review-red.log:353`, `review-green.log:196`). The subsequent full custodian run rechecked the restored tree: 520 passed, one privileged test ignored. |
| C4-diff-cov | No measurement; base-state caveat, not a confirmed patch defect | The wrapper stopped because the patch did not apply to `origin/main`, before measuring coverage (`gate-logs/C4-diff-cov.log:10`). The brief requires a stacked prerequisite (`brief.md:10`); the supplied snapshot contains it and independently compiles. This does not establish a compile failure or a coverage percentage. |
| C5-mutants | Incomplete mutation result, not surviving-mutant evidence | The full log reports 69 mutations: 51 caught, 15 unviable, three 97-second timeouts in matching; no missed mutant is reported (`gate-logs/C5-mutants.log:10`). These timeouts neither prove a production hang nor establish that every mutation was killed. |
| T4-batch-review | FAIL as frozen; two test findings retained, other classes dispositioned below | The log provides nine concerns, including duplicate classes (`gate-logs/T4-batch-review.log:10`); it does not demonstrate nine independent defects. |
| T4-contribution | N/A | The PR description is deliberately absent at Check; the substantive audit must rerun at publish (`gate-logs/T4-contribution.log:8`). No human clearance is owed for this deferral. |
| host-tikv | PASS, log-backed | The log shows actual compilation/checking of TiKV dependencies, custodian and server feature arms, with exit zero (`gate-logs/host-tikv.log:2`, `:7`, `:204`). This is compile evidence, not live TiKV service testing. |

The instance-scoped wrapper paths were not treated as missing target files. Their captured logs were used for the full CI, coverage, mutation and batch-review runs; no unavailable-log escalation is needed. Both declared external dependencies, typos and the documentation renderer, were actually exercised independently (`review-docs-render.log:2`); no alias, shim, or code-read substitutes for those checks.

The batch review's other concerns are settled or outside this slice's declared change:

- **In-place late writes (two reports): not reopened.** The in-place ordering exposure is explicitly accepted under #1100 (`brief.md:240`, `crates/custodian/src/premark.rs:50`); legacy unlink/supersede routing belongs to child #1085 (`brief.md:251`). The reviewed [#1100 deferral](https://github.com/getwyrd/wyrd/issues/1100) describes the delayed-mark/late-write cause. These reports do not justify reintroducing the rejected in-place pre-mark or changing the approved scope.
- **Destination drain fence: reproduced, but tracked scope exclusion.** Planting `desired:dserver:2=draining` immediately before adoption still gives `Changed` and placement `[3,4,2]` on both shapes (`review-probes.log:42`; the adoption at `crates/custodian/src/premark.rs:546` pins the orphan, not the desired-state key). Proposal 0016 requires that fence (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:670`), but the path-based prior-art investigation found the explicit deferral to #653 in [#722's scope](https://github.com/getwyrd/wyrd/issues/722), inherited from [#682](https://github.com/getwyrd/wyrd/issues/682). Under `AGENTS.md:215`, this is not a new in-PR fix request.
- **Retaining a vacated `reclaiming` mark: not a defect against this brief.** Preserving and pinning those exact bytes is an explicit requirement (`brief.md:210`, `crates/custodian/src/premark.rs:556`); overwriting the mark would violate the GC reclaim-intent boundary. The new absent/stamped-to-reclaiming races pass for both movers and both shapes (`crates/custodian/tests/move_destination_marks.rs:1501`, `crates/custodian/src/rebalance.rs:2283`).
- **Topology-only candidate (two reports): reproduced, but the abort is expressly retained.** With server 2 reclaiming, topology-only server 6 and usable server 7, the probe sees `Satisfied`, a queued repair, and no write to 7 (`review-probes.log:30`). The brief explicitly acknowledges aborting on a destination outside the fleet (`brief.md:70`), and the inherited abort/accounting behavior is covered by `crates/custodian/tests/reconstruction.rs:1951` from [#238](https://github.com/getwyrd/wyrd/pull/238). The deployed loop builds fleet and topology together (`crates/server/src/custodian.rs:293`, `:592`). Do not silently expand this child into a fleet-selection redesign; F1 is separately included because its no-capacity outcome is explicitly required by the carry-forward.
- **Seeded partial-fan-out coverage: settled under #682.** The brief excludes the new interleaving campaign, and both movers carry the tracking marker (`brief.md:253`, `crates/custodian/src/reconstruction.rs:1253`, `crates/custodian/src/rebalance.rs:671`). The required nonce replay is a distinct obligation and passed under actual `--cfg madsim` (`review-dst-replay.log:243`).

The C5 symptom-guard smell-test does not fire: mark classification and write authorization implement the storage protocol's own admission and time bounds; they do not probe an optional capability to mask eager initialization. The repaired pre-mark/adoption-conflict paths now refuse certification while the root remains current (`crates/custodian/src/reconstruction.rs:1361`), and the revised DST rival pins its vacated mark (`crates/dst/tests/custodian.rs:6187`).

Prior-art was independently checked by affected production path using GitHub's commit collection with `path=crates/custodian/src/reconstruction.rs` and `path=crates/custodian/src/rebalance.rs`, plus closed-PR and closed-unmerged searches for those paths. The returned history matches the brief's anchors: reconstruction `659ed31`, `9599aad`, staged predecessor `5377850`; rebalance `f481cd3`, `0538720`, `c40948a`. [#826](https://github.com/getwyrd/wyrd/pull/826) supplies the staged precedent; [#850](https://github.com/getwyrd/wyrd/pull/850) and [#884](https://github.com/getwyrd/wyrd/pull/884) supply committed repoint callers. Closed-unmerged [#647](https://github.com/getwyrd/wyrd/pull/647) is the larger ancestor, whose size rejection and scoped successors are recorded in [#722](https://github.com/getwyrd/wyrd/issues/722). No returned prior work supersedes this destination-mark fix. No other checkout was consulted.

For fitness-to-purpose sign-off, the observed result is 520 passing in-process custodian tests, including 36 inline evacuation tests, plus the same-seed replay. The repair discriminator is executable with `cargo test -p wyrd-custodian --test move_destination_marks`; the nonce check is `RUSTFLAGS='--cfg madsim' cargo test -p wyrd-dst --test custodian a_committed_moves_pre_mark_nonce_replays_under_its_seed`. On the appropriate privileged host, the declared follow-ups are `WYRD_TIER1=1 cargo xtask disk-faults` and `WYRD_TIER2=1 cargo xtask kill-reconstruct` (commands grounded in `.github/workflows/tier1-disk-faults.yml:19`, `.github/workflows/tier2-kill-reconstruct.yml:25`, `xtask/src/main.rs:145`). Confirm that faulted-fragment reconstruction restores redundancy and reads, without dangling placements or prematurely reclaimed bytes. These are manual follow-ups explicitly excluded from this Check's dependency set, not retroactive missing-tool gate failures.

<!-- pdca:leaf-complete -->

### Advisory — adversary

# Adversarial review — issue 1084 (committed-move destination pre-mark)

What I re-ran, on a scratch copy of the patched tree at `$PDCA_TARGET`: the new test file
(48 tests), rebalance's inline evacuation legs (36 tests), and 17 hand-made mutations of the fix,
plus one scratch-only repro test. `cargo` was available, so nothing below is provisional on
tooling.

## Refutations

- NEEDS-HUMAN [human] — **An in-place repair can still be stranded by a gateway DELETE or flat overwrite, which is not the trigger the #1100 deferral names.** The new docs say the only unreference that can mark an in-place position earlier than the pre-mark batch is "a competing move of the same fragment whose adoption was built before that batch" (`crates/custodian/src/gc.rs:263-269`, `crates/custodian/src/premark.rs:37`, `:50-56`, `:60`). They also say a lost repair leaves what it wrote "under its pre-mark ... or on a position the placement still names" (`docs/design/architecture/06-runtime-view.md:40`). That is false for `metadata::unlink`. Unlink writes blind legacy marks stamped with the caller's clock (`crates/core/src/metadata.rs:2186-2189`), and the gateway reads `now_millis()` before unlink's two reads and its commit (`crates/server/src/lib.rs:705`; the overwrite does the same at `:315`). The in-place batch does not touch the root, so an unlink that read before the batch still commits after it. **Concrete failing case** (scratch test modelled on D10b, flat shape, in-place repair on server 5, write held): the DELETE is stamped at batch − 12 s and commits after the batch. GC runs at batch + 29.5 s, finds the position empty and the legacy mark at least `D` = 41 s old, and sweeps it (`gc.rs:913-929`). The write is then released, and the D server accepts it because it is before its deadline at batch + 30 s. Result: the fragment is on server 5, there is no inode and no mark, and the pass answered `Satisfied`. This needs the gateway to stall for more than `D − W_write` = 11 s between reading its clock and committing, and the worst case is bytes kept for ever, never bytes lost: the same class as #1100. T4 raised this twice (`premark.rs:480`). It is a new trigger, not a restatement of #1100, so rejecting it as "restates #1100" would be wrong. Decide whether to widen #1100 to "any unreference whose mark is stamped before the in-place batch and commits after it" and correct the three doc passages above, or to handle it in code.
- NEEDS-HUMAN [impl] — **A mutation of the pre-mark's pins survives every relevant test.** I dropped the repoint's preconditions (the root generation and the `seg:` record) from the pre-mark batch: `.take(0)` on the fold at `crates/custodian/src/premark.rs:461-467`. All of these still pass: `move_destination_marks` (48), `--lib rebalance` (36), `segmented_map_repoint`, `reconstruction`, `segmented_map_reconstruction` and `staged_protection`. The doc claims these pins mean "a move whose record has already moved writes nothing at all" (`premark.rs:447-451`). The brief's Scope also requires the in-place stale-mark delete to run "under its exact bytes and the repoint's pins". I found no safety failure without the pins: the per-position mark pin, the adoption's own root pin, and the drain's exact-bytes re-stamp (`crates/custodian/src/retirement.rs:743-749`) cover every interleaving I tried. So this is a missing test, not a live bug. Fix: add one leg per shape where a supersede lands between the assessment and the pre-mark, and assert that no `put_fragment` is sent (`Fleet::sent_to` is empty) and no pre-mark key is written.

## Claims I tried to refute and could not

- **Red→green evidence holds, and the mutation reds the brief relies on are real.** C4-verify's red leg has 44 of 48 failing. The 4 that pass on the base are the two "no free domain" controls and the two D10a crash legs. Some legs are red on the base only because a setup `expect` fails (D7c and D9 at `crates/custodian/tests/move_destination_marks.rs:1031`; D7a, D7b and D7d at `:1780`, `:1806`, `:1885`). So I checked each property with a targeted mutation, and every one was caught on **both** movers and both shapes:
  - nonce made constant (D8);
  - adoption no longer requires the destination's pre-mark (D5, D8);
  - `W_repoint` gate removed (D7a, D7d);
  - `<` changed to `<=` at `premark.rs:505` (D7d);
  - a deadline refusal treated as landed (D7b, D7c, D10b);
  - writes sent with `None` instead of a deadline, separately in `reconstruction.rs:1329` and `rebalance.rs:694` (D7b, D7c, D9 / the unverified-copy leg);
  - `Stopped` no longer withholding certification (D4, D5, D7, D8, D9, D10b);
  - `lost()` always answering `Conflict` (the F1 legs);
  - the vacated mark written blind, in each of the `Kept`, `Absent` and `Stamped` arms (D4, F2);
  - a pre-mark written at an in-place position (D1, D10a);
  - a stale in-place mark kept (D1, D10a);
  - evacuation ignoring a lost adoption (D4, D5, D6, D8);
  - a stale evacuation pre-mark counted as a move (D7a, D7d).
- **The nonce replays under madsim.** `a_committed_moves_pre_mark_nonce_replays_under_its_seed` passed in the C4-ci DST leg, and it compares the full pre-mark bytes, nonces included, across two fresh-thread runtimes (`crates/dst/tests/custodian.rs:6869-6919`). A source madsim does not seed would make it fail. I did not re-run its madsim mutations. One brief claim is doubtful: madsim virtualises `SystemTime`, so a "wall clock" nonce would probably replay and *not* turn this test red. D8 still catches a stamp-derived identity.
- **The verdict's root check matches the repoint's pin.** `still_current` compares `encode(prior)` (`crates/custodian/src/reconstruction.rs:1377-1380`), the same bytes `repoint_chunk` pins (`crates/core/src/metadata.rs:3523`), so a root encoding mismatch cannot turn an F1 refusal into a certifying `Conflict`.
- **A batch with only preconditions still checks them.** An in-place repair over an absent mark commits a pre-mark batch with no writes. TiKV evaluates every precondition inside the transaction regardless (`crates/metadata-tikv/src/lib.rs:1386-1406`), and redb passes D10.
- **T4's "matching admits servers absent from the fleet" (`premark.rs:386`) cannot happen in the deployed wiring.** The deployed loop builds the fleet and the topology from the same reachable set (`crates/server/src/custodian.rs:304-305`). Only a library caller that passes a topology wider than its fleet reaches it, and that caller gets the base's `unplaced` abort. That is a sound reason to reject the finding.
- **T4's "vacated `reclaiming` mark left in place" (`premark.rs:556`)** argues against the brief's D4 requirement (left byte for byte, ADR-0045), not against the code. It is a scope question for the brief, not a defect in the fix.
- **Red gates that are not evidence against the fix.** C4-diff-cov's failure is a stale-base artifact: the patch targets `main` plus #1013, so it does not apply on `origin/main`. The C5 run's 3 timeouts in `choose`/`assign`/`augment` are hangs, which means the mutants were effectively caught. They were not missed.

<!-- pdca:leaf-complete -->

### Advisory — code-review

- NEEDS-HUMAN [impl] — **Evacuation fixtures still ignore the new write deadline.** The contexts updated at `crates/custodian/tests/rebalance.rs:863`, `crates/custodian/tests/segmented_map_evacuate.rs:284`, and `crates/custodian/tests/segmented_map_rebalance.rs:206` now drive deadline-bearing copies, but their `MemDServer::put_fragment` implementations discard that deadline (`rebalance.rs:132`, `segmented_map_evacuate.rs:181`, `segmented_map_rebalance.rs:105`, all under `crates/custodian/tests/`). Consequently, even a zero write window permits successful evacuation in these fixtures, whereas the production store rejects the write at its deadline. Share each context's `ManualClock` with its destination stores and enforce the pre-write and post-publication deadline checks, as the updated reconstruction double does. This is a test-fidelity gap in the newly exercised contract; the separate D7 tests do enforce deadlines.

No additional introduced production-correctness bugs or worthwhile reuse/simplification findings identified within the stated scope. Reviewed target source and frozen gate evidence; no tests were rerun.

<!-- pdca:leaf-complete -->

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C5 Causal adequacy — Correct capacity-blocked certification as the carry-forward requires — both shapes still answer `Satisfied` with a missing fragment and queued repair, and the new test enshrines that answer (`brief.md:379`, `crates/custodian/tests/move_destination_marks.rs:1420`, `crates/custodian/src/reconstruction.rs:539`, `review-probes.log:6`).
- [ ] T5 Judgment — Make the touched evacuation doubles enforce the destination deadline — both accept and store a write with deadline zero, weakening the suite's production-seam fidelity despite correct dedicated D7 coverage (`AGENTS.md:203`, `crates/custodian/tests/rebalance.rs:132`, `crates/custodian/tests/segmented_map_evacuate.rs:181`, `review-probes.log:54`, `review-probes.log:75`).
- [ ] Validation — fitness-to-purpose — Decide whether the demonstrated in-process repair/GC behavior is sufficient for this slice and schedule the privileged durability follow-up — real disk-fault and Docker kill/reconstruct outcomes were not exercised, and deployed rebalance wiring remains explicitly excluded (`brief.md:247`, `brief.md:255`, `review-custodian.log:730`, `AGENTS.md:97`).
- [ ] **An in-place repair can still be stranded by a gateway DELETE or flat overwrite, which is not the trigger the #1100 deferral names.** The new docs say the only unreference that can mark an in-place position earlier than the pre-mark batch is "a competing move of the same fragment whose adoption was built before that batch" (`crates/custodian/src/gc.rs:263-269`, `crates/custodian/src/premark.rs:37`, `:50-56`, `:60`). They also say a lost repair leaves what it wrote "under its pre-mark ... or on a position the placement still names" (`docs/design/architecture/06-runtime-view.md:40`). That is false for `metadata::unlink`. Unlink writes blind legacy marks stamped with the caller's clock (`crates/core/src/metadata.rs:2186-2189`), and the gateway reads `now_millis()` before unlink's two reads and its commit (`crates/server/src/lib.rs:705`; the overwrite does the same at `:315`). The in-place batch does not touch the root, so an unlink that read before the batch still commits after it. **Concrete failing case** (scratch test modelled on D10b, flat shape, in-place repair on server 5, write held): the DELETE is stamped at batch − 12 s and commits after the batch. GC runs at batch + 29.5 s, finds the position empty and the legacy mark at least `D` = 41 s old, and sweeps it (`gc.rs:913-929`). The write is then released, and the D server accepts it because it is before its deadline at batch + 30 s. Result: the fragment is on server 5, there is no inode and no mark, and the pass answered `Satisfied`. This needs the gateway to stall for more than `D − W_write` = 11 s between reading its clock and committing, and the worst case is bytes kept for ever, never bytes lost: the same class as #1100. T4 raised this twice (`premark.rs:480`). It is a new trigger, not a restatement of #1100, so rejecting it as "restates #1100" would be wrong. Decide whether to widen #1100 to "any unreference whose mark is stamped before the in-place batch and commits after it" and correct the three doc passages above, or to handle it in code.
- [ ] **A mutation of the pre-mark's pins survives every relevant test.** I dropped the repoint's preconditions (the root generation and the `seg:` record) from the pre-mark batch: `.take(0)` on the fold at `crates/custodian/src/premark.rs:461-467`. All of these still pass: `move_destination_marks` (48), `--lib rebalance` (36), `segmented_map_repoint`, `reconstruction`, `segmented_map_reconstruction` and `staged_protection`. The doc claims these pins mean "a move whose record has already moved writes nothing at all" (`premark.rs:447-451`). The brief's Scope also requires the in-place stale-mark delete to run "under its exact bytes and the repoint's pins". I found no safety failure without the pins: the per-position mark pin, the adoption's own root pin, and the drain's exact-bytes re-stamp (`crates/custodian/src/retirement.rs:743-749`) cover every interleaving I tried. So this is a missing test, not a live bug. Fix: add one leg per shape where a supersede lands between the assessment and the pre-mark, and assert that no `put_fragment` is sent (`Fleet::sent_to` is empty) and no pre-mark key is written.
- [ ] **Evacuation fixtures still ignore the new write deadline.** The contexts updated at `crates/custodian/tests/rebalance.rs:863`, `crates/custodian/tests/segmented_map_evacuate.rs:284`, and `crates/custodian/tests/segmented_map_rebalance.rs:206` now drive deadline-bearing copies, but their `MemDServer::put_fragment` implementations discard that deadline (`rebalance.rs:132`, `segmented_map_evacuate.rs:181`, `segmented_map_rebalance.rs:105`, all under `crates/custodian/tests/`). Consequently, even a zero write window permits successful evacuation in these fixtures, whereas the production store rejects the write at its deadline. Share each context's `ManualClock` with its destination stores and enforce the pre-write and post-publication deadline checks, as the updated reconstruction double does. This is a test-fidelity gap in the newly exercised contract; the separate D7 tests do enforce deadlines.
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 9 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_1084/review-
- [ ] **The repair round trip is not forced by the stated setup.** `brief.md:63-76` says repair is forced like evacuation and prescribes “lose it on 5, then on 2.” On the target, evacuation excludes draining servers (`crates/custodian/src/rebalance.rs:528`), but committed repair excludes only survivors’ domains (`crates/custodian/src/reconstruction.rs:1122-1126`). Both servers 2 and 5 therefore remain eligible after either loss; the selector orders them by utilization and domain label (`crates/core/src/placement.rs:289-301`). With unchanged topology/utilization, a first selection of 2 is followed by an in-place repair on 2, not the required return to 5. Specify the per-pass topology/utilization or drain-state changes that force each direction, and require placement assertions before the mark assertions. Otherwise D2 fails for fixture reasons even after the destination-mark fix.
- [ ] **D7a cannot detect its promised boundary mutation.** `brief.md:91-94` advances the clock past both the write-authorization window and the pre-mark’s grace, while `brief.md:246-247` requires this leg to kill `<` → `<=`. Both comparisons refuse a write at the prescribed late instant; they differ only at exactly `stamp + W_REPOINT_MILLIS`. The target’s peer rejects equality (`crates/custodian/src/reconstruction/staged.rs:680`), and its constants leave room to exercise that boundary before the D-server deadline (`crates/custodian/src/gc.rs:202,218`: 30,000 versus 10,000 ms). Add a distinct exact-boundary arm asserting zero destination calls and a retained obligation; keep the post-GC arm for the missing-gate mutation.
- [ ] **The nonce’s replay requirement lacks the claimed verification.** `brief.md:167-171` proposes OS-seeded `RandomState` while requiring deterministic seed replay, checked with `cargo xtask ci`. The target’s DST runner sets `MADSIM_TEST_NUM=50` and runs the tests (`xtask/src/main.rs:1811-1817`); it does not enable the separate `MADSIM_TEST_CHECK_DETERMINISM` option documented at `crates/dst/src/lib.rs:90-91`. D8 checks distinct identities within one execution, not reproducibility across executions. Require an explicit repeated-same-seed check that observes the minted pre-mark identities and relevant outcomes, and state its command/assertion. A green seed sweep alone cannot establish the proposed nonce source satisfies the replay constraint.
- [ ] size backstop — this slice is behaving oversized: patch is 408 KB (threshold 100 KB); patch touches 24 files (threshold 20). Recommend answering `iterate-plan` at sign-off and authoring the split in the re-plan (`pdca split`), rather than `iterate-do`: a slice that is too big yields implementation-shaped findings every round, and splitting authors briefs, which is Plan's beat.

## 7. Proven / not proven
- Proven by which oracle: gates overall = fail (stub oracles).
- Unproven / needs manual run: anything flagged in §6.

## 8. Ready-to-ship attachments
- patch.diff
- tracker-comment.md     (ALWAYS, every tracker item)
- build-notes.md         (builder rationale — for the human, not the reviewer)

## 9. Check sign-off                     ← human completes Check here
- Disposition confirmed / overridden:
- Outcome: iterated-to-Do
- Iteration delta (if iterating): Keep the design (destination pre-mark + adoption + write gate; nonce and deadline). Not a split. Attempt 5's carry-forward fixes landed; fix only these, and don't grow the patch beyond them: - No-free-domain verdict: "asserting Blocked" meant the pass verdict. Make `Assessment::Blocked` non-certifying (`reconstruction.rs:377`: `repair_blocked += 1; refused = true`) so a capacity-blocked repair answers `Reconciled::Blocked`. Keep the capacity class (gauge reads 1, no `repair-refused` audit line). Change both `d4_no_free_domain` legs (`move_destination_marks.rs:1420`) to require `Blocked`, and update any existing test that asserted `Satisfied` for this case. - Evacuation doubles must enforce the write deadline: `MemDServer::put_fragment` in `crates/custodian/tests/rebalance.rs:132`, `segmented_map_evacuate.rs:181`, `segmented_map_rebalance.rs:105`. Share the context's ManualClock and refuse at or after the deadline, as the updated reconstruction double does (`tests/reconstruction.rs:114`). - Pre-mark pins test: one leg per shape where a supersede lands between the assessment and the pre-mark; assert no `put_fragment` is sent (`Fleet::sent_to` empty) and no pre-mark key is written. It must kill the `.take(0)` mutation on the pin fold (`premark.rs:461-467`). - DELETE/overwrite trigger for the #1100 class: a gateway unlink or flat overwrite stamped before the in-place batch and committed after it can strand an in-place fragment (bytes kept, never lost). Handle it as docs, not code: correct `gc.rs:263-269`, `premark.rs:37`, `:50-56`, `:60` and `06-runtime-view.md:40` so they name "any unreference whose mark is stamped before the in-place batch and commits after it" as the #1100 exception, and widen #1100 to match (note it in build-notes for the human to update the tracker).
- By / date: Eduard Ralph / 2026-11-10

## 10. Act candidates (hints for the next Act review)
- Plan advisory: 3 finding(s); brief revised: yes (plan-advisory-*.md)
- (empty is the common case)
