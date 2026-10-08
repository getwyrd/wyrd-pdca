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
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 15.21s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

The #843 patch adds seeded Tier-0 coverage for restore-session fencing under concurrent writers and ambiguous commits; no implementation defect was found, and fitness-to-purpose remains for human sign-off.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The coverage obligation is bounded and falsifiable: both writer arms, ambiguous outcomes, a witnessed stale-preimage conflict, and two destructive mutations are specified; `brief.md:18`, `brief.md:54`, `brief.md:59`, `brief.md:68`. |
| C2 Reproduction (red pre-fix) | N/A | This is new coverage over an existing production fence; stashing the patch exposes zero matching properties, as expected, rather than a failing production regression; `brief.md:81`, `reviewer-baseline.log:7`. |
| C3 Change | PASS | The change stays within the authorized existing test file and preserves the earlier property bodies; the target diff exactly matches the supplied patch; `brief.md:105`, `patch.diff:16`, `patch.diff:933`. |
| C4 Verification (red→green) | PASS | Both required mutations failed at seed 1, and the restored 50-seed DST suite passed all 31 custodian tests; frozen full CI also passed, with the independent cache-lock caveat resolved below; `reviewer-mutation-a.log:249`, `reviewer-mutation-b.log:11`, `reviewer-dst.log:305`, `gate-logs/C4-ci.log:3920`. |
| C5 Causal adequacy | PASS | The oracle detects the two forbidden outcomes directly: two winners from one epoch and a transition missing its retirement obligations; the reachability assertion requires a recorded stale-preimage conflict in each arm; `crates/dst/tests/custodian.rs:5576`, `crates/dst/tests/custodian.rs:5446`, `crates/dst/tests/custodian.rs:5862`. |
| T1 Structure | PASS | Production reconciliation runs through a forwarding simulated-TiKV seam with instance-owned observations, preserving the yielding commit model and avoiding shared mutable globals; `crates/dst/tests/custodian.rs:5094`, `crates/dst/tests/custodian.rs:5152`, `crates/dst/tests/custodian.rs:5531`, `crates/dst/tests/support/mod.rs:300`, `reviewer-scanners.log:7`. |
| T2 Shape | PASS | The existing campaign macro and regression-seed loop include the new coverage; no production API, persisted format, dependency, or documentation contract changes; `crates/dst/tests/custodian.rs:6046`, `crates/dst/tests/custodian.rs:6103`, `patch.diff:1`. |
| T3 Runtime | PASS | The restored simulation campaign completes across 50 seeds, including actual contested windows and ambiguous-commit retries; the custodian suite takes 15.61 seconds here; `xtask/src/main.rs:1716`, `xtask/src/main.rs:1750`, `reviewer-dst.log:284`, `reviewer-dst.log:296`, `reviewer-dst.log:305`. |
| T4 Contribution | N/A | Contribution artifacts are intentionally drafted after Check; their substantive audit remains owed at publish, exactly as the deferred gate specifies; `gate-logs/T4-contribution.log:10`. |
| T5 Judgment | PASS | Path-based merged-history and closed-unmerged PR checks found no duplicate restore-fence coverage; simulated writer batches and payload-specific obligations match the scoped protocol claims; `reviewer-prior-art.log:3`, `reviewer-prior-art.log:8`, `reviewer-prior-art.log:28`, `crates/dst/tests/custodian.rs:5292`, `crates/dst/tests/custodian.rs:5455`. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether the demonstrated Tier-0 race, atomicity, and recovery coverage is sufficient to close #843's coverage milestone; its claim is scoped to the production fence over seeded simulated sessions; `brief.md:18`, `brief.md:105`, `brief.md:116`. |

Repository citations resolve against the supplied `$PDCA_TARGET` (`./target`); evidence citations resolve in this review directory. Both prerequisite fence shapes are present. The final target diff is byte-for-byte identical to `patch.diff`, and `crates/custodian/src/restore.rs` is restored to its base contents.

The independent red→green evidence supports D1–D4:

