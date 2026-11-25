# Result — issue 1015 / multipart-reaper-open-sessions

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect: An `Open` multipart session whose client went silent (never aborted, never
  completed) is never fenced. Its staged parts stay protected on purpose (#637, 0016 decision
  2), its admission slot is never released, and `MAX_SESSIONS` such sessions make a permanent
  `503 SlowDown`. 0016 makes the reaper a hard ordering requirement (`0016:234-268`). Nothing in
  the custodian evaluates an `Open` session today, and no operator command runs such a pass.
- Success criterion: a NEW test file `crates/server/tests/multipart_reaper.rs` drives the
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
- Repo + branch target: getwyrd/wyrd @ main
- Scope: decision 6's `Open`-session half as a custodian loop dispatched from
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

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: likely-fix
- Confidence: medium
- Recommendation: (set by Do)

## 3. Correctness (Check — chain)
- C1 Spec: none — brief.md
- C2 Reproduction (red pre-fix): none — (no gate configured)
- C3 Change: none — patch.diff
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): unverifiable — gate exceeded its 7200s timeout
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass — run-verify.sh: PASS — red without the fix, green with it (6 test(s) ran red).
- C4 diff coverage: changed lines executed by the patch's tests: fail — diff coverage not measured — patch.diff does not apply on origin/main
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): pass — 166 mutants tested in 10m: 148 caught, 18 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 14 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_1015/review
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 13.58s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

#1015 adds reclamation of abandoned Open multipart uploads through the custodian and an operator one-shot; two reproduced defects, race-test gaps, and a drain-ownership decision remain.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The Open-only scope, durable outcomes, prerequisite drain, and accepted slot-cycle deferral are explicit; judge this slice against those boundaries (brief.md:30, brief.md:165, brief.md:281). |
| C2 Reproduction (red pre-fix) | PASS | Independently stashing the tracked fix while retaining the new subprocess test ran six tests and produced six assertion failures; leg B's durable preservation remains vacuous on the base, with its failure coming from the command-status assertion (reviewer-red-green.log:11, reviewer-red-green.log:101; crates/server/tests/multipart_reaper.rs:569). |
| C3 Change | PASS | The patch reaches both the operator command and the deployed fleet's fenced control point, with the living CLI/runtime documentation updated; no additional state-machine scope is required for this wiring (crates/server/src/cli.rs:1097; crates/server/src/custodian.rs:677; docs/design/architecture/06-runtime-view.md:94). |
| C4 Verification (red→green) | NEEDS-HUMAN | Obtain a complete dependency audit on a checkout retaining the pinned RustSec submodule — independent red→green and focused checks pass, but this disposable target lacks that dependency and the frozen full CI timed out; neither establishes full CI success (reviewer-red-green.log:177; reviewer-ci.log:3752, reviewer-ci.log:4406; gate-logs/C4-ci.log:7). |
| C5 Causal adequacy | NEEDS-HUMAN [impl] | Make an absent admission profile refuse dependent teardown and keep independent reaping reachable after reconstruction errors — the reproduced cases otherwise mutate unchecked sessions or indefinitely retain abandoned uploads (crates/custodian/src/reaper.rs:425; crates/custodian/src/reconciliation.rs:170, crates/custodian/src/reconciliation.rs:236). |
| T1 Structure | PASS | The existing teardown fence and retirement machinery remain the shared authorities, and the new record-only entry uses the same drain; the trait and clock seams remain coherent (crates/custodian/src/reaper.rs:919; crates/custodian/src/retirement.rs:571; crates/server/src/custodian.rs:630). |
| T2 Shape | PASS | Session work resumes across passes, range reads are bounded, and the largest fence is checked against independently counted operation and byte costs (crates/custodian/src/reaper.rs:177, crates/custodian/src/reaper.rs:535; crates/custodian/src/reaper/tests.rs:812). |
| T3 Runtime | FAIL | A recurring repair-prefix read error prevents every combined fleet pass from reaching the reaper, although reaping succeeds against that same store independently; this defeats eventual admission-slot recovery (crates/custodian/src/reconciliation.rs:168; reviewer-reconstruction-fault.log:1). |
| T4 Contribution | FAIL | The reproduced review concerns still need a fixed or rejected disposition under the standing rubric; the separate contribution-artifact check is N/A because its substantive audit is mandatory at publish (gate-logs/T4-batch-review.log:10; gate-logs/T4-contribution.log:10; crates/custodian/src/reaper.rs:425). |
| T5 Judgment | NEEDS-HUMAN [impl] | Replace X60/X62's fabricated expired-lease transitions with reachable production interleavings and demonstrate the specified negations — their current passing results do not establish the required live-upload safety (crates/dst/tests/custodian.rs:7325, crates/dst/tests/custodian.rs:7374; crates/core/src/multipart/stage.rs:1581). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Sign off this Open-only slice's operational fitness and assign ownership of the post-check first-Create/drain race — profile refusal can still be bypassed after the final read, while the brief reserves drain changes for #659's layer (crates/custodian/src/reaper.rs:64; brief.md:13; reviewer-postcheck-race.log:1). |

