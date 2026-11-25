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
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass — run-verify.sh: PASS — red without the fix, green with it (10 test(s) ran red).
- C4 diff coverage: changed lines executed by the patch's tests: fail — diff coverage not measured — patch.diff does not apply on origin/main
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): fail — 191 mutants tested in 11m: 1 missed, 168 caught, 22 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 11 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_1015/review
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 13.53s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Review #1015's abandoned Open multipart-session reaper and metadata-only operator one-shot: the binding red→green is reproduced, but three implementation findings remain.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The Open-only reclamation, liveness protection, clock ownership and profile-refusal outcomes are falsifiable; the latest carry-forward explicitly settles the stacked base and excluded follow-ups (`brief.md:26`, `brief.md:315`). |
| C2 Reproduction (red pre-fix) | PASS | Stashing the production changes while retaining the new subprocess test ran 10 tests and failed all 10; abandonment remained durable in the store, rather than failing to compile (`reviewer-red.log:131`; `crates/server/tests/multipart_reaper.rs:492`). |
| C3 Change | PASS | The change reaches both the operator command and the fleet loop through the existing fenced maintenance entry point, within this slice's remit (`crates/server/src/custodian.rs:481`, `crates/server/src/custodian.rs:680`). |
| C4 Verification (red→green) | PASS | Restoring the patch made all 10 binding tests pass; the frozen full gate also completed successfully; local rerun limitations and accepted unmeasured coverage are distinguished below (`reviewer-restored-green.log:19`, `gate-logs/C4-ci.log:5055`). |
| C5 Causal adequacy | NEEDS-HUMAN [impl] | Fix F1/F2 before relying on bounded residency or the completion signal—persistent page faults prevent revisits, and an old clean cursor certifies uploads now eligible for reaping (`crates/custodian/src/reaper.rs:654`, `crates/custodian/src/reaper.rs:714`). |
| T1 Structure | PASS | The shared abort fence, metadata trait and existing retirement implementation remain the controlling seams; the production reaper and drain receive the same store and clock (`crates/custodian/src/reaper.rs:1086`, `crates/server/src/custodian.rs:618`, `crates/server/src/custodian.rs:634`). |
| T2 Shape | PASS | Work and fence sizes are bounded, the maximum fence is checked against independent counts, and the new flag is documented in both living architecture views (`crates/custodian/src/reaper/tests.rs:1223`, `docs/design/architecture/06-runtime-view.md:94`, `docs/design/architecture/08-crosscutting-concepts.md:113`). |
| T3 Runtime | FAIL | A repeatable failure after one listing page freezes the saved position; four later passes leave readable, now-idle sessions Open, so one bad range prevents their reclamation (F1; `crates/custodian/src/reaper.rs:693`, `reviewer-probes.log:17`). |
| T4 Contribution | NEEDS-HUMAN | Confirm affected-path prior art against merged and closed/rejected work—the supplied target has one synthetic commit and no remote, and the bundle only records a July Plan search (`reviewer-prior-art.log:1`, `brief.md:258`); the contribution-artifact audit itself is N/A until publish. |
| T5 Judgment | NEEDS-HUMAN [impl] | Correct F3's late-refusal audit before operators rely on it—it claims no fencing after an upload has durably become Aborting (`crates/custodian/src/reaper.rs:1187`, `reviewer-probes.log:7`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Accept the seven-day idle window, thirty-day residency ceiling and operator recovery procedure for intended deployments—automated correctness checks cannot choose retention policy or establish operational fitness (`crates/custodian/src/reaper.rs:183`, `crates/custodian/src/reaper.rs:189`, `crates/server/src/cli.rs:1107`). |

**Three findings are independently reproduced against the patched production entry point.** Source citations above and below are relative to `$PDCA_TARGET`; evidence files are relative to this review directory. The target is readable and matches `patch.diff` (`git apply --reverse --check` succeeds). No production source was edited for the probes.

