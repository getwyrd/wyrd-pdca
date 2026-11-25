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
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass — run-verify.sh: PASS — red without the fix, green with it (42 test(s) ran red).
- C4 diff coverage: changed lines executed by the patch's tests: fail — diff coverage not measured — patch.diff does not apply on origin/main
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): fail — 68 mutants tested in 12m: 2 missed, 49 caught, 14 unviable, 3 timeouts

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 10 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_1084/review
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 13.58s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Issue #1084’s destination pre-marks for committed repair and evacuation need implementation iteration: a failed repair can certify success, and the vacated-mark fencing lacks a load-bearing regression.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The required durability outcomes, both map shapes, clock/deadline rules, library-level evacuation scope, and settled exceptions are explicit and falsifiable; `brief.md:46`, `brief.md:92`, `crates/custodian/src/premark.rs:50`. |
| C2 Reproduction (red pre-fix) | PASS | Independently stashing the tracked fix while retaining the new regression file produced 40 assertion failures and 2 passes, including stale destination marks and premature reclamation; `reviewer-red.log:315`. |
| C3 Change | PASS | The changes address the specified committed-move lifecycle and necessary fixture migration without adding dependencies or changing GC behavior; `crates/custodian/src/premark.rs:452`, `crates/custodian/src/rebalance.rs:677`, `crates/custodian/src/reconstruction.rs:1303`. |
| C4 Verification (red→green) | PASS | Restoring the patch produced 42/42 regression passes; the broader custodian run passed 512 tests, and nonce replay passed independently; full CI and TiKV compilation are supported by frozen logs, while diff coverage remains unmeasured because its base rejected the stacked patch; `reviewer-restored-green.log:49`, `reviewer-replay.log:243`, `gate-logs/C4-diff-cov.log:10`. |
| C5 Causal adequacy | NEEDS-HUMAN [impl] | Correct the new mark-conflict outcomes before claiming redundancy restored — pre-mark/adoption CAS losses can leave the original fragment missing yet return `Satisfied` (F1); `crates/custodian/src/reconstruction.rs:1306`, `crates/custodian/src/reconstruction.rs:1336`, `crates/custodian/src/reconstruction.rs:482`. |
| T1 Structure | PASS | A shared pre-mark lifecycle and destination matcher preserve narrow store seams, instance-local state, and one context clock; no capability-probe symptom guard was introduced; `crates/custodian/src/premark.rs:182`, `crates/custodian/src/premark.rs:459`, `crates/custodian/src/rebalance.rs:98`. |
| T2 Shape | PASS | Shared selection retains position-level exclusion, write authorization closes at the required boundary, and living architecture documentation accompanies the context change; `crates/custodian/src/premark.rs:319`, `crates/custodian/src/premark.rs:505`, `docs/design/architecture/06-runtime-view.md:40`. |
| T3 Runtime | FAIL | Six independent flat/segmented probes show a degraded repair returning `Satisfied` after mark-only pre-mark conflicts or GC defeating adoption; the obligation remains owed (F1); `reviewer-probes-final.log:55`, `reviewer-probes-final.log:89`, `crates/custodian/src/reconstruction.rs:536`. |
| T4 Contribution | FAIL | The completed review still has two confirmed, unresolved implementation/test findings; affected-path history and closed/unmerged PR checks ran, and contribution-artifact checking is separately N/A until publish; `gate-logs/T4-batch-review.log:10`, `gate-logs/T4-contribution.log:10`, `AGENTS.md:221`. |
| T5 Judgment | NEEDS-HUMAN [impl] | Add adversarial coverage of vacated-mark changes after their read — removing every vacated-mark adoption precondition still passes all 116 tests in the two relevant suites (F2), so the required fencing regression is not established; `crates/custodian/tests/move_destination_marks.rs:1400`, `crates/custodian/src/rebalance.rs:2242`, `reviewer-vacated-mutant.log:130`. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether the demonstrated durability behavior is sufficient for sign-off after F1/F2 are resolved — evacuation is exercised at the library seam, while deployed reconstruction still supplies no rebalance context; `crates/custodian/src/rebalance.rs:87`, `crates/server/src/custodian.rs:679`. |