Source citations above and below resolve under `$PDCA_TARGET`; brief, gate-log and reviewer-log citations resolve in the review directory. The supplied target is coherent: both test builds succeeded, the stash was restored, and `git apply --reverse --check ../patch.diff` succeeded. No production or submitted test source was edited.

The implementation corrections are supported by independent execution:

- **F1 — Missing admission evidence still authorizes mutations.** With an Aborting session, committed part, owned residue and obligation but no `mpuctl`, the real operator one-shot returned exit 1 while creating **six orphan marks and deleting the owned entry** (`reviewer-missing-ledger.log:1`). The absent-ledger branch only increments `held`, then returns `Some(pass)` (`crates/custodian/src/reaper.rs:425`); `Reaped::Ran(Blocked)` still authorizes retirement (`crates/custodian/src/reconciliation.rs:236`). Treat this store-wide loss of profile evidence as a refusal, preserving foreign-clock teardown when a matching ledger exists. Add an end-to-end absent-ledger/no-mutation case; the present unit test exercises only the judgment (`crates/custodian/src/reaper/tests.rs:959`).
- **F2 — A reconstruction fault can starve reaping indefinitely.** A redb-backed adapter failing only `scan("repair:")` left an idle session Open for three combined reconstruction/reaper passes. The same adapter allowed a reaper-only pass to fence it to Aborting (`reviewer-reconstruction-fault.log:1`). The early `?` precedes the new dispatch (`crates/custodian/src/reconciliation.rs:170`); the repair read is at `crates/custodian/src/reconstruction.rs:221`. Preserve the reconstruction error while allowing the independent reaper its turn, with a regression over both inputs.
- **F3 — Required race evidence bypasses production lease semantics.** X60 commits a fresh part after a slot lease that expired days earlier (`crates/dst/tests/custodian.rs:7325`); X62 similarly installs a fresh renewal using a handwritten batch (`crates/dst/tests/custodian.rs:7365`). Production refuses to prepare a renewal from that lapsed authority (`crates/core/src/multipart/stage.rs:1581`). X62 also never shows the upload subsequently committing. Exercise the production renewal prepared while live, hold its commit across expiry and the reaper's reads, then release it and verify both the conflicting fence and eventual part commit, as required at `docs/design/proposals/draft/0016-multipart-commit-protocol.md:2275`. The improved pre-first-chunk test does drive production renewal and subsequent commit (`crates/dst/tests/custodian.rs:7585`, crates/dst/tests/custodian.rs:7630`).

The remaining scope decision concerns an actual unchecked interval. In a separate record-interleaving probe, an initially empty redb store acquired a different-profile ledger and a fresh session's completed Abort state immediately after the second ledger read. `reconcile_step` returned `Changed`, wrote six marks, and removed the owned residue (`reviewer-postcheck-race.log:1`). This models the metadata interleaving, not a full production Create/Upload/Abort execution. The implementation explicitly acknowledges this interval (`crates/custodian/src/reaper.rs:64`). Its existing first-Create tests instead preseed sessions and inject only a ledger write (`crates/custodian/src/reaper/tests.rs:906`), so they do not discharge this boundary. Assign the fix across the reaper/#659 seam before claiming profile refusal is complete. The previously accepted slot reserve→commit→release gap remains settled and is **not** re-raised (brief.md:284; crates/dst/tests/custodian.rs:8247).

Verification and frozen-gate adjudication:

- **Independent PASS:** subprocess red 0/6 → green 6/6; 23 reaper unit tests; 13 GC tests, including physical reclamation exactly at grace and the combined reconstruction/reaper branch; 11 deployed-GC tests; six reaper DST campaigns with `RUSTFLAGS="--cfg madsim" MADSIM_TEST_NUM=50`, with the accepted slot-cycle case ignored (`reviewer-red-green.log:101`, `reviewer-red-green.log:177`, `reviewer-reaper-units.log:79`, `reviewer-gc.log:20`, `reviewer-custodian-gc.log:18`, `reviewer-dst.log:240`). Physical GC is post-change evidence, not the binding red→green.
- **Independent scanners:** `git diff --check`, formatting, `typos`, documentation lint and rendering passed; the renderer audited 101 pages (`reviewer-doc-lint.log:1`, `reviewer-doc-render.log:2`). The brief's `typos` and `docs-renderer` dependencies were exercised without shims.
- **C4-ci — host caveat / incomplete audit:** the frozen run ended at its 7,200-second limit in `custodian_gc` (`gate-logs/C4-ci.log:3005`). That suite passes independently here. A full `cargo xtask ci` rerun finished with exit 1: the unchanged advisory-db test and advisory-pin stage fail because `.github/advisory-db` has no gitlink in the synthetic target's index (`reviewer-ci.log:3752`, `reviewer-ci.log:4406`; `xtask/tests/advisory_db.rs:65`). The dependency audit was therefore not exercised. The other executed checks, including DST, passed; these host failures are not patch defects.
- **C4-verify — PASS:** the frozen six-test red→green is independently reproduced (`gate-logs/C4-verify.log:110`). **C4-diff-cov — unmeasured base-state caveat:** its log says the patch did not apply to `origin/main`; it provides no coverage number and no compilation defect (`gate-logs/C4-diff-cov.log:10`).
- **C5-mutants — logged PASS:** the captured run reports 166 mutants, 148 caught and 18 unviable; no surviving mutant is shown (`gate-logs/C5-mutants.log:10`). This aggregate is not evidence that the brief's named concurrency negations were exercised. **host-tikv — logged PASS:** both feature clippy commands completed (`gate-logs/host-tikv.log:2`, `gate-logs/host-tikv.log:211`). These instance-scoped runs were adjudicated from their supplied logs.
- **T4-batch-review — logged FAIL, triaged:** its 14 entries include duplicates and unproven scenarios; they are not treated as 14 confirmed defects. F1/F2 were independently reproduced and F3 grounded in the required contract. **T4-contribution — N/A:** artifacts are absent by design and the substantive audit must rerun at publish (`gate-logs/T4-contribution.log:10`).

Prior-art searches ran by affected implementation, test and architecture paths, against main's file history and closed-PR discussions. No prior reaper module or subprocess test was returned; related hits concern the prerequisite multipart and custodian work. Captured path queries and results are in `reviewer-prior-art.json:1` and `reviewer-prior-art-extra.json:1`. The scratch repository itself contains only the harness's synthetic base, so it cannot independently establish merged history. These path searches are a bounded prior-art check, not a claim to have audited every historical PR diff. No additional `INTEGRATION.md` was supplied or found in the target. Real fleet Tier-1/Tier-2 observation remains follow-up under the standing rubric, not evidence claimed by these metadata and simulation tests.

<!-- pdca:leaf-complete -->

### Advisory — adversary

# Adversarial review — #1015 (625.1: multipart reaper, `Open` half)

I could not refute the red→green or the reaper's core judgment. I found one concrete hole the builder can fix, one #659-layer defect the reaper exposes, and three calls a human has to make. All path:line citations are against `$PDCA_TARGET` (the post-fix tree). Every probe below ran the real `wyrd` binary or the real DST rows, from a scratch copy of that tree.

## Findings

- NEEDS-HUMAN [impl] — **A lost `mpuctl` ledger makes the reaper hold, but the drain still writes.** `crates/custodian/src/reaper.rs:425-435` turns "sessions listed, `mpuctl` still absent at the last read" into a per-session hold (`Reaped::Ran(Blocked)`). `crates/custodian/src/reconciliation.rs:236-238` then lets the retirement leg run (`:179-185` deployed, `:189-199` one-shot). Concrete case, run through `wyrd custodian --reap-abandoned-uploads`: one `Aborting` session with its `retire:bytes:s:<id>:0` obligation, a committed part, in-flight residue, and **no** `mpuctl`. One-shot 1 wrote 6 `orphan:` marks and deleted the `sidx:` entry. One-shot 2 deleted `part:`, `psum:` and the obligation. Only the terminal delete stopped, at `crates/custodian/src/retirement/sessions.rs:958`. The same store with `mpuctl` set to undecodable bytes is refused, and the store diff is empty. Both cases mean "profile unknown", but only one of them stops the drain. The patch's own rule says it should stop: `reaper.rs:19-23` ("a custodian sized for another profile acts on neither") and the 06-runtime-view paragraph both say the drain needs a checked profile. The unit test `crates/custodian/src/reaper/tests.rs:950-964` checks only the pass's own tally, over an `Open` session the drain would never touch, so it cannot see this. Fix: answer `Reaped::Refused` when the last read is still `Absent` and the listing is not empty. Then add a step-level test (an `Aborting` session plus its obligation, no `mpuctl`) that asserts the store is byte-identical. The other option is to document why the drain is safe with no profile, which would undercut the refusal rationale. T4 flagged this at `reaper.rs:425`/`:436`, and it reproduces.