- **Missing precondition:** temporarily removing the session-byte condition at `crates/custodian/src/restore.rs:824` makes D1 fail with `MADSIM_TEST_SEED=1`. In the Open arm, a writer landing at 5,500 µs and the fence both commit; the assertion observes two winners instead of one (`reviewer-mutation-a.log:249`).
- **Split transaction:** temporarily committing the session transition before its obligations at `crates/custodian/src/restore.rs:832` makes D2 fail at seed 1. The Completing arm receives an unknown-but-landed transition and has neither required retirement obligation (`reviewer-mutation-b.log:11`). Each mutation was restored independently.
- **Restored green:** `cargo xtask dst` reruns DST clippy and the entire simulation crate, with all 31 custodian properties passing across 50 seeds, including the three new properties and existing regression-seed replay (`reviewer-dst.log:203`, `reviewer-dst.log:305`). An explicit restored rerun starting at the mutation seed (`MADSIM_TEST_SEED=1`, 50 seeds) also passes all three properties (`reviewer-green-final.log:1`, `reviewer-green-final.log:11`). The assertions verify obligation payloads, inode/dirent publication, audit ordering, and the second pass's settled state (`crates/dst/tests/custodian.rs:5431`, `crates/dst/tests/custodian.rs:5700`, `crates/dst/tests/custodian.rs:5786`, `crates/dst/tests/custodian.rs:5807`).

The CI rerun encountered a resolved host limitation, not a patch failure. `cargo xtask ci` passed typos, docs lint/render, repository guards, formatting, workspace clippy/build/tests, and cargo-machete, then stopped because the advisory database lock was outside the writable sandbox (`reviewer-ci.log:3288`). All three dependency-audit commands subsequently passed with identical parsed repository policies and only `advisories.db-path` redirected to this sandbox; the real audit tool and advisory database were exercised (`reviewer-deny.log:1`, `reviewer-deny.log:16`, `reviewer-deny.log:28`, `reviewer-deny.log:32`). Conformance and the statics scanner also passed independently (`reviewer-scanners.log:2`, `reviewer-scanners.log:7`). Both named external dependencies were exercised: typos and the renderer's successful link audit appear at `reviewer-ci.log:2` and `reviewer-ci.log:10`.

The frozen evidence has the following limits and dispositions:

- **C4-ci: PASS.** The captured full gate ends with all checks passed (`gate-logs/C4-ci.log:3920`); the independent monolithic rerun's cache interruption is disclosed above.
- **C4-verify: PASS, green-only.** The captured log explicitly sets madsim and 50 seeds and reports 31 custodian tests passing; it does not supply a pre-fix failing test (`gate-logs/C4-verify.log:11`, `gate-logs/C4-verify.log:41`, `gate-logs/C4-verify.log:62`). The independent mutations supply the required falsifiability evidence.
- **C4-diff-cov and C5-mutants: N/A.** There are no changed production Rust lines and no diff-selected mutants; neither log is evidence that D4 ran (`gate-logs/C4-diff-cov.log:12`, `gate-logs/C5-mutants.log:10`).
- **T4-batch-review: PASS as recorded.** Its captured output reports zero blocking findings; it contains no individual review reasoning, so this review makes its own judgments (`gate-logs/T4-batch-review.log:10`).
- **host-tikv: PASS.** Both feature-enabled clippy commands passed independently as well as in the frozen gate (`reviewer-host-tikv.log:101`, `reviewer-host-tikv.log:200`, `gate-logs/host-tikv.log:110`, `gate-logs/host-tikv.log:209`).
- **T4-contribution: N/A.** The publish-time audit remains mandatory; the intentional deferral needs no human clearance (`gate-logs/T4-contribution.log:10`).

The prior-art investigation checked the affected path's merged history and all 356 closed PRs, then inspected changed-file lists for all 19 unmerged PRs. Only #647 and #336 touched this file; their patches concern segmented publication/repoint and earlier crash/read/global-state coverage, respectively, and neither adds restore-session-fence coverage (`reviewer-prior-art.log:8`, `reviewer-prior-art.log:28`). The brief's recorded #682/#722 overlaps and #809 prior fixture work remain acknowledged (`brief.md:133`). No capability-probe symptom guard, scope expansion, or undischarged external dependency was identified.

### Advisory — adversary

# Adversarial review — #843 (restore-fence DST coverage)

**Verdict: I could not break the main claim.** The three new properties run the production
fence (`reconcile_after_restore` → `fence_session`, `crates/custodian/src/restore.rs:801-866`)
over `SimTikvMetadataStore`, which has a hop inside each commit. They fail when the fence is
broken. I tried to break the evidence in four ways and only found a weak check on the
*reported cause* (first bullet). What I ran, in a scratch copy of `$PDCA_TARGET` with
`RUSTFLAGS=--cfg madsim MADSIM_TEST_NUM=50`:

- Green leg: `restore_fence_never_shares_the_epoch_with_a_session_writer`,
  `restore_fence_settles_an_ambiguous_commit_on_the_next_pass` and
  `restore_fence_reaches_the_contested_window` all pass. This matches `gate-logs/C4-ci.log:3853-3865`.
- D4(a), with `.require(key, read)` removed at `crates/custodian/src/restore.rs:824`: D1 and D3
  go **red** at `crates/dst/tests/custodian.rs:5576`. Open arm, writer at 5500 µs: the fence
  and the writer were both `Committed`, so the left side was 2 and the right side 1.
- D4(b), session in one commit and obligations in a second (`restore.rs:828-832`): D2 goes
  **red** at `custodian.rs:5446`. Open arm, `Landed`: the session moved to `Aborting` and
  `retire:` was empty. The reverse order (obligations first, then session) turns all three
  tests red.
- I checked the `stale: true` label (`custodian.rs:5589`) against the sim's commit model
  (`crates/dst/tests/support/mod.rs:300-357`, two 1 ms hops with locks taken at prewrite). A
  writer apply logged between the fence's read and the fence's answer always happens *before*
  the fence's prewrite. So the `Conflict` in that window is a failed precondition, not a lost
  lock race. A lock-race conflict logs the writer *after* the fence's answer, so it is labeled
  `stale: false`. D3's reachability claim is therefore sound.

## Findings

- NEEDS-HUMAN [impl] — `crates/dst/tests/custodian.rs:5660-5666`: when the writer wins, the
  test accepts `ChangedUnderPass | LostConflict | ObligationKeyTaken { .. }` as the reason the
  pass gives. That lets a wrong diagnosis through. **Concrete failing case:** change
  `restore.rs:844-856` to check the obligation keys *before* re-reading the session, so
  `ChangedUnderPass` is never reported. In the Completing arm the pass then reports
  `ObligationKeyTaken { retire:records:s:<id>:3 }`. That key is the root flip's own `{parts}`
  obligation, and the operator summary turns it into "whose retirement key was already taken
  by another obligation" (`crates/server/src/cli.rs:1484`, `:1494`) for an upload that simply
  published. The Open arm reports `LostConflict` instead. With this change all three new tests
  still pass, and so does the existing `crates/custodian/tests/restore_completing_fence.rs`.
  Only `restore_open_fence.rs:592` catches it, and only for the Open shape. This patch is the
  only test that models the flip sharing the fence's `retire:records:s:<id>:<E>` key, so it is
  the place to pin this. In this model, every writer-won schedule re-reads the session
  (`restore.rs:844`) after the writer has applied, so `ChangedUnderPass` is the only correct
  cause. I narrowed the match to `ChangedUnderPass` alone and re-ran on the **unmodified**
  fence: all three tests pass across 50 seeds. So the stricter check costs nothing and adds a
  real check.
