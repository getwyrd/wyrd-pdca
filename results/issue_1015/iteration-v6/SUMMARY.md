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
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): fail — 200 mutants tested in 16m: 177 caught, 22 unviable, 1 timeouts

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 8 blocking, 2 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_1015/review-
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 13.68s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Review #1015: reclaim abandoned Open multipart sessions through the custodian and operator one-shot; independent red→green succeeds, with two narrow implementation concerns remaining.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | Idle reclamation, protection of progressing uploads, the residency ceiling and profile refusal have falsifiable durable-state criteria; the latest scope decisions are explicit (`brief.md:32`, `brief.md:333`). |
| C2 Reproduction (red pre-fix) | PASS | Stashing production changes while retaining the new subprocess test reproduced 10 executed failures, including unchanged abandoned-session state; the binding red is executable rather than a compile failure (`reviewer-red.log:9`, `reviewer-red.log:133`, `target/crates/server/tests/multipart_reaper.rs:181`). |
| C3 Change | PASS | The approved slice reaches both the operator command and healthy deployed loop through the fenced control point, reusing the teardown builder and drain; the current carry-forward requests are covered (`target/crates/custodian/src/reconciliation.rs:150`, `target/crates/server/src/custodian.rs:680`, `target/crates/custodian/src/reaper.rs:750`). |
| C4 Verification (red→green) | PASS | Restoring the exact patch changed the same 10 tests to green; focused deployed-loop, custodian, GC and seeded DST reruns also passed; frozen full CI passed, while diff coverage remains unmeasured for the explicitly accepted stacked-base reason (`reviewer-restored-green.log:37`, `gate-logs/C4-ci.log:5059`, `brief.md:338`). |
| C5 Causal adequacy | NEEDS-HUMAN [impl] | Strengthen the required maximum-fence proof: the purported widest fixture omits legal near-ceiling session encodings, so it cannot establish the requested worst-case budget regression protection; F1 below (`target/crates/custodian/src/reaper/tests.rs:1314`, `target/crates/core/src/multipart/create.rs:345`). |
| T1 Structure | PASS | The new policy/context preserve trait-based dependency direction and share the production clock; record-only retirement reuses the same drain and explicitly holds work needing a fleet (`target/crates/server/src/custodian.rs:634`, `target/crates/custodian/src/retirement.rs:429`, `target/crates/custodian/src/retirement.rs:586`). |
| T2 Shape | PASS | The larger test surface remains within the expressly retained single slice; architecture docs cover the CLI, lifecycle and persisted walk position, and formatting/docs scanners pass (`brief.md:333`, `target/docs/design/architecture/06-runtime-view.md:94`, `reviewer-scanners.log:3`). |
| T3 Runtime | PASS | Reruns protect live uploads, reclaim both byte sets at the single-pass grace boundary and exercise production renewal interleavings; external metadata awaits inherit the backend termination contract (`target/crates/custodian/tests/gc.rs:1425`, `target/crates/traits/src/lib.rs:1333`, `reviewer-dst.log:249`). |
| T4 Contribution | NEEDS-HUMAN | Confirm affected-path merged history and closed/rejected prior work: the supplied target has only one synthetic base commit and no remote, and frozen logs supply no such history, so prior-art clearance cannot be certified; publish-artifact checking is separately N/A until publish (`reviewer-prior-art.log:2`, `reviewer-prior-art.log:14`, `gate-logs/T4-contribution.log:10`). |
| T5 Judgment | NEEDS-HUMAN [impl] | Make uncertain fence outcomes truthful to operators: a landed fence with a lost reply is reported as leaving the session in place, while the drain proceeds and no fence is counted on either pass; F2 below (`target/crates/custodian/src/reaper.rs:1218`, `target/crates/custodian/src/reaper.rs:1233`, `reviewer-probes.log:138`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Approve the seven-day idle window, thirty-day administrative ceiling and this slice's healthy-fleet/one-shot operating scope: tests establish behavior but cannot decide whether ending a still-progressing over-age upload fits operator expectations (`target/crates/custodian/src/reaper.rs:189`, `target/crates/custodian/src/reaper.rs:195`, `brief.md:160`). |

Two corrections are warranted; neither establishes a new data-loss defect. The binding test independently fails before the fix and passes afterward. Accepted protocol gaps and rejected finding classes remain settled, and this review does not gate acceptance.

- **F1 — The largest-fence test does not exercise the largest legal session.** Its session uses 1,024 ASCII object bytes and 256 content-type bytes (`target/crates/custodian/src/reaper/tests.rs:1322`), whereas Create checks the widest reachable encoded record against the metadata value ceiling (`target/crates/core/src/multipart/create.rs:345`). An independent probe called production Create successfully with a 98,207-byte session and constructed its full-slot fence: 20 operations and 199,802 bytes (`reviewer-probes.log:163`). This still fits the current 500-operation/5,000,000-byte budgets: the finding is a missing required boundary proof, not an observed oversized production batch. Extend the regression to the largest admitted encoded shape, including escaping/headroom, or assert a conservative independently derived ceiling that covers every admitted session.
- **F2 — An unknown commit result is not evidence that the upload stayed untouched.** The error arm unconditionally calls `hold`, whose audit says the session was left in place; its detail correctly describes uncertainty, contradicting that conclusion (`target/crates/custodian/src/reaper.rs:1218`, `target/crates/custodian/src/reaper.rs:1233`). A probe wrapped real redb atomic commits, applied the fence, then returned the seam's out-of-flight `CommitUnknownResult`. The first step returned Blocked with the session already Aborting and its retirement obligation drained; the next deleted the session. Both passes emitted zero fenced counts (`reviewer-probes.log:140`, `reviewer-probes.log:152`, `reviewer-probes.log:162`). Distinguish uncertain outcomes from definite failures in the audit and telemetry; a later read may establish state, but an in-flight unknown must remain uncertain. Add a regression that actually applies the batch before losing its reply; the existing failure test only exercises a non-applied write (`target/crates/custodian/src/reaper/tests.rs:1285`).

Verification is supported by independent executions and the frozen logs, with these limits:

- **Independent execution:** 10 pre-fix failures became 10 passes after stash/pop; 12 deployed-loop tests, 37 custodian unit tests and 14 GC tests passed. Six reaper DST cases passed with `MADSIM_TEST_NUM=20`; the already accepted slot-cycle case stayed ignored. Formatting, `git diff --check`, typos, documentation lint and a real 101-page render/link audit passed (`reviewer-red.log:133`, `reviewer-restored-green.log:21`, `reviewer-custodian.log:93`, `reviewer-custodian.log:113`, `reviewer-dst.log:249`, `reviewer-scanners.log:12`). The original target patch/status was restored; no product or test edits were made.
- **Frozen gates:** the instance-scoped wrapper runs were adjudicated from their supplied logs. Full CI reached “all checks passed,” including its 50-seed DST configuration; both TiKV clippy commands completed (`gate-logs/C4-ci.log:5059`, `target/xtask/src/main.rs:1780`, `gate-logs/host-tikv.log:211`). The brief's external tools, typos and docs-renderer, were actually exercised, including independently here. Mutation testing reported 177 caught, 22 unviable and one timeout among 200 mutants; it did not report a surviving mutant, but its timeout is not a clean mutation-gate pass (`gate-logs/C5-mutants.log:13`).
- **Coverage caveat:** the diff-coverage wrapper could not apply this stacked patch to origin/main and measured no coverage (`gate-logs/C4-diff-cov.log:10`). The latest brief expressly accepts this pending prerequisites; it is not a C4 compilation defect, and the supplied target itself built and ran successfully (`brief.md:338`).
- **Contribution-artifact gate: N/A.** The gate found no draft PR description by design; its substantive audit is required at publish, not a human-clearance item now (`gate-logs/T4-contribution.log:10`). The affected-path prior-art investigation ran across every patched path but found only the synthetic root commit; historical merged and rejected work remains the T4 decision above. Tier-1 disk-fault and Tier-2 kill/reconstruct observation is appropriate before production rollout of this custodian lifecycle; those campaigns are not claimed as exercised here.

The batch-review log's eight reported blockers reduce to F1/F2 after applying the supplied protocol (`gate-logs/T4-batch-review.log:10`). The stray-obligation drain issue is an accepted #659 follow-up; the two separate-context reports and missing ledger pin are expressly rejected in the carry-forward (`brief.md:308`, `brief.md:337`). The GC flag combination was exercised and returns successful reaper-only output (`reviewer-cli.log:9`): `--gc-expired-pending` selects a GC policy, while this documented one-shot deliberately runs no GC, so this is not evidence of a missed promised sweep (`target/crates/server/src/cli.rs:1025`, `target/crates/server/src/cli.rs:1098`). The progressing-upload leg's no-op weakness is the brief's acknowledged vacuity; the suite's positive reclamation legs and dedicated unit/DST cases supply its discrimination, so a successful no-op is not a new standalone defect (`brief.md:108`, `target/crates/server/tests/multipart_reaper.rs:492`).

No capability probe or fallback masks a load-time cause: profile, clock and fleet checks enforce explicit domain preconditions. The slot-cycle gap, stale clean-lap behavior deferred to #508, and #659 follow-ups are not reopened (`target/crates/custodian/src/reaper.rs:109`, `target/crates/custodian/src/reaper.rs:254`, `brief.md:338`).
<!-- pdca:leaf-complete -->

### Advisory — adversary

# Adversarial review — #1015 (625.1, multipart reaper `Open` half), iteration 6

Lens: refute the red→green and the reviewer's verdict; find the input that breaks the fix.
Grounded on the patched tree at `$PDCA_TARGET` (base `3c600b6` + patch). I found no correctness
defect in the reaper's judgment, fence or step wiring. Three small implementation items and one
process item follow; none of them loses data.

## Findings

- NEEDS-HUMAN [impl] — `crates/server/src/cli.rs:1007-1013`, `:1025-1029`, `:1103-1126`: the one-shot rejects only the `--reconcile-after-restore` combination. It parses `--gc-expired-pending` and then returns before anything reads it. Concrete case: `wyrd custodian --gc-expired-pending --reap-abandoned-uploads` reports the reaper's outcome and exits 0 (on a clean store), yet runs no expired-pending sweep, even though the operator just attested there are no writers so that the sweep could run. This is the rubric's "never silent success" class. T4 raised it too. Fix: refuse the combination, as `:1007` already does for the restore one-shot. `--endpoints/--ids/--failure-domains` are silently ignored the same way; the record-only drain then holds by-chunk records "for a pass that sees the whole fleet" even though a whole fleet was named, so refusing those too would be clearer.
- NEEDS-HUMAN [impl] — `crates/server/src/cli.rs:1671` → `:2019-2022` (`create_dir_all` + `RedbMetadataStore::open` → `Database::create`), default `--data-dir` `wyrd-data` (`:65`, relative): run from the wrong directory, or with a mistyped `--data-dir`, the one-shot creates an empty `meta.redb`. `reaper.rs:542-547` then answers `Satisfied` (no ledger, empty listing), and the command prints "satisfied — nothing to reap or drain" (`cli.rs:1110`) and exits 0. That is the hollow green that `cli.rs:1119-1120` says it avoids. Low severity: the restore one-shot shares the helper (`cli.rs:1632`), so the human may decline this with an issue reference as existing behaviour. A cheap fix is to name the opened store path in the outcome line, or to refuse when `meta.redb` did not exist before the open.
- NEEDS-HUMAN [impl] — `crates/custodian/src/reaper.rs:1216-1218` with `:1224-1235`: when the fence commit returns `Err` (for example TiKV/FDB commit-unknown), the session is held with the audit line "left a session in place it cannot judge or could not fence", and no `reaper_sessions_fenced_*` counter moves. If the batch actually landed, the same step's retirement leg (`reconciliation.rs:185-187`, `:199-201`) then walks and deletes that `Aborting` session. The operator sees "held / left in place" for an upload that was aborted and torn down, and no fence tally explains it, against the module's own rule at `reaper.rs:478-481`. No data effect: `Blocked` and a non-zero exit are the right fail-safe. The fix is wording and telemetry only: name the outcome as unknown, with a distinct action and counter. T4 raised the same thing.
- NEEDS-HUMAN [human] — `crates/dst/tests/custodian.rs:8535-8539` and `crates/custodian/src/reaper.rs:109-128`: the accepted slot reserve→commit→release gap still carries "tracker issue to be filed" in its `#[ignore]` reason, four rounds after it was accepted. The rubric treats a deferral as settled only when it is "tracked in #N". This gap is a destructive false positive (it aborts an upload that has just committed a part), and the module doc says its fix must land before #668/#508 create sessions in production. File the tracker and cite its number in the ignore reason and the module doc before merge, so the ordering requirement has an issue to hang on.

## Attempted to refute; could not

- **Red→green is real and on the production path.** `gate-logs/C4-verify.log`: 10 tests fail on the base, all through the real `wyrd` binary (`multipart_reaper.rs:155-165`, flag last), and 10 pass with the fix. The red is shallow (the base exits 2 with "needs a value"), as the brief prescribed. The green's store-state assertions carry the weight. No parallel re-implementation, and no mocked-away defect.
- **Each leg's fixture isolates its predicate.** A: 14d/10d/9d, so only the idle arm can reap it. Each of B's four sessions is saved by exactly one piece of evidence: (i) the sidx lease, because progress is 10d and there is no slot; (ii) the live slot lease; (iii) the psum from 1h ago; (iv) `reserved_at` from 1h ago with a lapsed lease. B also asserts exit 0, which rules out `Blocked`. C is over-age with everything live. E-open checks foreign holds plus the drain of the `Aborting` session. H checks the store is byte-identical with an `Aborting` session and its obligation present. Dropping any one predicate fails a named leg.
- **The DST rows can fail.** The X60 hook (`dst/tests/custodian.rs:7334-7343`) fires on the second progress read *in either order*, so a psum-first reader would fence and fail the row. X62 releases the renewal at `BeforeFenceOf`, so dropping the slot pins fails it. Its ceiling variant asserts the fence lands.
- **The GC case** (`custodian/tests/gc.rs:1425-1479`) now runs exactly one pass at `mark+grace−1` and one at `mark+grace`.
- **Batch budget.** The largest fence is 20 ops. Even a session record at the 100 KB value ceiling puts it near 200 KB against `MAX_BATCH_BYTES` = 5 MB, so T4's `reaper/tests.rs:1314` "short record" gap does not matter.
- **Profile-pin race** (T4, `reaper.rs:1181`): Create refuses any profile other than the stored one (`core/src/multipart/create.rs:11`, `:427`), so a profile can only be installed when no ledger exists. Any session a pass could then see is fresh, and neither arm fences a fresh session. The earlier rejection stands.
- **Permanent holds don't spread.** The deployed loop ignores the reconstruction pass's `Ok` value (`server/src/custodian.rs:684-696`). A foreign-clocked or stamp-ahead hold makes that pass `Blocked`, but GC and scrub still run.
- **Stamp-ahead hold.** δ_clock is the deployment-wide 1 s budget (`gc.rs:220-231`), so holding a stamp more than 1 s ahead is consistent with the leases and grace checks that share it.
- **C5's one TIMEOUT** (`cli.rs:1121`, `==`→`!=`) is not a surviving mutant. Every loop in `multipart_reaper.rs` is bounded (`CONVERGE_RUNS` = 6, `ONE_SHOT_BOUND`), and the suite runs in 0.34 s, so the timeout is most likely host load.
- **Other T4 blockers are already settled:** `reconciliation.rs:199` (stray obligation beside an `Open` session) was put out of scope as a #659 follow-up in iteration 3, and `reconciliation.rs:185/187` (separate stores) was rejected with a reason in iteration 5.
- **Diff coverage unmeasured** (`C4-diff-cov`): accepted at sign-off because the bundle stacks on unmerged prerequisites. C5 (177 caught, 0 missed) partly covers for it.

<!-- pdca:leaf-complete -->

### Advisory — code-review

- NEEDS-HUMAN [impl] — `crates/custodian/src/reaper.rs:1218`: distinguish an indeterminate fence commit from a definite failure. A backend may apply the fence and then return `CommitUnknownResult`; this branch emits “left a session in place” through `hold()` and records zero fences. `crates/custodian/src/reconciliation.rs:199` still dispatches the drain, so that same step can retire the upload, and subsequent passes cannot recover the missing fence tally. Classify the shared error type, report uncertainty explicitly, and reconcile the outcome where possible before emitting definitive telemetry. Add an apply-then-unknown regression; the current failing-commit double (`crates/custodian/src/reaper/tests.rs:215`) only errors before applying anything.

- NEEDS-HUMAN [impl] — `crates/custodian/src/reaper/tests.rs:1314`: `widest_open_session()` is not the largest legal session: it uses a 1,024-byte ASCII object and a 256-byte content type, whereas Create admits records according to their widest encoded state up to `MAX_VALUE_BYTES` (`crates/core/src/multipart/create.rs:345`). Consequently, `largest_fence_fits_both_batch_budgets()` checks a much smaller fence than its claimed bound and can miss byte-budget regressions affecting admitted uploads. Exercise a session near the actual admission ceiling, including JSON expansion, with all maximum-width slot pins, while retaining the independent cost assertion.

No additional actionable reuse, simplification, or efficiency findings within the agreed scope. Previously accepted gaps and rejected findings are excluded.

<!-- pdca:leaf-complete -->

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C5 Causal adequacy — Strengthen the required maximum-fence proof: the purported widest fixture omits legal near-ceiling session encodings, so it cannot establish the requested worst-case budget regression protection; F1 below (`target/crates/custodian/src/reaper/tests.rs:1314`, `target/crates/core/src/multipart/create.rs:345`).
- [ ] T4 Contribution — Confirm affected-path merged history and closed/rejected prior work: the supplied target has only one synthetic base commit and no remote, and frozen logs supply no such history, so prior-art clearance cannot be certified; publish-artifact checking is separately N/A until publish (`reviewer-prior-art.log:2`, `reviewer-prior-art.log:14`, `gate-logs/T4-contribution.log:10`).
- [ ] T5 Judgment — Make uncertain fence outcomes truthful to operators: a landed fence with a lost reply is reported as leaving the session in place, while the drain proceeds and no fence is counted on either pass; F2 below (`target/crates/custodian/src/reaper.rs:1218`, `target/crates/custodian/src/reaper.rs:1233`, `reviewer-probes.log:138`).
- [ ] Validation — fitness-to-purpose — Approve the seven-day idle window, thirty-day administrative ceiling and this slice's healthy-fleet/one-shot operating scope: tests establish behavior but cannot decide whether ending a still-progressing over-age upload fits operator expectations (`target/crates/custodian/src/reaper.rs:189`, `target/crates/custodian/src/reaper.rs:195`, `brief.md:160`).
- [ ] `crates/server/src/cli.rs:1007-1013`, `:1025-1029`, `:1103-1126`: the one-shot rejects only the `--reconcile-after-restore` combination. It parses `--gc-expired-pending` and then returns before anything reads it. Concrete case: `wyrd custodian --gc-expired-pending --reap-abandoned-uploads` reports the reaper's outcome and exits 0 (on a clean store), yet runs no expired-pending sweep, even though the operator just attested there are no writers so that the sweep could run. This is the rubric's "never silent success" class. T4 raised it too. Fix: refuse the combination, as `:1007` already does for the restore one-shot. `--endpoints/--ids/--failure-domains` are silently ignored the same way; the record-only drain then holds by-chunk records "for a pass that sees the whole fleet" even though a whole fleet was named, so refusing those too would be clearer.
- [ ] `crates/server/src/cli.rs:1671` → `:2019-2022` (`create_dir_all` + `RedbMetadataStore::open` → `Database::create`), default `--data-dir` `wyrd-data` (`:65`, relative): run from the wrong directory, or with a mistyped `--data-dir`, the one-shot creates an empty `meta.redb`. `reaper.rs:542-547` then answers `Satisfied` (no ledger, empty listing), and the command prints "satisfied — nothing to reap or drain" (`cli.rs:1110`) and exits 0. That is the hollow green that `cli.rs:1119-1120` says it avoids. Low severity: the restore one-shot shares the helper (`cli.rs:1632`), so the human may decline this with an issue reference as existing behaviour. A cheap fix is to name the opened store path in the outcome line, or to refuse when `meta.redb` did not exist before the open.
- [ ] `crates/custodian/src/reaper.rs:1216-1218` with `:1224-1235`: when the fence commit returns `Err` (for example TiKV/FDB commit-unknown), the session is held with the audit line "left a session in place it cannot judge or could not fence", and no `reaper_sessions_fenced_*` counter moves. If the batch actually landed, the same step's retirement leg (`reconciliation.rs:185-187`, `:199-201`) then walks and deletes that `Aborting` session. The operator sees "held / left in place" for an upload that was aborted and torn down, and no fence tally explains it, against the module's own rule at `reaper.rs:478-481`. No data effect: `Blocked` and a non-zero exit are the right fail-safe. The fix is wording and telemetry only: name the outcome as unknown, with a distinct action and counter. T4 raised the same thing.
- [ ] `crates/dst/tests/custodian.rs:8535-8539` and `crates/custodian/src/reaper.rs:109-128`: the accepted slot reserve→commit→release gap still carries "tracker issue to be filed" in its `#[ignore]` reason, four rounds after it was accepted. The rubric treats a deferral as settled only when it is "tracked in #N". This gap is a destructive false positive (it aborts an upload that has just committed a part), and the module doc says its fix must land before #668/#508 create sessions in production. File the tracker and cite its number in the ignore reason and the module doc before merge, so the ordering requirement has an issue to hang on.
- [ ] `crates/custodian/src/reaper.rs:1218`: distinguish an indeterminate fence commit from a definite failure. A backend may apply the fence and then return `CommitUnknownResult`; this branch emits “left a session in place” through `hold()` and records zero fences. `crates/custodian/src/reconciliation.rs:199` still dispatches the drain, so that same step can retire the upload, and subsequent passes cannot recover the missing fence tally. Classify the shared error type, report uncertainty explicitly, and reconcile the outcome where possible before emitting definitive telemetry. Add an apply-then-unknown regression; the current failing-commit double (`crates/custodian/src/reaper/tests.rs:215`) only errors before applying anything.
- [ ] `crates/custodian/src/reaper/tests.rs:1314`: `widest_open_session()` is not the largest legal session: it uses a 1,024-byte ASCII object and a 256-byte content type, whereas Create admits records according to their widest encoded state up to `MAX_VALUE_BYTES` (`crates/core/src/multipart/create.rs:345`). Consequently, `largest_fence_fits_both_batch_budgets()` checks a much smaller fence than its claimed bound and can miss byte-budget regressions affecting admitted uploads. Exercise a session near the actual admission ceiling, including JSON expansion, with all maximum-width slot pins, while retaining the independent cost assertion.
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 8 blocking, 2 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_1015/review-
- [ ] **The advertised binding red→green leg has no compatible verification path.** `brief.md:56-61` calls physical fragment reclamation in the existing `crates/custodian/tests/gc.rs` “the binding red→green leg,” using the reaper through in-process `reconcile_step`. But `brief.md:137-140` selects only the NEW server test for C4, and `brief.md:220-224` explicitly says a direct call cannot compile against both trees after the signature change. The target exposes the current signature at `crates/custodian/src/reconciliation.rs:126-134`; the added reaper context does not exist there. Thus the subprocess test can establish RED for durable metadata, while the specified GC test supplies post-change evidence, not the promised same-test RED→GREEN for physical reclamation. Revise the criterion to distinguish those claims, or specify a base-compatible path and command that actually runs the physical-reclamation assertion on both trees.
- [ ] **The required batch splitter crosses an unresolved ownership boundary.** `brief.md:175-176` requires “the batch splitter unit-tested,” although `brief.md:13-17` assigns the drain and owned-`sidx:` walk exclusively to #659 and prohibits reimplementing them. This slice's reap fence must remain atomic: `docs/design/proposals/draft/0016-multipart-commit-protocol.md:647-649` explicitly says the fence's slot pins cannot be split; its batch inventory assigns the splittable work to retirement and owned-entry drain steps (`:668`, `:672`). The existing reusable fence builder returns one `WriteBatch` (`crates/core/src/multipart/abort.rs:77-92`). Identify the prerequisite-owned splitter whose tests are to be reused, and require this slice to verify its unsplit fence's byte/operation bounds; otherwise remove the splitter requirement. As written, Do must choose between inventing extra batching machinery and violating an explicit acceptance requirement.
- [ ] size backstop — this slice is behaving oversized: patch is 426 KB (threshold 100 KB); patch touches 46 files (threshold 20). Recommend answering `iterate-plan` at sign-off and authoring the split in the re-plan (`pdca split`), rather than `iterate-do`: a slice that is too big yields implementation-shaped findings every round, and splitting authors briefs, which is Plan's beat.

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
- Iteration delta (if iterating): Rebuild inside this slice (attempt 7, narrow). Red->green and full CI pass; the remaining findings are implementation bugs, so no re-plan. Fix these, each with a test that fails today: 1. F2 — uncertain fence commit reported as "left in place" (crates/custodian/src/reaper.rs:1216-1235): when the fence commit returns an error that may have applied (e.g. CommitUnknownResult), do not call hold()'s "left a session in place" audit and do not leave the fence tally silent. Classify the error; report the outcome as unknown with its own audit action and counter. Regression: a commit double that applies the batch and then returns the unknown result (the current double at reaper/tests.rs:215 errors before applying). 2. F1 — largest-fence test is not the largest legal session (crates/custodian/src/reaper/tests.rs:1314): widest_open_session() uses a 1,024-byte object and 256-byte content type. Build a session at the real admission ceiling Create allows (crates/core/src/multipart/create.rs:345, including JSON escaping/headroom) with all maximum-width slot pins, and keep the independent cost assertion against MAX_BATCH_OPS / MAX_BATCH_BYTES. 3. One-shot silently ignores flags (crates/server/src/cli.rs:1007-1029, :1103-1126): refuse --reap-abandoned-uploads combined with --gc-expired-pending, and with --endpoints/--ids/--failure-domains, the same way :1007 refuses --reconcile-after-restore. Test each refused combination exits non-zero with a clear message. 4. Wrong --data-dir gives a false "satisfied" (cli.rs:1671 -> :2019-2022): the one-shot must not report "nothing to reap or drain" on a store it just created. Refuse when meta.redb did not exist before the open (or at minimum name the opened store path in the outcome line). Test with a non-existent data dir. Keep as before: do NOT rebase (stacked on unmerged #1027, #1007-#1013; diff coverage unmeasured is accepted); the slot-cycle gap stays accepted/ignored (its tracker number is a human item for sign-off); #659 follow-ups stay out of scope; the batch-splitter and binding-red->green plan findings stay resolved; F2-of-iteration-5 (stale clean-lap) stays deferred to #508; the recorded rejections in review-rejected.md stand.
- By / date: Eduard Ralph / 2026-11-10

## 10. Act candidates (hints for the next Act review)
- Plan advisory: 2 finding(s); brief revised: yes (plan-advisory-*.md)
- (empty is the common case)