1. **F1 — A persistent listing fault permanently starves the readable prefix.** When `checked_page` fails, `walk` breaks with the last successful position, then persists that same position. Every subsequent pass starts immediately before the same failing page. Sessions visited earlier, while fresh, are never judged again as they become abandoned. A four-session fixture with three entries per page reproduces this: fail the page after ID 2, advance eight days, and run four more real `reconcile_step` calls. Every call still fails at that boundary, the cursor stays at ID 2, and ID 0 remains Open. The retirement leg also encounters the injected fault; that does not explain away the reaper's independently stuck cursor. Provide a bounded retry/wrap policy that revisits readable sessions while continuing to report the unreadable range. Grounding: `crates/custodian/src/reaper.rs:654`, `crates/custodian/src/reaper.rs:693`; probe `pdca-reviewer-1015-probes/src/lib.rs:104`; observations `reviewer-probes.log:17`.

2. **F2 — A persisted clean lap can falsely report that nothing remains to reap.** With 65 sessions under a matching profile that admits 100, put the first 64 one millisecond inside the idle window and the last session fresh. Pass one returns Partial and persists the clean marker. Advance the shared clock two milliseconds: pass two visits only the fresh tail, returns Satisfied, and leaves all 64 now-idle sessions Open. The marker carries neither a judgment deadline nor a freshness bound. An operator following “re-run until satisfied” stops at a false completion signal; the gap between invocations is unbounded. Preserve a sound meaning for completion across invocations and test time advancing between pages. This concerns the supported multi-pass path; the shipped profile's ordinary population is below 64. Grounding: `crates/custodian/src/reaper.rs:590`, `crates/custodian/src/reaper.rs:714`, `crates/server/src/cli.rs:1110`; the existing lap test keeps time fixed (`crates/custodian/src/reaper/tests.rs:840`); independent probe `pdca-reviewer-1015-probes/src/lib.rs:94`, result `reviewer-probes.log:28`.

3. **F3 — Late profile refusal gives a false no-mutation assurance.** Seed one idle session with a matching ledger, allow its fence to commit, and remove the ledger immediately before the second profile read. The step correctly returns Blocked and suppresses retirement, but its warning says it “fences nothing” while the durable session is Aborting. The same emitter serves initial and final refusal. Distinguish refusal before action from stopping further action, so the audit accurately tells operators what changed. Grounding: `crates/custodian/src/reaper.rs:487`, `crates/custodian/src/reaper.rs:1187`; probe `pdca-reviewer-1015-probes/src/lib.rs:137`; conflicting state and warning appear together at `reviewer-probes.log:7`.

The standalone probes are runnable from this directory with:

```sh
CARGO_TARGET_DIR="$PWD/pdca-reviewer-1015-build" \
TMPDIR="$PWD/pdca-reviewer-1015-tmp" \
cargo test --offline --manifest-path pdca-reviewer-1015-probes/Cargo.toml -- --nocapture
```

They currently report **3 failed, 1 passed** (`reviewer-probes.log:43`). The passing counter-probe injects a persistent budget-lookahead fault: the complete step returns Blocked. Consequently, the frozen review's blanket claim that this fault produces a successful CLI exit is not adopted merely from the reaper-only outcome.

**Verification establishes the specified happy paths, with explicit limits.** The independent binding sequence used `git stash push`, retained the added subprocess test, ran the pre-fix test, restored with `git stash pop`, and reran it: 10 failed → 10 passed. The frozen run agrees (`gate-logs/C4-verify.log:12`, `gate-logs/C4-verify.log:141`). Leg B's durable survival property is vacuously true without any reaper; its red test result is the command-status assertion, not independent proof of liveness protection (`gate-logs/C4-verify.log:65`). The production-renewal DST rows, source-first race, slot-fence race, stale-snapshot case and foreign-clock teardown provide the relevant positive coverage. The physical GC case runs one pass just before grace and one at grace, as requested (`crates/custodian/tests/gc.rs:1425`); it is post-change evidence, not a same-test pre-fix red.