- `crates/dst/tests/custodian.rs:5863` (D3, Completing arm) — a limit, not a defect. In this
  arm the fence's `Conflict` would happen even without the session-bytes precondition. The
  flip writes `retire:records:s:<id>:3` (`custodian.rs:5346-5347`, put at `:5370`), the same key the
  fence `require_absent`s (`restore.rs:825-827`), so the fence loses on that key alone.
  Mutation (a) above was caught **only** by the Open arm. That meets the brief ("at least one
  seed of D1"), but the Completing arm's "stale-preimage conflict" does not show that the
  session precondition is what did the work. Nobody should read the Completing arm as guarding
  that precondition.
- NEEDS-HUMAN [human] — `crates/custodian/src/restore.rs:785` still says
  `// deferred: #843 — seeded Tier-0 DST coverage of this fence (809.5).`, and the doc comment
  at `restore.rs:478-482` names only `restore_two_readings_never_license_a_mark` as the DST pin.
  When this bundle closes #843, the marker will point at a closed issue and still say the
  coverage is missing. The brief's scope ("nothing outside that file") correctly kept the
  builder out of `restore.rs`. A human should decide whether to allow a one-line comment edit
  in this PR or file a follow-up.
- `gate-logs/C4-verify.log:62-64` — the `C4-verify` row in `check-gates.json` says "red->green:
  pass", but the gate ran green-only ("the per-fix RED can't be isolated"). `C4-diff-cov` and
  `C5-mutants` measured nothing on this test-only diff ("n/a", "No mutants to filter"). The
  brief said this in advance, so it is not a refutation. But no gate showed a red in this
  bundle. The red exists only in the builder's D4 notes and in my re-run above.

## Tried and could not break

- **Whether the test reaches production code:** the tap (`custodian.rs` `FenceMeta`) passes
  every call through to `SimTikvMetadataStore`. It intercepts only to log, and to strike in
  D2. It is not bare `MemMeta`. The writer commits through the same store but bypasses the
  strike.
- **Log attribution:** if the fence's listing ever stopped going through
  `scan_page(MPU_PREFIX)`, the test would classify against the staged read and fail loudly
  (fence and writer both counted as winners). It would not pass silently.
- **Existing properties unchanged (L):** the two new calls are added at the end of the
  regression-seed chain (`custodian.rs:6103-6104`), so earlier properties' RNG draws do not
  move. No existing assertion was edited.

### Advisory — code-review

- No findings on either lens: no introduced correctness/test-fidelity bugs or actionable reuse, simplification, or efficiency issues found. Reviewed the recording tap (`crates/dst/tests/custodian.rs:5094`), concurrent-writer oracle (`crates/dst/tests/custodian.rs:5509`), ambiguous-commit recovery (`crates/dst/tests/custodian.rs:5740`), and contested-window assertions (`crates/dst/tests/custodian.rs:5854`) against the read-only target.

Validation evidence: frozen C4-ci logs show all three new properties passing; C4-verify records a passing madsim run with 50 seeds. Tests were not rerun during this advisory review.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] Validation — fitness-to-purpose — Decide whether the demonstrated Tier-0 race, atomicity, and recovery coverage is sufficient to close #843's coverage milestone; its claim is scoped to the production fence over seeded simulated sessions; `brief.md:18`, `brief.md:105`, `brief.md:116`.
- [ ] `crates/dst/tests/custodian.rs:5660-5666`: when the writer wins, the test accepts `ChangedUnderPass | LostConflict | ObligationKeyTaken { .. }` as the reason the pass gives. That lets a wrong diagnosis through. **Concrete failing case:** change `restore.rs:844-856` to check the obligation keys *before* re-reading the session, so `ChangedUnderPass` is never reported. In the Completing arm the pass then reports `ObligationKeyTaken { retire:records:s:<id>:3 }`. That key is the root flip's own `{parts}` obligation, and the operator summary turns it into "whose retirement key was already taken by another obligation" (`crates/server/src/cli.rs:1484`, `:1494`) for an upload that simply published. The Open arm reports `LostConflict` instead. With this change all three new tests still pass, and so does the existing `crates/custodian/tests/restore_completing_fence.rs`. Only `restore_open_fence.rs:592` catches it, and only for the Open shape. This patch is the only test that models the flip sharing the fence's `retire:records:s:<id>:<E>` key, so it is the place to pin this. In this model, every writer-won schedule re-reads the session (`restore.rs:844`) after the writer has applied, so `ChangedUnderPass` is the only correct cause. I narrowed the match to `ChangedUnderPass` alone and re-ran on the **unmodified** fence: all three tests pass across 50 seeds. So the stricter check costs nothing and adds a real check.
- [ ] `crates/custodian/src/restore.rs:785` still says `// deferred: #843 — seeded Tier-0 DST coverage of this fence (809.5).`, and the doc comment at `restore.rs:478-482` names only `restore_two_readings_never_license_a_mark` as the DST pin. When this bundle closes #843, the marker will point at a closed issue and still say the coverage is missing. The brief's scope ("nothing outside that file") correctly kept the builder out of `restore.rs`. A human should decide whether to allow a one-line comment edit in this PR or file a follow-up.
- [ ] **D1 rejects a valid publication winner.** `brief.md:24-28` requires a winning root flip to leave “no `retire:` key” naming that epoch. The target’s contract explicitly gives the root flip `retire:records:s:<id>:<E>` for published parts and potentially `retire:bytes:s:<id>:<E>` for unnamed parts (`crates/core/src/multipart.rs:3144-3148`; `docs/design/proposals/draft/0016-multipart-commit-protocol.md:662`). A faithful publication fixture therefore fails this oracle; omitting its obligations makes the simulated writer incomplete. Revise D1 to distinguish publication’s obligations from restore’s teardown obligations, checking their payloads as well as keys.
- [ ] **D1 models the Complete fence with the wrong epoch.** `brief.md:22-24` starts at `Open@E` and makes the writer transition to `Completing@E`. The cited protocol requires `Open@E → Completing@E+1`, with publication preconditioned on that new epoch (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:704-708`); the source also documents the epoch bump (`crates/core/src/multipart.rs:2173`). Correct the writer’s transition and explicitly distinguish the contested preimage epoch from the winner’s epoch. Otherwise the tests can certify a writer that does not follow the protocol and misattribute retirement records.
- [ ] **The specified store does not provide the scheduling seams claimed.** `brief.md:83-85` names `MemMeta` and says every read and commit spans a simulated network hop. Its `get`, `scan`, and `commit` execute directly under a mutex without yielding (`crates/dst/tests/custodian.rs:112-124`, `:139-152`). The cited restore harness actually uses `RecordingMeta`, wrapping `SimTikvMetadataStore` (`:1830-1843`, `:1956`), as does `AmbiguousSweepMeta` (`:3974-3975`). Revise Production reach to require that simulated store or an equivalent forwarding wrapper; using bare `MemMeta` does not expose the session read-to-commit race the coverage is meant to exercise.
- [ ] **D3’s two winners do not establish that the contested window was reached.** `brief.md:35-38` accepts observing a fence win and a writer win, without requiring evidence that the writer committed after the pass captured its session preimage and before the conditional fence commit. Those outcome counts alone do not distinguish a contested CAS from serial executions. The cited precedent records actual read answers (`crates/dst/tests/custodian.rs:1825-1828`) and explicitly requires an observed between-read landing (`:2179-2195`). Require recorded read/commit ordering and a stale-preimage conflict for each session arm. D4 does not close this gap because `brief.md:39-42` permits demonstrating only the split-commit mutation instead of the missing-precondition mutation.

## 7. Proven / not proven
- Proven by which oracle: gates overall = pass (stub oracles).
- Unproven / needs manual run: anything flagged in §6.

## 8. Ready-to-ship attachments
- patch.diff
- tracker-comment.md     (ALWAYS, every tracker item)
- build-notes.md         (builder rationale — for the human, not the reviewer)

## 9. Check sign-off                     ← human completes Check here
- Disposition confirmed / overridden:
- Outcome: iterated-to-Do
- Iteration delta (if iterating): Auto-iterate (round 1): rebuilding for the implementation-level findings — `crates/dst/tests/custodian.rs:5660-5666`: when the writer wins, the test accepts `ChangedUnderPass | LostConflict | ObligationKeyTaken { .. }` as the reason the pass gives. That lets a wrong diagnosis through. **Concrete failing case:** change `restore.rs:844-856` to check the obligation keys *before* re-reading the session, so `ChangedUnderPass` is never reported. In the Completing arm the pass then reports `ObligationKeyTaken { retire:records:s:<id>:3 }`. That key is the root flip's own `{parts}` obligation, and the operator summary turns it into "whose retirement key was already taken by another obligation" (`crates/server/src/cli.rs:1484`, `:1494`) for an upload that simply published. The Open arm reports `LostConflict` instead. With this change all three new tests still pass, and so does the existing `crates/custodian/tests/restore_completing_fence.rs`. Only `restore_open_fence.rs:592` catches it, and only for the Open shape. This patch is the only test that models the flip sharing the fence's `retire:records:s:<id>:<E>` key, so it is the place to pin this. In this model, every writer-won schedule re-reads the session (`restore.rs:844`) after the writer has applied, so `ChangedUnderPass` is the only correct cause. I narrowed the match to `ChangedUnderPass` alone and re-ran on the **unmodified** fence: all three tests pass across 50 seeds. So the stricter check costs nothing and adds a real check.. 5 finding(s) needing human judgment were deferred to sign-off, not addressed here.
- By / date: auto-iterate / 2026-09-30

## 10. Act candidates (hints for the next Act review)
- Plan advisory: 4 finding(s); brief revised: yes (plan-advisory-*.md)
- (empty is the common case)
