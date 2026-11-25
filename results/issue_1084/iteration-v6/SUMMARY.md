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
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass — run-verify.sh: PASS — red without the fix, green with it (50 test(s) ran red).
- C4 diff coverage: changed lines executed by the patch's tests: fail — diff coverage not measured — patch.diff does not apply on origin/main
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): fail — 71 mutants tested in 12m: 53 caught, 15 unviable, 3 timeouts

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 3 blocking, 4 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_1084/review-
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 13.56s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Review #1084: committed repair and evacuation must protect destination fragments with fresh reclamation evidence, bounded writes and conditional adoption, without certifying unfinished repairs.

Advisory outcome: the destination protocol passes its regression checks, but false repair certification still needs a builder correction. Four independent probes reproduce it; the other batch-review concerns are a settled recovery-policy limitation and a test-coverage recommendation.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | Acceptance is observable for both movers and map shapes, with library-only evacuation and existing deferrals explicit; `brief.md:43`, `brief.md:241`, `docs/design/proposals/draft/0016-multipart-commit-protocol.md:1287`. |
| C2 Reproduction (red pre-fix) | PASS | Stashing production changes while retaining the added test reproduced 48 assertion failures and 2 passes, including stale destination marks and false certification; `reviewer-red.log:383`, `crates/custodian/tests/move_destination_marks.rs:1421`. |
| C3 Change | PASS | Both committed movers use the agreed protocol, the staged selector is shared, and API/clock changes have matching fixtures and living documentation; `crates/custodian/src/reconstruction.rs:1317`, `crates/custodian/src/rebalance.rs:675`, `docs/design/architecture/06-runtime-view.md:40`. |
| C4 Verification (red→green) | PASS | Restoring the patch yields 50/50 regression passes and 522 custodian passes; DST and TiKV checks also pass. Full local CI has only the snapshot gitlink caveat detailed below; `reviewer-restored-green.log:252`, `reviewer-ci.log:4648`, `reviewer-tikv.log:202`. |
| C5 Causal adequacy | NEEDS-HUMAN [impl] | Keep an unchanged damaged chunk non-certifying after an unrelated root update — four probes return `Satisfied` with its fragment missing and obligation queued; `crates/custodian/src/reconstruction.rs:1373`, `crates/custodian/tests/move_destination_marks.rs:1704`, `reviewer-probes.log:5`. |
| T1 Structure | PASS | The shared protocol preserves metadata/chunk-store trait boundaries and one context clock for each move; no new crate or mutable global is introduced; `crates/custodian/src/premark.rs:456`, `crates/custodian/src/premark.rs:529`, `reviewer-ci.log:4094`. |
| T2 Shape | PASS | The selector extraction and context-literal changes fit the agreed scope; GC changes are comments and the size/split decision is already settled; `crates/custodian/src/reconstruction/staged.rs:317`, `crates/custodian/src/gc.rs:255`, `brief.md:389`. |
| T3 Runtime | PASS | Fan-out waits for all write outcomes before adoption, and mixed success/refusal probes preserve both marks and refuse the whole repair on both shapes; `crates/custodian/src/reconstruction.rs:1337`, `crates/custodian/src/rebalance.rs:695`, `reviewer-probes.log:18`, `reviewer-probes.log:41`. |
| T4 Contribution | FAIL | The required batch review still has an unresolved, independently reproduced certification finding; path-based prior-art checks ran, while contribution-artifact auditing is deferred to publish; `gate-logs/T4-batch-review.log:10`, `gate-logs/T4-contribution.log:10`, `reviewer-prior-art.json:1`. |
| T5 Judgment | PASS | The mandatory mark/deadline checks address the storage cause, not an optional-capability symptom guard; tracked exceptions and the live-reference reclaiming policy remain settled rather than silently broadening this patch; `crates/custodian/src/premark.rs:51`, `crates/custodian/src/premark.rs:111`, `crates/custodian/src/premark.rs:167`. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Approve deployment fitness and arrange the scoped Tier-1/Tier-2 follow-up — automated evidence covers deployed-shape repair and library-level evacuation, while privileged disk/container scenarios remain unexecuted; `brief.md:254`, `AGENTS.md:78`, `crates/custodian/tests/tier1_disk_faults.rs:1`. |

