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
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass — run-verify.sh: PASS — red without the fix, green with it (55 test(s) ran red).
- C4 diff coverage: changed lines executed by the patch's tests: fail — diff coverage not measured — patch.diff does not apply on origin/main
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): fail — 71 mutants tested in 12m: 53 caught, 15 unviable, 3 timeouts

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 6 blocking, 3 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_1084/review-
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 13.64s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Review of #1084’s destination pre-marks for committed repair and evacuation: the specified safety protocol reproduces red→green; the off-fleet fallback contract needs a human scope decision.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | X47, write deadlines, per-move identity and D1–D10 define falsifiable obligations, with the prerequisite drain present; `brief.md:52`, `target/docs/design/proposals/draft/0016-multipart-commit-protocol.md:2587`, `target/crates/custodian/src/reconciliation.rs:187`. |
| C2 Reproduction (red pre-fix) | PASS | Stashing the production changes while retaining the new test independently reproduced 53 assertion failures and 2 passes; `reviewer-red.log:563`, matching `gate-logs/C4-verify.log:434`. |
| C3 Change | PASS | The change addresses destination evidence, adoption and refusal accounting within the planned surface, including living documentation and deadline-aware fixtures; `target/crates/custodian/src/premark.rs:456`, `target/docs/design/architecture/06-runtime-view.md:40`. |
| C4 Verification (red→green) | PASS | After stash restoration all 55 regression tests and 530 custodian tests passed; full frozen CI is green, while the independent CI run has the missing-gitlink caveat below, not a patch compile failure; `reviewer-green.log:260`, `gate-logs/C4-ci.log:5177`, `reviewer-ci.log:4008`. |
| C5 Causal adequacy | NEEDS-HUMAN | Decide whether off-fleet fallback selection belongs in this bundle or a separate tracked issue — both movers can overlook a usable fallback, and repair reports `Satisfied` while still owed, but the brief explicitly retains off-fleet aborts and deployed topology is aligned; `target/crates/custodian/src/premark.rs:390`, `target/crates/custodian/src/reconstruction.rs:503`, `brief.md:72`, `reviewer-probe.log:9`. |
| T1 Structure | PASS | The shared admission/pre-mark seam preserves backend dependency direction and instance-local state without adding a crate or context field to reconstruction; `target/crates/custodian/src/premark.rs:186`, `target/crates/custodian/src/premark.rs:346`, `target/crates/custodian/src/reconstruction/staged.rs:371`. |
| T2 Shape | PASS | Production changes stay focused on the common move lifecycle; the large fixture updates implement the expressly required clock/deadline migration and do not justify reopening the settled split decision; `brief.md:227`, `brief.md:403`, `target/crates/custodian/src/premark.rs:504`. |
| T3 Runtime | PASS | Both movers dispatch concurrently and reject the whole move after a mixed write outcome; one context clock controls stamps and deadlines, and nonce replay passes under madsim; `reviewer-probe.log:3`, `target/crates/custodian/src/reconstruction.rs:1354`, `target/crates/custodian/src/rebalance.rs:692`, `reviewer-ci.log:4551`. |
| T4 Contribution | FAIL | The frozen batch-review gate remains red with its fallback concern awaiting the C5 scope disposition; duplicate test-gap entries and the vacated-mark complaint are adjudicated below, TiKV compile evidence passes, and contribution-artifact audit is N/A until publish; `gate-logs/T4-batch-review.log:10`, `gate-logs/host-tikv.log:211`, `gate-logs/T4-contribution.log:10`. |
| T5 Judgment | PASS | The fix removes the missing-evidence cause rather than probing around an eager side effect; settled #1100, #825 and #682 deferrals remain settled, and prior-art searches covered all 25 affected paths in merged and closed-unmerged PRs; `target/crates/custodian/src/premark.rs:51`, `target/crates/custodian/src/premark.rs:111`, `target/crates/custodian/src/reconstruction.rs:1280`, `reviewer-prior-art.json:1`. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether the exercised in-process safety guarantees suffice for sign-off and which deployment durability follow-up to run — Tier-1 disk faults and Tier-2 kill/reconstruct were intentionally not exercised, and evacuation remains a library-level feature; `brief.md:55`, `brief.md:254`, `target/xtask/src/faults.rs:79`, `target/xtask/src/faults.rs:554`. |

