# Result — issue 843 / restore-fence-dst

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect: the restore session fence (children 3 and 4) is a new path. It races concurrent
  writers on the session record and installs obligations that lead to deletes, yet it has no
  seeded Tier-0 DST coverage, only scripted in-process interleavings. The repo's rule is "a new
  destructive or concurrent path lands with seeded Tier-0 DST coverage" (`AGENTS.md:188-190`).
  The existing restore properties (`crates/dst/tests/custodian.rs:1954-2200`) run the pass
  against ordinary inode publication, never a multipart session. The staged-handoff driver
  (`:2617-2625`, `:2850`) has only GC and reconstruction arms. #809 iteration 1's review raised
  this three times as blocking.
- Success criterion: `crates/dst/tests/custodian.rs` gains seeded properties that run the
  production `reconcile_after_restore` fence, registered with `dst_campaign_test!` and swept over
  the run seed (50 seeds under `cargo xtask dst`):
  **(D1) The fence and a concurrent session writer never both win, and neither wins by half.**
  Two arms, each with a writer landing at a seed-drawn instant during the pass. `E` is the
  contested preimage epoch; every transition out of it lands at `E+1` (the state machine,
  `0016:538-552`).
  - *Open arm.* A resurrected `Open@E` session. The writer is the Complete fence
    `Open@E → Completing@E+1` (`0016:704-708`, `multipart.rs:2173`): one commit preconditioned
    on the session's exact `Open@E` bytes, stamping `fenced_at_millis`, `segments_written` and
    `publish_target` and bumping `attempts` (0016's Complete-fence row, `0016:660`), installing
    no obligation. The nonce is already on the record (child-2
    puts `segment_nonce` on every session record) and the writer leaves it unchanged.
  - *Completing arm.* A resurrected `Completing@E` session with `seg:` records. The writer is
    the root flip `Completing@E → Completed@E+1`, faithful to 0016's row (`0016:662`): one
    commit preconditioned on the session's exact `Completing@E` bytes that writes the session,
    the published inode and its dirent, and the flip's own `retire:records:s:<id>:<E>`
    `{parts: <the published set>}` (`multipart.rs:3148`). The fixture names every part, so no
    `retire:bytes:{parts}`.
  On every seed, judged from what the pass's own read of that session returned (recorded by the
  store wrapper, see Production reach), exactly one transition out of `@E` landed:
  - *the fence won:* the session is `Aborting@E+1`; every obligation children 3 and 4 install
    under token `s:<id>:<E>` is present and decodes through `decode_retire_obligation` with the
    **fence's** payload (`{session, all}`, plus `{seg: (nonce, E)}` in the Completing arm); and
    nothing was published;
  - *the writer won after the pass read `@E`* (its fence commit was refused): the writer's state
    stands, every `retire:` value under token `E` is the writer's own (none in the Open arm, the
    flip's `{parts}` in the Completing arm) and never a fence payload, and the pass named the
    session;
  - *the writer won before the pass read the session:* the pass treats the writer's state as it
    treats any session: in the Open arm it fences `Completing@E+1` as child-4 specifies
    (`Aborting@E+2`, obligations under token `E+1`); in the Completing arm it leaves
    `Completed@E+1` untouched and does not name it.
  Never an obligation without its transition, a transition without its obligations, or a fence
  payload under a key the writer's transition owns. Every obligation is checked by payload, not
  by key alone.
  **(D2) An ambiguous fence commit is settled by the next pass.** The fence's commit is answered
  as an unknown outcome and is applied whole or not at all, by a seed-drawn fate. The pass returns
  `Err`, and the `DANGLING` line for a dangling chunk seeded in the same store was emitted before
  it. A second pass leaves the session fenced with exactly one set of obligations, whichever fate
  the store took.
  **(D3) The contested window is reached, and shown to be reached.** A reachability leg walks the
  writer's landing span in one run, as
  `prop_restore_two_readings_cover_the_divergence_window` does (`:2165-2195`), and asserts on
  the **recorded** order, not on outcome counts: for **each** arm, at least one landing point
  where the pass's read of the session returned the `@E` preimage **and** its fence commit was
  then answered `Conflict` because the writer had landed in between (a stale-preimage conflict),
  plus at least one where the fence won. Two winners alone do not prove the window was
  contested: serial runs produce both. A property that never reaches its interleaving proves
  nothing.
  **(D4) Demonstrated falsifiability, recorded in `build-notes.md`.** Temporarily break the fence
  on the builder's machine, **twice**, one mutation at a time: (a) drop its precondition on the
  session bytes, which at least one seed of D1 must then fail; (b) split its transition and
  obligations into two commits, which at least one seed of D1 or D2 must then fail. Paste each
  seed and failure into `build-notes.md`, then restore the fence. None of the breakage ships.
  Mutation (a) is the one D1 and D3 exist for, so it is not optional.
  **(L) `cargo xtask ci` green**, with every existing property in the file unchanged in what it
  asserts. A span constant may move if the longer pass needs it; say so if one does.