Source citations above and below are grounded in `$PDCA_TARGET` (`target/`); `brief.md`, gate logs and `reviewer-*` evidence are in this review directory. The target compiled with the patch and after its stash/restore cycle. Builder notes were not read, and no production source was edited.

**F1 — P1: a root change does not establish that the damaged chunk was repaired.** `lost()` turns every changed root into `RepairOutcome::Conflict` (`crates/custodian/src/reconstruction.rs:1373`). That outcome only emits a counter (`crates/custodian/src/reconstruction.rs:491`), leaving `refused` false and allowing `Satisfied` (`crates/custodian/src/reconstruction.rs:546`). A sibling placement edit or metadata-only root-version bump can leave this chunk's reference and missing fragment unchanged. `Satisfied` then contradicts its public meaning (`crates/custodian/src/reconciliation.rs:23`).

The independent probes bump only the root version immediately before either the pre-mark or adoption commit. All four combinations of phase and map shape return `Satisfied`, retain placement `[3, 4, 5]`, leave fragment 2 absent on server 5, and keep the repair obligation. See `reviewer-probes.log:5`, `reviewer-probes.log:19`, `reviewer-probes.log:29`, and `reviewer-probes.log:43`. The submitted D6 regression currently locks in that wrong verdict at `crates/custodian/tests/move_destination_marks.rs:1704`.

Correct the certification decision and its tests: either conservatively withhold certification after the failed move, or establish that the current reference no longer owes this repair before allowing certification. A changed root byte string alone is insufficient. This does not require changing the destination pre-mark design or discarding the repair obligation.

**The other two batch-review items have narrower dispositions.**