All `target/…` citations refer to the supplied `$PDCA_TARGET`. Its prerequisite drain and redb dependency are present, and `git apply --reverse --check ../patch.diff` succeeds after restoration. The reviewed source was not edited. Scratch probes and captured output are confined to this harness directory.

**The remaining behavioral concern is real but its scope is disputed.** An independent probe uses an RS(2,1) placement `[3,4,5]`, a reclaiming destination on server 2, server 6 present only in topology, and an available server 7. After excluding position 2, `Committed::candidate` admits 6 without consulting the fleet (`target/crates/custodian/src/premark.rs:390`). Both callers stop when resolving its absent store (`target/crates/custodian/src/reconstruction.rs:1241`, `target/crates/custodian/src/rebalance.rs:582`), so neither tries 7. Across two consecutive passes and both flat and segmented maps, evacuation stays `Blocked`; reconstruction answers `Satisfied` with the fragment still missing and its obligation queued. Removing only server 6 from topology makes the first pass move to 7 and answer `Changed` (`reviewer-probe.log:9`, `reviewer-probe.log:18`). Reproduce with `python3 -S reviewer_probe.py`; the probe compiles against the built target libraries and copies the existing fixture into scratch, without changing the reviewed code.

This does **not** establish a deployed reconstruction regression: `live_reconstruction_view` constructs fleet and topology together (`target/crates/server/src/custodian.rs:293`). The off-fleet `Aborted` path and its certification behavior predate this patch, and `brief.md:72` explicitly anticipates that abort. Consequently C5 is plain `NEEDS-HUMAN`, not an automatic implementation iteration: retain that bounded contract and record a separate issue, or expand this bundle’s criterion to filter unavailable fallback candidates and require a non-certifying outcome when no usable assignment remains. No corresponding tracking issue was established by the searches; none was created by this advisory review.

**The other batch findings do not establish additional in-scope defects.**

- The four mixed-outcome test entries are two duplicated observations. Holding only the second write does not prove concurrent dispatch, but it does test the optional mixed-result requirement in `brief.md:409`. Independently holding the **first** write proved the second was sent and landed before release, for each mover and map shape; the delayed first write then expired and the whole move answered `Blocked` (`reviewer-probe.log:3`). Strengthening the committed tests is useful regression hardening, not evidence that dispatch is currently serial.
- Preserving an unreadable vacated mark is explicitly required (`brief.md:210`, `target/crates/custodian/src/premark.rs:560`). Adoption still pins its exact bytes. GC separately reports malformed marks on its audit seam (`target/crates/custodian/src/gc.rs:2070`, `target/crates/custodian/src/gc.rs:2391`). Requiring adoption to fail merely because it preserved those bytes would contradict the approved scope; reject that batch complaint.
- The three already recorded rejections in the frozen summary are not reopened. The named #1100, #825 and #682 limitations are not new findings.

**Verification supports the bounded claim, with explicit evidence limits.**

- Independent red: `cargo test --offline --locked -p wyrd-custodian --test move_destination_marks` with tracked production changes stashed, leaving the new test in place: 53 failed, 2 passed. Independent green after automatic stash restoration: `cargo test --offline --locked -p wyrd-custodian`: 530 passed, one intentionally ignored Tier-1 test. The 55-test regression file is entirely green (`reviewer-red.log:563`, `reviewer-green.log:260`, `reviewer-green.log:740`).
- Independent `cargo xtask ci` ran spelling, docs lint/render, formatting, clippy, build, tests, dependency-use checking, conformance, static-state/orchestrator scans and DST. The nonce replay and all executed DST suites passed. It exited 1 because the disposable repository has no `.github/advisory-db` gitlink: that breaks both its index test and the advisory-pin step (`reviewer-ci.log:4008`, `reviewer-ci.log:4664`). This is a target-copy limitation; no missing pin was synthesized and no shim was used. The frozen full CI log records the real run passing (`gate-logs/C4-ci.log:5177`). `typos` and `docs-renderer`, the brief’s declared external dependencies, both actually ran locally (`reviewer-ci.log:2`, `reviewer-ci.log:7`).
- Diff coverage is **unmeasured**, not green: its wrapper tested application on `origin/main` and failed before measuring anything (`gate-logs/C4-diff-cov.log:10`). This base/application caveat is not a C4 patch defect; this supplied target applies and compiles the patch.
- Mutation evidence is **incomplete**, not a set of three demonstrated survivors: the captured run shows 53 caught, 15 unviable and three timeouts, all in destination matching (`gate-logs/C5-mutants.log:13`). Those timeouts do not prove a production hang or a missed assertion. The instance-scoped scanner was adjudicated from its captured output.
- TiKV feature compilation is supported by the frozen crate/server clippy log, not an independent local TiKV rerun (`gate-logs/host-tikv.log:3`, `gate-logs/host-tikv.log:211`). The contribution-artifact row is **N/A**: those artifacts are drafted after Check and their substantive audit must run at publish (`gate-logs/T4-contribution.log:10`).