**F1 — Repair can report restored redundancy after losing to GC (high priority).** Both new failure sites return `RepairOutcome::Conflict`: the pre-mark CAS at `crates/custodian/src/reconstruction.rs:1306` and adoption at `crates/custodian/src/reconstruction.rs:1336`. Their caller only emits telemetry (`crates/custodian/src/reconstruction.rs:482`); it sets neither `changed` nor `refused`. With no other work, the final branch returns `Satisfied` (`crates/custodian/src/reconstruction.rs:550`). A changed destination mark does not imply a competing placement winner: the original placement and missing fragment can be completely unchanged. This contradicts the certification contract at `crates/custodian/src/reconciliation.rs:34`.

The existing D5 test discards the repair verdict at `crates/custodian/tests/move_destination_marks.rs:1491`. In an isolated copy of that harness, retaining the verdict and asserting it is not `Satisfied` fails in all four existing D5 arms: flat/segmented, GC held after reclaim intent/after fragment deletion. Two additional probes change only the destination mark immediately before the pre-mark commit; both shapes also return `Satisfied` while retaining the repair obligation. These six failures are recorded in `reviewer-probes-final.log:55` and `reviewer-probes-final.log:89`. Classify these unfinished repairs as non-certifying, preserve the obligation, and cover both new CAS failure sites with verdict assertions and appropriate audit reasons. This is distinct from the settled in-place late-adoption exception #1100.

**F2 — The vacated-mark tests do not prove the CAS fence (medium priority).** The two D4 helpers plant a mark before the move starts and check its final value; neither changes it between the adoption’s read and commit (`crates/custodian/tests/move_destination_marks.rs:1407`, `crates/custodian/src/rebalance.rs:2249`). That proves classification and preservation, but not the preconditions at `crates/custodian/src/premark.rs:550` and `crates/custodian/src/premark.rs:554`.

I copied the custodian crate into isolated review scratch and removed only the `require_absent`/exact-value requirements from all three vacated-mark arms, retaining their existing puts/preservation. Both suites remained green: 74 library tests and 42 repair regressions (`reviewer-vacated-mutant.log:82`, `reviewer-vacated-mutant.log:130`). This directly misses the requested mutation in `brief.md:282`. Add absent→present and changed-exact-bytes races after the read, for both movers and map shapes, asserting failed adoption and preservation of the intervening mark. The frozen mutation run additionally reports uncovered refusal classification and blocked-count mutations; those are coverage evidence, not independently proven production defects (`gate-logs/C5-mutants.log:16`).

**The original regression evidence is reproducible, but does not discharge these findings.** All source citations above were checked in `$PDCA_TARGET`, the harness’s disposable copy. The base contains the prerequisite retirement drain and redb dependency. I stashed the tracked patch, ran the added test file against the base, restored the stash, and reran it. `git apply --reverse --check ../patch.diff` and `git diff --check` both passed afterward; the reviewed target source was not patched by the reviewer. Additional probes and the mutant were confined to the per-run scratch root.

| Evidence | Adjudication | Basis |
|----------|--------------|-------|
| C4-verify | PASS | Independent 40-fail/2-pass → 42-pass run agrees with `gate-logs/C4-verify.log:330`; commands used `cargo test --offline -p wyrd-custodian --test move_destination_marks`. |
| C4-ci | PASS | Frozen full workspace fmt/clippy/build/test/dependency/conformance/DST run ends successfully at `gate-logs/C4-ci.log:5159`; independently reran all custodian tests, the nonce replay, formatting, spelling, docs lint/render, and whitespace checks. This does not claim an independent rerun of the entire workspace gate. |
| C4-diff-cov | N/A | Coverage was never measured: the wrapper failed applying this dependent patch to `origin/main` (`gate-logs/C4-diff-cov.log:10`). This is a base-state caveat, not a compilation or patch-correctness defect; the supplied target supports the red→green run. |
| C5-mutants | FAIL | Frozen evidence shows 2 missed, 49 caught, 14 unviable, and 3 timeouts (`gate-logs/C5-mutants.log:18`); the independent vacated-mark mutant also survives. The complete 68-mutant campaign was not rerun. |
| T4-batch-review | FAIL | Ten reported rows reduce to the two confirmed findings above plus the dispositions below; unresolved F1/F2 prevent claiming the review work is settled (`gate-logs/T4-batch-review.log:10`). |
| T4-contribution | N/A | Artifacts are absent by design at Check; the substantive contribution audit must rerun at publish (`gate-logs/T4-contribution.log:10`). No human clearance is needed for this deferral. |
| host-tikv | PASS | Frozen output contains actual compilation of the TiKV crate and server selection arms, both successful; this was log-adjudicated, not independently rerun (`gate-logs/host-tikv.log:211`). |

