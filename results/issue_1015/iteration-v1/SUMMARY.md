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
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): pass — xtask ci: all checks passed
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass — run-verify.sh: PASS — red without the fix, green with it (5 test(s) ran red).
- C4 diff coverage: changed lines executed by the patch's tests: fail — diff coverage not measured — patch.diff does not apply on origin/main
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): fail — 142 mutants tested in 39m: 85 missed, 40 caught, 17 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 4 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_1015/review-
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 15.09s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

#1015 adds reclamation of abandoned Open multipart uploads and an operator one-shot; two independently reproduced correctness defects still need fixes.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The idle-only, progressing, over-age, foreign-clock and profile-refusal cases define observable durable-state outcomes and explicit slice boundaries; brief.md:30, brief.md:79, brief.md:214. |
| C2 Reproduction (red pre-fix) | PASS | Independently stashing tracked changes while retaining the new subprocess test ran five tests and failed all five; A/C/E failed durable convergence, H lacked its signal, and B failed command execution rather than preservation; reviewer-red.log:267, crates/server/tests/multipart_reaper.rs:196. |
| C3 Change | PASS | The change stays within the Open-session slice, shares the abort fence and retirement implementation, and supplies the operator and fleet-loop entry points without changing persisted formats; crates/custodian/src/reaper.rs:629, crates/custodian/src/retirement.rs:586, crates/server/src/custodian.rs:679. |
| C4 Verification (red→green) | PASS | Restoring the patch independently changes the same five tests from failing to passing; full-CI reproduction has only the synthetic-index caveat below, while the frozen full gate passed; reviewer-green.log:14, reviewer-ci.log:4573, gate-logs/C4-ci.log:5019. |
| C5 Causal adequacy | NEEDS-HUMAN [impl] | Close the absent→reserved→absent slot race before treating idle evidence as current—an upload can commit fresh progress and then be fenced and orphan-marked by this pass; crates/custodian/src/reaper.rs:579, crates/core/src/multipart/commit.rs:1199, reviewer-repros.log:59. |
| T1 Structure | PASS | The shared fenced control point, metadata trait seam and common retirement implementation preserve dependency direction and avoid a second teardown algorithm; crates/custodian/src/reconciliation.rs:135, crates/custodian/src/retirement.rs:564. |
| T2 Shape | PASS | The public policy supports later slices, range reads have explicit cardinality bounds, and unsplittable fences are checked against the existing operation/byte limits; crates/custodian/src/reaper.rs:140, crates/custodian/src/reaper.rs:491, crates/custodian/src/reaper.rs:653, crates/custodian/src/reaper.rs:819. |
| T3 Runtime | FAIL | Profile refusal does not prevent downstream retirement: the one-shot emits the mismatch and fails but still adds six orphan marks and deletes an owned entry; crates/custodian/src/reconciliation.rs:197, reviewer-repros.log:67. |
| T4 Contribution | FAIL | The batch review's four reports reduce to two unresolved, independently reproduced defects; path-based prior-art checks and living-doc updates are present, and the publish-artifact audit is N/A until publish; gate-logs/T4-batch-review.log:10, gate-logs/T4-contribution.log:10, docs/design/architecture/06-runtime-view.md:94. |
| T5 Judgment | NEEDS-HUMAN [impl] | Make profile refusal stop dependent retirement and extend H beyond Open-only fixtures—the current green test cannot expose writes to already-Aborting uploads or existing obligations; crates/custodian/src/reconciliation.rs:191, crates/server/tests/multipart_reaper.rs:667, reviewer-repros.log:67. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Confirm that seven days without progress and a thirty-day absolute lifetime fit supported customer uploads and capacity recovery needs—these defaults deliberately discard staged uploads, including live ones at the ceiling; crates/custodian/src/reaper.rs:101, docs/design/architecture/08-crosscutting-concepts.md:113. |

Source citations above and below resolve relative to `$PDCA_TARGET` (`target/`); brief, gate-log and reviewer-evidence citations resolve in the bundle directory. The target is the harness's disposable, self-contained base commit `ab43b8e`, labelled pre-fix base `3c600b657ccb7e54a4a60c72b220b7bf1d33ad4a`. The patch was restored after the red run; `git apply --reverse --check ../patch.diff` and `git diff --check` both succeed. No production source was edited for this review.