**Prior-art checking ran by affected path.** `reviewer-prior-art.json` records 50 searches: merged and closed-unmerged PRs for each changed path. Relevant merged work includes [#826’s staged pre-mark protocol](https://github.com/getwyrd/wyrd/pull/826), [#850’s segmented repair](https://github.com/getwyrd/wyrd/pull/850), and [#884’s segmented evacuation](https://github.com/getwyrd/wyrd/pull/884). The closed-unmerged [#647](https://github.com/getwyrd/wyrd/pull/647#issuecomment-5137104571) was a broader segmented-map proposal closed for needing a smaller scope, not a rejection of this destination-mark protocol. The local target contains one synthetic base commit, so those remote records and the brief’s named commits supplement, rather than masquerade as, local merged history. Searches are not proof of exhaustive historical absence.

**Deployment validation remains the human’s decision.** The direct redb reconciliation tests and first-write-held probes were exercised. For the intentionally deferred real-environment follow-up, run `WYRD_TIER1=1 cargo xtask disk-faults` on the designated root/device-mapper test host and/or `WYRD_TIER2=1 cargo xtask kill-reconstruct` on the designated Docker host. Confirm the workload keeps reading correctly, the repair backlog drains after loss, redundancy returns, and the scenarios pass their final data checks. These are off-Check follow-ups, not newly imposed dependencies or permission requests. `INTEGRATION.md` was absent from the supplied bundle and target; this report does not attest to unseen human-only checklist entries.
<!-- pdca:leaf-complete -->

### Advisory — adversary

# Adversarial review — issue 1084 (committed-move destination pre-mark), iteration 7

**Verdict: I could not refute the fix.** I re-checked the red→green evidence, ran six mutation probes of my own that the mutants gate cannot generate, and tried the clock, deadline, nonce and choice paths for a breaking input. Every probe that touches a safety property went red. What follows is one minor test gap, one scope question, and my take on the T4 batch review's six blocking findings.

## Evidence

- `gate-logs/C4-verify.log`: all 55 legs in `crates/custodian/tests/move_destination_marks.rs` fail with production reverted and pass with it applied. The failures are property failures, not compile errors: for example `:1063` (a mark on a position the placement names), `:1310` (D2's stale mark), `:1347` (D3's early reclaim), `:1446` (`Satisfied` instead of `Blocked`). The legs run through `reconcile_step` over the production `premark`/`repair_chunk` code, not a copy of it.
- The nonce replay test `a_committed_moves_pre_mark_nonce_replays_under_its_seed` ran green under `--cfg madsim` (`gate-logs/C4-ci.log:5064`). It is red for a process-wide counter by construction: run 1 draws 0,1 and run 2 draws 2,3, so the `assert_eq!(first, second)` at `crates/dst/tests/custodian.rs:6914-6918` fails. It is red for a wall-clock nonce through the `assert_ne!` at `:6908-6911`.
- C5 (`gate-logs/C5-mutants.log`): 0 mutants missed. The 3 timeouts (`premark.rs:273`, `:290`, `:328`) are mutants that turn `choose`'s loop into an endless one, so the tests hang. That counts as detected. I traced the unmutated loop: each round either considers a new server or reads a new position, so it terminates (`premark.rs:235-276`).
- My probes. Each was applied to a scratch copy, run, and reverted. Each went red as listed:
  - **No deadline on the writes** (`Some(deadline)` → `None`): `reconstruction.rs:1356` turned 10 repair legs red (D7b, D7c, D9, D10b, mixed). `rebalance.rs:694` turned 8 evacuation legs red.
  - **Constant nonce, format kept** (`premark.rs:633`): both D8 legs went red for each mover (`move_destination_marks.rs:2116`, `rebalance.rs:2744`).
  - **Pre-mark written at an absent in-place position** (`premark.rs:484`): both D10a crash legs went red.
  - **Stale in-place mark kept instead of deleted** (`premark.rs:483`): D1 in-place and D10a lost-adoption went red on both shapes.
  - **`RepairOutcome::Conflict` certifying again**, i.e. iteration 6's F1 reverted by dropping `refused = true` at `reconstruction.rs:499-502`: 8 legs went red (D6 ×6, D10a ×2).
- Breaking inputs I tried that came up empty:
  - **Clock base.** The deployed loop's clock is wall-clock epoch milliseconds (`crates/server/src/cli.rs:1109`, `:1962`). That is the same base an `FsChunkStore` judges a deadline on, so `stamp + W_WRITE` is compared like for like.
  - **Zero write window.** No custodian context still passes a zero window.
  - **Unbounded write await.** The comment's claim that each write's await is bounded by the client's request timeout holds (`crates/chunkstore-grpc/src/client.rs:264-271`).
  - **Tier-1/Tier-2 tests.** They moved to `SystemClock` plus `W_WRITE_MILLIS`, and they compile in CI (`C4-ci.log:730`, `:762`).

## Findings

- **Test gap, low priority; no rebuild needed for it alone.** `crates/custodian/src/premark.rs:546` stamps the vacated position's mark "as the position stops being named" (the doc at `:524-526`). Mutating it to `OrphanMark::legacy(self.stamp)` (the pre-mark's stamp, up to `W_REPOINT + W_WRITE` = 40 s earlier) passed all 197 tests across `move_destination_marks`, `reconstruction`, `segmented_map_{repoint,reconstruction,evacuate,rebalance}`, `rebalance` and the lib tests. This is not a regression: the base stamped this mark from the pass's start `now_millis`, which is earlier still. A one-line fix exists: in the D7c/D7d one-millisecond-early twins, which move the clock before adopting, assert `rig.mark(SOURCE) == Some(legacy(stamp + after))` inside `adopted_inside_grace` (`move_destination_marks.rs:1987-1997`).
- NEEDS-HUMAN [human] — **The committed movers' admission differs from the staged peer's, and 0016's drain fence is missing.** `Committed::candidate` (`crates/custodian/src/premark.rs:386-395`) admits every topology server. The staged admission excludes servers outside the fleet and servers with a `desired:dserver:<S>` record (`crates/custodian/src/reconstruction/staged.rs:402-412`), and both of the staged move's batches pin `require_absent(desired:dserver:<S_new>)` (`staged.rs:526`, `:611`). Proposal 0016's segment-repoint row, which the brief tells Do to read, lists that same fence for "reconstruction / rebalance of a committed segmented object" (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:670`). The committed pre-mark and adoption (`premark.rs:456-494`, `:529-564`) carry no such pin. Concrete case: with `desired:dserver:2` set, a committed repair of a fragment lost on 5, under topology {2,3,4}, writes to and adopts onto draining server 2. This is **older than this patch**: the base's committed repoint had no fence and no drain filter either, and the brief's Scope bullets do not list it. So I think it should be declined with a tracking issue reference rather than fixed here. A human should decide whether to file it, or whether to widen this child's scope.