- NEEDS-HUMAN [human] — **A stray obligation beside an `Open` session: #659's drain deletes a still-open session's committed part, and the reaper raises no damage signal.** The fixture is an idle `Open@0` session plus a `retire:bytes:s:<id>:0` key. That is the state the client Abort names as damage (`Stop::ObligationKeyTaken`, `crates/core/src/multipart/abort.rs:337-341`, classified by `lost()` at `:374`). The reaper's idle fence conflicts, and `reaper.rs:964-970` counts that as `lost`. The pass answers `Partial` and exits 0, with no hold and no NEEDS-HUMAN line. In the same steps, the retirement leg acts on the stray obligation. One-shot 1 marked the committed part's three positions. One-shot 2 deleted `part:<id>:000001` and `psum:<id>:000001` while `mpu:<id>` was still `Open@0`. The reaper's fence landed only in one-shot 3. So T4's "Conflict forever" claim (`reaper.rs:964`) does **not** reproduce as stated. The reason it doesn't is worse: the drain tears down a session that is still open. This only happens on a damaged store, because no normal writer leaves that key beside an unchanged `Open@E`. Under the brief it is a §6 item against #659's drain: does the drain check that the session has left `Open` before acting on a `{session, all}` obligation? A reaper-side mirror of Abort's `lost()` (hold on `Taken`) would add the missing signal, but it would not stop the drain, because holds don't gate the leg.

- NEEDS-HUMAN [human] — **The C4-ci gate (it blocks acceptance) timed out. The stall does not reproduce and looks environmental, but the gate is still not green.** The log stops in `crates/server/tests/custodian_gc.rs`, with 9 tests "running for over 60 seconds" (`gate-logs/C4-ci.log:3000-3013`). One of the stuck tests, `deployed_run_loop_refuses_duplicate_endpoints`, returns at `crates/server/src/cli.rs:1160-1173` without running any reaper code (it does not take the one-shot branch at `:1102`). I re-ran `custodian_gc` 6 times scoped to its package, and once with the gate's `--workspace --exclude wyrd-dst` feature set. It passed 11/11 every time, in about 0.2 s. `multipart_reaper` passed 6/6. Every `wyrd-custodian` reaper test passed inside the gate log itself (`C4-ci.log:1742-1764`, `:1799-1800`). The gate never reached the DST (seeded simulation) tier, so I ran it myself under `--cfg madsim` with 50 seeds: 6 reaper rows pass, and the slot-cycle row stays ignored, as accepted at sign-off. Still not run by any gate: `cargo deny`, `cargo machete`, conformance, and the DST clippy leg. A human should re-run `cargo xtask ci` before sign-off.