**Two defects are established by execution.** The batch review's three profile-refusal reports describe the same defect and are counted once.

1. **A fresh successful part commit can lose its entire upload to the idle arm.** The reaper reads an empty slot range and old/empty summaries, then prepares a fence whose slot preconditions require absence (`crates/custodian/src/reaper.rs:383`, `crates/custodian/src/reaper.rs:579`). Before that fence commits, a new request can reserve a slot, write an owned intent, and commit its part. Reservation changes only the slot (`crates/core/src/multipart/stage.rs:1330`); part commit writes the part/summary and deletes the slot, without changing the session bytes (`crates/core/src/multipart/commit.rs:1199`). Thus every fence precondition holds again although the observed progress is stale. My harness injects those preconditioned batches over real redb immediately before the real reaper fence. All three batches commit; the reaper then returns `Changed`, leaves `Some(Aborting)`, and installs three orphan marks over the freshly committed part (`reviewer-repros.log:58`). This needs a serialization condition that detects completed slot cycles, plus a regression for this ordering. The current X60/X62 tests cover a commit between the two reads and a renewal of a present slot, respectively, rather than this cycle (`crates/dst/tests/custodian.rs:7298`, `crates/dst/tests/custodian.rs:7351`). If the repair requires changing a prerequisite's record protocol, re-enter Plan for that scope rather than silently expanding this slice.

2. **A profile-refused pass still mutates retirement state.** `reaper::reconcile` returns `Blocked` for a differing or unreadable ledger (`crates/custodian/src/reaper.rs:284`), but the dispatcher aggregates that result and unconditionally invokes retirement in both branches (`crates/custodian/src/reconciliation.rs:183`, `crates/custodian/src/reconciliation.rs:197`). I seeded the existing H-style mismatched profile with a valid already-Aborting session, its obligation, committed part and owned residue, then ran the actual one-shot subprocess. It emitted `reaper-profile-mismatch` and exited unsuccessfully, yet added six orphan marks and removed the `sidx:` entry (`reviewer-repros.log:67`). The brief promises no mutation on refusal; this behavior also contradicts the new operator documentation (`docs/design/architecture/06-runtime-view.md:94`). Distinguish a global profile refusal from an ordinary per-session hold, suppress dependent retirement on refusal, and cover terminal sessions/obligations in H. Preserve the separately required foreign-clock teardown behavior.

**The supplied positive evidence reproduces, but does not cover these defects.** Independent evidence is retained in the bundle; the extra repro crate is scratch-only and calls the unchanged target code.

| Evidence | Adjudication |
|----------|--------------|
| C4-verify | PASS: independent `cargo test --offline --locked -p wyrd-server --test multipart_reaper` ran 5/5 failing with production changes stashed, then 5/5 passing after `git stash pop`; reviewer-red.log:367 and reviewer-green.log:14 corroborate gate-logs/C4-verify.log:121. B's base-state preservation is vacuous; its red exit-status assertion is not proof of idle-arm safety. |
| C4-ci | Frozen run PASS. Independent `cargo xtask ci` exits 1 solely because the synthetic target index has no `.github/advisory-db` gitlink: the index test and advisory-pin preparation fail for that same reason (reviewer-ci.log:3919, reviewer-ci.log:4573). This is a target-copy caveat, not a patch compilation defect. Fmt, clippy, build, the reached functional tests, conformance/statics checks and the DST suite pass. The frozen run contains the complete original success (gate-logs/C4-ci.log:5019). |
| Physical reclamation and DST | PASS as post-change evidence: the GC grace-window case passes, as do X60, X62, X19 and the pre-first-chunk case (crates/custodian/tests/gc.rs:1275; reviewer-ci.log:4456). These are not claimed as same-test pre-fix red→green. |
| C4-diff-cov | Coverage was not measured: the wrapper could not apply the patch to its `origin/main` base (gate-logs/C4-diff-cov.log:10). The supplied target applies and builds. This is a base-alignment caveat, not evidence of a patch defect or a coverage percentage. |
| C5-mutants | The frozen run records 142 mutants: 85 missed, 40 caught, 17 unviable (gate-logs/C5-mutants.log:98). I read the survivor list; it includes profile/summary/lease decisions as well as telemetry and boundary arithmetic. Its output does not identify the full selected test set, so it cannot establish that the server/DST tests are inert. The instance wrapper is not supplied here; this campaign was not independently rerun. The brief's bespoke negations are not shown in these logs and are not claimed as independently reproduced. The two findings above rest on independent reproductions, not the survivor count. |
| T4-batch-review | FAIL confirmed in substance: four emitted reports reduce to the two reproduced defects above (gate-logs/T4-batch-review.log:10). |
| T4-contribution | N/A: contribution artifacts are intentionally drafted later; the substantive check must rerun at publish (gate-logs/T4-contribution.log:10). No human clearance is needed for this deferral. |
| host-tikv and FDB branch | PASS: independently reran both TiKV clippy commands, including server `tikv,etcd`, and additionally compiled the server's `fdb` feature/tests (reviewer-tikv.log:202; reviewer-fdb.log:70). These are compilation results, not claims of a live distributed-cluster test. |
| External dependencies | The named `typos` and docs renderer were actually exercised: spell check, docs lint, 101-page render and link audit ran successfully (brief.md:136; reviewer-ci.log:2). No shim or code-read substitute was used for them. |