- **Referenced `reclaiming` destination: reproduced, declined as an in-PR recovery-protocol change.** With the original server as the only free domain, three repair/GC cycles leave the mark and obligation unchanged on both shapes (`reviewer-probes.log:3`, `reviewer-probes.log:27`). Admission rejects the mark (`crates/custodian/src/premark.rs:167`), while GC protects referenced positions even during its fragment-less sweep (`crates/custodian/src/gc.rs:918`). However, [#1083's J5 and scope](https://github.com/getwyrd/wyrd/issues/1083) explicitly retain this condition for human resolution and exclude cancelling live-reference reclaim intent. Do not reopen that settled policy or ask this child to overwrite a reclaiming mark. The stall is an operational limitation, not evidence that safe cancellation exists.
- **Mixed successful/refused writes: test recommendation, no demonstrated implementation failure.** The submitted multi-fragment legs exercise success, while deadline-refusal legs exercise one write. Independent RS(2,2) repair probes let the first write land and expire the second: both map shapes return `Blocked`, preserve the old placement and repair obligation, and retain both destination marks (`reviewer-probes.log:18`, `reviewer-probes.log:41`). The shared reducer retains a refusal across successful answers (`crates/custodian/src/premark.rs:600`). Retain a focused mixed-result regression for both movers when iterating; the independent repair result does not itself supply permanent evacuation coverage.

**Verification supports the exercised protocol, with two distinct evidence limits.**

- Independent command: `cargo test --offline --locked -p wyrd-custodian --test move_destination_marks`, with `--manifest-path "$PDCA_TARGET/Cargo.toml"` and `CARGO_TARGET_DIR="$PWD/reviewer-build"`. The added test was retained while the production patch was stashed; the red run failed 48/50 assertions. After `git stash pop`, `cargo test --offline --locked -p wyrd-custodian` passed 522 tests, including all 50 added regressions and the inline evacuation legs; one privileged Tier-1 test was ignored. Logs: `reviewer-red.log`, `reviewer-restored-green.log`.
- Independent `cargo xtask ci` reran formatting, clippy, compilation, workspace tests, docs lint/render/link audit, spelling, unused-dependency checks, conformance, repository guards and DST. The 50-seed DST campaign passed, including the nonce replay (`reviewer-ci.log:4615`) and all 43 active custodian DST tests (`reviewer-ci.log:4648`). The slow ceiling-refusal test finished successfully in the same approximately 150-second suite duration as the frozen run.
- **Snapshot caveat:** local CI exits 1 because the synthetic base has no `.github/advisory-db` gitlink. This breaks the index-structure test and prevents the pinned dependency audit (`reviewer-ci.log:4072`, `reviewer-ci.log:4730`). The patch changes neither that gitlink nor the audit. The frozen original run actually resolves the pin and passes all three dependency-wall invocations (`gate-logs/C4-ci.log:4514`, `gate-logs/C4-ci.log:4525`, `gate-logs/C4-ci.log:4528`, `gate-logs/C4-ci.log:4531`). This is a target-state limitation, not a C4 patch defect, and local full CI is not claimed green.
- Independent TiKV checks passed: `cargo clippy --offline --locked -p wyrd-metadata-tikv --features tikv --tests` and `cargo clippy --offline --locked -p wyrd-server --features tikv,etcd --tests`, with `WYRD_TIKV_TOOLCHAIN=1` (`reviewer-tikv.log:103`, `reviewer-tikv.log:202`). The declared `typos` and docs-renderer dependencies were exercised, not replaced by shims or code-reading.
- **Diff coverage is unmeasured:** its frozen wrapper tried to apply the stacked patch to `origin/main` and failed before measuring coverage (`gate-logs/C4-diff-cov.log:10`). The supplied target independently builds and runs. Do not convert the remote-base mismatch into a compile/applicability defect.
- The mutation log reports **53 caught, 15 unviable, 3 timeouts, no missed mutant reported**, from 71 candidates (`gate-logs/C5-mutants.log:13`). Its nonzero exit is not evidence of a surviving behavior mutant; the three timeout verdicts remain inconclusive. The instance-scoped mutation/diff-coverage/review wrappers were adjudicated from their supplied logs, not sought in another checkout.
- **T4-contribution: N/A.** The gate found the contribution artifacts absent by design; its substantive audit must rerun at publish (`gate-logs/T4-contribution.log:10`). No missing-artifact human escalation is warranted at Check.

**Prior art was checked by affected source path and across closed work.** GitHub main-history queries for `crates/custodian/src/reconstruction.rs`, `crates/custodian/src/rebalance.rs`, and `crates/custodian/src/reconstruction/staged.rs` confirm the cited repair/rebalance lineage and the existing staged peer; `crates/custodian/src/premark.rs` has no main history. Closed issue/PR searches used the two committed movers' full paths, including merged [#826](https://github.com/getwyrd/wyrd/pull/826) and closed, unmerged [#647](https://github.com/getwyrd/wyrd/pull/647). Query summaries are retained in `reviewer-prior-art.json`. No DCO finding was inferred from commit metadata. Deferrals #825, #682 and #1100 remain settled.

**Human validation remains the deployment decision.** On suitable off-Check hosts, run `WYRD_TIER1=1 cargo xtask disk-faults` with root/device-mapper and `WYRD_TIER2=1 cargo xtask kill-reconstruct` with Docker. Confirm each repaired placement names present, intact fragments in distinct domains and reconstructed bytes match the original; do not count a skipped scenario as validation (`xtask/src/faults.rs:79`, `xtask/src/faults.rs:554`, `crates/chunkstore-grpc/tests/tier2_kill_reconstruct.rs:822`). These are the brief's explicit off-Check scenarios, not newly invented Check dependencies. Evacuation remains library-level; production wiring is outside this slice.

As already requested by `brief.md:395`, widen [#1100](https://github.com/getwyrd/wyrd/issues/1100)'s tracker description to include gateway unlink and flat overwrite whose early-stamped mark commits after the in-place batch. The current tracker still describes the late competing-move trigger; the patch's documentation already covers the broader class (`crates/custodian/src/premark.rs:51`, `docs/design/architecture/06-runtime-view.md:40`). This is follow-through on the accepted deferral, not a demand to implement its fence here.

The independent probes are reproducible with `python3 reviewer-probes.py` after the custodian build above. Their expected review result is four false-certification failures, two failures demonstrating the settled reclaiming limitation, and two passing mixed-write checks. The submitted patch remains unchanged.
<!-- pdca:leaf-complete -->

### Advisory — adversary

# Adversarial review — #1084 (committed repair and evacuation pre-mark their destinations)

I could not break the shipped production logic. I found one test gap that lets a required
precondition be deleted with every custodian test still green, and I proved its consequence with a
scratch probe. Two design questions the batch review (T4) already raised hold up, and I add
evidence for both. All line numbers are on `$PDCA_TARGET` (the patched tree).

## Findings

- NEEDS-HUMAN [impl] — **No test covers the pre-mark's `seg:` pin. Without that pin, a pre-mark is left on a position the placement names.** `crates/custodian/src/premark.rs:465-471` copies the repoint's preconditions into the pre-mark batch: the root, plus the `seg:` record for a segmented map. I cut them down to the root alone (`.filter(|p| p.key.starts_with(b"inode:"))`) and ran the full `cargo test -p wyrd-custodian`. Every test passed, for both movers. The only pins test, `d6_superseded_before_the_premark` (`crates/custodian/tests/move_destination_marks.rs:1735`), races a supersede, which moves the root, so the root pin alone is enough to pass it. **Concrete failing case under that mutation** (scratch probe, segmented object): a competing move of fragment 2 to server 2 has already adopted. Its `seg:` rewrite has landed and its copy is on server 2. It has deleted its own pre-mark. All of this happens between this repair's `repoint_chunk` read (`crates/custodian/src/reconstruction.rs:1263`) and its pre-mark commit (`premark.rs:487`). The root never moves and `orphan:2:<chunk>:2` reads absent, so the pre-mark commits. The adoption then loses on the `seg:` pin, and the pass answers `Blocked`. What it leaves behind is `{"orphaned_at_millis":10000000,"event":"repoint:…c0ffee00:88e9ec3153a1b573"}`, sitting on a position the live `seg:` record names. That is the D1/D2 defect this child exists to remove, and a later supersede turns it into D3's early reclaim. On the shipped code the same probe is green: the pre-mark loses and nothing is written. The brief counts two moves of one chunk as in scope (D8), so this interleaving is in scope too. The code is right; only a regression guard is missing. Fix: on the segmented shape, add one leg per mover that arms `premarks_free` with `Act::PlantFirst` of a `seg:`-only rewrite, then asserts that server 2 was sent no write and holds no mark.

- NEEDS-HUMAN [human] — **A lost move still certifies when the root moved but the chunk did not.** `lost()` (`crates/custodian/src/reconstruction.rs:1368-1378`) answers `Conflict` whenever the root has left the planned generation. `Conflict` sets no hole (`:491`, `:546`). The new D6 test locks this in: `move_destination_marks.rs:1704-1708` bumps only the root's version (chunks unchanged). The move writes its fragment, the adoption loses, and the obligation stays queued. The test then requires `Reconciled::Satisfied`, whose doc says "Reality already matched the desired state; nothing was done" (`crates/custodian/src/reconciliation.rs:23-24`). That breaks the brief's invariant "no pass reports redundancy restored while a repair it owes is still undone" (brief.md:151). It also disagrees with evacuation, which answers `Blocked` for the same race (`rebalance.rs:203`, `:531-533`). This pass's own gauge still shows the chunk as under-replicated. I found no wired production writer that does this today. Backfill does exactly this (`crates/custodian/src/backfill.rs:239-247`) but has no caller yet, and the test's racer models it. This is the T4 blocking finding at `reconstruction.rs:1376`. It already happened on the base for a lost CAS, but this patch's `lost()` and its new test now lock it in. The iteration-4 sign-off (F1) covered only the case where the root is unchanged. The human should decide whether `lost()` must re-resolve the chunk before it certifies.

- NEEDS-HUMAN [human] — **A `reclaiming` mark on a position the placement still names blocks an in-place repair forever, while the code says it will clear.** `destination_mark` rules out every `reclaiming` mark (`premark.rs:167`), including one at the in-place position. GC never finishes that reclaim. Its safety gate skips a referenced position before reaching the `Reclaiming` arm (`crates/custodian/src/gc.rs:666-686`), and the sweep of marks with no fragment keeps referenced positions too (`gc.rs:919-921`). So the doc on `Assessment::Refused`, "It clears when GC finishes the reclaim" (`reconstruction.rs:802-803`), is false in this case. The only audit line is info-level, with no NEEDS-HUMAN (`reconstruction.rs:1624-1631`). Concrete case, from state left by pre-patch code: a base-era move adopted position (5, c, 2) after GC had read it as unreferenced. GC then swapped the old mark there to `reclaiming`, deleted the fragment, and died before deleting the key (the brief's failure (2)). If server 5's domain is the only free one, every pass now answers `Blocked` plus `repair-refused`, and nothing ever clears it. The new code cannot create this state, because every adoption now requires its own pre-mark. So it is a question about state left by older code: should the in-place batch clear such a mark, or should the refusal escalate to NEEDS-HUMAN? This is T4's `premark.rs:167` finding.

- Other hand mutations that survive the whole custodian suite. None is a defect in the shipped code; I list them so the human can judge whether guards are wanted:
  - `premark.rs:475`: drop `require_absent` for a destination read as absent. Two writers could create a mark there in the window: another move's pre-mark, which the nonce makes harmless, and the post-restore pass, whose mark this would only re-stamp later. Still, 0016's "each P_new's mark as read" precondition is only half covered by tests.
  - `premark.rs:546`: stamp the vacated mark with the pre-mark's stamp instead of the adoption's clock read. For an evacuation, that back-dates the grace of the live vacated fragment by however long the move took. No test moves the clock between pre-mark and adoption and then checks the vacated stamp.
  - `premark.rs:610`: `write-unverified` stops taking priority over `write-deadline-expired` in the audit reason. Audit text only; it overlaps T4's mixed-refusal test gap at `premark.rs:600`.
  - `premark.rs:510`: take the deadline from the moment of authorization instead of the pre-mark stamp. Not a defect: 0016 allows "no later than W_write after that authorization" (`gc.rs:254`).

- Attempts to refute that failed:
  - **Red→green.** C4-verify ran the new file red on the base for the right reasons. The base answers `Satisfied` while it still owes a repair, writes over `reclaiming` vacated marks, never pre-marks, and leaves the old destination mark in place. With the fix it is green. I re-ran the full `cargo test -p wyrd-custodian` on a scratch copy of the patched tree: all green.
  - **Production path.** The legs drive `reconcile_step` over `RedbMetadataStore::in_memory()`. The test doubles judge the deadline on the context clock, as `FsChunkStore` does (`crates/chunkstore-fs/src/lib.rs:286-288`). The gRPC client forwards the deadline (`crates/chunkstore-grpc/src/client.rs:288-293`). The deployed loop feeds one `LoopClock` to both `now_millis` and `ctx.clock`, with `W_WRITE_MILLIS` (`crates/server/src/custodian.rs:587`, `:618-619`). The container tiers now use `SystemClock`, so real D servers don't refuse a deadline built from a logical clock (`tier1_disk_faults.rs`, `tier2_kill_reconstruct.rs`).
  - **Hand mutations.** 14 of my 18 were caught, including kinds cargo-mutants cannot generate: blind vacated writes (absent and stamped), the `Kept` pin dropped, `reclaiming` vacated marks re-stamped, the stale in-place mark kept, the adoption not requiring its pre-mark, `lost()` inverted either way, no deadline on either mover, and a refused assessment that certifies.
  - **Nonce.** The replay test ran under `--cfg madsim` and passed (C4-ci log). It also asserts that two moves in one run get distinct identities. `RandomState::new()` advances the per-thread keys on every call, so moves on one thread differ.
  - **`choose` / `assign`** (`premark.rs:226-340`). Each round either adds a server or reads a new position, so the loop ends. An alternate server in the same failure domain is still found, because a ruled-out position is skipped before its domain is marked visited (`:328`). The three C5 timeouts are mutants that loop forever, not real inputs that could.
  - **Key collisions.** Evacuation never builds an in-place destination, because its pool excludes the draining server (`rebalance.rs:555`). A destination key and a vacated key cannot collide inside one batch: either the servers differ, or the fragment is in place and filtered out (`premark.rs:538`).
  - **C4-diff-cov.** Its "does not apply on origin/main" result is the base mismatch the brief predicts (main plus #1013's head). It is not evidence against the fix, but diff coverage stays unmeasured.

<!-- pdca:leaf-complete -->

### Advisory — code-review

No additional actionable findings on correctness introduced by this diff or on reuse, simplification, and efficiency.

Reviewed the shared destination selection and pre-mark/adoption protocol (`crates/custodian/src/premark.rs:226`, `crates/custodian/src/premark.rs:456`, `crates/custodian/src/premark.rs:529`), both committed movers, the staged selector extraction, and the changed race/deadline tests against the target source and frozen gate evidence. Existing deferrals and the frozen T4 findings are not reopened or overridden by this advisory. No gates were rerun; diff coverage remains unmeasured as recorded.

<!-- pdca:leaf-complete -->

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C5 Causal adequacy — Keep an unchanged damaged chunk non-certifying after an unrelated root update — four probes return `Satisfied` with its fragment missing and obligation queued; `crates/custodian/src/reconstruction.rs:1373`, `crates/custodian/tests/move_destination_marks.rs:1704`, `reviewer-probes.log:5`.
- [ ] Validation — fitness-to-purpose — Approve deployment fitness and arrange the scoped Tier-1/Tier-2 follow-up — automated evidence covers deployed-shape repair and library-level evacuation, while privileged disk/container scenarios remain unexecuted; `brief.md:254`, `AGENTS.md:78`, `crates/custodian/tests/tier1_disk_faults.rs:1`.
- [ ] **No test covers the pre-mark's `seg:` pin. Without that pin, a pre-mark is left on a position the placement names.** `crates/custodian/src/premark.rs:465-471` copies the repoint's preconditions into the pre-mark batch: the root, plus the `seg:` record for a segmented map. I cut them down to the root alone (`.filter(|p| p.key.starts_with(b"inode:"))`) and ran the full `cargo test -p wyrd-custodian`. Every test passed, for both movers. The only pins test, `d6_superseded_before_the_premark` (`crates/custodian/tests/move_destination_marks.rs:1735`), races a supersede, which moves the root, so the root pin alone is enough to pass it. **Concrete failing case under that mutation** (scratch probe, segmented object): a competing move of fragment 2 to server 2 has already adopted. Its `seg:` rewrite has landed and its copy is on server 2. It has deleted its own pre-mark. All of this happens between this repair's `repoint_chunk` read (`crates/custodian/src/reconstruction.rs:1263`) and its pre-mark commit (`premark.rs:487`). The root never moves and `orphan:2:<chunk>:2` reads absent, so the pre-mark commits. The adoption then loses on the `seg:` pin, and the pass answers `Blocked`. What it leaves behind is `{"orphaned_at_millis":10000000,"event":"repoint:…c0ffee00:88e9ec3153a1b573"}`, sitting on a position the live `seg:` record names. That is the D1/D2 defect this child exists to remove, and a later supersede turns it into D3's early reclaim. On the shipped code the same probe is green: the pre-mark loses and nothing is written. The brief counts two moves of one chunk as in scope (D8), so this interleaving is in scope too. The code is right; only a regression guard is missing. Fix: on the segmented shape, add one leg per mover that arms `premarks_free` with `Act::PlantFirst` of a `seg:`-only rewrite, then asserts that server 2 was sent no write and holds no mark.
- [ ] **A lost move still certifies when the root moved but the chunk did not.** `lost()` (`crates/custodian/src/reconstruction.rs:1368-1378`) answers `Conflict` whenever the root has left the planned generation. `Conflict` sets no hole (`:491`, `:546`). The new D6 test locks this in: `move_destination_marks.rs:1704-1708` bumps only the root's version (chunks unchanged). The move writes its fragment, the adoption loses, and the obligation stays queued. The test then requires `Reconciled::Satisfied`, whose doc says "Reality already matched the desired state; nothing was done" (`crates/custodian/src/reconciliation.rs:23-24`). That breaks the brief's invariant "no pass reports redundancy restored while a repair it owes is still undone" (brief.md:151). It also disagrees with evacuation, which answers `Blocked` for the same race (`rebalance.rs:203`, `:531-533`). This pass's own gauge still shows the chunk as under-replicated. I found no wired production writer that does this today. Backfill does exactly this (`crates/custodian/src/backfill.rs:239-247`) but has no caller yet, and the test's racer models it. This is the T4 blocking finding at `reconstruction.rs:1376`. It already happened on the base for a lost CAS, but this patch's `lost()` and its new test now lock it in. The iteration-4 sign-off (F1) covered only the case where the root is unchanged. The human should decide whether `lost()` must re-resolve the chunk before it certifies.
- [ ] **A `reclaiming` mark on a position the placement still names blocks an in-place repair forever, while the code says it will clear.** `destination_mark` rules out every `reclaiming` mark (`premark.rs:167`), including one at the in-place position. GC never finishes that reclaim. Its safety gate skips a referenced position before reaching the `Reclaiming` arm (`crates/custodian/src/gc.rs:666-686`), and the sweep of marks with no fragment keeps referenced positions too (`gc.rs:919-921`). So the doc on `Assessment::Refused`, "It clears when GC finishes the reclaim" (`reconstruction.rs:802-803`), is false in this case. The only audit line is info-level, with no NEEDS-HUMAN (`reconstruction.rs:1624-1631`). Concrete case, from state left by pre-patch code: a base-era move adopted position (5, c, 2) after GC had read it as unreferenced. GC then swapped the old mark there to `reclaiming`, deleted the fragment, and died before deleting the key (the brief's failure (2)). If server 5's domain is the only free one, every pass now answers `Blocked` plus `repair-refused`, and nothing ever clears it. The new code cannot create this state, because every adoption now requires its own pre-mark. So it is a question about state left by older code: should the in-place batch clear such a mark, or should the refusal escalate to NEEDS-HUMAN? This is T4's `premark.rs:167` finding.
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 3 blocking, 4 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_1084/review-
- [ ] **The repair round trip is not forced by the stated setup.** `brief.md:63-76` says repair is forced like evacuation and prescribes “lose it on 5, then on 2.” On the target, evacuation excludes draining servers (`crates/custodian/src/rebalance.rs:528`), but committed repair excludes only survivors’ domains (`crates/custodian/src/reconstruction.rs:1122-1126`). Both servers 2 and 5 therefore remain eligible after either loss; the selector orders them by utilization and domain label (`crates/core/src/placement.rs:289-301`). With unchanged topology/utilization, a first selection of 2 is followed by an in-place repair on 2, not the required return to 5. Specify the per-pass topology/utilization or drain-state changes that force each direction, and require placement assertions before the mark assertions. Otherwise D2 fails for fixture reasons even after the destination-mark fix.
- [ ] **D7a cannot detect its promised boundary mutation.** `brief.md:91-94` advances the clock past both the write-authorization window and the pre-mark’s grace, while `brief.md:246-247` requires this leg to kill `<` → `<=`. Both comparisons refuse a write at the prescribed late instant; they differ only at exactly `stamp + W_REPOINT_MILLIS`. The target’s peer rejects equality (`crates/custodian/src/reconstruction/staged.rs:680`), and its constants leave room to exercise that boundary before the D-server deadline (`crates/custodian/src/gc.rs:202,218`: 30,000 versus 10,000 ms). Add a distinct exact-boundary arm asserting zero destination calls and a retained obligation; keep the post-GC arm for the missing-gate mutation.
- [ ] **The nonce’s replay requirement lacks the claimed verification.** `brief.md:167-171` proposes OS-seeded `RandomState` while requiring deterministic seed replay, checked with `cargo xtask ci`. The target’s DST runner sets `MADSIM_TEST_NUM=50` and runs the tests (`xtask/src/main.rs:1811-1817`); it does not enable the separate `MADSIM_TEST_CHECK_DETERMINISM` option documented at `crates/dst/src/lib.rs:90-91`. D8 checks distinct identities within one execution, not reproducibility across executions. Require an explicit repeated-same-seed check that observes the minted pre-mark identities and relevant outcomes, and state its command/assertion. A green seed sweep alone cannot establish the proposed nonce source satisfies the replay constraint.
- [ ] size backstop — this slice is behaving oversized: patch is 429 KB (threshold 100 KB); patch touches 25 files (threshold 20). Recommend answering `iterate-plan` at sign-off and authoring the split in the re-plan (`pdca split`), rather than `iterate-do`: a slice that is too big yields implementation-shaped findings every round, and splitting authors briefs, which is Plan's beat.

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
- Iteration delta (if iterating): Keep the design (destination pre-mark + gated write with deadline + adoption consuming the pre-mark; per-move nonce). Not a split: production logic is ~67 KB, the size backstop fires on test volume (as ruled in rounds 4 and 5). Fix only these, and don't grow the patch beyond them: - F1 (all three reviewers): a lost move must not certify when the root moved but this chunk did not. `lost()` (`reconstruction.rs:1368-1378`) maps any root change to `RepairOutcome::Conflict`, which leaves `refused` false and lets the pass answer `Satisfied` with the fragment still missing and the obligation queued. Make that case non-certifying (answer `Blocked`, as evacuation does for the same race, `rebalance.rs:203`), or re-resolve the chunk and certify only if the current reference no longer owes the repair. A changed root byte string alone is not enough. Change the D6 test (`move_destination_marks.rs:1704-1708`) to require the non-`Satisfied` verdict, and cover both phases (before pre-mark, before adoption) and both map shapes, as the reviewer probes do. - `seg:` pin regression guard (adversary): no test fails if the pre-mark batch pins only the root (`premark.rs:465-471`). On the segmented shape, add one leg per mover that arms `premarks_free` with `Act::PlantFirst` of a `seg:`-only rewrite (root unchanged), then asserts server 2 was sent no write and holds no mark. - Doc fix only: `Assessment::Refused`'s comment "It clears when GC finishes the reclaim" (`reconstruction.rs:802-803`) is false for a `reclaiming` mark on a still-referenced position (GC never finishes it there). Correct the comment to say so and point at #1083; do not change the refusal behaviour. - Optional, if cheap: one mixed-result leg per mover (one write lands, one refused) asserting the whole move refuses (`premark.rs:600`).
- By / date: Eduard Ralph / 2026-11-10

## 10. Act candidates (hints for the next Act review)
- Plan advisory: 3 finding(s); brief revised: yes (plan-advisory-*.md)
- (empty is the common case)