- Repo + branch target: getwyrd/wyrd @ main
- Scope: new seeded properties and their harness in `crates/dst/tests/custodian.rs`,
  mirroring the restore nemesis harness and the staged-handoff driver. Nothing outside that file.
  / out of scope: any production change. If a property finds a real defect in the fence, stop,
  record the seed and the failure in `build-notes.md` and report it, rather than fixing
  production here. Also out: any other test file; docs; any edit to 0016 or an ADR.

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: likely-fix
- Confidence: medium
- Recommendation: (set by Do)

## 3. Correctness (Check — chain)
- C1 Spec: none — brief.md
- C2 Reproduction (red pre-fix): none — (no gate configured)
- C3 Change: none — patch.diff
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): pass — xtask ci: all checks passed
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass —                as its own file to earn the full red->green.
- C4 diff coverage: changed lines executed by the patch's tests: pass — diff coverage n/a — no production Rust lines changed
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): pass — INFO No mutants to filter

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): pass — review-branch: 0 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_843/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 15.00s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Review #843: add seeded Tier-0 coverage for restore-session fence races, atomic retirement obligations, and recovery from ambiguous commits.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The two writer races, unknown-outcome recovery, reachability, and two required mutations have concrete acceptance criteria within one existing test file (`brief.md:18`, `brief.md:68`, `brief.md:105`). |
| C2 Reproduction (red pre-fix) | N/A | This is new coverage over existing production behavior; stash/pop confirmed the three properties are absent before the patch, while falsifiability is demonstrated by mutations (`brief.md:76`, `reviewer-integrity.log:7`). |
| C3 Change | PASS | Scope is preserved: only the designated DST test file changes, and all prior property bodies and span constants remain identical (`crates/dst/tests/custodian.rs:4996`, `reviewer-integrity.log:1`). |
| C4 Verification (red→green) | PASS | Independent mutation reds return to a green 50-seed full DST suite after restoration; frozen CI also passes, and both named external tools ran successfully (`reviewer-restored-green.log:1`, `reviewer-restored-green.log:149`, `reviewer-scanners.log:1`, `gate-logs/C4-ci.log:3929`). |
| C5 Causal adequacy | PASS | The oracle detects both shared-epoch wins and a transition missing its obligations: removing the session precondition fails D1 at seed 851, and splitting commits fails D2 at seed 843 (`crates/dst/tests/custodian.rs:5585`, `crates/dst/tests/custodian.rs:5455`, `reviewer-mutant-drop-cas.log:265`, `reviewer-mutant-split-commit.log:17`). |
| T1 Structure | PASS | The production pass remains under test through the existing simulated-store seam; recording and fault state are instance-owned, preserving the DST isolation convention (`crates/dst/tests/custodian.rs:5099`, `crates/dst/tests/custodian.rs:5540`). |
| T2 Shape | PASS | Payload checks distinguish the publishing writer's retirement records from fence teardown, and campaign registration preserves the determinism barrier; formatting and spelling pass (`crates/dst/tests/custodian.rs:5464`, `crates/dst/tests/custodian.rs:6069`, `reviewer-scanners.log:1`). |
| T3 Runtime | PASS | Both arms prove a recorded stale-preimage conflict as well as fence victory; ambiguous outcomes preserve the earlier dangling signal and settle on the next pass (`crates/dst/tests/custodian.rs:5885`, `crates/dst/tests/custodian.rs:5806`, `reviewer-restored-green.log:83`). |
| T4 Contribution | N/A | Contribution text is deliberately drafted after Check; the mandatory publish rerun owes its substantive verdict, so the deferred row requires no human clearance (`gate-logs/T4-contribution.log:10`). |
| T5 Judgment | PASS | The previous diagnosis weakness is closed: the exact shared retirement-key misdiagnosis now fails in the Completing arm; path-based merged and closed/unmerged prior art supplies no duplicate coverage (`crates/dst/tests/custodian.rs:5671`, `reviewer-mutant-wrong-diagnosis.log:25`, `reviewer-prior-art.log:52`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether the specified two session races and both ambiguous-commit outcomes provide sufficient Tier-0 evidence to close #843; execution and falsifiability are established, but acceptance of that coverage remains a sign-off judgment (`brief.md:18`, `crates/dst/tests/custodian.rs:5748`, `crates/dst/tests/custodian.rs:5857`). |

No implementation defect was found. Independent execution establishes the required race and ambiguity coverage, the prior review's regression is caught, and the patch stays within scope. Source citations above are relative to `$PDCA_TARGET`; evidence citations are relative to this review directory.

- **Independent execution:** stashing the test patch removed all three new properties; popping restored the exact patch blob. With `RUSTFLAGS='--cfg madsim' MADSIM_TEST_NUM=50 MADSIM_TEST_SEED=843`, `cargo test --offline --locked -p wyrd-dst` passed all 83 tests, including all 31 custodian tests; one documentation example remains ignored. The production source was restored after the mutation campaign, and the final diff contains only the submitted test change (`reviewer-restored-green.log:104`, `reviewer-restored-green.log:147`, `reviewer-integrity.log:6`).
- **Demonstrated red:** removing the session-byte precondition lets both competitors commit in the Open arm at 5,500 microseconds; D1 fails at seed 851 and reachability fails at seed 843. Splitting the session transition from its obligations makes D2 fail at seed 843 with an empty retirement namespace after an applied unknown commit. Checking obligation keys before the session reread also fails at seed 843 in the Completing arm, rejecting the writer's legitimate published-parts obligation as the reason for the lost fence (`reviewer-mutant-drop-cas.log:256`, `reviewer-mutant-split-commit.log:18`, `reviewer-mutant-wrong-diagnosis.log:26`). No capability-probe or symptom-guard concern applies.
- **Gate accounting:** the captured full CI shows formatting, compilation, tests, dependency checks, conformance, statics checks, and DST success; the TiKV feature log shows both requested compilations finishing. These wider gates are assessed from frozen logs, not claimed as independent reruns (`gate-logs/C4-ci.log:3332`, `gate-logs/C4-ci.log:3929`, `gate-logs/host-tikv.log:110`, `gate-logs/host-tikv.log:209`). The verify wrapper explicitly ran green-only (`gate-logs/C4-verify.log:62`); diff coverage is inapplicable to this test-only patch (`gate-logs/C4-diff-cov.log:10`), and the diff-mutant scanner generated no mutants (`gate-logs/C5-mutants.log:10`), so neither substitutes for the independent mutation results. The batch-review log reports zero blocking findings (`gate-logs/T4-batch-review.log:10`). Independent spelling, formatting, documentation lint/render, and whitespace checks passed; the renderer built 99 pages and passed its link audit (`reviewer-scanners.log:1`). Neither external dependency is outstanding.
- **Prior art:** the affected-path history query returned 28 commits, beginning with `5377850`, `f683dbe`, and `dd81029`. All 19 closed, unmerged PRs were checked by actual changed file paths, including rename origins. Only [#647](https://github.com/getwyrd/wyrd/pull/647) and [#336](https://github.com/getwyrd/wyrd/pull/336) touched this file; their changes concern segmented publication/resolution and determinism/reconstruction, respectively, and neither patch contains a restore-pass call or restore-fence property (`reviewer-prior-art.log:30`, `reviewer-prior-art.log:52`). The supplied target's base and patched test blobs exactly match `patch.diff`; there is no target-state caveat (`reviewer-integrity.log:4`).

### Advisory — adversary

# Adversarial review — #843 (restore session fence, seeded Tier-0 DST)

**Verdict: could not refute.** I re-ran the brief's falsifiability step (D4) myself instead of
trusting `build-notes.md` (withheld). In a scratch copy of `$PDCA_TARGET` I built `wyrd-dst`
under `--cfg madsim` (`MADSIM_TEST_NUM=50`), broke the production fence in
`crates/custodian/src/restore.rs` one way at a time, and ran the three new tests. Each break was
undone before the next. The gates C4-verify (green-only), C5-mutants ("No mutants to filter")
and C4-diff-cov (n/a) prove nothing about red here, because the patch changes no production
code. These manual runs are the real red→green evidence.

| Mutation of the production fence | D1 campaign | D2 ambiguous | D3 coverage |
|---|---|---|---|
| (a) drop `require(key, read)` at `restore.rs:824` | **FAIL** | ok | **FAIL** |
| (b1) split: session commit first, obligations second | ok | **FAIL** (`retire:` namespace empty after `Landed`) | ok |
| (b2) split: obligations first, session second | **FAIL** | **FAIL** | **FAIL** |
| iteration-1 finding: check obligation keys before the session re-read (`restore.rs:844-856`) | **FAIL** | ok | **FAIL** |
| read an unknown commit outcome as `Conflict` (`restore.rs:832`) | ok | **FAIL** | ok |
| unmodified fence, `MADSIM_TEST_NUM=1000`, plus `committed_regression_seeds_stay_green` | ok | ok | ok |

Attacks tried, and what came of each:

- `crates/dst/tests/custodian.rs:5585` (the "exactly one transition out of @E" assert): mutation
  (a) fails here at **one** landing point only: Open arm, writer at 5500 µs. In that schedule the
  writer's prewrite (6.5 ms) comes before the fence's `mpu:` read (7 ms), and its apply (7.5 ms)
  comes before the fence's prewrite (8 ms). When the writer decides *after* the read (6500 µs),
  SimTikv refuses the fence on the writer's lock (`crates/dst/tests/support/mod.rs:322-331`), not
  on the precondition. So DST coverage of the session precondition depends on a single 1 ms slot.
  That is fine because D3 asserts that slot is reached for each arm (`custodian.rs:5886`), so a
  longer pass that pushed it out of `RESTORE_FENCE_SPAN` (`:5046`) would fail loudly. Not a defect.
- `custodian.rs:5598` (`stale` classification): I checked it against the SimTikv model. A fence
  `Conflict` logged before the writer's answer can only be a lock refusal, because the writer
  answers at apply time and releases its locks then. `stale: true` therefore really does mean the
  writer had applied before the fence's prewrite. The D3 claim holds. In the Completing arm that
  conflict could also come from `require_absent(retire:records:…:E)`. The doc comment at
  `:5873-5876` says so, and the Open arm carries mutation (a), as the brief requires.
- `custodian.rs:5675` (narrowed to `ChangedUnderPass`): the iteration-1 carry-forward is fixed.
  Moving the obligation-key check ahead of the session re-read now fails D1 and D3 in the
  Completing arm (it reports `ObligationKeyTaken` on the flip's own key).
- Dropping the fence's `require_absent` on its obligation keys (`restore.rs:825-827`) **survives**
  all three new properties. In this race the session precondition covers it, since the writer
  always rewrites the session too. The existing per-pass test
  `crates/custodian/tests/restore_completing_fence.rs` (`neither_obligation_overwrites_one_already_there`)
  catches it, and the brief does not ask DST to. Not a finding.
- Flakiness: the pass and writer run on fixed 1 ms hops (`support/mod.rs:190-192`), `MemDServer`
  adds no hops, and the +500 µs offset rules out ties, so each (arm, delay) pair is one fixed
  schedule. It passed 1000 seeds.
- Observation, no action needed: for the same reason, the D1 seeded campaign
  (`custodian.rs:5747-5755`) replays only schedules that D3 already walks in full on every seed
  (`:5877`). The madsim seed only chooses which of the 15 slots to replay. The doc text "so 50
  seeds sweep the schedule space" (`:5747`) and the 1-in-a-million math for `RESTORE_FENCE_DRAWS`
  (`:5047-5051`) overstate what the seed adds. Nothing is wrong, because D3 is the deterministic
  guarantee. This matches the existing restore nemesis legs (`:2144`).

- NEEDS-HUMAN [human] — `crates/custodian/src/restore.rs:785` still says
  `// deferred: #843 — seeded Tier-0 DST coverage of this fence (809.5).` This patch is #843 and
  delivers that coverage, but the brief's scope ("Nothing outside that file … out of scope: any
  production change") keeps the marker in place. Under the rubric's "Deferrals are settled" rule,
  that marker tells later reviewers the fence's DST coverage is still deferred, which will be false
  once this merges. A human should decide whether to allow this one-line comment removal in this
  PR or file a follow-up. It is a scope call, not a build defect.

### Advisory — code-review

No findings. No introduced correctness bugs or actionable reuse, simplification, or efficiency issues found in this diff.

Reviewed the changed tests against the target source, production restore fence, and simulated store. Frozen C4-ci and C4-verify evidence records successful execution, including the 50-seed DST campaign; no gates were re-run.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [x] Validation — fitness-to-purpose — Decide whether the specified two session races and both ambiguous-commit outcomes provide sufficient Tier-0 evidence to close #843; execution and falsifiability are established, but acceptance of that coverage remains a sign-off judgment (`brief.md:18`, `crates/dst/tests/custodian.rs:5748`, `crates/dst/tests/custodian.rs:5857`).
- [x] `crates/custodian/src/restore.rs:785` still says `// deferred: #843 — seeded Tier-0 DST coverage of this fence (809.5).` This patch is #843 and delivers that coverage, but the brief's scope ("Nothing outside that file … out of scope: any production change") keeps the marker in place. Under the rubric's "Deferrals are settled" rule, that marker tells later reviewers the fence's DST coverage is still deferred, which will be false once this merges. A human should decide whether to allow this one-line comment removal in this PR or file a follow-up. It is a scope call, not a build defect.
- [x] **D1 rejects a valid publication winner.** `brief.md:24-28` requires a winning root flip to leave “no `retire:` key” naming that epoch. The target’s contract explicitly gives the root flip `retire:records:s:<id>:<E>` for published parts and potentially `retire:bytes:s:<id>:<E>` for unnamed parts (`crates/core/src/multipart.rs:3144-3148`; `docs/design/proposals/draft/0016-multipart-commit-protocol.md:662`). A faithful publication fixture therefore fails this oracle; omitting its obligations makes the simulated writer incomplete. Revise D1 to distinguish publication’s obligations from restore’s teardown obligations, checking their payloads as well as keys.
- [x] **D1 models the Complete fence with the wrong epoch.** `brief.md:22-24` starts at `Open@E` and makes the writer transition to `Completing@E`. The cited protocol requires `Open@E → Completing@E+1`, with publication preconditioned on that new epoch (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:704-708`); the source also documents the epoch bump (`crates/core/src/multipart.rs:2173`). Correct the writer’s transition and explicitly distinguish the contested preimage epoch from the winner’s epoch. Otherwise the tests can certify a writer that does not follow the protocol and misattribute retirement records.
- [x] **The specified store does not provide the scheduling seams claimed.** `brief.md:83-85` names `MemMeta` and says every read and commit spans a simulated network hop. Its `get`, `scan`, and `commit` execute directly under a mutex without yielding (`crates/dst/tests/custodian.rs:112-124`, `:139-152`). The cited restore harness actually uses `RecordingMeta`, wrapping `SimTikvMetadataStore` (`:1830-1843`, `:1956`), as does `AmbiguousSweepMeta` (`:3974-3975`). Revise Production reach to require that simulated store or an equivalent forwarding wrapper; using bare `MemMeta` does not expose the session read-to-commit race the coverage is meant to exercise.
- [x] **D3’s two winners do not establish that the contested window was reached.** `brief.md:35-38` accepts observing a fence win and a writer win, without requiring evidence that the writer committed after the pass captured its session preimage and before the conditional fence commit. Those outcome counts alone do not distinguish a contested CAS from serial executions. The cited precedent records actual read answers (`crates/dst/tests/custodian.rs:1825-1828`) and explicitly requires an observed between-read landing (`:2179-2195`). Require recorded read/commit ordering and a stale-preimage conflict for each session arm. D4 does not close this gap because `brief.md:39-42` permits demonstrating only the split-commit mutation instead of the missing-precondition mutation.
- [x] `crates/custodian/src/restore.rs:785` still says `// deferred: #843 — seeded Tier-0 DST coverage of this fence (809.5).`, and the doc comment at `restore.rs:478-482` names only `restore_two_readings_never_license_a_mark` as the DST pin. When this bundle closes #843, the marker will point at a closed issue and still say the coverage is missing. The brief's scope ("nothing outside that file") correctly kept the builder out of `restore.rs`. A human should decide whether to allow a one-line comment edit in this PR or file a follow-up.

## 7. Proven / not proven
- Proven by which oracle: gates overall = pass (stub oracles).
- Unproven / needs manual run: anything flagged in §6.

## 8. Ready-to-ship attachments
- patch.diff
- tracker-comment.md     (ALWAYS, every tracker item)
- build-notes.md         (builder rationale — for the human, not the reviewer)

## 9. Check sign-off                     ← human completes Check here
- Disposition confirmed / overridden:
- Outcome: merged-wider
- Iteration delta (if iterating):
- By / date: Eduard Ralph / 2026-09-30

## 10. Act candidates (hints for the next Act review)
- Plan advisory: 4 finding(s); brief revised: yes (plan-advisory-*.md)
- (empty is the common case)
- Follow-up (#843): remove the `deferred: #843` marker at `crates/custodian/src/restore.rs:785` and update the DST pin list at `restore.rs:478-482` to name the new restore-fence properties.
