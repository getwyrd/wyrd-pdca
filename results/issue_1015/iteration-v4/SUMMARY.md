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
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass — run-verify.sh: PASS — red without the fix, green with it (7 test(s) ran red).
- C4 diff coverage: changed lines executed by the patch's tests: fail — diff coverage not measured — patch.diff does not apply on origin/main
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): pass — 172 mutants tested in 11m: 151 caught, 21 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 11 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_1015/review
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 13.56s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Review of #1015’s abandoned Open multipart-session reaper and operator one-shot: red→green is reproduced, but the final ledger-loss refusal and the specified GC regression assertion need correction.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The slice has falsifiable durable-state criteria and explicit boundaries; the later carry-forward accepts the slot-cycle gap and defers the two drain races (`brief.md:26`, `brief.md:304`, `brief.md:308`). |
| C2 Reproduction (red pre-fix) | PASS | Independently keeping the subprocess test while stashing the fix ran seven tests and failed all seven; abandonment remains durable on the base, while B’s preservation property is vacuously true despite its command-status failure (`reviewer-red.log:106`, `crates/server/tests/multipart_reaper.rs:202`, `crates/server/tests/multipart_reaper.rs:581`). |
| C3 Change | PASS | The change reaches the operator and fleet paths through the existing fenced control point and shared teardown builder; no second teardown algorithm or record-format change is introduced (`crates/server/src/custodian.rs:481`, `crates/server/src/custodian.rs:672`, `crates/custodian/src/reaper.rs:979`). |
| C4 Verification (red→green) | PASS | Restoring the patch passes all seven tests; the frozen full CI completed, and the local rerun passes the relevant unit, GC and DST cases; its missing advisory-db gitlink and the frozen coverage base mismatch are host/target caveats, not patch failures (`reviewer-green.log:16`, `gate-logs/C4-ci.log:5044`, `reviewer-ci.log:4332`, `reviewer-ci.log:4412`, `gate-logs/C4-diff-cov.log:10`). |
| C5 Causal adequacy | NEEDS-HUMAN [impl] | Refuse a ledger lost after the initial matching read—otherwise the final check authorizes retirement with an unknown profile; the independent probe returned Changed and installed an orphan mark after that loss (F1; `crates/custodian/src/reaper.rs:438`, `crates/custodian/src/reaper.rs:443`, `pdca-reviewer-1015-probes/result.log:7`). |
| T1 Structure | PASS | The reaper uses narrow metadata/clock seams, reuses the Abort fence/classifier, and shares the existing retirement machinery; deployed contexts use the same store and clock (`crates/custodian/src/reaper.rs:267`, `crates/core/src/multipart/abort.rs:380`, `crates/server/src/custodian.rs:611`, `crates/server/src/custodian.rs:634`). |
| T2 Shape | FAIL | Operators cannot discover the new recovery command in the executable’s usage output; add its spelling, one-pass behavior and lack of an endpoints requirement there (F3; `crates/server/src/cli.rs:484`, `crates/server/src/cli.rs:1005`, `reviewer-help.log:6`). |
| T3 Runtime | PASS | Session work and fence sizes are bounded, cursor damage is contained, and metadata operations inherit the backend termination contract; the new seeded production renewal/commit interleavings pass (`crates/custodian/src/reaper.rs:545`, `crates/custodian/src/reaper.rs:1002`, `crates/traits/src/lib.rs:1333`, `reviewer-ci.log:4306`, `reviewer-ci.log:4311`). |
| T4 Contribution | NEEDS-HUMAN | Confirm the disposition of prior closed/rejected work before publication—searches ran by all 46 affected paths, but closed-PR prose search cannot exhaust changed-file history and the local target contains only a synthetic base; contribution-artifact auditing itself is deferred to publish (details below; `reviewer-prior-art.jsonl:1`, `gate-logs/T4-contribution.log:10`). |
| T5 Judgment | NEEDS-HUMAN [impl] | Preserve the required single-pass GC assertion—the submitted test waits three passes before checking reclamation, so it can miss a regression that delays deletion; the stricter one-pass copy passes against the unchanged implementation (F2; `brief.md:60`, `crates/custodian/tests/gc.rs:1444`, `pdca-reviewer-1015-probes/gc-single-pass.log:3`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Accept this Open-only slice’s operational fitness after the findings are resolved—durable-state and seeded concurrency evidence support its behavior, but do not decide release suitability across the startup and lifecycle cases explicitly reserved for sibling slices (`brief.md:163`, `crates/custodian/src/reaper.rs:143`). |

All source citations refer to the supplied `$PDCA_TARGET`; brief and evidence citations refer to this report’s directory. The target is the disposable synthetic base `9abd75e` (labelled pre-fix base `3c600b657ccb7e54a4a60c72b220b7bf1d33ad4a`) with this patch applied. Stash restoration succeeded, and `git apply --reverse --check ../patch.diff` and `git diff --check` pass. No implementation was edited.

**F1 — Refuse final-read ledger loss before running retirement (P2).** When the first profile read matches, `probed` is `None`. If the final read finds `AdmissionProfile::Absent`, `refusal()` accepts it and the early return at `reaper.rs:443` bypasses the missing-ledger handling below. `reconciliation.rs:200` consequently runs the record-only drain. This is the missing-ledger fault case already in scope, observed later in the pass; it is distinct from the explicitly deferred first-Create-after-check race.

The standalone probe links the compiled, unmodified custodian and drives public `reconcile_step`: seed a matching ledger and an Aborting session with a committed part and a bytes-retirement obligation; remove only `mpuctl` immediately before its second read. Observed: `Changed`, a new `orphan:1:99001:0` mark, and a changed retirement obligation. Expected: `Blocked` with no retirement writes beyond the injected ledger deletion. Handle the present-to-absent transition before the early return and add this step-level regression. The existing lost-ledger test starts with the ledger already absent and cannot detect this transition (`crates/custodian/src/reaper/tests.rs:1056`). Reproduction: `python3 pdca-reviewer-1015-probes/run.py`; output is in `pdca-reviewer-1015-probes/result.log:7`.

**F2 — Assert physical reclamation after one GC pass (P2, test fidelity).** The acceptance criterion requires one pass at `mark + grace - 1`, then one at `mark + grace` (`brief.md:60`). The submitted test instead performs three passes at each instant and checks only afterwards (`crates/custodian/tests/gc.rs:1425`, `crates/custodian/tests/gc.rs:1444`). This permits the first two eligible passes to reclaim nothing without failing the test. In a scratch copy of that test, changing only those two loop counts from three to one compiles and passes against the patched libraries. Retain that stronger assertion in the submitted test. This is post-change physical-reclamation evidence, not a same-test red→green claim.

**F3 — Include the one-shot in built-in usage (P3).** Running `target/target/debug/wyrd --help` prints the custodian invocation without `--reap-abandoned-uploads` and explains only the existing restore/GC flags (`reviewer-help.log:6`; `crates/server/src/cli.rs:484`). The architecture documentation does describe the new operation, but the executable’s own operator-facing usage should expose it too.

The independent checks support the main behavior, with these precise limits:

- **Red→green:** `cargo test --offline --locked -p wyrd-server --test multipart_reaper`, with production changes stashed and the added test retained, exits 101 with seven assertion failures; after `git stash pop`, it exits 0 with seven passes. Logs: `reviewer-red.log`, `reviewer-green.log`. B’s baseline failure is the unknown command’s exit status, not evidence that the pre-fix system wrongly reaps active uploads.
- **Full CI rerun:** spelling, docs lint/render/link audit, formatting, clippy, build, conformance and the exercised runtime/DST tests pass. The custodian DST suite reports 41 passed and the accepted slot-cycle test ignored (`reviewer-ci.log:4332`). The final command exits 1 because both the advisory-db test and advisory audit require `.github/advisory-db` to be a gitlink, absent from this synthetic copy (`reviewer-ci.log:3758`, `reviewer-ci.log:4412`). The frozen host log actually fetches the pinned RustSec database, runs deny, and completes all checks (`gate-logs/C4-ci.log:4388`, `gate-logs/C4-ci.log:5044`). No patch verification failure is inferred from the missing gitlink. Both named external dependencies, `typos` and the docs renderer, ran in this sandbox (`reviewer-ci.log:2`, `reviewer-ci.log:7`).
- **No-fleet refusal:** the additional probe gives an Aborting session a part with untrusted placement and no fleet; public `reconcile_step` returns `Blocked` and leaves the store byte-identical (`pdca-reviewer-1015-probes/result.log:8`). The batch review’s test-gap entry is not promoted to a behavior defect.
- **Source review:** clocks remain explicit and shared; malformed progress records are held, not silently accepted; the idle fence retains its slot preconditions. The C5 symptom-guard smell test does not fire: these are required data/clock checks, not capability probes masking eager initialization. Tier-1/Tier-2 observation can complement the seeded custodian evidence at deployment, but no such campaign was run here.

The frozen gates are adjudicated separately from the implementation findings:

| Gate | Assessment | Evidence |
|------|------------|----------|
| C4-ci | PASS on the frozen host; local host caveat above | The full captured log ends with all checks passed (`gate-logs/C4-ci.log:5044`). |
| C4-verify | PASS, independently reproduced | Seven red and seven green; `reviewer-red.log:106`, `reviewer-green.log:16`. |
| C4-diff-cov | Unmeasured; target/base caveat | The wrapper could not apply the bundle to `origin/main`; it never measured coverage (`gate-logs/C4-diff-cov.log:10`). The supplied patched target builds and its tests run. |
| C5-mutants | PASS on recorded evidence; not independently rerun | 172 tested: 151 caught, 21 unviable. The log does not enumerate the brief’s individual numbered negations, so none is claimed separately verified (`gate-logs/C5-mutants.log:13`). |
| T4-batch-review | Recorded FAIL; findings independently triaged below | Eleven entries, including duplicates and accepted deferrals (`gate-logs/T4-batch-review.log:10`). |
| T4-contribution | N/A | The artifacts are absent by design at Check; the substantive contribution audit runs at publish (`gate-logs/T4-contribution.log:10`). |
| host-tikv | PASS on recorded compile evidence; not independently rerun | The captured command checks the metadata crate and server selection arms and finishes successfully (`gate-logs/host-tikv.log:2`, `gate-logs/host-tikv.log:211`). |

The batch review’s three ledger-loss entries reduce to F1, and its usage entry to F3. Its post-check first-Create/drain race and ignored slot-cycle entries are settled by `brief.md:308` and `brief.md:309`; they are not reopened. Its two atomic-profile-pin entries do not demonstrate a permitted live-session profile replacement: the design requires the population to drain before a profile swap (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:2077`). Its mixed-store-context entry has no affected production caller: the deployed caller supplies the same `meta` to both contexts (`crates/server/src/custodian.rs:611`, `crates/server/src/custodian.rs:634`).

The pagination observation is reproducible but qualified: with a custom profile admitting 100 sessions, 64 fresh sessions return `Partial` on six consecutive passes (`pdca-reviewer-1015-probes/result.log:1`). A full last page advertises continuation; the empty resumed pass then cannot certify a head-to-tail scan (`crates/custodian/src/reaper.rs:607`). This matches the documented per-pass certification rule, and `SESSIONS_PER_PASS` deliberately exceeds the shipped profile’s maximum (`crates/custodian/src/reaper.rs:194`). It is not presented as a shipped admission-capacity defect. When non-default profiles are wired into the operator command, its instruction to repeat until `Satisfied` must account for this limitation (`crates/server/src/cli.rs:1111`).

The prior-art check ran by every affected file path, covering recent merged/default-branch history and closed-work searches; the query method and 46 results are in `reviewer-prior-art.jsonl`. The new reaper path has no returned merged or closed predecessor. Existing shared-fence work appears in [the Abort implementation](https://github.com/getwyrd/wyrd/commit/4595b4a8d5b29065ff2ea2c2beb2a5edf8f32085). A separate closed-unmerged path search identified [PR #647](https://github.com/getwyrd/wyrd/pull/647), concerning segmented maps and reconciliation, not this Open reaper. These searches cannot prove that a closed PR whose prose omits its changed paths contains no relevant rejected design; that remaining historical judgment is the T4 decision owed. No additional `INTEGRATION.md` was present in the supplied bundle or target; the supplied standing rubric was applied.
<!-- pdca:leaf-complete -->

### Advisory — adversary

# Adversarial review — #1015 (625.1: reaper, `Open` half)

I could not break the reaper's own logic: its arms, its read order, its slot pins, the profile refusal or the clock guard. I did find one new safety branch that no test covers, and I proved it with a mutant that survives every relevant suite. There is also one CLI nit and one gap in the evidence.

## Findings

- NEEDS-HUMAN [impl] — **No test covers the record-only drain's no-fleet hold, and a mutant that removes it survives.** `crates/custodian/src/retirement.rs:429-435` (the new `None` arm of `ByChunk::whole`) is the only thing that stops the one-shot's record-only drain from retiring an *untrusted* record (a placement whose length is not the scheme's fragment count, or an `sidx:` value that will not decode) against an empty fleet. In a scratch copy I replaced that arm with `return Ok(())`. With the mutant in place, all 422 `wyrd-custodian` tests still pass, and so do the `wyrd-server` suites `multipart_reaper` (7), `custodian_gc` (11) and `custodian_day_one` (15). What the mutant does: `listed()` walks the empty fleet (`retirement.rs:461`), every wanted chunk gets zero fragments, `by_chunk_part` counts each chunk covered, and it deletes the `part:` record (`crates/custodian/src/retirement/marks.rs:591-616`). That leaves fragments with no `orphan:` mark and no record, which GC keeps for good. This is the permanent-leak class C-1 rules out. A grep for `partial-fleet`/`PartialFleet` across `reaper/tests.rs`, `multipart_reaper.rs`, `gc.rs` and the DST `reaper_rows` finds nothing. So the claim at `docs/design/architecture/06-runtime-view.md:94` ("holding for a whole-fleet pass anything that could only be retired by listing the fleet") has no evidence behind it. **Fix:** add a record-only step test (or a one-shot leg). Seed an `Aborting` session with its `{session, all}` obligation, a `part:` whose placement length is wrong, and an `sidx:` that will not decode. Assert those records are byte-identical afterwards, the step answers `Blocked` with `partial-fleet` named, and the one-shot exits non-zero.

- NEEDS-HUMAN [impl] — **The usage text leaves out the new flag.** The custodian usage line at `crates/server/src/cli.rs:484` lists and explains `--reconcile-after-restore` and `--gc-expired-pending` (`:485-492`), but not `--reap-abandoned-uploads`. The brief asks for one spelling across the test, the runbook and the docs, and an operator reading `wyrd`'s help cannot find the one-shot. Add the flag, with one line saying it needs no `--endpoints` and exits non-zero when a session is held or the profile check refuses.

- NEEDS-HUMAN [human] — **Diff coverage has not been measured in any of the four rounds.** `gate-logs/C4-diff-cov.log` says "patch.diff does not apply on origin/main", although each carry-forward (iterations 1–3) asked for a rebase. The mutant above is a concrete case of a new safety line that no test runs, which is exactly what diff coverage would have shown. A human has to decide which of two things is true: the bundle can be rebased (are #1027 and #1007–#1013 on `main` yet?), or the gate must measure against the stacked base. Until one happens, nobody can claim that every changed line was exercised.

## Attempted and could not refute

- **The red→green evidence.** I re-ran `cargo test -p wyrd-server --test multipart_reaper` on the unmodified post-fix tree in scratch: 7/7 pass. The red leg (`gate-logs/C4-verify.log`) fails only because the base parser does not know the flag ("needs a value"), which is the falsifiability the brief designed. The actual checking is on the green side, and it is strong. Leg B and leg H compare whole-store snapshots, leg A names the committed part's positions explicitly, and `assert_licensed` checks the mark, the drained obligation, and that no `part:`/`sidx:` record still names the position. The tests drive the real binary, not a parallel entry point.
- **Leg A is idle-arm-only.** The session was created 14 days ago, against a 30-day `W_session`. A ceiling miscomputed at `W_open` would also fence all four of leg B's 14-day sessions, so B guards against it.
- **X60 catches negation (2).** The trigger at `crates/dst/tests/custodian.rs:7335-7343` fires before the *second* read of either range, whichever comes first. A reader that reads `psum:` first would then see neither the new part nor the slot, and would fence. The row asserts `Open`.
- **An `sidx:`-only renewal that slips past the slot pins.** This can't happen: `renewal_batch` (`crates/core/src/multipart/stage.rs:1620-1631`) requires and rewrites the slot in the same batch as the owned entries, so every renewal moves a pinned key.
- **The largest fence.** `largest_fence_fits_both_batch_budgets` (`crates/custodian/src/reaper/tests.rs:935`) uses a 1 KiB object name rather than a session at the value ceiling. Even so, three 100 KB values plus 16 pins is about 0.3 MB against 5 MB, and 20 ops against 500 (`crates/core/src/multipart.rs:5914` asserts the three values fit). The claim holds.
- **T4's `reaper.rs:443` (the ledger vanishes between the two checks) and `reaper.rs:1016` (the fence does not pin `mpuctl`).** No production writer deletes `mpuctl` or changes its stored profile. Create either compare-and-sets on the prior bytes or writes under `require_absent` (`crates/core/src/multipart/create.rs:684-695`), and the terminal delete puts the released count (`crates/custodian/src/retirement/sessions.rs:975-985`). Any session created after the first read is fresh, so neither arm can fence it. Only outside damage reaches these paths, so I would not block on them.
- **T4's `reconciliation.rs:187`** (the profile is checked on `reaper.meta` but the drain runs on `ctx.meta`). The deployed loop builds both from the same `meta` (`crates/server/src/custodian.rs:611-612`, `:634-635`). That makes it a hazard in the public API for future #633/625.2 callers, not a live bug.
- **The slot reserve→commit→release gap** (`reaper.rs:95-114`) is the accepted, tracked gap, so I have not raised it again.

Toolchain note: the DST rows build only under `--cfg madsim`, and I did not rebuild under madsim. My plain build ran 0 of them. `gate-logs/C4-ci.log:4920-4951` shows them green, and none of them seeds an untrusted placement.

<!-- pdca:leaf-complete -->

### Advisory — code-review

- NEEDS-HUMAN [impl] — **P2: Refuse a ledger lost during the pass.** `crates/custodian/src/reaper.rs:443`: when the initial profile matches, `probed` is `None`. If the final ledger read returns `AdmissionProfile::Absent`, `refusal()` accepts it and this early return authorizes retirement. An already-`Aborting` session can consequently have its residue marked/deleted despite the now-unknown admission profile; the terminal counter check happens after that work. Treat a transition from a matching ledger to absence as a store-wide refusal. Add an interleaving regression that removes `mpuctl` before the second read and asserts no subsequent drain mutations; the existing lost-ledger regression only covers absence at the first read.

- NEEDS-HUMAN [impl] — **P2: The advertised one-shot stopping condition can remain unreachable.** `crates/custodian/src/reaper.rs:607`, `crates/server/src/cli.rs:1111`: with 64 unchanged healthy sessions, the full final page carries a continuation, so the first pass exhausts its budget and reports `Partial`. The next pass reads an empty terminal page but also reports `Partial` because it did not start at the head. These passes repeat indefinitely; larger persistent populations likewise never satisfy `from_head && reached_end`. Such populations are supported by the configurable profile and the explicitly supported above-cap case. The new instruction to rerun until `Satisfied` therefore sends operators into an endless cycle even when nothing is eligible. Handle the exact-budget terminal boundary and make completion reporting/guidance meaningful for multiple-window listings, with regressions using 64 and 65 retained sessions.

- NEEDS-HUMAN [impl] — **P3: Add the new operator flag to built-in usage.** `crates/server/src/cli.rs:1005` introduces `--reap-abandoned-uploads`, but the custodian usage at `crates/server/src/cli.rs:484` still lists only the older flags and explains only the restore one-shot. Add this spelling and its metadata-only, no-endpoints behavior so operators can discover the new command from the binary.

No additional material reuse, simplification, or efficiency findings. Reviewed the target source against the supplied diff and frozen gate evidence; no builds were rerun. The accepted slot-cycle gap and deferred #659 races were not reopened.
<!-- pdca:leaf-complete -->

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C5 Causal adequacy — Refuse a ledger lost after the initial matching read—otherwise the final check authorizes retirement with an unknown profile; the independent probe returned Changed and installed an orphan mark after that loss (F1; `crates/custodian/src/reaper.rs:438`, `crates/custodian/src/reaper.rs:443`, `pdca-reviewer-1015-probes/result.log:7`).
- [ ] T4 Contribution — Confirm the disposition of prior closed/rejected work before publication—searches ran by all 46 affected paths, but closed-PR prose search cannot exhaust changed-file history and the local target contains only a synthetic base; contribution-artifact auditing itself is deferred to publish (details below; `reviewer-prior-art.jsonl:1`, `gate-logs/T4-contribution.log:10`).
- [ ] T5 Judgment — Preserve the required single-pass GC assertion—the submitted test waits three passes before checking reclamation, so it can miss a regression that delays deletion; the stricter one-pass copy passes against the unchanged implementation (F2; `brief.md:60`, `crates/custodian/tests/gc.rs:1444`, `pdca-reviewer-1015-probes/gc-single-pass.log:3`).
- [ ] Validation — fitness-to-purpose — Accept this Open-only slice’s operational fitness after the findings are resolved—durable-state and seeded concurrency evidence support its behavior, but do not decide release suitability across the startup and lifecycle cases explicitly reserved for sibling slices (`brief.md:163`, `crates/custodian/src/reaper.rs:143`).
- [ ] **No test covers the record-only drain's no-fleet hold, and a mutant that removes it survives.** `crates/custodian/src/retirement.rs:429-435` (the new `None` arm of `ByChunk::whole`) is the only thing that stops the one-shot's record-only drain from retiring an *untrusted* record (a placement whose length is not the scheme's fragment count, or an `sidx:` value that will not decode) against an empty fleet. In a scratch copy I replaced that arm with `return Ok(())`. With the mutant in place, all 422 `wyrd-custodian` tests still pass, and so do the `wyrd-server` suites `multipart_reaper` (7), `custodian_gc` (11) and `custodian_day_one` (15). What the mutant does: `listed()` walks the empty fleet (`retirement.rs:461`), every wanted chunk gets zero fragments, `by_chunk_part` counts each chunk covered, and it deletes the `part:` record (`crates/custodian/src/retirement/marks.rs:591-616`). That leaves fragments with no `orphan:` mark and no record, which GC keeps for good. This is the permanent-leak class C-1 rules out. A grep for `partial-fleet`/`PartialFleet` across `reaper/tests.rs`, `multipart_reaper.rs`, `gc.rs` and the DST `reaper_rows` finds nothing. So the claim at `docs/design/architecture/06-runtime-view.md:94` ("holding for a whole-fleet pass anything that could only be retired by listing the fleet") has no evidence behind it. **Fix:** add a record-only step test (or a one-shot leg). Seed an `Aborting` session with its `{session, all}` obligation, a `part:` whose placement length is wrong, and an `sidx:` that will not decode. Assert those records are byte-identical afterwards, the step answers `Blocked` with `partial-fleet` named, and the one-shot exits non-zero.
- [ ] **The usage text leaves out the new flag.** The custodian usage line at `crates/server/src/cli.rs:484` lists and explains `--reconcile-after-restore` and `--gc-expired-pending` (`:485-492`), but not `--reap-abandoned-uploads`. The brief asks for one spelling across the test, the runbook and the docs, and an operator reading `wyrd`'s help cannot find the one-shot. Add the flag, with one line saying it needs no `--endpoints` and exits non-zero when a session is held or the profile check refuses.
- [ ] **Diff coverage has not been measured in any of the four rounds.** `gate-logs/C4-diff-cov.log` says "patch.diff does not apply on origin/main", although each carry-forward (iterations 1–3) asked for a rebase. The mutant above is a concrete case of a new safety line that no test runs, which is exactly what diff coverage would have shown. A human has to decide which of two things is true: the bundle can be rebased (are #1027 and #1007–#1013 on `main` yet?), or the gate must measure against the stacked base. Until one happens, nobody can claim that every changed line was exercised.
- [ ] **P2: Refuse a ledger lost during the pass.** `crates/custodian/src/reaper.rs:443`: when the initial profile matches, `probed` is `None`. If the final ledger read returns `AdmissionProfile::Absent`, `refusal()` accepts it and this early return authorizes retirement. An already-`Aborting` session can consequently have its residue marked/deleted despite the now-unknown admission profile; the terminal counter check happens after that work. Treat a transition from a matching ledger to absence as a store-wide refusal. Add an interleaving regression that removes `mpuctl` before the second read and asserts no subsequent drain mutations; the existing lost-ledger regression only covers absence at the first read.
- [ ] **P2: The advertised one-shot stopping condition can remain unreachable.** `crates/custodian/src/reaper.rs:607`, `crates/server/src/cli.rs:1111`: with 64 unchanged healthy sessions, the full final page carries a continuation, so the first pass exhausts its budget and reports `Partial`. The next pass reads an empty terminal page but also reports `Partial` because it did not start at the head. These passes repeat indefinitely; larger persistent populations likewise never satisfy `from_head && reached_end`. Such populations are supported by the configurable profile and the explicitly supported above-cap case. The new instruction to rerun until `Satisfied` therefore sends operators into an endless cycle even when nothing is eligible. Handle the exact-budget terminal boundary and make completion reporting/guidance meaningful for multiple-window listings, with regressions using 64 and 65 retained sessions.
- [ ] **P3: Add the new operator flag to built-in usage.** `crates/server/src/cli.rs:1005` introduces `--reap-abandoned-uploads`, but the custodian usage at `crates/server/src/cli.rs:484` still lists only the older flags and explains only the restore one-shot. Add this spelling and its metadata-only, no-endpoints behavior so operators can discover the new command from the binary.
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 11 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_1015/review
- [ ] **The advertised binding red→green leg has no compatible verification path.** `brief.md:56-61` calls physical fragment reclamation in the existing `crates/custodian/tests/gc.rs` “the binding red→green leg,” using the reaper through in-process `reconcile_step`. But `brief.md:137-140` selects only the NEW server test for C4, and `brief.md:220-224` explicitly says a direct call cannot compile against both trees after the signature change. The target exposes the current signature at `crates/custodian/src/reconciliation.rs:126-134`; the added reaper context does not exist there. Thus the subprocess test can establish RED for durable metadata, while the specified GC test supplies post-change evidence, not the promised same-test RED→GREEN for physical reclamation. Revise the criterion to distinguish those claims, or specify a base-compatible path and command that actually runs the physical-reclamation assertion on both trees.
- [ ] **The required batch splitter crosses an unresolved ownership boundary.** `brief.md:175-176` requires “the batch splitter unit-tested,” although `brief.md:13-17` assigns the drain and owned-`sidx:` walk exclusively to #659 and prohibits reimplementing them. This slice's reap fence must remain atomic: `docs/design/proposals/draft/0016-multipart-commit-protocol.md:647-649` explicitly says the fence's slot pins cannot be split; its batch inventory assigns the splittable work to retirement and owned-entry drain steps (`:668`, `:672`). The existing reusable fence builder returns one `WriteBatch` (`crates/core/src/multipart/abort.rs:77-92`). Identify the prerequisite-owned splitter whose tests are to be reused, and require this slice to verify its unsplit fence's byte/operation bounds; otherwise remove the splitter requirement. As written, Do must choose between inventing extra batching machinery and violating an explicit acceptance requirement.
- [ ] size backstop — this slice is behaving oversized: patch is 370 KB (threshold 100 KB); patch touches 46 files (threshold 20). Recommend answering `iterate-plan` at sign-off and authoring the split in the re-plan (`pdca split`), rather than `iterate-do`: a slice that is too big yields implementation-shaped findings every round, and splitting authors briefs, which is Plan's beat.

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
- Iteration delta (if iterating): Rebuild inside this slice (attempt 5, the last auto round). Product code is ~94 KB (reaper.rs 55 KB); the size backstop trips on tests/docs, as in rounds 1-3. Findings are narrow; fix these, each with a test that fails today: 1. F1 — ledger lost mid-pass: when the first `mpuctl` read matches and the final read finds it Absent, `refusal()` accepts it and the early return at reaper.rs:443 lets the record-only drain run (reconciliation.rs:200). Treat matching->absent as a store-wide Refused. Add a step-level `reconcile_step` regression that removes `mpuctl` just before the second read and asserts no drain mutation (reviewer probe: pdca-reviewer-1015-probes/run.py). The existing lost-ledger test (reaper/tests.rs:1056) only covers absence at the first read. 2. F2 — in crates/custodian/tests/gc.rs:1425/:1444, run ONE GC pass at mark+grace-1 and ONE at mark+grace (brief.md:60), not three each; assert reclamation after the single pass. 3. No-fleet hold test: the `None` arm of `ByChunk::whole` (retirement.rs:429-435) has no test; a mutant removing it survives all suites. Add a record-only step test (or one-shot leg): Aborting session with its {session, all} obligation, a `part:` with wrong-length placement, an undecodable `sidx:` -> records byte-identical, step Blocked naming partial-fleet, one-shot exits non-zero. 4. Usage text: add `--reap-abandoned-uploads` to the custodian usage at cli.rs:484, with one line: metadata-only, needs no --endpoints, exits non-zero when a session is held or the profile check refuses. 5. Exact-budget boundary (reaper.rs:607, cli.rs:1111): a full final page must not leave the one-shot reporting Partial for ever; handle the terminal boundary so "rerun until Satisfied" is reachable, with regressions at 64 and 65 retained sessions. Do NOT rebase again: the patch stacks on unmerged #1027 and #1007-#1013, so it cannot apply on origin/main; diff coverage stays unmeasured until they land — accepted. Keep the slot-cycle gap accepted/ignored and the #659 follow-ups out of scope, as before. The batch-splitter and binding-red->green plan findings were already resolved in the brief (brief.md:264); do not reopen them.
- By / date: Eduard Ralph / 2026-11-10

## 10. Act candidates (hints for the next Act review)
- Plan advisory: 2 finding(s); brief revised: yes (plan-advisory-*.md)
- (empty is the common case)