The declared `typos` and docs-renderer dependencies were exercised without substitutes. Independent docs rendering generated 101 pages and passed its link audit (`reviewer-docs.log:1`). The custodian run totals 512 passed, zero failed, and one explicitly ignored privileged Tier-1 test (`reviewer-custodian-tests.log:722`). The nonce replay passed under `RUSTFLAGS="--cfg madsim"` (`reviewer-replay.log:241`). No deployment/container validation is claimed. The brief excludes those scenarios; the repository’s optional follow-up remains `cargo xtask disk-faults` on a suitable host, observing reconstruction’s return to full redundancy without read errors.

**The other batch reports do not establish additional in-scope defects.**

- The in-place `reclaiming` refusal follows D4’s explicit prohibition (`brief.md:92`) and the no-writer-overwrites-reclaiming contract (`crates/core/src/metadata.rs:154`). Allowing this write would contradict the specified safety rule; the suggested eligibility change is rejected for this patch.
- The two outside-fleet fallback reports were investigated. With candidate order 2→1→5, position 2 forbidden and server 1 absent from the fleet, both shapes abort; removing candidate 1 lets the next pass repair on 5 (`reviewer-probes-final.log:84`). This demonstrates the existing permitted outside-fleet abort contract, not a requirement that this patch exhaust every reachable alternative: `brief.md:72` explicitly anticipates that abort, and `crates/custodian/src/premark.rs:340` preserves it. I do not turn that broader placement-policy concern into another implementation request here.
- The DST rival writes its vacated mark without a precondition (`crates/dst/tests/custodian.rs:6181`), but this particular property starts with no vacated mark and schedules only the placement/root rival inside adoption; it schedules no GC writer to that key (`crates/dst/tests/custodian.rs:6257`, `crates/dst/tests/custodian.rs:6214`). The claimed forbidden interleaving is not demonstrated by this property. Its root and segment pins are present. Expanded seeded GC interleavings are explicitly deferred to #682; that deferral, #825, and #1100 remain settled. F2 concerns the separately required scripted fencing regression and does not reopen them.