The independent broader rerun completed the build, changed-surface tests, physical GC regression, conformance and all DST suites, including **41 passed / 1 accepted ignored** in the custodian campaign (`reviewer-ci.log:1377`, `reviewer-ci.log:3917`). Its overall status is **not green**: the synthetic target omits the advisory-database submodule gitlink, failing the index test and advisory setup (`reviewer-ci.log:3535`); invoking xtask from the review directory also selected the default `stable` toolchain, which lacks fmt/clippy components. The repository-pinned toolchain was then used from the target directory: formatting passed, workspace/all-target Clippy passed (`reviewer-clippy.log:292`), and DST Clippy passed (`reviewer-dst-clippy.log:237`). Spelling, docs lint/render, statics, dependency-use and conformance scanners ran successfully. These host/check-out limitations do not contradict the frozen full-CI pass and are not reported as patch defects. The frozen feature-compilation and mutation logs supply the remaining evidence; their instance-scoped wrappers were not treated as missing target files.

All seven named frozen gate logs were read. Their substantive dispositions are:

- **C4-ci / host-tikv: PASS on frozen evidence.** Full CI finishes at `gate-logs/C4-ci.log:5055`; TiKV crate/server feature compilation finishes at `gate-logs/host-tikv.log:211`. These are actual compilation/test outputs, not tool aliases or code-reading substitutes. The brief's external dependencies, `typos` and the docs renderer, both ran in the frozen gate and in this review's checks (`gate-logs/C4-ci.log:11`, `reviewer-ci.log:12`).
- **C4-diff-cov: unmeasured, accepted base-state caveat.** The log reports failure to apply on `origin/main`, not a failed source compilation (`gate-logs/C4-diff-cov.log:10`). The latest brief explicitly accepts the stack on unmerged prerequisites and says not to rebase again (`brief.md:324`). This is not a C4 patch defect.
- **C5-mutants: one survivor, inspected.** The log records 191 tested, 168 caught, 22 unviable, and one surviving `&&`→`||` at the budget lookahead (`gate-logs/C5-mutants.log:13`). That variant broadens probe scheduling after an end/fault; the summary alone does not establish a new false-reap defect or identify every required named negation. F1–F3 rest on the independent probes, not an inference from mutant counts.
- **T4-batch-review: findings triaged, not copied as eleven distinct defects.** Its line 10 contains repeated cursor and ledger findings. F1–F3 above are independently grounded. No additional ledger-CAS defect is asserted without a protocol-reachable hot profile swap: profile replacement requires a drained population (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:2077`), and the requested late ledger-loss refusal is present and tested. The separate-context allegation does not occur in the supplied production caller, which passes the same metadata store and clock to both contexts (`crates/server/src/custodian.rs:618`, `crates/server/src/custodian.rs:634`).
- **T4-contribution: N/A.** `pr-description.md` is absent by design; the substantive contribution audit must rerun at publish (`gate-logs/T4-contribution.log:10`). No human clearance is owed for that deferral.

**Previously accepted boundaries remain settled.** The slot reserve→commit→release gap stays documented and ignored, as the carry-forward requires; neither it nor the #659 drain follow-ups is re-raised. No optional-capability or load-time symptom guard was introduced: the `probe` function inspects listing termination, not a capability that should already exist. No root-cause dispute or scope re-entry is inferred. Tier-1 disk-fault and Tier-2 kill/reconstruct observation remains appropriate before deployment for this custodian change; it is not represented as a missing merge gate. The supplied target contains no `INTEGRATION.md`, so no additional §4 human-only rules could be derived locally. The unresolved prior-art decision and operational fitness decision are recorded in the complete table above.

<!-- pdca:leaf-complete -->

### Advisory — adversary

# Adversarial review — #1015 (625.1: multipart reaper, `Open` half)

**Bottom line:** I could not break the binding red→green. Two claims around it do fail, and I
proved both by running probes on a scratch copy: (1) no test notices if the deployed loop stops
passing the reaper; (2) a listing fault that keeps recurring leaves the uploads already read
unjudged forever. Toolchain was available; both probes ran.

## Red→green evidence — tried to refute it, could not

- `gate-logs/C4-verify.log`: all 10 tests fail on the base, every one at argument parsing
  (`flag --reap-abandoned-uploads needs a value`), and all 10 pass with the fix. The test runs the
  real binary as a subprocess against redb and checks what the store holds
  (`crates/server/tests/multipart_reaper.rs:155-165`). Leg A's fixture can only be reaped by the
  idle arm (created now−14d, progress now−10d, leases now−9d; `:495-510`). Leg B is not vacuous
  after the fix, because it also asserts a successful exit (`:611-616`). Leg H's red on the base
  comes only from its signal assertion (`:781-784`), as the brief predicted. I found no
  tautology and no mocked-away defect.
- The X60/X62 DST rows (seeded deterministic simulation tests) drive the production `UploadPart`
  against the real reaper (`crates/dst/tests/custodian.rs:7468-7641`). X60 keeps a live slot
  lease, so reading `psum:` first would see the new part in neither range and fence it, which
  means the row does discriminate. The slot-cycle gap is an accepted deferral (`:8535-8539`), so
  I did not re-raise it.

## Findings

- NEEDS-HUMAN [impl] — **No test covers the reaper's wiring into the deployed loop.**
  `crates/server/src/custodian.rs:680` passes `Some(&reaper)` to the reconstruction pass. In a
  scratch copy I changed it to `None` and ran every suite that drives `run_reconstruction_until`:
  `custodian_gc` (11), `custodian_day_one` (15) and `closed_write_path` (1). All 27 still pass.
  The only "deployed" coverage is a hand copy of the loop's shape that calls `reconcile_step`
  directly (`crates/custodian/tests/gc.rs:1484-1530`, `deployed_step`), and the DST rows do the
  same (`crates/dst/tests/custodian.rs:7387-7401`). So nothing tests the brief's Production-reach
  claim ("a healthy fleet therefore reaps every interval"). A merge that drops the argument would
  ship with every gate green. Fix: add one case to `crates/server/tests/custodian_gc.rs` that
  seeds an idle `Open` session, runs `drive_passes` (`:1341`), and asserts the session is fenced
  and torn down.

- NEEDS-HUMAN [impl] — **A listing fault that keeps recurring starves the uploads before it,
  forever.** On a page fault, `crates/custodian/src/reaper.rs:654-658` stops the walk, and
  `:692-695` saves the cursor at the last key read. Each later pass resumes there, hits the same
  fault, and never returns to the head. Under the shipped profile this needs only 17 sessions
  (`SESSION_PAGE` = 16, `:238`). Probe (scratch `reaper::tests`): 5 sessions kept at T, with a
  fault on the page after the 3rd. At T+8d all are idle past `W_open`, yet five more passes each
  report `held: 1, kept: 0, fenced_idle: 0`, the cursor stays at `mpu:0202…`, and sessions 0–2
  stay `Open`. The hold names the cursor key, not the starved sessions. The existing test at
  `crates/custodian/src/reaper/tests.rs:1003-1033` asserts this behaviour as intended ("the next
  pass resumes at the fault"). Fix: keep the hold, but start the next pass from the head after a
  listing fault. The one surviving C5 mutant (`reaper.rs:679`, `&&`→`||`) only changes the cursor
  on this same post-fault path, so a test for the fix would also kill it. This confirms T4's
  `reaper.rs:693` / `:697` items.

- NEEDS-HUMAN [impl] — **Fences committed earlier in a pass are dropped from the counters when
  the pass's last ledger read fails or refuses.** `reaper.rs:476` runs
  `check_admission_profile(..).await?` after `walk` has already committed fences. A single
  transient read fault there (no damage needed) makes `pass` return `Err`, so `reconcile`
  (`:448-456`) never calls `emit_pass`, and `reaper_sessions_fenced_*` never counts those fences.
  A refusal at that read (`:477-479`, `:487-493`) also skips `emit_pass`, and its message says the
  reaper "fences nothing" (`:1187-1189`). The per-fence info line (`:1239-1247`) still records
  each fence, so this affects counters and log accuracy, not data. Fix: emit the tallies on every
  exit after `walk`, and say in the refusal text that fences may already have committed.

- NEEDS-HUMAN [human] — **T4 triage: three of the 11 T4 blockers can only be reached through
  outside damage today; record them as rejected or deferred, with a reason.**
  (a) "The fence has no admission-ledger precondition" (`reaper.rs:1123`, raised 3×) requires
  `mpuctl` (the admission ledger) to disappear or change profile while a session is `Open`. Only
  two writers touch it: Create (`crates/core/src/multipart/create.rs:684-695`, which refuses a
  profile change at `:427`) and the terminal delete
  (`crates/custodian/src/retirement/sessions.rs:975-985`). Both require the prior bytes, and
  neither deletes the ledger or changes its profile. Pinning `mpuctl` in the fence would also make
  every fence lose to any concurrent Create or terminal delete, since both rewrite `count`.
  (b) The stale-clean-lap items (`reaper.rs:712`/`:714`, 3×) need more than 64 sessions listed.
  The only profile this custodian accepts admits at most 46 (`reaper.rs:205`; Create's cap at
  `create.rs:433-440`). They become reachable once #508 plumbs in a larger profile, so a deferral
  pointing there fits.
  (c) The `reconciliation.rs:187` convention item is real, but only at the API level: the server
  passes one store and one clock into both contexts (`custodian.rs:611-639`).
  Recording these decisions stops T4 from going red on them every round.

- NEEDS-HUMAN [human] — **Low priority: the pass reads every open session's full `psum:` range
  every 30 s.** `visit` reads the whole `psum:` range (up to 10,016 records) even when a live slot
  has already decided the session will be kept (`reaper.rs:794-807`). It does this for every
  `Open` session inside `W_session`, on every `--interval-secs` (default 30, `cli.rs:896`). At the
  shipped cap the worst case is 46 × 10,016 ≈ 460k record reads per interval, against windows
  measured in days. The doc at `reaper.rs:253-257` calls this "a few bounded ranges". This slice
  owns the cadence, so decide before #668 creates real sessions whether to run the pass less often
  or skip the `psum:` read when a slot is live (keeping the session is the safe direction).

I also tried these and could not break them: the drain and the reaper reading different clocks
(both shapes hand the same `LoopClock` to both, `custodian.rs:470-476`, `:634-639`); the one-shot
failing to finish on normal crash residue (that residue decodes as trusted, and leg A finishes
within 6 runs); and leg H passing on the base for the wrong reason (it fails there on the
missing signal).

<!-- pdca:leaf-complete -->

### Advisory — code-review

- NEEDS-HUMAN [impl] — `crates/custodian/src/reaper.rs:693`: A persistent page-read fault permanently strands the cursor before that page. After a pass keeps the readable prefix and fails on the next page, subsequent passes resume at the same failing boundary; uploads in the prefix are never reconsidered when they cross `W_open` or `W_session`. Recover or periodically restart the walk after a failed page so readable uploads still receive judgments. Add a regression that advances the clock while the later-page fault persists and verifies that an earlier upload is fenced.

- NEEDS-HUMAN [impl] — `crates/custodian/src/reaper.rs:714`: The persisted clean-lap flag certifies time-dependent judgments without recording their freshness. With 65 retained uploads, let the first 64 be six days old and the last newly created. The first invocation keeps 64 and saves a clean cursor; two days later, the next invocation keeps the last upload and reports `Satisfied`, although the first 64 are now idle-reapable. The record-only drain leaves these `Open` uploads alone, so the CLI tells the operator there is nothing to reap. Invalidate accumulated certification when earlier judgments can have expired, and test advancing the clock between passes; the current lap tests keep time fixed.

- NEEDS-HUMAN [impl] — `crates/custodian/src/reaper.rs:477`, `crates/custodian/src/reaper.rs:1187`: A final profile refusal discards the pass's tallies and emits “fences nothing” even when fences already committed. The existing regression at `crates/custodian/src/reaper/tests.rs:1400` explicitly expects such a committed fence before the ledger disappears. Returning `None` also bypasses `emit_pass`, so those fences disappear from the outcome counters. Preserve and emit the completed work on a final refusal, distinguish it from a refusal before work began, and assert both the audit message and counters in that regression.

No additional reuse, simplification, or efficiency findings. Reviewed against the read-only target and frozen gate evidence; no builds or tests were rerun. The brief's accepted deferrals are excluded.
<!-- pdca:leaf-complete -->

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C5 Causal adequacy — Fix F1/F2 before relying on bounded residency or the completion signal—persistent page faults prevent revisits, and an old clean cursor certifies uploads now eligible for reaping (`crates/custodian/src/reaper.rs:654`, `crates/custodian/src/reaper.rs:714`).
- [ ] T4 Contribution — Confirm affected-path prior art against merged and closed/rejected work—the supplied target has one synthetic commit and no remote, and the bundle only records a July Plan search (`reviewer-prior-art.log:1`, `brief.md:258`); the contribution-artifact audit itself is N/A until publish.
- [ ] T5 Judgment — Correct F3's late-refusal audit before operators rely on it—it claims no fencing after an upload has durably become Aborting (`crates/custodian/src/reaper.rs:1187`, `reviewer-probes.log:7`).
- [ ] Validation — fitness-to-purpose — Accept the seven-day idle window, thirty-day residency ceiling and operator recovery procedure for intended deployments—automated correctness checks cannot choose retention policy or establish operational fitness (`crates/custodian/src/reaper.rs:183`, `crates/custodian/src/reaper.rs:189`, `crates/server/src/cli.rs:1107`).
- [ ] **No test covers the reaper's wiring into the deployed loop.** `crates/server/src/custodian.rs:680` passes `Some(&reaper)` to the reconstruction pass. In a scratch copy I changed it to `None` and ran every suite that drives `run_reconstruction_until`: `custodian_gc` (11), `custodian_day_one` (15) and `closed_write_path` (1). All 27 still pass. The only "deployed" coverage is a hand copy of the loop's shape that calls `reconcile_step` directly (`crates/custodian/tests/gc.rs:1484-1530`, `deployed_step`), and the DST rows do the same (`crates/dst/tests/custodian.rs:7387-7401`). So nothing tests the brief's Production-reach claim ("a healthy fleet therefore reaps every interval"). A merge that drops the argument would ship with every gate green. Fix: add one case to `crates/server/tests/custodian_gc.rs` that seeds an idle `Open` session, runs `drive_passes` (`:1341`), and asserts the session is fenced and torn down.
- [ ] **A listing fault that keeps recurring starves the uploads before it, forever.** On a page fault, `crates/custodian/src/reaper.rs:654-658` stops the walk, and `:692-695` saves the cursor at the last key read. Each later pass resumes there, hits the same fault, and never returns to the head. Under the shipped profile this needs only 17 sessions (`SESSION_PAGE` = 16, `:238`). Probe (scratch `reaper::tests`): 5 sessions kept at T, with a fault on the page after the 3rd. At T+8d all are idle past `W_open`, yet five more passes each report `held: 1, kept: 0, fenced_idle: 0`, the cursor stays at `mpu:0202…`, and sessions 0–2 stay `Open`. The hold names the cursor key, not the starved sessions. The existing test at `crates/custodian/src/reaper/tests.rs:1003-1033` asserts this behaviour as intended ("the next pass resumes at the fault"). Fix: keep the hold, but start the next pass from the head after a listing fault. The one surviving C5 mutant (`reaper.rs:679`, `&&`→`||`) only changes the cursor on this same post-fault path, so a test for the fix would also kill it. This confirms T4's `reaper.rs:693` / `:697` items.
- [ ] **Fences committed earlier in a pass are dropped from the counters when the pass's last ledger read fails or refuses.** `reaper.rs:476` runs `check_admission_profile(..).await?` after `walk` has already committed fences. A single transient read fault there (no damage needed) makes `pass` return `Err`, so `reconcile` (`:448-456`) never calls `emit_pass`, and `reaper_sessions_fenced_*` never counts those fences. A refusal at that read (`:477-479`, `:487-493`) also skips `emit_pass`, and its message says the reaper "fences nothing" (`:1187-1189`). The per-fence info line (`:1239-1247`) still records each fence, so this affects counters and log accuracy, not data. Fix: emit the tallies on every exit after `walk`, and say in the refusal text that fences may already have committed.
- [ ] **T4 triage: three of the 11 T4 blockers can only be reached through outside damage today; record them as rejected or deferred, with a reason.** (a) "The fence has no admission-ledger precondition" (`reaper.rs:1123`, raised 3×) requires `mpuctl` (the admission ledger) to disappear or change profile while a session is `Open`. Only two writers touch it: Create (`crates/core/src/multipart/create.rs:684-695`, which refuses a profile change at `:427`) and the terminal delete (`crates/custodian/src/retirement/sessions.rs:975-985`). Both require the prior bytes, and neither deletes the ledger or changes its profile. Pinning `mpuctl` in the fence would also make every fence lose to any concurrent Create or terminal delete, since both rewrite `count`. (b) The stale-clean-lap items (`reaper.rs:712`/`:714`, 3×) need more than 64 sessions listed. The only profile this custodian accepts admits at most 46 (`reaper.rs:205`; Create's cap at `create.rs:433-440`). They become reachable once #508 plumbs in a larger profile, so a deferral pointing there fits. (c) The `reconciliation.rs:187` convention item is real, but only at the API level: the server passes one store and one clock into both contexts (`custodian.rs:611-639`). Recording these decisions stops T4 from going red on them every round.
- [ ] **Low priority: the pass reads every open session's full `psum:` range every 30 s.** `visit` reads the whole `psum:` range (up to 10,016 records) even when a live slot has already decided the session will be kept (`reaper.rs:794-807`). It does this for every `Open` session inside `W_session`, on every `--interval-secs` (default 30, `cli.rs:896`). At the shipped cap the worst case is 46 × 10,016 ≈ 460k record reads per interval, against windows measured in days. The doc at `reaper.rs:253-257` calls this "a few bounded ranges". This slice owns the cadence, so decide before #668 creates real sessions whether to run the pass less often or skip the `psum:` read when a slot is live (keeping the session is the safe direction).
- [ ] `crates/custodian/src/reaper.rs:693`: A persistent page-read fault permanently strands the cursor before that page. After a pass keeps the readable prefix and fails on the next page, subsequent passes resume at the same failing boundary; uploads in the prefix are never reconsidered when they cross `W_open` or `W_session`. Recover or periodically restart the walk after a failed page so readable uploads still receive judgments. Add a regression that advances the clock while the later-page fault persists and verifies that an earlier upload is fenced.
- [ ] `crates/custodian/src/reaper.rs:714`: The persisted clean-lap flag certifies time-dependent judgments without recording their freshness. With 65 retained uploads, let the first 64 be six days old and the last newly created. The first invocation keeps 64 and saves a clean cursor; two days later, the next invocation keeps the last upload and reports `Satisfied`, although the first 64 are now idle-reapable. The record-only drain leaves these `Open` uploads alone, so the CLI tells the operator there is nothing to reap. Invalidate accumulated certification when earlier judgments can have expired, and test advancing the clock between passes; the current lap tests keep time fixed.
- [ ] `crates/custodian/src/reaper.rs:477`, `crates/custodian/src/reaper.rs:1187`: A final profile refusal discards the pass's tallies and emits “fences nothing” even when fences already committed. The existing regression at `crates/custodian/src/reaper/tests.rs:1400` explicitly expects such a committed fence before the ledger disappears. Returning `None` also bypasses `emit_pass`, so those fences disappear from the outcome counters. Preserve and emit the completed work on a final refusal, distinguish it from a refusal before work began, and assert both the audit message and counters in that regression.
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 11 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_1015/review
- [ ] **The advertised binding red→green leg has no compatible verification path.** `brief.md:56-61` calls physical fragment reclamation in the existing `crates/custodian/tests/gc.rs` “the binding red→green leg,” using the reaper through in-process `reconcile_step`. But `brief.md:137-140` selects only the NEW server test for C4, and `brief.md:220-224` explicitly says a direct call cannot compile against both trees after the signature change. The target exposes the current signature at `crates/custodian/src/reconciliation.rs:126-134`; the added reaper context does not exist there. Thus the subprocess test can establish RED for durable metadata, while the specified GC test supplies post-change evidence, not the promised same-test RED→GREEN for physical reclamation. Revise the criterion to distinguish those claims, or specify a base-compatible path and command that actually runs the physical-reclamation assertion on both trees.
- [ ] **The required batch splitter crosses an unresolved ownership boundary.** `brief.md:175-176` requires “the batch splitter unit-tested,” although `brief.md:13-17` assigns the drain and owned-`sidx:` walk exclusively to #659 and prohibits reimplementing them. This slice's reap fence must remain atomic: `docs/design/proposals/draft/0016-multipart-commit-protocol.md:647-649` explicitly says the fence's slot pins cannot be split; its batch inventory assigns the splittable work to retirement and owned-entry drain steps (`:668`, `:672`). The existing reusable fence builder returns one `WriteBatch` (`crates/core/src/multipart/abort.rs:77-92`). Identify the prerequisite-owned splitter whose tests are to be reused, and require this slice to verify its unsplit fence's byte/operation bounds; otherwise remove the splitter requirement. As written, Do must choose between inventing extra batching machinery and violating an explicit acceptance requirement.
- [ ] size backstop — this slice is behaving oversized: patch is 405 KB (threshold 100 KB); patch touches 46 files (threshold 20). Recommend answering `iterate-plan` at sign-off and authoring the split in the re-plan (`pdca split`), rather than `iterate-do`: a slice that is too big yields implementation-shaped findings every round, and splitting authors briefs, which is Plan's beat.

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
- Iteration delta (if iterating): Rebuild inside this slice (attempt 6, narrow). Red->green and full CI pass; findings dropped 15 -> 14 -> 11 and are implementation-shaped, so no re-plan. Fix these, each with a test that fails today: 1. F1 — persistent listing fault starves the readable prefix forever (crates/custodian/src/reaper.rs:654-658, :692-695): after a page fault the cursor is saved at the failing boundary and every later pass resumes there. Keep the hold/signal, but start the next pass from the head after a listing fault. Regression: fault a later page persistently, advance the clock past W_open, assert an earlier session is fenced. Update the test at reaper/tests.rs:1003-1033 that currently asserts the stuck behaviour. This should also kill the surviving C5 mutant at reaper.rs:679. 2. Deployed-loop wiring test: crates/server/src/custodian.rs:680 can be changed from Some(&reaper) to None with all suites green. Add one case to crates/server/tests/custodian_gc.rs that seeds an idle Open session, runs drive_passes (:1341), and asserts it is fenced and torn down. 3. F3 — late profile refusal (reaper.rs:476-493, :1187-1189): emit the pass tallies (emit_pass) on every exit after walk, including a transient read error and a final refusal; distinguish "refused before acting" from "stopped after fencing N" in the message. Assert message and counters in the regression at reaper/tests.rs:1400. Deferred / not to fix: F2 (stale clean-lap, reaper.rs:714) needs >64 sessions; the only accepted profile caps at 46 — defer to #508 and say so in build-notes and a doc comment. Record as rejected with reasons the T4 items reachable only through outside damage: no mpuctl precondition on the fence (reaper.rs:1123) and the reconciliation.rs:187 separate-context convention (one store and clock passed to both, custodian.rs:611-639). The psum-read cost per interval is low priority; do not change cadence in this round. Keep as before: do NOT rebase (stacked on unmerged #1027, #1007-#1013; diff coverage unmeasured is accepted); the slot-cycle gap stays accepted/ignored; #659 follow-ups stay out of scope; the batch-splitter and binding-red->green plan findings stay resolved (brief.md:264).
- By / date: Eduard Ralph / 2026-11-10

## 10. Act candidates (hints for the next Act review)
- Plan advisory: 2 finding(s); brief revised: yes (plan-advisory-*.md)
- (empty is the common case)