- NEEDS-HUMAN [human] — **Diff coverage has gone unmeasured for a third round.** The gate reports "patch.diff does not apply on origin/main" (`gate-logs/C4-diff-cov.log`). Both earlier carry-forwards asked for a rebase. If the base carries prerequisites that are not on `main` yet (the brief's ordering note says #1007 had no published branch), rebasing cannot fix this. A human has to decide whether to land the prerequisites first or accept C5 (166 mutants: 148 caught, 18 unviable, 0 missed) as the substitute.

- NEEDS-HUMAN [human] — **In the deployed loop, a reconstruction error also stops the reaper.** The reaper runs inside the reconstruction pass's `reconcile_step` (`crates/server/src/custodian.rs:671-695`), after `reconstruction::reconcile(...)?` at `crates/custodian/src/reconciliation.rs:168-170`. If reconstruction hits a store error every tick, the reaper and the drain are skipped every interval. The only log line says "reconstruction pass failed" (`custodian.rs:693`). Abandoned sessions then keep their admission slots until an operator runs the one-shot. The same file rejects this kind of shared failure for scrub and GC (`custodian.rs:640-647`, the Codex #461 rule). The brief chose this wiring ("the deployed fleet loop's existing `reconcile_step` call passes the reaper context"), so this is a scope call, probably for 625.2's driver, not a builder fix. T4 flagged it at `reconciliation.rs:170`.

## Attempted to refute, could not

- **The red→green.** I re-ran `multipart_reaper` green (6/6). On the base, the red is the parse failure the brief predicts (`flag --reap-abandoned-uploads needs a value`, `gate-logs/C4-verify.log`). Leg B goes red on the base only through its exit-status assertion. That fits the brief, which says B is vacuous on the base and must be shown by negations.
- **That the tests are inert.** I broke the code three ways in a scratch copy, and each named test failed. Reading `psum:` before `slot:` fails DST X60 (`crates/dst/tests/custodian.rs:7339`). Dropping the idle fence's slot pins fails X62 (`:7399`). Leaving slot reservation stamps out of the progress maximum fails leg B (`crates/server/tests/multipart_reaper.rs:569`).
- **An endless listing loop in `walk()`** (`reaper.rs:498-531`): `checked_page` refuses a page that does not advance (`crates/custodian/src/gc.rs:2108-2121`).
- **The `sidx:` bound at `reaper.rs:817` being too tight for a large session.** A part commit deletes its own owned entries (`crates/core/src/multipart/commit.rs:13`, `:1215-1240`). A crashed part keeps its slot while the session is `Open` (0016's `slot:` row). So `MAX_INFLIGHT_PARTS × MAX_PART_CHUNKS` holds.
- **T4's profile-swap race** (`reaper.rs:956`): Create never replaces a stored profile (`crates/core/src/multipart/create.rs:427`, `ProfileSkew`), so the race needs someone to hand-edit `mpuctl`.
- **An unknown commit outcome read as a conflict:** `reaper.rs:971-973` holds it, as the rubric's *Transactions* rule requires.
- **Leg A being reapable by the ceiling instead of the idle arm:** the fixture is 14 days old, and `W_session` is 30 days (`reaper.rs:165`).

Not re-raised: the slot reserve→commit→release gap (accepted at sign-off, `reaper.rs:81-100`, DST row ignored with a reason), and the `Completing` judgments (`// deferred: #1017`, `reaper.rs:602`).

<!-- pdca:leaf-complete -->

### Advisory — code-review

- NEEDS-HUMAN [impl] — `crates/custodian/src/reaper.rs:425`: A missing admission ledger is treated as a per-session hold, so `pass` still returns `Some(pass)` and `reap` authorizes retirement (`crates/custodian/src/reconciliation.rs:236`). With an `Aborting` session, owned entries and a retirement obligation but no `mpuctl`, the drain can mark fragments and delete staged records without ever verifying the admitting profile; only its terminal deletion checks the missing ledger. Make this a store-wide refusal and add a full `reconcile_step` regression asserting byte-identical state. The current missing-ledger test exercises only an `Open` session through the reaper alone.

- NEEDS-HUMAN [impl] — `crates/custodian/src/reaper.rs:488`: Cursor validation accepts any value starting with `mpu:`, including a damaged value larger than the backend's key limit. Such a value is legal to store as a value but fails when used as the next scan's key. The listing-error branch preserves `position`, and the unchanged-cursor check then leaves the offending value installed, so every subsequent pass fails before judging any session. Bound the cursor before using it and reset oversized values, following the existing GC cursor handling at `crates/custodian/src/gc.rs:1919` and `:2035`; test a backend that rejects oversized scan keys.

- NEEDS-HUMAN [impl] — `crates/dst/tests/custodian.rs:7375`: X62 fabricates a renewal from a lease that expired more than seven days earlier to `t0 + LEASE`. Production renewal checks liveness and derives the replacement expiry from the same clock sample (`crates/core/src/multipart/stage.rs:1673`), so it cannot produce this transition. The test also never verifies that the upload subsequently commits. X60 similarly injects a freshly stamped part commit over a long-expired slot (`crates/dst/tests/custodian.rs:7328`). Exercise reachable writer schedules: keep X60's lease live, and pause X62's production renewal after its live check, advance just across expiry, then release it between the reaper's read and fence and finish the upload. The existing R2 staging test at `crates/core/tests/multipart_upload_part_stage.rs:4978` provides that renewal schedule.

Reviewed read-only against the target and frozen gate evidence. The explicitly accepted slot-cycle gap is not re-raised. No additional simplification or efficiency findings.
<!-- pdca:leaf-complete -->

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C4 Verification (red→green) — Obtain a complete dependency audit on a checkout retaining the pinned RustSec submodule — independent red→green and focused checks pass, but this disposable target lacks that dependency and the frozen full CI timed out; neither establishes full CI success (reviewer-red-green.log:177; reviewer-ci.log:3752, reviewer-ci.log:4406; gate-logs/C4-ci.log:7).
- [ ] C5 Causal adequacy — Make an absent admission profile refuse dependent teardown and keep independent reaping reachable after reconstruction errors — the reproduced cases otherwise mutate unchecked sessions or indefinitely retain abandoned uploads (crates/custodian/src/reaper.rs:425; crates/custodian/src/reconciliation.rs:170, crates/custodian/src/reconciliation.rs:236).
- [ ] T5 Judgment — Replace X60/X62's fabricated expired-lease transitions with reachable production interleavings and demonstrate the specified negations — their current passing results do not establish the required live-upload safety (crates/dst/tests/custodian.rs:7325, crates/dst/tests/custodian.rs:7374; crates/core/src/multipart/stage.rs:1581).
- [ ] Validation — fitness-to-purpose — Sign off this Open-only slice's operational fitness and assign ownership of the post-check first-Create/drain race — profile refusal can still be bypassed after the final read, while the brief reserves drain changes for #659's layer (crates/custodian/src/reaper.rs:64; brief.md:13; reviewer-postcheck-race.log:1).
- [ ] **A lost `mpuctl` ledger makes the reaper hold, but the drain still writes.** `crates/custodian/src/reaper.rs:425-435` turns "sessions listed, `mpuctl` still absent at the last read" into a per-session hold (`Reaped::Ran(Blocked)`). `crates/custodian/src/reconciliation.rs:236-238` then lets the retirement leg run (`:179-185` deployed, `:189-199` one-shot). Concrete case, run through `wyrd custodian --reap-abandoned-uploads`: one `Aborting` session with its `retire:bytes:s:<id>:0` obligation, a committed part, in-flight residue, and **no** `mpuctl`. One-shot 1 wrote 6 `orphan:` marks and deleted the `sidx:` entry. One-shot 2 deleted `part:`, `psum:` and the obligation. Only the terminal delete stopped, at `crates/custodian/src/retirement/sessions.rs:958`. The same store with `mpuctl` set to undecodable bytes is refused, and the store diff is empty. Both cases mean "profile unknown", but only one of them stops the drain. The patch's own rule says it should stop: `reaper.rs:19-23` ("a custodian sized for another profile acts on neither") and the 06-runtime-view paragraph both say the drain needs a checked profile. The unit test `crates/custodian/src/reaper/tests.rs:950-964` checks only the pass's own tally, over an `Open` session the drain would never touch, so it cannot see this. Fix: answer `Reaped::Refused` when the last read is still `Absent` and the listing is not empty. Then add a step-level test (an `Aborting` session plus its obligation, no `mpuctl`) that asserts the store is byte-identical. The other option is to document why the drain is safe with no profile, which would undercut the refusal rationale. T4 flagged this at `reaper.rs:425`/`:436`, and it reproduces.
- [ ] **A stray obligation beside an `Open` session: #659's drain deletes a still-open session's committed part, and the reaper raises no damage signal.** The fixture is an idle `Open@0` session plus a `retire:bytes:s:<id>:0` key. That is the state the client Abort names as damage (`Stop::ObligationKeyTaken`, `crates/core/src/multipart/abort.rs:337-341`, classified by `lost()` at `:374`). The reaper's idle fence conflicts, and `reaper.rs:964-970` counts that as `lost`. The pass answers `Partial` and exits 0, with no hold and no NEEDS-HUMAN line. In the same steps, the retirement leg acts on the stray obligation. One-shot 1 marked the committed part's three positions. One-shot 2 deleted `part:<id>:000001` and `psum:<id>:000001` while `mpu:<id>` was still `Open@0`. The reaper's fence landed only in one-shot 3. So T4's "Conflict forever" claim (`reaper.rs:964`) does **not** reproduce as stated. The reason it doesn't is worse: the drain tears down a session that is still open. This only happens on a damaged store, because no normal writer leaves that key beside an unchanged `Open@E`. Under the brief it is a §6 item against #659's drain: does the drain check that the session has left `Open` before acting on a `{session, all}` obligation? A reaper-side mirror of Abort's `lost()` (hold on `Taken`) would add the missing signal, but it would not stop the drain, because holds don't gate the leg.
- [ ] **The C4-ci gate (it blocks acceptance) timed out. The stall does not reproduce and looks environmental, but the gate is still not green.** The log stops in `crates/server/tests/custodian_gc.rs`, with 9 tests "running for over 60 seconds" (`gate-logs/C4-ci.log:3000-3013`). One of the stuck tests, `deployed_run_loop_refuses_duplicate_endpoints`, returns at `crates/server/src/cli.rs:1160-1173` without running any reaper code (it does not take the one-shot branch at `:1102`). I re-ran `custodian_gc` 6 times scoped to its package, and once with the gate's `--workspace --exclude wyrd-dst` feature set. It passed 11/11 every time, in about 0.2 s. `multipart_reaper` passed 6/6. Every `wyrd-custodian` reaper test passed inside the gate log itself (`C4-ci.log:1742-1764`, `:1799-1800`). The gate never reached the DST (seeded simulation) tier, so I ran it myself under `--cfg madsim` with 50 seeds: 6 reaper rows pass, and the slot-cycle row stays ignored, as accepted at sign-off. Still not run by any gate: `cargo deny`, `cargo machete`, conformance, and the DST clippy leg. A human should re-run `cargo xtask ci` before sign-off.
- [ ] **Diff coverage has gone unmeasured for a third round.** The gate reports "patch.diff does not apply on origin/main" (`gate-logs/C4-diff-cov.log`). Both earlier carry-forwards asked for a rebase. If the base carries prerequisites that are not on `main` yet (the brief's ordering note says #1007 had no published branch), rebasing cannot fix this. A human has to decide whether to land the prerequisites first or accept C5 (166 mutants: 148 caught, 18 unviable, 0 missed) as the substitute.
- [ ] **In the deployed loop, a reconstruction error also stops the reaper.** The reaper runs inside the reconstruction pass's `reconcile_step` (`crates/server/src/custodian.rs:671-695`), after `reconstruction::reconcile(...)?` at `crates/custodian/src/reconciliation.rs:168-170`. If reconstruction hits a store error every tick, the reaper and the drain are skipped every interval. The only log line says "reconstruction pass failed" (`custodian.rs:693`). Abandoned sessions then keep their admission slots until an operator runs the one-shot. The same file rejects this kind of shared failure for scrub and GC (`custodian.rs:640-647`, the Codex #461 rule). The brief chose this wiring ("the deployed fleet loop's existing `reconcile_step` call passes the reaper context"), so this is a scope call, probably for 625.2's driver, not a builder fix. T4 flagged it at `reconciliation.rs:170`.
- [ ] `crates/custodian/src/reaper.rs:425`: A missing admission ledger is treated as a per-session hold, so `pass` still returns `Some(pass)` and `reap` authorizes retirement (`crates/custodian/src/reconciliation.rs:236`). With an `Aborting` session, owned entries and a retirement obligation but no `mpuctl`, the drain can mark fragments and delete staged records without ever verifying the admitting profile; only its terminal deletion checks the missing ledger. Make this a store-wide refusal and add a full `reconcile_step` regression asserting byte-identical state. The current missing-ledger test exercises only an `Open` session through the reaper alone.
- [ ] `crates/custodian/src/reaper.rs:488`: Cursor validation accepts any value starting with `mpu:`, including a damaged value larger than the backend's key limit. Such a value is legal to store as a value but fails when used as the next scan's key. The listing-error branch preserves `position`, and the unchanged-cursor check then leaves the offending value installed, so every subsequent pass fails before judging any session. Bound the cursor before using it and reset oversized values, following the existing GC cursor handling at `crates/custodian/src/gc.rs:1919` and `:2035`; test a backend that rejects oversized scan keys.
- [ ] `crates/dst/tests/custodian.rs:7375`: X62 fabricates a renewal from a lease that expired more than seven days earlier to `t0 + LEASE`. Production renewal checks liveness and derives the replacement expiry from the same clock sample (`crates/core/src/multipart/stage.rs:1673`), so it cannot produce this transition. The test also never verifies that the upload subsequently commits. X60 similarly injects a freshly stamped part commit over a long-expired slot (`crates/dst/tests/custodian.rs:7328`). Exercise reachable writer schedules: keep X60's lease live, and pause X62's production renewal after its live check, advance just across expiry, then release it between the reaper's read and fence and finish the upload. The existing R2 staging test at `crates/core/tests/multipart_upload_part_stage.rs:4978` provides that renewal schedule.
- [ ] C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance) unverifiable — gate exceeded its 7200s timeout
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 14 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_1015/review
- [ ] **The advertised binding red→green leg has no compatible verification path.** `brief.md:56-61` calls physical fragment reclamation in the existing `crates/custodian/tests/gc.rs` “the binding red→green leg,” using the reaper through in-process `reconcile_step`. But `brief.md:137-140` selects only the NEW server test for C4, and `brief.md:220-224` explicitly says a direct call cannot compile against both trees after the signature change. The target exposes the current signature at `crates/custodian/src/reconciliation.rs:126-134`; the added reaper context does not exist there. Thus the subprocess test can establish RED for durable metadata, while the specified GC test supplies post-change evidence, not the promised same-test RED→GREEN for physical reclamation. Revise the criterion to distinguish those claims, or specify a base-compatible path and command that actually runs the physical-reclamation assertion on both trees.
- [ ] **The required batch splitter crosses an unresolved ownership boundary.** `brief.md:175-176` requires “the batch splitter unit-tested,” although `brief.md:13-17` assigns the drain and owned-`sidx:` walk exclusively to #659 and prohibits reimplementing them. This slice's reap fence must remain atomic: `docs/design/proposals/draft/0016-multipart-commit-protocol.md:647-649` explicitly says the fence's slot pins cannot be split; its batch inventory assigns the splittable work to retirement and owned-entry drain steps (`:668`, `:672`). The existing reusable fence builder returns one `WriteBatch` (`crates/core/src/multipart/abort.rs:77-92`). Identify the prerequisite-owned splitter whose tests are to be reused, and require this slice to verify its unsplit fence's byte/operation bounds; otherwise remove the splitter requirement. As written, Do must choose between inventing extra batching machinery and violating an explicit acceptance requirement.
- [ ] size backstop — this slice is behaving oversized: patch is 327 KB (threshold 100 KB); patch touches 45 files (threshold 20). Recommend answering `iterate-plan` at sign-off and authoring the split in the re-plan (`pdca split`), rather than `iterate-do`: a slice that is too big yields implementation-shaped findings every round, and splitting authors briefs, which is Plan's beat.

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
- Iteration delta (if iterating): Rebuild inside this slice (attempt 4). Findings are narrowing; fix these, each with a test that fails today: 1. F1 — an absent `mpuctl` ledger with a non-empty listing must answer `Reaped::Refused` (store-wide), not per-session holds (reaper.rs:425-435), so the dependent retirement leg does not run (reconciliation.rs:236). Add a step-level `reconcile_step` test: Aborting session + committed part + residue + retire: obligation, no `mpuctl` -> store byte-identical. Keep foreign-clock teardown working when a matching ledger exists. 2. F2 — a reconstruction error must not starve the reaper: keep the reconstruction error but let the reaper/drain run in the same step (reconciliation.rs:168-170); regression covering both inputs with a failing `repair:` scan. 3. F3 — rewrite DST X60/X62 with reachable production interleavings: X60 keeps its slot lease live; X62 pauses the production renewal after its live check, crosses expiry, releases it between the reaper's read and fence, then shows the conflicting fence AND the eventual part commit (0016:2275; see multipart_upload_part_stage.rs:4978 for the schedule). 4. Bound the reaper cursor (reaper.rs:488): reset oversized/damaged values like GC does (gc.rs:1919, :2035); test with a backend rejecting oversized scan keys. Out of scope (filed as #659 follow-ups, SUMMARY §10): the stray-obligation-beside-Open drain case and the post-check first-Create/drain race — do not change #659's drain. Keep the slot-cycle gap accepted and ignored as before. The C4-ci timeout looked environmental (custodian_gc passes in isolation); make sure the full `cargo xtask ci` completes.
- By / date: Eduard Ralph / 2026-11-10

## 10. Act candidates (hints for the next Act review)
- Plan advisory: 2 finding(s); brief revised: yes (plan-advisory-*.md)
- (empty is the common case)
- Follow-up (#659 layer): #659's drain acts on a `{session, all}` retire: obligation while the session is still `Open@E` (stray obligation on a damaged store deletes a live session's committed part; reaper raises no damage signal). File against #659.
- Follow-up (#659 layer): post-check first-Create/drain race — a profile installed after the reaper's final `mpuctl` read can bypass refusal (reaper.rs:64, reviewer-postcheck-race.log). Fix across the reaper/#659 seam; file against #659.