**Prior art was checked by affected production path, including rejected work.** Independent GitHub queries for `crates/custodian/src/reconstruction.rs` and `crates/custodian/src/rebalance.rs` returned the main-history commits listed in `brief.md:313`, including [the staged pre-mark implementation](https://github.com/getwyrd/wyrd/commit/5377850562b1fd1c3a297b8b03b2a17cc19b179d), [segmented repair](https://github.com/getwyrd/wyrd/commit/9599aad732c24996670d1d705e0641bf18685731), and [segmented evacuation](https://github.com/getwyrd/wyrd/commit/05387201b3a0321527509e2b5497bd89676569d1). Closed-PR searches by both exact paths returned the related historical work; repeating with `is:unmerged` returned only [the broader segmented-map proposal PR #647](https://github.com/getwyrd/wyrd/pull/647), not a rejected committed-destination pre-mark fix. Query summaries are saved in `reviewer-prior-art.json`. The pre-fix execution independently confirms the committed-path defect remains in this bundle’s base.
<!-- pdca:leaf-complete -->

### Advisory — adversary

# Adversarial review — issue 1084 (committed moves pre-mark their destinations)

Method: re-read the patched tree at `$PDCA_TARGET`, then ran probes and mutations in a scratch copy of it (`cargo test -p wyrd-custodian`, every probe driven through `reconcile_step` with the patch's own rig). One finding breaks a stated invariant; two are test gaps that let the fix's own claims regress unseen.

## Refutations

- NEEDS-HUMAN [impl] — **A repair that loses to GC still answers `Satisfied`.** `crates/custodian/src/reconstruction.rs:1303-1307` (pre-mark lost) and `:1329-1337` (adoption lost) both return `RepairOutcome::Conflict`. `reconcile` only logs that (`:482`), leaves it out of `hole` (`:536`), and answers `Satisfied` (`:549-550`). Before this patch, `Conflict` only meant "another writer moved the record". Now it also covers a destination mark that GC changed, and in that case nothing else will repair the chunk. Two probes, both shapes, each answered `Satisfied` with the obligation still queued and the placement still naming lost server 5: (A) the D5 race exactly as `d5` scripts it (adoption released while the key holds `reclaiming`); (B) GC's swap to `reclaiming` landing between the assessment and the pre-mark commit (`Act::PlantFirst` on `premarks_free`), so the pre-mark loses and nothing is written. This breaks the brief's invariant ("no pass reports redundancy restored while a repair it owes is still undone"). It also disagrees with evacuation: rebalance treats any move that did not persist as non-certifying (`crates/custodian/src/rebalance.rs:203`), and its own D5 leg asserts `Blocked` (`rebalance.rs:2335`). The repair D5 and D6 legs throw the verdict away (`crates/custodian/tests/move_destination_marks.rs:1491`, `:1532`), so the suite can't see this. This confirms T4's findings at `reconstruction.rs:1306/1308/1336`; a reviewer should not wave them off as "a lost CAS is not a hole". Fix: stop certifying when a pre-mark or adoption loses while the root is still the generation the move planned on (`still_current`, `:1345`), and assert the verdict in D5/D6.
- NEEDS-HUMAN [impl] — **No test checks the vacated-mark precondition** (Re-plan item 6, the blind write iteration 3 was faulted for). I removed all three preconditions in `crates/custodian/src/premark.rs:548-557` and kept the read-based put/keep choice. Every `wyrd-custodian` test still passed: 74 lib tests (the inline rebalance legs included), 42 in `move_destination_marks.rs`, and every other test file. `d4_vacated` (`move_destination_marks.rs:1400-1427`) and its rebalance twin plant the mark *before* the move, so they only test what the adoption read, not what it pins. Add a leg that changes the vacated mark between the adoption's read (`premark.rs:538`) and its commit (`:559`), and assert that the adoption loses and the new bytes survive.
- NEEDS-HUMAN [impl] — **The C5 survivors are real test gaps.** First, `premark.rs:373-375` (`ruled_out -> true`): no test tells a capacity-only `Assessment::Blocked` apart from a mark-caused `Assessment::Refused` (`reconstruction.rs:925-929`). Under that mutant, every pass with no free domain would log `repair-refused`/`no-usable-destination` and answer `Blocked`, and nothing fails. Second, `reconstruction.rs:379` (`+=` → `*=`): nothing reads the `repair_blocked` gauge for a refused chunk. One leg with no free domain and no marks, asserting the verdict and the absence of `repair-refused`, plus one gauge assertion, would close both.
- NEEDS-HUMAN [impl] — **The DST rival mover is a hand-written copy that writes the vacated mark blind** (`crates/dst/tests/custodian.rs:6174-6181`). Production pins that mark (`premark.rs:548-557`), so the seeded model allows an adoption production would reject. This falls under the rubric's *Test fidelity* class. Small fix: read the vacated key and pin it, as `PreMarked::adopt` does.

## Attempted and could not refute

- **Red→green evidence.** The legs go through `reconcile_step` (`move_destination_marks.rs:917`) over `RedbMetadataStore`. The D-server double checks the deadline before storing and checks it again after publishing, using the test clock (`:645-662`), as `FsChunkStore` does. C4-verify shows all 42 legs red on the base. This is the production path, not a parallel re-implementation.
- **Per-move nonce.** With `move_event` forced to a constant nonce, all four D8 legs (both movers, both shapes) went red. The same-seed replay test exists (`crates/dst/tests/custodian.rs:6848`) and passed in C4-ci's madsim leg (`gate-logs/C4-ci.log:5046`).
- **Write gate and deadline.** `authorize` uses strict `<` against `stamp + W_REPOINT_MILLIS` (`premark.rs:505`), and the deadline comes from the pre-mark's own stamp. Every C5 mutant on those lines was caught. D7d tests the exact boundary.
- **Fleet vs topology (T4 at `premark.rs:373`, `reconstruction.rs:924`).** Not reachable in the deployed loop: `live_reconstruction_view` adds a server to the fleet and the topology together (`crates/server/src/custodian.rs:303-305`). Only a library-built context could hit it. Not raised.
- **In-place late-adoption race.** Deferred to #1100 with in-code markers (`premark.rs:50-56`, `reconstruction.rs:1317`). Settled; not raised.

## Gate notes (not refutations)

- C4-diff-cov's "fail" is about the base, not the fix: the bundle is built on #1013's head, as the brief requires, so `patch.diff` does not apply on `origin/main`. Diff coverage was never measured, so the gate says nothing about whether this fix works.

<!-- pdca:leaf-complete -->

### Advisory — code-review

- NEEDS-HUMAN [impl] — `crates/custodian/src/reconstruction.rs:1306` and `crates/custodian/src/reconstruction.rs:1336`: The new mark-only CAS failures are classified as `Conflict`, whose handler does not withhold certification (`crates/custodian/src/reconstruction.rs:482`). If GC changes the destination mark before pre-marking, or reclaims the destination before adoption, the original placement can remain unchanged and degraded. With no other work, the pass nevertheless returns `Satisfied`, leaving its repair queued; a caller driving until satisfaction can stop prematurely. Return a non-`Satisfied` outcome unless the repair is shown to be resolved. Capture and assert this verdict in D5, which currently discards it (`crates/custodian/tests/move_destination_marks.rs:1491`), and cover the pre-mark conflict too.

- NEEDS-HUMAN [impl] — `crates/custodian/tests/move_destination_marks.rs:1400` and `crates/custodian/src/rebalance.rs:2242`: The tests named “writes the vacated mark under a precondition” plant the mark before the move starts and never change it between the adoption's read and commit. They verify mark classification/preservation, but would still pass with the vacated `require_absent`/exact-value preconditions removed from `crates/custodian/src/premark.rs:550`. For both movers, interrupt adoption after its read, replace an absent or stamped vacated mark with `reclaiming`, then verify adoption conflicts and preserves those bytes. This distinguishes the concurrency protection from a blind overwrite that merely preserves marks already reclaiming when read.

No additional reuse, simplification, or efficiency findings. Reviewed against the target source and frozen gate evidence; no builds or tests were rerun.

<!-- pdca:leaf-complete -->

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C5 Causal adequacy — Correct the new mark-conflict outcomes before claiming redundancy restored — pre-mark/adoption CAS losses can leave the original fragment missing yet return `Satisfied` (F1); `crates/custodian/src/reconstruction.rs:1306`, `crates/custodian/src/reconstruction.rs:1336`, `crates/custodian/src/reconstruction.rs:482`.
- [ ] T5 Judgment — Add adversarial coverage of vacated-mark changes after their read — removing every vacated-mark adoption precondition still passes all 116 tests in the two relevant suites (F2), so the required fencing regression is not established; `crates/custodian/tests/move_destination_marks.rs:1400`, `crates/custodian/src/rebalance.rs:2242`, `reviewer-vacated-mutant.log:130`.
- [ ] Validation — fitness-to-purpose — Decide whether the demonstrated durability behavior is sufficient for sign-off after F1/F2 are resolved — evacuation is exercised at the library seam, while deployed reconstruction still supplies no rebalance context; `crates/custodian/src/rebalance.rs:87`, `crates/server/src/custodian.rs:679`.
- [ ] **A repair that loses to GC still answers `Satisfied`.** `crates/custodian/src/reconstruction.rs:1303-1307` (pre-mark lost) and `:1329-1337` (adoption lost) both return `RepairOutcome::Conflict`. `reconcile` only logs that (`:482`), leaves it out of `hole` (`:536`), and answers `Satisfied` (`:549-550`). Before this patch, `Conflict` only meant "another writer moved the record". Now it also covers a destination mark that GC changed, and in that case nothing else will repair the chunk. Two probes, both shapes, each answered `Satisfied` with the obligation still queued and the placement still naming lost server 5: (A) the D5 race exactly as `d5` scripts it (adoption released while the key holds `reclaiming`); (B) GC's swap to `reclaiming` landing between the assessment and the pre-mark commit (`Act::PlantFirst` on `premarks_free`), so the pre-mark loses and nothing is written. This breaks the brief's invariant ("no pass reports redundancy restored while a repair it owes is still undone"). It also disagrees with evacuation: rebalance treats any move that did not persist as non-certifying (`crates/custodian/src/rebalance.rs:203`), and its own D5 leg asserts `Blocked` (`rebalance.rs:2335`). The repair D5 and D6 legs throw the verdict away (`crates/custodian/tests/move_destination_marks.rs:1491`, `:1532`), so the suite can't see this. This confirms T4's findings at `reconstruction.rs:1306/1308/1336`; a reviewer should not wave them off as "a lost CAS is not a hole". Fix: stop certifying when a pre-mark or adoption loses while the root is still the generation the move planned on (`still_current`, `:1345`), and assert the verdict in D5/D6.
- [ ] **No test checks the vacated-mark precondition** (Re-plan item 6, the blind write iteration 3 was faulted for). I removed all three preconditions in `crates/custodian/src/premark.rs:548-557` and kept the read-based put/keep choice. Every `wyrd-custodian` test still passed: 74 lib tests (the inline rebalance legs included), 42 in `move_destination_marks.rs`, and every other test file. `d4_vacated` (`move_destination_marks.rs:1400-1427`) and its rebalance twin plant the mark *before* the move, so they only test what the adoption read, not what it pins. Add a leg that changes the vacated mark between the adoption's read (`premark.rs:538`) and its commit (`:559`), and assert that the adoption loses and the new bytes survive.
- [ ] **The C5 survivors are real test gaps.** First, `premark.rs:373-375` (`ruled_out -> true`): no test tells a capacity-only `Assessment::Blocked` apart from a mark-caused `Assessment::Refused` (`reconstruction.rs:925-929`). Under that mutant, every pass with no free domain would log `repair-refused`/`no-usable-destination` and answer `Blocked`, and nothing fails. Second, `reconstruction.rs:379` (`+=` → `*=`): nothing reads the `repair_blocked` gauge for a refused chunk. One leg with no free domain and no marks, asserting the verdict and the absence of `repair-refused`, plus one gauge assertion, would close both.
- [ ] **The DST rival mover is a hand-written copy that writes the vacated mark blind** (`crates/dst/tests/custodian.rs:6174-6181`). Production pins that mark (`premark.rs:548-557`), so the seeded model allows an adoption production would reject. This falls under the rubric's *Test fidelity* class. Small fix: read the vacated key and pin it, as `PreMarked::adopt` does.
- [ ] `crates/custodian/src/reconstruction.rs:1306` and `crates/custodian/src/reconstruction.rs:1336`: The new mark-only CAS failures are classified as `Conflict`, whose handler does not withhold certification (`crates/custodian/src/reconstruction.rs:482`). If GC changes the destination mark before pre-marking, or reclaims the destination before adoption, the original placement can remain unchanged and degraded. With no other work, the pass nevertheless returns `Satisfied`, leaving its repair queued; a caller driving until satisfaction can stop prematurely. Return a non-`Satisfied` outcome unless the repair is shown to be resolved. Capture and assert this verdict in D5, which currently discards it (`crates/custodian/tests/move_destination_marks.rs:1491`), and cover the pre-mark conflict too.
- [ ] `crates/custodian/tests/move_destination_marks.rs:1400` and `crates/custodian/src/rebalance.rs:2242`: The tests named “writes the vacated mark under a precondition” plant the mark before the move starts and never change it between the adoption's read and commit. They verify mark classification/preservation, but would still pass with the vacated `require_absent`/exact-value preconditions removed from `crates/custodian/src/premark.rs:550`. For both movers, interrupt adoption after its read, replace an absent or stamped vacated mark with `reclaiming`, then verify adoption conflicts and preserves those bytes. This distinguishes the concurrency protection from a blind overwrite that merely preserves marks already reclaiming when read.
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 10 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_1084/review
- [ ] **The repair round trip is not forced by the stated setup.** `brief.md:63-76` says repair is forced like evacuation and prescribes “lose it on 5, then on 2.” On the target, evacuation excludes draining servers (`crates/custodian/src/rebalance.rs:528`), but committed repair excludes only survivors’ domains (`crates/custodian/src/reconstruction.rs:1122-1126`). Both servers 2 and 5 therefore remain eligible after either loss; the selector orders them by utilization and domain label (`crates/core/src/placement.rs:289-301`). With unchanged topology/utilization, a first selection of 2 is followed by an in-place repair on 2, not the required return to 5. Specify the per-pass topology/utilization or drain-state changes that force each direction, and require placement assertions before the mark assertions. Otherwise D2 fails for fixture reasons even after the destination-mark fix.
- [ ] **D7a cannot detect its promised boundary mutation.** `brief.md:91-94` advances the clock past both the write-authorization window and the pre-mark’s grace, while `brief.md:246-247` requires this leg to kill `<` → `<=`. Both comparisons refuse a write at the prescribed late instant; they differ only at exactly `stamp + W_REPOINT_MILLIS`. The target’s peer rejects equality (`crates/custodian/src/reconstruction/staged.rs:680`), and its constants leave room to exercise that boundary before the D-server deadline (`crates/custodian/src/gc.rs:202,218`: 30,000 versus 10,000 ms). Add a distinct exact-boundary arm asserting zero destination calls and a retained obligation; keep the post-GC arm for the missing-gate mutation.
- [ ] **The nonce’s replay requirement lacks the claimed verification.** `brief.md:167-171` proposes OS-seeded `RandomState` while requiring deterministic seed replay, checked with `cargo xtask ci`. The target’s DST runner sets `MADSIM_TEST_NUM=50` and runs the tests (`xtask/src/main.rs:1811-1817`); it does not enable the separate `MADSIM_TEST_CHECK_DETERMINISM` option documented at `crates/dst/src/lib.rs:90-91`. D8 checks distinct identities within one execution, not reproducibility across executions. Require an explicit repeated-same-seed check that observes the minted pre-mark identities and relevant outcomes, and state its command/assertion. A green seed sweep alone cannot establish the proposed nonce source satisfies the replay constraint.
- [ ] size backstop — this slice is behaving oversized: patch is 389 KB (threshold 100 KB); patch touches 24 files (threshold 20). Recommend answering `iterate-plan` at sign-off and authoring the split in the re-plan (`pdca split`), rather than `iterate-do`: a slice that is too big yields implementation-shaped findings every round, and splitting authors briefs, which is Plan's beat.

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
- Iteration delta (if iterating): Keep the design (destination pre-mark + adoption + write gate; nonce and deadline held). Production logic is ~67 KB; the size backstop fires on test volume, so this is not a split. Fix only: - F1: a pre-mark or adoption that loses on the destination mark while the root is still the planned generation (`still_current`) must not answer `Satisfied` (`reconstruction.rs:1306/1336/482`). Match rebalance (non-certifying → `Blocked`). Capture and assert the verdict in repair D5 and D6 (`move_destination_marks.rs:1491/1532`), and add the pre-mark-conflict probe (GC swap between assessment and pre-mark). - F2: for both movers and both shapes, change the vacated mark between the adoption's read (`premark.rs:538`) and commit (`:559`), absent→reclaiming and stamped→reclaiming. Assert that the adoption loses and the bytes survive. - C5 survivors (`premark.rs:373`, `reconstruction.rs:379`): one leg with no free domain and no marks, asserting `Blocked` and no `repair-refused`, plus a `repair_blocked` gauge assertion. - DST rival (`crates/dst/tests/custodian.rs:6174-6181`): read and pin the vacated mark, as `PreMarked::adopt` does. No new features. Don't grow the patch beyond these.
- By / date: Eduard Ralph / 2026-11-10

## 10. Act candidates (hints for the next Act review)
- Plan advisory: 3 finding(s); brief revised: yes (plan-advisory-*.md)
- (empty is the common case)
