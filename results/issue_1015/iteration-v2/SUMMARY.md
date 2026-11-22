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
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass — run-verify.sh: PASS — red without the fix, green with it (6 test(s) ran red).
- C4 diff coverage: changed lines executed by the patch's tests: fail — diff coverage not measured — patch.diff does not apply on origin/main
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): fail — 144 mutants tested in 11m: 4 missed, 122 caught, 18 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 15 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_1015/review
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 15.20s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Review of #1015: reclaim abandoned `Open` multipart uploads through the custodian and an operator one-shot; the patch still aborts a freshly progressing upload in a reproduced race.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The brief gives falsifiable idle, ceiling, clock, profile and durable-reclamation criteria, with sibling work explicitly excluded; the unresolved protocol decision is identified under C5 (`brief.md:30`, `brief.md:165`, `brief.md:212`). |
| C2 Reproduction (red pre-fix) | PASS | With the tracked fix stashed and the new subprocess test retained, six tests compiled, ran and failed; reclamation legs failed durable-state assertions (`reviewer-red.log:7`, `reviewer-red.log:34`, `reviewer-red.log:115`). |
| C3 Change | PASS | The change reaches the real operator and fleet control point, reuses the shared abort fence and existing drain, and updates both living architecture documents; it stays within the stated Open-session surface (`crates/custodian/src/reaper.rs:700`, `crates/server/src/custodian.rs:672`, `docs/design/architecture/06-runtime-view.md:94`, `docs/design/architecture/08-crosscutting-concepts.md:113`). |
| C4 Verification (red→green) | FAIL | The binding subprocess suite independently goes 6 red → 6 green, but the explicitly ignored safety regression fails when enabled: actual `Aborting`, expected `Open`; this failure is on the usable patched target, unrelated to the diff-coverage base mismatch (`reviewer-restored-green.log:15`, `reviewer-slot-cycle.log:242`, `crates/dst/tests/custodian.rs:7871`). |
| C5 Causal adequacy | NEEDS-HUMAN | Decide Plan re-entry for the missing serialization edge—an intervening reserve→commit→release cycle defeats the idle fence and discards fresh progress, while the brief forbids unilateral protocol changes (`crates/custodian/src/reaper.rs:644`, `crates/core/src/multipart/commit.rs:1199`, `brief.md:222`, `brief.md:270`). |
| T1 Structure | PASS | The production seam remains trait-based, shares the deployment clock across judgment and retirement, and factors the existing drain without duplicating teardown logic (`crates/custodian/src/retirement.rs:549`, `crates/custodian/src/reaper.rs:198`, `crates/server/src/custodian.rs:618`). |
| T2 Shape | FAIL | Invalid summary keys can become trusted liveness evidence and yield `Satisfied`, violating the structural-validation boundary; the existing key parser rejects the reproduced input (`crates/custodian/src/reaper.rs:574`, `crates/core/src/multipart.rs:1480`, `reviewer-probes.log:11`). |
| T3 Runtime | FAIL | A newly bootstrapped mismatched profile permits a drain mutation, and restarting at the first ID can permanently shield later abandoned sessions in a valid over-cap ledger; both were reproduced (`crates/custodian/src/reaper.rs:345`, `crates/custodian/src/reaper.rs:381`, `reviewer-probes.log:3`, `reviewer-probes.log:18`). |
| T4 Contribution | FAIL | The frozen batch review has unresolved, independently grounded defects; path-based prior-art searches found related prerequisites but no earlier reaper implementation, and contribution-artifact auditing is deferred to publish (`gate-logs/T4-batch-review.log:10`, `reviewer-prior-art.json:1`, `gate-logs/T4-contribution.log:10`). |
| T5 Judgment | NEEDS-HUMAN [impl] | Strengthen the tests before treating them as safety evidence—pre-first-chunk DST overwrites lease bytes after time jumps and never demonstrates a production renewal followed by successful part commit; slot-page continuation is also unprotected by the current small fixtures (`crates/dst/tests/custodian.rs:7471`, `crates/dst/tests/custodian.rs:7492`, `crates/custodian/src/reaper/tests.rs:134`, `gate-logs/C5-mutants.log:15`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether the seven-day idle window, thirty-day administrative ceiling and this slice's fleet/one-shot reach meet operator expectations after the safety findings are resolved—these policies can terminate uploads and govern admission recovery (`crates/custodian/src/reaper.rs:127`, `crates/custodian/src/reaper.rs:133`, `brief.md:157`). |

The advisory conclusion is to resolve the protocol decision and the reproduced implementation defects before treating this slice as fit for automatic reclamation. The ordinary acceptance fixtures pass, but they exclude a failing concurrent upload. Separate probes also demonstrate incorrect profile authorization, starvation and corruption classification.

All source citations below refer to the supplied `$PDCA_TARGET`, whose single base commit identifies upstream base `3c600b657ccb7e54a4a60c72b220b7bf1d33ad4a`. Input and evidence citations refer to this review directory. The target compiled successfully; reverse-application checking of `patch.diff` passed. Stash/pop restored its tracked diff byte-for-byte, and no production source was edited for this review.

1. **P1 — A completed slot cycle defeats the idle fence.** A session is idle when read; before its fence commits, a request reserves a previously absent slot and successfully commits a new part. Part commit requires the session but does not rewrite it, and deletes the slot (`crates/core/src/multipart/commit.rs:1199`, `crates/core/src/multipart/commit.rs:1215`). The reaper's session equality and `require_absent(slot)` therefore still hold (`crates/custodian/src/reaper.rs:644`). The upload becomes `Aborting` despite fresh durable progress, preventing completion and authorizing teardown. Running the supplied ignored regression at seed 1 produced exactly that state (`reviewer-slot-cycle.log:242`). Its documentation is candid, but a known-gap comment without a tracked deferral does not discharge the required invariant. The decision owed is how to change the serialization protocol within an agreed scope; a successful slot-occupancy check alone cannot witness a completed cycle.

   Reproduce from the target with `RUSTFLAGS='--cfg madsim' MADSIM_TEST_SEED=1 MADSIM_TEST_NUM=1 cargo test -p wyrd-dst --test custodian reaper_slot_cycle_between_the_reads_and_the_fence -- --ignored --exact --nocapture`. The review ran it with build and temporary directories redirected inside this sandbox.

2. **P1 — An absent ledger authorizes work under an unchecked profile.** `AdmissionProfile::Absent` substitutes the local session bound and continues scanning (`crates/custodian/src/reaper.rs:343`). A first Create can install another valid profile after that read, then its client can Abort before the session listing. The reaper returns `Ran`, allowing the record-only retirement leg to act (`crates/custodian/src/reconciliation.rs:186`). A standalone probe installed precisely that fresh Create/Abort persisted state at the ledger-read boundary: the pass returned `Changed` and committed one write instead of refusing (`reviewer-probes.log:3`). This violates leg H's no-mutation contract even without any age-based fence. Make the profile check govern the work it authorizes, including bootstrap transitions, and add the concurrent regression; the existing pre-seeded mismatch test does not exercise this interval.

3. **P1 — The traversal bound can strand a valid abandoned session forever.** Every pass starts with no cursor and stops after `ledger.max_sessions()` entries, calling anything later damage (`crates/custodian/src/reaper.rs:344`, `crates/custodian/src/reaper.rs:361`, `crates/custodian/src/reaper.rs:381`). The record contract explicitly permits `count > max_sessions` after a cap reduction (`crates/core/src/multipart.rs:2003`). With a matching profile deriving cap 1, count 2, a lower-ID foreign-clocked Open upload and a higher-ID abandoned local upload, six passes all returned `Blocked`, committed nothing, and left the abandoned upload `Open` (`reviewer-probes.log:18`). The same first entry will be revisited indefinitely. Bound work while preserving progress across the remaining IDs; do not confuse a legitimate over-cap population with structural corruption. The same count-based reasoning also cannot establish corruption from a non-snapshot traversal during concurrent churn; this is part of the traversal concern, not an additional independently reproduced finding.

4. **P2 — A malformed `psum:` key is silently accepted.** `latest_psum` validates the value but never parses its key (`crates/custodian/src/reaper.rs:569`). A valid summary at `psum:<id>:not-a-part-number` supplies a recent progress stamp and makes the pass report `Satisfied`. The probe first verified that `parse_psum_key` rejects that key, then observed the production outcome (`reviewer-probes.log:11`). Validate each summary's key before using its timestamp and hold/name malformed entries through the existing error path. The existing damage test covers an undecodable summary value, not this case (`crates/custodian/src/reaper/tests.rs:465`).

5. **P2 — The liveness evidence is weaker than its stated contract.** The pre-first-chunk row directly inserts renewed slot bytes, advances by up to a day against a 60-second lease, and inserts another renewed value; it never runs the renewal loop or attempts the eventual part commit (`crates/dst/tests/custodian.rs:7057`, `crates/dst/tests/custodian.rs:7471`). The normative scenario requires both continued survival and a successful subsequent commit (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:2259`); X62 likewise checks the fence outcome but omits the subsequent successful part commit (`crates/dst/tests/custodian.rs:7383`, `docs/design/proposals/draft/0016-multipart-commit-protocol.md:2275`). Retain the focused record-level cases, but add evidence using valid renewal timing and the production commit seam. Also exercise `read_slots` across multiple short pages: the fixture has two slot indices and a three-entry page cap, explaining why deletion of its continuation arm survived (`crates/custodian/src/reaper/tests.rs:117`, `crates/custodian/src/reaper/tests.rs:134`).

**Verification supports the bounded happy paths, not the excluded safety case.** Independent runs produced six subprocess passes after restoration, thirteen reaper unit passes, one physical GC grace-window pass, and five active reaper DST passes with the slot-cycle regression ignored. Enabling that regression produced one failure. The three reviewer probes link the compiled production libraries and call the real `reconcile_step`; they do not modify the judged source. Their source is `pdca-reviewer-1015-probes.rs`, and their runnable binary is `pdca-reviewer-1015-probes`. Run `./pdca-reviewer-1015-probes --nocapture --test-threads=1` to see all three failed safety assertions. Commands and outcomes are recorded in `reviewer-checks.txt:1`.

The subprocess leg B's durable-state assertion remains vacuously true on the base: its red is the unknown flag's unsuccessful exit, not a demonstration that the old implementation mistakenly reaps a live upload (`crates/server/tests/multipart_reaper.rs:569`, `crates/server/tests/multipart_reaper.rs:578`, `reviewer-red.log:27`). The physical GC case is post-change evidence only, as the brief requires. No same-test pre-fix red is claimed for it.

The frozen gates were adjudicated as follows:

- **C4-ci:** the captured output shows actual spelling, docs rendering, formatting, clippy, builds, tests, dependency audits and DST, ending in success (`gate-logs/C4-ci.log:11`, `gate-logs/C4-ci.log:5031`). It explicitly skips the known failing slot-cycle test (`gate-logs/C4-ci.log:4908`). I independently ran spelling, docs lint, the real renderer/link audit, formatting and the targeted tests; all applicable checks passed. Both named external dependencies, `typos` and `docs-renderer`, were exercised without substitutes. Full workspace CI was not independently repeated.
- **C4-verify:** independently reproduced its six-test red→green result (`gate-logs/C4-verify.log:125`, `reviewer-red.log:115`, `reviewer-restored-green.log:15`).
- **C4-diff-cov:** no coverage measurement exists; the wrapper failed to apply the patch to its `origin/main` (`gate-logs/C4-diff-cov.log:10`). This is a base-state/coverage caveat, not evidence that the supplied patched target cannot compile. C4's FAIL above rests on the executed slot-cycle regression, not this gate.
- **C5-mutants:** the log records 144 tested, 122 caught, 18 unviable and four missed (`gate-logs/C5-mutants.log:13`). The two page-size arithmetic mutants retain continuation and do not by themselves establish a production bug. Deleting continuation exposes the coverage gap in finding 5. Subtracting the delete count is equivalent for this fence shape, which contains no deletes; the independent cost assertion counts 20 operations (`crates/custodian/src/reaper/tests.rs:677`). The complete mutation campaign was not rerun.
- **T4-batch-review:** its 15 entries include repeated reports, not 15 distinct defects (`gate-logs/T4-batch-review.log:10`). The profile, slot-cycle, traversal, summary-key and liveness-test concerns ground as described above. The additional Complete-race test request is not treated as a separately proven runtime defect: no failing Complete interleaving was established, and the brief explicitly selects this slice's DST rows (`brief.md:152`); no claim of Complete-race verification is made. The child-3 state exits remain excluded under #1017, rather than being pulled into this patch.
- **Contribution-artifact audit: N/A.** The artifacts are intentionally drafted after Check; substantive auditing must rerun at publish (`gate-logs/T4-contribution.log:10`). This creates no human-clearance item.
- **host-tikv:** the frozen log shows both actual feature-enabled clippy compilations completed successfully (`gate-logs/host-tikv.log:112`, `gate-logs/host-tikv.log:211`). This establishes compilation, not a live TiKV service exercise; no live-service claim is needed for the brief's record-level acceptance.

**Prior art was checked by affected path.** Default-branch commit-history queries for `crates/custodian/src/reaper.rs`, `crates/custodian/src/reconciliation.rs`, `crates/custodian/src/retirement.rs` and `crates/server/src/custodian.rs`, plus closed/rejected-work searches naming the reaper, its tests, CLI and dispatch paths, found related maintenance prerequisites but no earlier reaper implementation. A broader closed reaper search located the discontinued protocol work [#636](https://github.com/getwyrd/wyrd/issues/636), whose scope explicitly excludes the reaper loop in favor of #625. Query results are preserved in `reviewer-prior-art.json:1` and `reviewer-prior-art-followup.json:1`; no other local checkout was searched.

The standing rubric was applied, including structural decode boundaries, one-clock ownership, shared trait seams, documentation currency and DST fidelity. No DCO finding was inferred from commits. The profile/clock checks are required domain guards, not capability probes masking a load-time cause, so the C5 symptom-guard smell-test does not independently fire. No `INTEGRATION.md` exists in the supplied target; the explicit human-only decisions are carried in the complete table above. The default Tier-1/Tier-2 suites were not exercised; their later custodian observation complements, and cannot discharge, the reproduced Tier-0 failure.

<!-- pdca:leaf-complete -->

### Advisory — adversary

# Adversarial review — #1015 (625.1: multipart reaper, `Open` half)

**Verdict.** I could not break the red→green proof. The subprocess test runs the real binary through the production path, and it asserts on store state. But the patch still ships one confirmed false-positive path (the slot-cycle gap). It is documented, not fixed, and no issue tracks it. Two carry-forward items are only half done: the fleet-loop half of the profile-refusal fix has no test, and the bundle still does not apply on `origin/main`.

## Findings

- NEEDS-HUMAN [human] — `crates/custodian/src/reaper.rs:56-72`, `:629-648`: the idle fence can still abort an upload that has just committed a part. Concrete case: the reaper reads slot index 0 as free, `psum:` as 10 days old and every lease as lapsed. Before its fence commits, a client reserves index 0 (`crates/core/src/multipart/stage.rs:1330-1333` only *requires* `mpu:`), stages, and commits part 2 (`crates/core/src/multipart/commit.rs:1199-1215`, which deletes the slot again and also only *requires* `mpu:`). Every fence precondition still holds, so the fence lands and the new part gets orphan-marked and later GC'd. 0016:2258 rules out exactly this false positive. I checked both production batches, and the builder is right that point preconditions over today's records cannot close it. The DST (deterministic simulation) row that reproduces it (`crates/dst/tests/custodian.rs:7589`) is `#[ignore]`d at `:7874` and names no tracking issue, so under the rubric this is not a settled deferral. Decision needed: accept it with an issue that blocks #668/#508 (the first real session creator), or require the 0016 change first. The window is narrow (a whole reserve→stage→commit has to fit between one session's reads and its fence), but it exists. T4 still lists it three times as blocking (`reaper.rs:644`).
- NEEDS-HUMAN [impl] — `crates/custodian/src/reconciliation.rs:177-183`: the deployed-loop branch (reconstruction and reaper both passed) is never tested with a reaper in it. Every `reconcile_step` call that passes `Some(&reaper)` passes `None` for reconstruction (`crates/custodian/tests/gc.rs`, the DST rows, the one-shot). The deployed loop itself is driven only by `crates/server/tests/custodian_gc.rs`, and only with a matching profile. So changing line 177 to drain even on `Reaped::Refused` (carry-forward item 2's defect, in this branch) passes every test. Leg H covers only the record-only branch (`:184-197`). Add one test with `Some(&ctx)` and a mismatched profile that asserts an `Aborting` session and its `retire:` obligation are untouched, and one with a matching profile that asserts a fenced session drains in the same step.
- NEEDS-HUMAN [impl] — `gate-logs/C4-diff-cov.log:10`: "patch.diff does not apply on origin/main" again, so diff coverage is still unmeasured. The carry-forward asked for this rebase by name.
- NEEDS-HUMAN [impl] — `crates/custodian/src/reaper.rs:569-579`: `latest_psum` decodes each value but never parses its key. `read_slots` does parse slot keys (`:517`), and `parse_psum_key` already exists (`crates/core/src/multipart.rs:1480`). Concrete case: a record at `psum:<id>:junk` whose value decodes with a fresh `committed_at_millis` keeps an abandoned session alive until `W_session`, with no hold and no signal. ADR-0045 says a structural fault surfaces as an error, never as a value. Fix: parse the key and hold on failure, as `read_slots` does.
- NEEDS-HUMAN [impl] — `crates/dst/tests/custodian.rs:7457-7500`: the pre-first-chunk row writes each renewed slot straight into `meta.inner.kv` (`:7478`, `:7496`) between passes. It never commits the production renewal batch (`stage.rs:1620-1632`, which requires the session and slot bytes), and nothing interleaves with a pass. So half (a) is a record-level rerun of leg B(ii), not the seeded concurrency row the brief asks for (rubric: *Test fidelity*). Drive it through the production renewal, or fire the renewal from the `Hooked` store mid-pass, as X62 does.
- NEEDS-HUMAN [human] — `crates/custodian/src/reaper.rs:426-430`, `:315-325`; `crates/server/src/cli.rs:1109`: a foreign-clocked `Open` session only bumps `foreign`, which `Pass::outcome` ignores. Concrete case: if the ledger is full of foreign-clocked, over-age sessions, the one-shot prints "satisfied — nothing to reap or drain" and exits 0, while every admission slot stays taken. Leg E-open asserts that exit 0 (`crates/server/tests/multipart_reaper.rs:204`, via `reap_until`). The only signal is a warn log, even though the module defines a hold as "a session the pass cannot judge" (`reaper.rs:759`). 0016 gives this session's exit to an operator verb (#633), so whether it should fail the one-shot is a scope call. Either way, "nothing to reap" is false.
- NEEDS-HUMAN [human] — `crates/custodian/src/reaper.rs:178-186`, `crates/server/src/custodian.rs:474`, `:590`: the custodian's profile is always `Budget::shipped()`, with no override, while `create_session` accepts any `Budget` (`crates/core/src/multipart/create.rs:336-341`). A refusal now also skips #659's drain in the fleet loop. So on any store admitted under a non-shipped profile, the fleet loop stops draining client-aborted uploads as well, and their slots are never released. Before this patch it drained them. `crates/server/tests/custodian_gc.rs:1230-1234` had to swap its 64-session fixture profile for the shipped one for exactly this reason. The sign-off asked for the refusal itself. The open question is who plumbs the profile into `ReaperPolicy` (likely #508), and whether that must land before any non-shipped profile can be admitted.
- NEEDS-HUMAN [human] — `crates/custodian/src/reaper.rs:442`, `:460-466`: both arms use `saturating_sub`, so a session whose `created_at_millis` is ahead of the custodian's clock is silently kept until real time catches up. Concrete case: a gateway whose wall clock runs a year fast creates a session under `clock_source: "wall"`. Neither arm can fire for a year, and the slot stays taken with no hold and no signal, which breaks invariant (3), bounded residency. 0016:1972 discusses future stamps only for cross-source writers. Holding, with a signal, any stamp more than a set tolerance past `now` would keep residency bounded. Whether that belongs in this slice or in 0016 is a design call.

## Tried and could not refute

- Red→green (`gate-logs/C4-verify.log`): all 6 tests fail on the base at argument parsing ("flag `--reap-abandoned-uploads` needs a value") and pass with the fix. The test spawns the real binary (`crates/server/tests/multipart_reaper.rs:148-183`), which runs `cli.rs:1102` → `CustodianService::reap_abandoned_uploads_once` → `reconcile_step`, so it is production code, not a copy. The green side asserts on store snapshots, ranges and decoded marks, not on exit status alone. As the brief says, leg B is vacuous on the base: there it fails only on exit status. I did not re-run the gate, and the frozen log is consistent.
- Carry-forward item 3 (a read fault must not skip the drain): `reaper.rs:393-396` turns a per-session fault into a hold, and only an `mpuctl` read fault fails the step. The DST row covers it. I could not break it.
- The profile `Absent` race (T4, `reaper.rs:345`): a session created after the absent read is fresh, so neither arm fences it, and a profile with more in-flight slots makes `read_slots` hold (`:518-524`) rather than judge. No harmful case found.
- `listing-past-admission` under churn (T4, `reaper.rs:381`): this needs a second drainer running at the same time (two custodians on TiKV/FDB, which `cli.rs:1048-1069` already warns against). The worst case is a false hold for one pass.
- Fence budget: the largest fence is 20 ops (`crates/custodian/src/reaper/tests.rs:646-688`), counted independently of `fence_cost`, and it fits both budgets.
- X60 row (`crates/dst/tests/custodian.rs:7302`): if the reads ran in the opposite order (`psum:` first), the injected commit would show up in neither range and the fence's pins would still hold, so the row does catch the wrong order.

## C5 survivors (advisory)

- `crates/custodian/src/reaper.rs:539`: deleting `read_slots`' next-page arm survives because no test reads a slot range across two pages (that needs a backend scan cap under 17). If the arm were missing, the pins would turn the fence into a `Conflict`, not a wrong reap, so this is a test gap, not a safety hole. `:507:39` and `:654:60` are equivalent mutants (paging still covers the range, and the fence has no deletes).
<!-- pdca:leaf-complete -->

### Advisory — code-review

- NEEDS-HUMAN [human] — `crates/custodian/src/reaper.rs:644`: **The idle fence can destroy an upload that just committed a part.** After the progress reads, a request can reserve an index observed absent, commit its part, and delete that slot again. The fence's absent-slot and unchanged-session preconditions all pass; the drain then retires the newly committed part. The production commit has exactly this behavior (`crates/core/src/multipart/commit.rs:1199`). The regression at `crates/dst/tests/custodian.rs:7871` is ignored, including in the frozen CI run. Closing this needs a protocol/scope decision about durable progress evidence or reservation exclusion; documenting the gap does not protect uploads.

- NEEDS-HUMAN [impl] — `crates/custodian/src/reaper.rs:345`: **An absent ledger is treated as permission to act under an unchecked profile.** After this read, a concurrent first Create can install another profile (the supported bootstrap at `crates/core/src/multipart/create.rs:426`) and its session can be aborted before the retirement scan. The pass never validates that profile, yet `crates/custodian/src/reconciliation.rs:231` authorizes the drain to mark/delete its records. Keep the profile check valid for the sessions actually consumed, including this bootstrap interleaving, and test that a newly installed mismatch prevents mutations.

- NEEDS-HUMAN [impl] — `crates/custodian/src/reaper.rs:381`: **The admission cap becomes a permanent scan cutoff.** Every pass starts at the first ID and stops after `max_sessions`, although the codec explicitly permits `count > max_sessions` after a lowered cap (`crates/core/src/multipart.rs:2003`). If the first capped set consists of retained Completed or foreign-clocked sessions, later abandoned Open sessions are never visited, even past the residency ceiling. Also, independent pages can observe more than the cap through ordinary delete/create churn without any simultaneous overpopulation. Treat the limit as a resumable work budget, preserving forward progress instead of declaring all such listings damaged.

- NEEDS-HUMAN [impl] — `crates/dst/tests/custodian.rs:7474`: **The pre-first-chunk test manufactures liveness instead of exercising renewal.** It overwrites slot bytes directly, advances by up to a day despite a 60-second lease, then resurrects the expired slot. No production renewal runs and no part is subsequently committed, so the test stays green if the live request cannot renew or finish. Drive a real staged request with renewals before expiry, verify its part commits after surviving the reaper, and use a separate crashed request for the lapse/reap half.

- NEEDS-HUMAN [impl] — `crates/custodian/src/reaper.rs:574`: **Malformed summary keys are accepted as progress.** Only the value is decoded, so a valid summary under `psum:<id>:000000` or another invalid suffix can keep an abandoned session alive and report it as kept without a damage signal. Validate the key with the existing `parse_psum_key` (`crates/core/src/multipart.rs:1480`) and hold/signal the session on failure, as the slot and owned-entry readers already do.

- `crates/custodian/src/reaper.rs:453` — **Efficiency:** every owned Open session scans and decodes its entire summary range before checking whether its creation/reservation time or a live slot lease already proves it must be kept. This repeats every repair interval, including for active uploads with many parts. After the ceiling check, return early on sufficient creation/slot evidence; preserve slot-before-summary ordering whenever summaries are needed.

<!-- pdca:leaf-complete -->

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C5 Causal adequacy — Decide Plan re-entry for the missing serialization edge—an intervening reserve→commit→release cycle defeats the idle fence and discards fresh progress, while the brief forbids unilateral protocol changes (`crates/custodian/src/reaper.rs:644`, `crates/core/src/multipart/commit.rs:1199`, `brief.md:222`, `brief.md:270`).
- [ ] T5 Judgment — Strengthen the tests before treating them as safety evidence—pre-first-chunk DST overwrites lease bytes after time jumps and never demonstrates a production renewal followed by successful part commit; slot-page continuation is also unprotected by the current small fixtures (`crates/dst/tests/custodian.rs:7471`, `crates/dst/tests/custodian.rs:7492`, `crates/custodian/src/reaper/tests.rs:134`, `gate-logs/C5-mutants.log:15`).
- [ ] Validation — fitness-to-purpose — Decide whether the seven-day idle window, thirty-day administrative ceiling and this slice's fleet/one-shot reach meet operator expectations after the safety findings are resolved—these policies can terminate uploads and govern admission recovery (`crates/custodian/src/reaper.rs:127`, `crates/custodian/src/reaper.rs:133`, `brief.md:157`).
- [ ] `crates/custodian/src/reaper.rs:56-72`, `:629-648`: the idle fence can still abort an upload that has just committed a part. Concrete case: the reaper reads slot index 0 as free, `psum:` as 10 days old and every lease as lapsed. Before its fence commits, a client reserves index 0 (`crates/core/src/multipart/stage.rs:1330-1333` only *requires* `mpu:`), stages, and commits part 2 (`crates/core/src/multipart/commit.rs:1199-1215`, which deletes the slot again and also only *requires* `mpu:`). Every fence precondition still holds, so the fence lands and the new part gets orphan-marked and later GC'd. 0016:2258 rules out exactly this false positive. I checked both production batches, and the builder is right that point preconditions over today's records cannot close it. The DST (deterministic simulation) row that reproduces it (`crates/dst/tests/custodian.rs:7589`) is `#[ignore]`d at `:7874` and names no tracking issue, so under the rubric this is not a settled deferral. Decision needed: accept it with an issue that blocks #668/#508 (the first real session creator), or require the 0016 change first. The window is narrow (a whole reserve→stage→commit has to fit between one session's reads and its fence), but it exists. T4 still lists it three times as blocking (`reaper.rs:644`).
- [ ] `crates/custodian/src/reconciliation.rs:177-183`: the deployed-loop branch (reconstruction and reaper both passed) is never tested with a reaper in it. Every `reconcile_step` call that passes `Some(&reaper)` passes `None` for reconstruction (`crates/custodian/tests/gc.rs`, the DST rows, the one-shot). The deployed loop itself is driven only by `crates/server/tests/custodian_gc.rs`, and only with a matching profile. So changing line 177 to drain even on `Reaped::Refused` (carry-forward item 2's defect, in this branch) passes every test. Leg H covers only the record-only branch (`:184-197`). Add one test with `Some(&ctx)` and a mismatched profile that asserts an `Aborting` session and its `retire:` obligation are untouched, and one with a matching profile that asserts a fenced session drains in the same step.
- [ ] `gate-logs/C4-diff-cov.log:10`: "patch.diff does not apply on origin/main" again, so diff coverage is still unmeasured. The carry-forward asked for this rebase by name.
- [ ] `crates/custodian/src/reaper.rs:569-579`: `latest_psum` decodes each value but never parses its key. `read_slots` does parse slot keys (`:517`), and `parse_psum_key` already exists (`crates/core/src/multipart.rs:1480`). Concrete case: a record at `psum:<id>:junk` whose value decodes with a fresh `committed_at_millis` keeps an abandoned session alive until `W_session`, with no hold and no signal. ADR-0045 says a structural fault surfaces as an error, never as a value. Fix: parse the key and hold on failure, as `read_slots` does.
- [ ] `crates/dst/tests/custodian.rs:7457-7500`: the pre-first-chunk row writes each renewed slot straight into `meta.inner.kv` (`:7478`, `:7496`) between passes. It never commits the production renewal batch (`stage.rs:1620-1632`, which requires the session and slot bytes), and nothing interleaves with a pass. So half (a) is a record-level rerun of leg B(ii), not the seeded concurrency row the brief asks for (rubric: *Test fidelity*). Drive it through the production renewal, or fire the renewal from the `Hooked` store mid-pass, as X62 does.
- [ ] `crates/custodian/src/reaper.rs:426-430`, `:315-325`; `crates/server/src/cli.rs:1109`: a foreign-clocked `Open` session only bumps `foreign`, which `Pass::outcome` ignores. Concrete case: if the ledger is full of foreign-clocked, over-age sessions, the one-shot prints "satisfied — nothing to reap or drain" and exits 0, while every admission slot stays taken. Leg E-open asserts that exit 0 (`crates/server/tests/multipart_reaper.rs:204`, via `reap_until`). The only signal is a warn log, even though the module defines a hold as "a session the pass cannot judge" (`reaper.rs:759`). 0016 gives this session's exit to an operator verb (#633), so whether it should fail the one-shot is a scope call. Either way, "nothing to reap" is false.
- [ ] `crates/custodian/src/reaper.rs:178-186`, `crates/server/src/custodian.rs:474`, `:590`: the custodian's profile is always `Budget::shipped()`, with no override, while `create_session` accepts any `Budget` (`crates/core/src/multipart/create.rs:336-341`). A refusal now also skips #659's drain in the fleet loop. So on any store admitted under a non-shipped profile, the fleet loop stops draining client-aborted uploads as well, and their slots are never released. Before this patch it drained them. `crates/server/tests/custodian_gc.rs:1230-1234` had to swap its 64-session fixture profile for the shipped one for exactly this reason. The sign-off asked for the refusal itself. The open question is who plumbs the profile into `ReaperPolicy` (likely #508), and whether that must land before any non-shipped profile can be admitted.
- [ ] `crates/custodian/src/reaper.rs:442`, `:460-466`: both arms use `saturating_sub`, so a session whose `created_at_millis` is ahead of the custodian's clock is silently kept until real time catches up. Concrete case: a gateway whose wall clock runs a year fast creates a session under `clock_source: "wall"`. Neither arm can fire for a year, and the slot stays taken with no hold and no signal, which breaks invariant (3), bounded residency. 0016:1972 discusses future stamps only for cross-source writers. Holding, with a signal, any stamp more than a set tolerance past `now` would keep residency bounded. Whether that belongs in this slice or in 0016 is a design call.
- [ ] `crates/custodian/src/reaper.rs:644`: **The idle fence can destroy an upload that just committed a part.** After the progress reads, a request can reserve an index observed absent, commit its part, and delete that slot again. The fence's absent-slot and unchanged-session preconditions all pass; the drain then retires the newly committed part. The production commit has exactly this behavior (`crates/core/src/multipart/commit.rs:1199`). The regression at `crates/dst/tests/custodian.rs:7871` is ignored, including in the frozen CI run. Closing this needs a protocol/scope decision about durable progress evidence or reservation exclusion; documenting the gap does not protect uploads.
- [ ] `crates/custodian/src/reaper.rs:345`: **An absent ledger is treated as permission to act under an unchecked profile.** After this read, a concurrent first Create can install another profile (the supported bootstrap at `crates/core/src/multipart/create.rs:426`) and its session can be aborted before the retirement scan. The pass never validates that profile, yet `crates/custodian/src/reconciliation.rs:231` authorizes the drain to mark/delete its records. Keep the profile check valid for the sessions actually consumed, including this bootstrap interleaving, and test that a newly installed mismatch prevents mutations.
- [ ] `crates/custodian/src/reaper.rs:381`: **The admission cap becomes a permanent scan cutoff.** Every pass starts at the first ID and stops after `max_sessions`, although the codec explicitly permits `count > max_sessions` after a lowered cap (`crates/core/src/multipart.rs:2003`). If the first capped set consists of retained Completed or foreign-clocked sessions, later abandoned Open sessions are never visited, even past the residency ceiling. Also, independent pages can observe more than the cap through ordinary delete/create churn without any simultaneous overpopulation. Treat the limit as a resumable work budget, preserving forward progress instead of declaring all such listings damaged.
- [ ] `crates/dst/tests/custodian.rs:7474`: **The pre-first-chunk test manufactures liveness instead of exercising renewal.** It overwrites slot bytes directly, advances by up to a day despite a 60-second lease, then resurrects the expired slot. No production renewal runs and no part is subsequently committed, so the test stays green if the live request cannot renew or finish. Drive a real staged request with renewals before expiry, verify its part commits after surviving the reaper, and use a separate crashed request for the lapse/reap half.
- [ ] `crates/custodian/src/reaper.rs:574`: **Malformed summary keys are accepted as progress.** Only the value is decoded, so a valid summary under `psum:<id>:000000` or another invalid suffix can keep an abandoned session alive and report it as kept without a damage signal. Validate the key with the existing `parse_psum_key` (`crates/core/src/multipart.rs:1480`) and hold/signal the session on failure, as the slot and owned-entry readers already do.
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 15 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_1015/review
- [ ] **The advertised binding red→green leg has no compatible verification path.** `brief.md:56-61` calls physical fragment reclamation in the existing `crates/custodian/tests/gc.rs` “the binding red→green leg,” using the reaper through in-process `reconcile_step`. But `brief.md:137-140` selects only the NEW server test for C4, and `brief.md:220-224` explicitly says a direct call cannot compile against both trees after the signature change. The target exposes the current signature at `crates/custodian/src/reconciliation.rs:126-134`; the added reaper context does not exist there. Thus the subprocess test can establish RED for durable metadata, while the specified GC test supplies post-change evidence, not the promised same-test RED→GREEN for physical reclamation. Revise the criterion to distinguish those claims, or specify a base-compatible path and command that actually runs the physical-reclamation assertion on both trees.
- [ ] **The required batch splitter crosses an unresolved ownership boundary.** `brief.md:175-176` requires “the batch splitter unit-tested,” although `brief.md:13-17` assigns the drain and owned-`sidx:` walk exclusively to #659 and prohibits reimplementing them. This slice's reap fence must remain atomic: `docs/design/proposals/draft/0016-multipart-commit-protocol.md:647-649` explicitly says the fence's slot pins cannot be split; its batch inventory assigns the splittable work to retirement and owned-entry drain steps (`:668`, `:672`). The existing reusable fence builder returns one `WriteBatch` (`crates/core/src/multipart/abort.rs:77-92`). Identify the prerequisite-owned splitter whose tests are to be reused, and require this slice to verify its unsplit fence's byte/operation bounds; otherwise remove the splitter requirement. As written, Do must choose between inventing extra batching machinery and violating an explicit acceptance requirement.
- [ ] size backstop — this slice is behaving oversized: patch is 250 KB (threshold 100 KB); patch touches 42 files (threshold 20). Recommend answering `iterate-plan` at sign-off and authoring the split in the re-plan (`pdca split`), rather than `iterate-do`: a slice that is too big yields implementation-shaped findings every round, and splitting authors briefs, which is Plan's beat.

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
- Iteration delta (if iterating): Rebuild inside this slice (logic ~54 KB, below the 150 KB split line). Last round's profile-refusal and read-fault fixes landed; red->green passes. Accepted gap: the slot reserve->commit->release race (reaper.rs:644) stays open in this slice. Do not try to close it and do not change 0016. Keep the DST row ignored, but give its #[ignore] a reason naming the follow-up (recorded in SUMMARY §10; tracker issue to be filed), and make the docs/module comments state the gap plainly instead of claiming it is covered. Fix, each with a test that fails today: 1. `latest_psum` must parse the key (`parse_psum_key`, multipart.rs:1480) and hold + signal on failure, like `read_slots` (reaper.rs:569-579). 2. Foreign-clocked Open sessions must not make the one-shot report "nothing to reap" with exit 0 (reaper.rs:426-430, :315-325; cli.rs:1109): count them as holds or report them truthfully; adjust leg E-open. 3. A `created_at_millis` ahead of the custodian clock beyond a set tolerance must hold with a signal, not be silently kept via saturating_sub (reaper.rs:442, :460-466). 4. The session cap must be a resumable work budget, not a fixed cutoff from the first ID (reaper.rs:381); churn between pages must not be read as damage. 5. An absent ledger must not authorize the drain under an unchecked profile when a concurrent first Create installs one (reaper.rs:345, reconciliation.rs:231); test that a newly installed mismatch mutates nothing. 6. Test the deployed-loop branch with both reconstruction and reaper passed (reconciliation.rs:177-183): mismatched profile leaves an Aborting session and its retire: obligation untouched; matching profile drains a fenced session in the same step. 7. Pre-first-chunk DST row (dst/tests/custodian.rs:7457-7500): drive the production renewal (stage.rs:1620-1632) before lease expiry and show the part commits after the reaper; use a separate crashed request for the lapse/reap half. Also: rebase onto current main so diff coverage is measured (asked for last round, still missing). Note in build-notes who must plumb a non-shipped profile into ReaperPolicy (reaper.rs:178-186; likely #508).
- By / date: Eduard Ralph / 2026-11-10

## 10. Act candidates (hints for the next Act review)
- Plan advisory: 2 finding(s); brief revised: yes (plan-advisory-*.md)
- (empty is the common case)
- Follow-up to file: close the reaper's slot reserve->commit->release race (idle fence can abort an upload that just committed a part; reaper.rs:644, commit.rs:1199, ignored DST row custodian.rs:7871) — needs durable progress evidence / 0016 change; must block #668/#508. Accepted as a known gap at #1015 sign-off 2026-11-18.