Reproduce the two additional failing assertions from this bundle directory:

```sh
CARGO_TARGET_DIR="$PWD/pdca-reviewer-1015-build" \
TMPDIR="$PWD/pdca-reviewer-1015-tmp" \
CARGO_BIN_EXE_wyrd="$PWD/pdca-reviewer-1015-build/debug/wyrd" \
cargo test --offline --manifest-path pdca-reviewer-1015-repros/Cargo.toml reviewer_ -- --nocapture
```

**The standing rubric and prior art were checked without reopening settled deferrals.** The new paths share one clock per lifecycle, reuse the existing fence/drain, update both living architecture documents, and add cases to the existing DST file. The new integration-test root forbids unsafe code (`crates/server/tests/multipart_reaper.rs:25`); no changed persisted fields or workflow filters require additional rubric findings. The capability-probe smell test does not fire: the absent-fleet refusal represents the explicitly supported record-only mode, and the clock/profile checks are domain preconditions, not guards concealing a load-time side effect. The #806, deployment-sibling, #1017 and existing drain deferrals remain settled.

Path-based GitHub default-branch history and closed-PR searches covered the substantive reaper, reconciliation, retirement, server CLI/loop and reaper/DST test paths; results are saved in `reviewer-prior-art.json`. The new reaper and subprocess-test paths have no default-branch history or closed-PR matches. Explicit closed-unmerged searches found [PR #647](https://github.com/getwyrd/wyrd/pull/647) for the reconciliation path, a separate segmented-map change; the CLI path had none. The broader closed-reaper search finds prerequisite staging/commit work, including [PR #1041](https://github.com/getwyrd/wyrd/pull/1041), rather than an existing abandoned-upload pass. This supports the brief's net-new classification; it does not erase the stacked-base caveat from diff coverage.

Human fitness sign-off remains owed after the two implementation issues are resolved. For this custodian durability change, follow-up observation of the standing Tier-1 disk-fault and Tier-2 kill/reconstruct campaigns is warranted; those campaigns remain non-gating under `AGENTS.md:78`. No GUI/manual observation is needed to establish either reported defect.

<!-- pdca:leaf-complete -->

### Advisory — adversary

# Adversarial review — #1015 (625.1, multipart reaper, `Open` half)

Grounded on the target tree (HEAD `2995151` pre-fix base, with the patch in the working tree). Probes ran in a scratch copy under `$PDCA_SCRATCH`, never in the target.

## The evidence (red→green)

- Not refuted. In `gate-logs/C4-verify.log`, all 5 legs of `crates/server/tests/multipart_reaper.rs` fail on the base with ``flag `--reap-abandoned-uploads` needs a value`` (the valueless-flag list at `crates/server/src/cli.rs:2879`) and pass with the fix. The test runs the real binary as a subprocess with the flag last (`multipart_reaper.rs:146-180`) and asserts on durable redb state. The one-shot goes through `CustodianService::reconcile_pass` → `reconcile_step` (`crates/server/src/custodian.rs:462-491`), the same control point the fleet loop uses (`:633-680`), so this is not a parallel copy of production. One caveat: leg B's red comes only from its exit-status check (`multipart_reaper.rs:538-543`), not from the property it tests, so "5 test(s) ran red" counts one leg that proves nothing on the base. The brief predicted this, so it is not a refutation.

## Refutations

- NEEDS-HUMAN [human] — **ABA race: the idle fence can abort an upload that just committed a part.** `crates/custodian/src/reaper.rs:574-580` pins every slot it saw free with `require_absent`. A reserve writes the slot (`crates/core/src/multipart/stage.rs:12`), and the part commit deletes it again while writing `part:`/`psum:`. The commit only requires `mpu:` and never writes it (`crates/core/src/multipart/commit.rs:13`). The sequence: the reaper reads `slot:` (index 1 free) → the client reserves index 1 → the reaper reads `psum:` → the client commits part 3, so slot 1 is gone again → fence: `require(mpu == read)` holds and `require_absent(slot:1)` holds → `Aborting`. **I reproduced this on the unmodified reaper** with a DST probe on the patch's own `Hooked` store (`Trigger::BeforeFenceOf`, net batch = part 3 plus its summary under `require(mpu == Open@0)`). Result: state `Aborting`, outcome `Changed`, and a part committed at `t0` gets retired. An empty part (two small batches) is the cheapest way to hit the window. Three places claim this case is covered, and all three are wrong for reserve-then-release: `reaper.rs:40-43`, the 06 doc (`docs/design/architecture/06-runtime-view.md:94`, "a renewal or reservation that lands after the reads makes the commit fail"), and 0016 itself (`0016:1881-1883`). A fix needs something the commit writes and the fence can pin (no bounded set of `psum:` pins exists). That means a format or 0016 change, which is outside this slice, so a human has to decide whether it blocks 625.1 or becomes a tracked follow-up. T4 raised the same race at `reaper.rs:579`.

- NEEDS-HUMAN [impl] — **A reaper store fault now skips #659's drain.** `crates/custodian/src/reconciliation.rs:176-187` (fleet path) and `:191-200` (one-shot) call `reaper::reconcile(..)?` before the retirement leg. The reaper turns every per-session range-read error into a whole-pass error (`reaper.rs:325`, `:383`, `:390`, `:408`). Before this patch, the drain ran after every successful reconstruction pass. **DST probe:** one `Open` session whose `psum:` `scan_page` fails, plus an unrelated client-aborted session with its `{session, all}` obligation. `reconcile_step` returns `Err("reconciliation store access: injected psum: range fault")` and the store is byte-identical. The control run with no fault drains the aborted session. If the fault on one session's range is persistent (a backend region fault, or a `checked_page` refusal at `crates/custodian/src/gc.rs:2099-2121`), it stops every teardown and every `mpuctl` decrement for good. That is the permanent `503 SlowDown` this slice exists to remove. The deployed loop also logs it as "reconstruction pass failed" (`crates/server/src/custodian.rs:692`). Fix: hold per-session read faults the same way decode faults are already held, or run the drain before returning the reaper's error. Add the fault case as a DST row.

- NEEDS-HUMAN [impl] — **The slot `reserved_at` part of the progress instant is untested.** I changed `reaper.rs:397-401` so slot reservations no longer count as progress (lease check kept). All 13 reaper tests stayed green: the 5 subprocess legs, the 3 unit tests, the `gc.rs` case and the 4 DST rows. In every fixture, the slot was either reserved more than `W_open` ago or still leased. B(ii) (`crates/server/tests/multipart_reaper.rs:499-503`) survives on its live lease, not on `reserved_at`, so the brief's negation (1) cannot fail B(ii) through the progress term. The missing case, which I ran as a probe: created 14 days ago, part committed 10 days ago, slot reserved 1 hour ago with its lease lapsed. This session must be kept (`0016:1832`, "never idle-abandoned within `W_open` of a reservation"). The probe passes on the real reaper and fails on the changed one (the session is fenced). Add it to leg B.

- NEEDS-HUMAN [impl] — **Two more checks no test can fail.** Both are C5 survivors that the server and DST tests don't catch either. (a) Replacing `fence_cost` with `(0, 0)` survives (`gate-logs/C5-mutants.log`, `reaper.rs:589`). `largest_fence_fits_both_batch_budgets` (`reaper.rs:819-848`) measures the fence with the same function it is supposed to check, and the budget guard at `reaper.rs:653` never fires in any test. The real margin is large (about 20 ops against `MAX_BATCH_OPS` = 500), so the risk is low, but the "fits both budgets" evidence is circular. Assert ops and bytes against an independent count. (b) Changing `pass.held > 0` to `<` survives (`reaper.rs:333`). No test seeds a session the reaper must hold (undecodable `mpu:`/`slot:`/`psum:`/`sidx:`), so the one-shot's non-zero exit on a hold (`crates/server/src/cli.rs:1102-1124`) is untested. That is the rubric's "never silent success" class.

- NEEDS-HUMAN [human] — **A profile mismatch stops the reaper but not the drain** (T4's three BLOCKING bullets at `reconciliation.rs:197`). `reaper.rs:287-298` returns `Blocked`, but `reconciliation.rs:196-200` and `:183-187` still run the retirement leg. So with a mismatched `mpuctl`, an existing client-aborted session still gets marks, deletes and a count decrement. Leg H (`multipart_reaper.rs:663-692`) seeds only `Open` sessions, so its "byte-identical" assertion says nothing about the drain. My read is that this is not a defect. The brief's H says "stops the reaper". The drain pages its slot deletes and doesn't depend on the profile (`crates/custodian/src/retirement/sessions.rs:175-178`, `:937`). It also ran with no profile check before this patch. But the T4 gate disagrees, and so do the 06 doc ("changes nothing", `06-runtime-view.md:94`) and the 08 doc ("refuses to act at all", `docs/design/architecture/08-crosscutting-concepts.md:113`). A human should choose: gate the drain too (a change to #659's behavior), or record a rejection reason for T4 and narrow the doc wording.

- NEEDS-HUMAN [impl] — **`WALL_CLOCK_SOURCE` claims a writer that doesn't exist.** Its doc (`reaper.rs:93-96`) calls it "the spelling every in-tree wall-clocked writer uses", but no production code sets `clock_source` today. `crates/core/src/multipart/create.rs:67` and `complete.rs:392` take it from the caller, and the only `"wall"` literals are in tests. The constant lives in `wyrd-custodian`, which a gateway crate should not depend on, so the future Create wiring will have to spell it separately. If the spellings differ, every session reads as foreign and is never reaped, only alarmed. Move the constant to `wyrd_core::multipart` and fix the doc.

- NEEDS-HUMAN [impl] — **The bundle is stale.** `gate-logs/C4-diff-cov.log` says `patch.diff` does not apply on `origin/main`. Diff coverage was never measured, and the C4/C5 evidence was taken against a base that `main` has since moved past. This slice relies on #1013's drain, which may have changed in the meantime. Rebase and re-run C4.

## Attempted, could not refute

- The arms and their boundaries: idle on `> W_open` (`<=` keeps), ceiling on `> W_session`, and `saturating_sub` on stamps in the future (keeps the session). Also the one `now` read per session before its ranges, and the source-first read order (the X60 row does catch the reversed order). The slot pins cover the whole `[0, MAX_INFLIGHT_PARTS)` key space, the ceiling sends no pins, and the clock guard runs before any range read. Leg A's fixture can only be reaped by the idle arm (14 days old against 7 and 30).
- Legs A, C and E-open assert on durable state and name every fragment position. The `gc.rs` case checks both `grace − 1` and `grace`.
- The one-shot has no real fencing on tikv/fdb, but that is the existing #365 gap, not something this patch added.

<!-- pdca:leaf-complete -->

### Advisory — code-review

- NEEDS-HUMAN [impl] — `crates/custodian/src/reaper.rs:579`: **The idle fence misses a complete slot reservation/release cycle.** After the reaper reads an empty slot range and old progress, a new `UploadPart` can reserve a slot, stage and commit before the fence lands. The part commit writes fresh `psum:` progress and deletes its slot and owned entries without changing `mpu:` (`crates/core/src/multipart/commit.rs:1199`). All `require_absent(slot)` conditions therefore pass again, and the reaper aborts and drains a progressing upload below `W_session`. Make the fence detect completed reservation cycles and add a seeded DST interleaving covering reserve → part commit between the reads and fence; the existing X60/X62 cases start with a slot already present.

- NEEDS-HUMAN [impl] — `crates/custodian/src/reconciliation.rs:197`: **Profile refusal does not prevent teardown mutations.** A mismatched or unreadable admission profile returns `Ok(Blocked)`, which is merely merged into the outcome before retirement runs unconditionally. An existing `Aborting` session or `retire:` obligation can therefore produce marks, deletions and, with a decodable ledger, admission-count changes despite the promised no-mutation refusal. The fleet branch does the same at `crates/custodian/src/reconciliation.rs:184`. Propagate profile refusal separately from ordinary per-session holds and suppress teardown on that refusal. Extend `crates/server/tests/multipart_reaper.rs:668` with pending teardown work; its current Open-only fixtures cannot expose this.

No additional reuse, simplification or efficiency findings.

<!-- pdca:leaf-complete -->

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C5 Causal adequacy — Close the absent→reserved→absent slot race before treating idle evidence as current—an upload can commit fresh progress and then be fenced and orphan-marked by this pass; crates/custodian/src/reaper.rs:579, crates/core/src/multipart/commit.rs:1199, reviewer-repros.log:59.
- [ ] T5 Judgment — Make profile refusal stop dependent retirement and extend H beyond Open-only fixtures—the current green test cannot expose writes to already-Aborting uploads or existing obligations; crates/custodian/src/reconciliation.rs:191, crates/server/tests/multipart_reaper.rs:667, reviewer-repros.log:67.
- [ ] Validation — fitness-to-purpose — Confirm that seven days without progress and a thirty-day absolute lifetime fit supported customer uploads and capacity recovery needs—these defaults deliberately discard staged uploads, including live ones at the ceiling; crates/custodian/src/reaper.rs:101, docs/design/architecture/08-crosscutting-concepts.md:113.
- [ ] **ABA race: the idle fence can abort an upload that just committed a part.** `crates/custodian/src/reaper.rs:574-580` pins every slot it saw free with `require_absent`. A reserve writes the slot (`crates/core/src/multipart/stage.rs:12`), and the part commit deletes it again while writing `part:`/`psum:`. The commit only requires `mpu:` and never writes it (`crates/core/src/multipart/commit.rs:13`). The sequence: the reaper reads `slot:` (index 1 free) → the client reserves index 1 → the reaper reads `psum:` → the client commits part 3, so slot 1 is gone again → fence: `require(mpu == read)` holds and `require_absent(slot:1)` holds → `Aborting`. **I reproduced this on the unmodified reaper** with a DST probe on the patch's own `Hooked` store (`Trigger::BeforeFenceOf`, net batch = part 3 plus its summary under `require(mpu == Open@0)`). Result: state `Aborting`, outcome `Changed`, and a part committed at `t0` gets retired. An empty part (two small batches) is the cheapest way to hit the window. Three places claim this case is covered, and all three are wrong for reserve-then-release: `reaper.rs:40-43`, the 06 doc (`docs/design/architecture/06-runtime-view.md:94`, "a renewal or reservation that lands after the reads makes the commit fail"), and 0016 itself (`0016:1881-1883`). A fix needs something the commit writes and the fence can pin (no bounded set of `psum:` pins exists). That means a format or 0016 change, which is outside this slice, so a human has to decide whether it blocks 625.1 or becomes a tracked follow-up. T4 raised the same race at `reaper.rs:579`.
- [ ] **A reaper store fault now skips #659's drain.** `crates/custodian/src/reconciliation.rs:176-187` (fleet path) and `:191-200` (one-shot) call `reaper::reconcile(..)?` before the retirement leg. The reaper turns every per-session range-read error into a whole-pass error (`reaper.rs:325`, `:383`, `:390`, `:408`). Before this patch, the drain ran after every successful reconstruction pass. **DST probe:** one `Open` session whose `psum:` `scan_page` fails, plus an unrelated client-aborted session with its `{session, all}` obligation. `reconcile_step` returns `Err("reconciliation store access: injected psum: range fault")` and the store is byte-identical. The control run with no fault drains the aborted session. If the fault on one session's range is persistent (a backend region fault, or a `checked_page` refusal at `crates/custodian/src/gc.rs:2099-2121`), it stops every teardown and every `mpuctl` decrement for good. That is the permanent `503 SlowDown` this slice exists to remove. The deployed loop also logs it as "reconstruction pass failed" (`crates/server/src/custodian.rs:692`). Fix: hold per-session read faults the same way decode faults are already held, or run the drain before returning the reaper's error. Add the fault case as a DST row.
- [ ] **The slot `reserved_at` part of the progress instant is untested.** I changed `reaper.rs:397-401` so slot reservations no longer count as progress (lease check kept). All 13 reaper tests stayed green: the 5 subprocess legs, the 3 unit tests, the `gc.rs` case and the 4 DST rows. In every fixture, the slot was either reserved more than `W_open` ago or still leased. B(ii) (`crates/server/tests/multipart_reaper.rs:499-503`) survives on its live lease, not on `reserved_at`, so the brief's negation (1) cannot fail B(ii) through the progress term. The missing case, which I ran as a probe: created 14 days ago, part committed 10 days ago, slot reserved 1 hour ago with its lease lapsed. This session must be kept (`0016:1832`, "never idle-abandoned within `W_open` of a reservation"). The probe passes on the real reaper and fails on the changed one (the session is fenced). Add it to leg B.
- [ ] **Two more checks no test can fail.** Both are C5 survivors that the server and DST tests don't catch either. (a) Replacing `fence_cost` with `(0, 0)` survives (`gate-logs/C5-mutants.log`, `reaper.rs:589`). `largest_fence_fits_both_batch_budgets` (`reaper.rs:819-848`) measures the fence with the same function it is supposed to check, and the budget guard at `reaper.rs:653` never fires in any test. The real margin is large (about 20 ops against `MAX_BATCH_OPS` = 500), so the risk is low, but the "fits both budgets" evidence is circular. Assert ops and bytes against an independent count. (b) Changing `pass.held > 0` to `<` survives (`reaper.rs:333`). No test seeds a session the reaper must hold (undecodable `mpu:`/`slot:`/`psum:`/`sidx:`), so the one-shot's non-zero exit on a hold (`crates/server/src/cli.rs:1102-1124`) is untested. That is the rubric's "never silent success" class.
- [ ] **A profile mismatch stops the reaper but not the drain** (T4's three BLOCKING bullets at `reconciliation.rs:197`). `reaper.rs:287-298` returns `Blocked`, but `reconciliation.rs:196-200` and `:183-187` still run the retirement leg. So with a mismatched `mpuctl`, an existing client-aborted session still gets marks, deletes and a count decrement. Leg H (`multipart_reaper.rs:663-692`) seeds only `Open` sessions, so its "byte-identical" assertion says nothing about the drain. My read is that this is not a defect. The brief's H says "stops the reaper". The drain pages its slot deletes and doesn't depend on the profile (`crates/custodian/src/retirement/sessions.rs:175-178`, `:937`). It also ran with no profile check before this patch. But the T4 gate disagrees, and so do the 06 doc ("changes nothing", `06-runtime-view.md:94`) and the 08 doc ("refuses to act at all", `docs/design/architecture/08-crosscutting-concepts.md:113`). A human should choose: gate the drain too (a change to #659's behavior), or record a rejection reason for T4 and narrow the doc wording.
- [ ] **`WALL_CLOCK_SOURCE` claims a writer that doesn't exist.** Its doc (`reaper.rs:93-96`) calls it "the spelling every in-tree wall-clocked writer uses", but no production code sets `clock_source` today. `crates/core/src/multipart/create.rs:67` and `complete.rs:392` take it from the caller, and the only `"wall"` literals are in tests. The constant lives in `wyrd-custodian`, which a gateway crate should not depend on, so the future Create wiring will have to spell it separately. If the spellings differ, every session reads as foreign and is never reaped, only alarmed. Move the constant to `wyrd_core::multipart` and fix the doc.
- [ ] **The bundle is stale.** `gate-logs/C4-diff-cov.log` says `patch.diff` does not apply on `origin/main`. Diff coverage was never measured, and the C4/C5 evidence was taken against a base that `main` has since moved past. This slice relies on #1013's drain, which may have changed in the meantime. Rebase and re-run C4.
- [ ] `crates/custodian/src/reaper.rs:579`: **The idle fence misses a complete slot reservation/release cycle.** After the reaper reads an empty slot range and old progress, a new `UploadPart` can reserve a slot, stage and commit before the fence lands. The part commit writes fresh `psum:` progress and deletes its slot and owned entries without changing `mpu:` (`crates/core/src/multipart/commit.rs:1199`). All `require_absent(slot)` conditions therefore pass again, and the reaper aborts and drains a progressing upload below `W_session`. Make the fence detect completed reservation cycles and add a seeded DST interleaving covering reserve → part commit between the reads and fence; the existing X60/X62 cases start with a slot already present.
- [ ] `crates/custodian/src/reconciliation.rs:197`: **Profile refusal does not prevent teardown mutations.** A mismatched or unreadable admission profile returns `Ok(Blocked)`, which is merely merged into the outcome before retirement runs unconditionally. An existing `Aborting` session or `retire:` obligation can therefore produce marks, deletions and, with a decodable ledger, admission-count changes despite the promised no-mutation refusal. The fleet branch does the same at `crates/custodian/src/reconciliation.rs:184`. Propagate profile refusal separately from ordinary per-session holds and suppress teardown on that refusal. Extend `crates/server/tests/multipart_reaper.rs:668` with pending teardown work; its current Open-only fixtures cannot expose this.
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 4 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_1015/review-
- [ ] **The advertised binding red→green leg has no compatible verification path.** `brief.md:56-61` calls physical fragment reclamation in the existing `crates/custodian/tests/gc.rs` “the binding red→green leg,” using the reaper through in-process `reconcile_step`. But `brief.md:137-140` selects only the NEW server test for C4, and `brief.md:220-224` explicitly says a direct call cannot compile against both trees after the signature change. The target exposes the current signature at `crates/custodian/src/reconciliation.rs:126-134`; the added reaper context does not exist there. Thus the subprocess test can establish RED for durable metadata, while the specified GC test supplies post-change evidence, not the promised same-test RED→GREEN for physical reclamation. Revise the criterion to distinguish those claims, or specify a base-compatible path and command that actually runs the physical-reclamation assertion on both trees.
- [ ] **The required batch splitter crosses an unresolved ownership boundary.** `brief.md:175-176` requires “the batch splitter unit-tested,” although `brief.md:13-17` assigns the drain and owned-`sidx:` walk exclusively to #659 and prohibits reimplementing them. This slice's reap fence must remain atomic: `docs/design/proposals/draft/0016-multipart-commit-protocol.md:647-649` explicitly says the fence's slot pins cannot be split; its batch inventory assigns the splittable work to retirement and owned-entry drain steps (`:668`, `:672`). The existing reusable fence builder returns one `WriteBatch` (`crates/core/src/multipart/abort.rs:77-92`). Identify the prerequisite-owned splitter whose tests are to be reused, and require this slice to verify its unsplit fence's byte/operation bounds; otherwise remove the splitter requirement. As written, Do must choose between inventing extra batching machinery and violating an explicit acceptance requirement.
- [ ] size backstop — this slice is behaving oversized: patch is 207 KB (threshold 100 KB); patch touches 40 files (threshold 20). Recommend answering `iterate-plan` at sign-off and authoring the split in the re-plan (`pdca split`), rather than `iterate-do`: a slice that is too big yields implementation-shaped findings every round, and splitting authors briefs, which is Plan's beat.

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
- Iteration delta (if iterating): Rebuild inside this slice (not a re-plan): the logic is ~69 KB; the size backstop tripped on test volume, not on two outcomes. Fix the reproduced defects: 1. Slot ABA race (reaper.rs:579): the idle fence's require_absent(slot) pins pass again after a reserve -> part-commit -> release cycle lands between the reads and the fence, so a progressing upload is aborted. Make the fence detect a completed slot cycle and add a seeded DST row for reserve -> commit between the reads and the fence. Fix the doc claims that say this case is covered (reaper.rs:40-43, 06-runtime-view.md:94). If it truly cannot be closed without a record-format or 0016 change, stop and say so in build-notes rather than expanding scope. 2. Profile refusal must mutate nothing: a Blocked profile result must also suppress the dependent retirement leg (reconciliation.rs:183-187, :191-200). Keep a global refusal separate from per-session holds, keep the foreign-clock teardown, and extend leg H with an already-Aborting session plus a retire: obligation. 3. A per-session read fault in the reaper must not skip #659's drain: hold it like decode faults are held (or run the drain first). Add a DST row. Also: rebase onto current main (the patch no longer applies, so diff coverage was not measured); add leg B's "slot reserved 1h ago, lease lapsed" case; assert fence cost against an independent count; test the non-zero exit on a held session; move WALL_CLOCK_SOURCE to wyrd_core::multipart and fix its doc.
- By / date: Eduard Ralph / 2026-11-10

## 10. Act candidates (hints for the next Act review)
- Plan advisory: 2 finding(s); brief revised: yes (plan-advisory-*.md)
- (empty is the common case)