## The T4 batch review's six blocking findings

- **`premark.rs:391` [BUG], "an off-fleet server displaces a usable fallback".** The mechanics are right, but the case cannot happen in the deployed reconstruction loop. `live_reconstruction_view` registers only reachable servers in the topology it hands the pass (`crates/server/src/custodian.rs:303-305`), so topology equals fleet there. Evacuation is library-only today, and an off-fleet abort there is non-persisted and so answers `Blocked` (`rebalance.rs:582-586`, `:203`). It is reachable only in a library repair context whose topology is wider than its fleet. There it ends as `RepairOutcome::Aborted`/`unplaced`, which certifies (`reconstruction.rs:503`). That is the base's behaviour for the first pick, and the brief leaves `unplaced` out of D9. If the reviewer wants parity anyway, it is the admission question above. I would not hold sign-off on it as a production bug.
- **`rebalance.rs:2855` and `move_destination_marks.rs:2411` [TEST-GAP] ×4, "a serial write loop passes the mixed-outcome tests".** True, but sending the writes concurrently is not a safety property. The move checks one gate (`premark.rs:504-511`), and every write's deadline is fixed from the pre-mark's stamp. A serial loop is therefore exactly as safe: each write is still refused at `stamp + W_write` by the D server. The brief's success criterion does not ask for concurrent dispatch. I recommend recording these as rejected with that reason.
- **`premark.rs:588` [CONVENTION], "an unreadable vacated mark is silently treated like `reclaiming`".** The brief prescribes this behaviour (Scope, *The vacated position's mark*: "`reclaiming` or undecodable ⇒ left byte for byte"). It is not silent: GC names an unreadable mark NEEDS-HUMAN on every pass (`crates/custodian/src/gc.rs:2070-2074`). The bytes left under it are the vacated copy, not the repaired fragment. The staged peer withholds the whole repair instead (`staged.rs:310-313`). The committed choice restores redundancy and leaves only garbage for a human, which is arguably better. I recommend recording it as rejected, citing the brief.

<!-- pdca:leaf-complete -->

### Advisory — code-review

- NEEDS-HUMAN [impl] — `crates/custodian/src/premark.rs:390`: The new mark-triggered fallback can select a server outside the fleet and let reconstruction certify an unfinished repair. For a library context with free domains ordered A, B, C, let A be in-fleet but its destination mark be `reclaiming`, B be absent from the fleet with no mark, and C be in-fleet and usable. `Committed::candidate` admits B solely from the topology, so `choose` stops there without trying C. Reconstruction then returns `Aborted` at `crates/custodian/src/reconstruction.rs:1244`; that outcome does not set `refused` at `crates/custodian/src/reconstruction.rs:503`, so a pass with just this repair returns `Satisfied` while its fragment remains missing and its obligation stays queued. Evacuation likewise repeatedly aborts despite C being usable. Filter unavailable candidates during the new fallback search, following the fleet-membership check in `crates/custodian/src/reconstruction/staged.rs:403`, and cover this mark-triggered case with a regression.

No additional reuse, simplification, or efficiency findings. Reviewed the target source and frozen gate evidence; no checks were rerun.

<!-- pdca:leaf-complete -->

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [x] C5 Causal adequacy — Decide whether off-fleet fallback selection belongs in this bundle or a separate tracked issue — both movers can overlook a usable fallback, and repair reports `Satisfied` while still owed, but the brief explicitly retains off-fleet aborts and deployed topology is aligned; `target/crates/custodian/src/premark.rs:390`, `target/crates/custodian/src/reconstruction.rs:503`, `brief.md:72`, `reviewer-probe.log:9`.
- [x] Validation — fitness-to-purpose — Decide whether the exercised in-process safety guarantees suffice for sign-off and which deployment durability follow-up to run — Tier-1 disk faults and Tier-2 kill/reconstruct were intentionally not exercised, and evacuation remains a library-level feature; `brief.md:55`, `brief.md:254`, `target/xtask/src/faults.rs:79`, `target/xtask/src/faults.rs:554`.
- [x] **The committed movers' admission differs from the staged peer's, and 0016's drain fence is missing.** `Committed::candidate` (`crates/custodian/src/premark.rs:386-395`) admits every topology server. The staged admission excludes servers outside the fleet and servers with a `desired:dserver:<S>` record (`crates/custodian/src/reconstruction/staged.rs:402-412`), and both of the staged move's batches pin `require_absent(desired:dserver:<S_new>)` (`staged.rs:526`, `:611`). Proposal 0016's segment-repoint row, which the brief tells Do to read, lists that same fence for "reconstruction / rebalance of a committed segmented object" (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:670`). The committed pre-mark and adoption (`premark.rs:456-494`, `:529-564`) carry no such pin. Concrete case: with `desired:dserver:2` set, a committed repair of a fragment lost on 5, under topology {2,3,4}, writes to and adopts onto draining server 2. This is **older than this patch**: the base's committed repoint had no fence and no drain filter either, and the brief's Scope bullets do not list it. So I think it should be declined with a tracking issue reference rather than fixed here. A human should decide whether to file it, or whether to widen this child's scope.
- [x] `crates/custodian/src/premark.rs:390`: The new mark-triggered fallback can select a server outside the fleet and let reconstruction certify an unfinished repair. For a library context with free domains ordered A, B, C, let A be in-fleet but its destination mark be `reclaiming`, B be absent from the fleet with no mark, and C be in-fleet and usable. `Committed::candidate` admits B solely from the topology, so `choose` stops there without trying C. Reconstruction then returns `Aborted` at `crates/custodian/src/reconstruction.rs:1244`; that outcome does not set `refused` at `crates/custodian/src/reconstruction.rs:503`, so a pass with just this repair returns `Satisfied` while its fragment remains missing and its obligation stays queued. Evacuation likewise repeatedly aborts despite C being usable. Filter unavailable candidates during the new fallback search, following the fleet-membership check in `crates/custodian/src/reconstruction/staged.rs:403`, and cover this mark-triggered case with a regression.
- [x] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 6 blocking, 3 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_1084/review-
- [x] **The repair round trip is not forced by the stated setup.** `brief.md:63-76` says repair is forced like evacuation and prescribes “lose it on 5, then on 2.” On the target, evacuation excludes draining servers (`crates/custodian/src/rebalance.rs:528`), but committed repair excludes only survivors’ domains (`crates/custodian/src/reconstruction.rs:1122-1126`). Both servers 2 and 5 therefore remain eligible after either loss; the selector orders them by utilization and domain label (`crates/core/src/placement.rs:289-301`). With unchanged topology/utilization, a first selection of 2 is followed by an in-place repair on 2, not the required return to 5. Specify the per-pass topology/utilization or drain-state changes that force each direction, and require placement assertions before the mark assertions. Otherwise D2 fails for fixture reasons even after the destination-mark fix.
- [x] **D7a cannot detect its promised boundary mutation.** `brief.md:91-94` advances the clock past both the write-authorization window and the pre-mark’s grace, while `brief.md:246-247` requires this leg to kill `<` → `<=`. Both comparisons refuse a write at the prescribed late instant; they differ only at exactly `stamp + W_REPOINT_MILLIS`. The target’s peer rejects equality (`crates/custodian/src/reconstruction/staged.rs:680`), and its constants leave room to exercise that boundary before the D-server deadline (`crates/custodian/src/gc.rs:202,218`: 30,000 versus 10,000 ms). Add a distinct exact-boundary arm asserting zero destination calls and a retained obligation; keep the post-GC arm for the missing-gate mutation.
- [x] **The nonce’s replay requirement lacks the claimed verification.** `brief.md:167-171` proposes OS-seeded `RandomState` while requiring deterministic seed replay, checked with `cargo xtask ci`. The target’s DST runner sets `MADSIM_TEST_NUM=50` and runs the tests (`xtask/src/main.rs:1811-1817`); it does not enable the separate `MADSIM_TEST_CHECK_DETERMINISM` option documented at `crates/dst/src/lib.rs:90-91`. D8 checks distinct identities within one execution, not reproducibility across executions. Require an explicit repeated-same-seed check that observes the minted pre-mark identities and relevant outcomes, and state its command/assertion. A green seed sweep alone cannot establish the proposed nonce source satisfies the replay constraint.
- [x] size backstop — this slice is behaving oversized: patch is 451 KB (threshold 100 KB); patch touches 25 files (threshold 20). Recommend answering `iterate-plan` at sign-off and authoring the split in the re-plan (`pdca split`), rather than `iterate-do`: a slice that is too big yields implementation-shaped findings every round, and splitting authors briefs, which is Plan's beat.

## 7. Proven / not proven
- Proven by which oracle: gates overall = fail (stub oracles).
- Unproven / needs manual run: anything flagged in §6.

## 8. Ready-to-ship attachments
- patch.diff
- tracker-comment.md     (ALWAYS, every tracker item)
- build-notes.md         (builder rationale — for the human, not the reviewer)

## 9. Check sign-off                     ← human completes Check here
- Disposition confirmed / overridden:
- Outcome: merged-wider
- Iteration delta (if iterating):
- By / date: Eduard Ralph / 2026-11-10

## 10. Act candidates (hints for the next Act review)
- Plan advisory: 3 finding(s); brief revised: yes (plan-advisory-*.md)
- (empty is the common case)
- Follow-up (#1084 sign-off): committed movers' fallback can pick a topology server absent from the fleet, abort, skip a usable server, and repair reports `Satisfied` while owed (`premark.rs:390`, `reconstruction.rs:503`) — file an issue; not reachable in the deployed loop.
- Follow-up (#1084 sign-off): committed repair/evacuation lack 0016's drain fence (`require_absent(desired:dserver:<S_new>)`) and the staged peer's drain filter (`premark.rs:386-395` vs `staged.rs:402-412`, `:526`, `:611`) — file an issue; predates #1084.
- Follow-up (#1084 sign-off): test gap — no test pins the vacated mark's stamp (`premark.rs:546`); add `rig.mark(SOURCE) == Some(legacy(stamp + after))` in the D7c/D7d one-ms-early twins (`move_destination_marks.rs:1987-1997`).
