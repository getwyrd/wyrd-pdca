# Result — issue 656 / multipart-admission-create-abort

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect: The first store-I/O slice of the multipart protocol: `CreateMultipartUpload`'s
  admission through the serialized fleet counter (`mpuctl`) plus session creation, and
  `AbortMultipartUpload`'s teardown fence, as `wyrd-core` functions over `MetadataStore`. They
  answer in the typed vocabulary already on `main` (`CreateOutcome`, `AbortOutcome`, `Refusal`,
  `Backpressure`: `crates/core/src/multipart.rs:4277-4452`). Today the module is deliberately
  pure (`crates/core/src/multipart.rs:5-7`, `:104-118`): nothing creates a session, and nothing
  but the restore pass fences one.
- Success criterion: A NEW test over `RedbMetadataStore::in_memory()` on a multi-thread
  tokio runtime shows all of the following.
  **(A) Admission under real contention.** On a fresh empty store, **8, 10 and 16** concurrent
  creates (each level on its own fresh store; all three asserted, not one) all answer
  `CreateOutcome::Created`. Their upload ids are pairwise distinct. Exactly N `mpu:` records
  exist (scan `MPU_PREFIX`), each decoding through `decode_session_record` as an `Open` session
  for the requested bucket inode and object name. Exactly N `seggrp:` markers exist, one per
  session's nonce. `mpuctl` decodes through `decode_admission_record` with `count() == N`
  exactly. These are the levels #508's rejected attempts measured as falsely refused (8 → 2
  refused, 10 → 3, 16 → 3; aws-cli's default `max_concurrent_requests` is 10). These free-running
  levels stay, but they are not the red (see A′): redb's `get` and `commit` never await inside
  (`crates/metadata-redb/src/lib.rs:113-117`, `:208-234`), so a valid schedule can serialize them.
  **(A′) Admission under a forced schedule.** The same test drives N = 8 creates through a
  test-local `MetadataStore` wrapper over `RedbMetadataStore` that controls the order: every
  creator's `mpuctl` read is held until all live creators have read it, before any of them may
  commit, so each round has one winner and the rest lose the CAS. The wrapper counts each
  creator's `mpuctl` CAS losses (it can tell creators apart by the `mpu:<id>` key in each batch).
  Assert that all 8 answer `Created`, that `mpuctl.count() == 8`, and that at least one creator
  lost the `mpuctl` CAS **more than `MAX_UPLOAD_ID_ATTEMPTS` (= 2, `:4983`) times** and still
  succeeded. That last assertion is what a shared, collision-sized budget fails on every run.
  **(B) The real bounds, each writing nothing.** With the ledger at its stored `max_sessions`, a
  create answers `Refused(SlowDown { pressure: SessionCap { .. } })`. A create whose local
  `Budget` differs from the profile stored in `mpuctl` answers `Refused(ProfileSkew { .. })`. A
  create for a bucket with no `bucket:` record answers `Refused(NoSuchBucket)`. A create whose
  bucket marker is **deleted or rewritten between Create's read and its commit** (the same
  wrapper, acting at commit) answers `Refused(NoSuchBucket)`. After each, the store is
  byte-identical to before: no `mpu:`, no `seggrp:`, and no `mpuctl` write.
  **(C) Two separate retry budgets.** When the first minted upload id collides with an existing
  `mpu:` record, the create re-mints and succeeds. When every one of `MAX_UPLOAD_ID_ATTEMPTS`
  mints collides, it refuses. Losing the `mpuctl` CAS never spends the collision budget, and a
  collision never spends the contention budget (`MAX_ADMISSION_CAS_ATTEMPTS`).
  **(D) Abort per decision 3 (`0016:977`).** On `Open@E` it answers `Fenced`; afterwards the
  session decodes as `Aborting@E+1`, and `retire:bytes:s:<id>:<E>` holds exactly the
  `{session, all}` obligation `SessionRecord::open_teardown` mints, committed in the **same**
  batch as the fence. A second Abort answers `AlreadyAborting`. **Two concurrent Aborts** of one
  `Open` session answer one `Fenced` and one `AlreadyAborting`, never `OperationAborted`. Abort
  of a `Completing` session answers `Refused(OperationAborted)` and leaves the store unchanged.
  Abort of a `Completed` session, or of an upload id with no record, answers
  `Refused(NoSuchUpload)`.
  **(E) Abort never moves the counter.** `mpuctl.count()` is identical before and after every
  leg of (D), and a `Completed` tombstone stays counted. The decrement belongs only to the
  terminal delete (#659).
  **(F) An unknown Create outcome is recovered under the original upload id (`0016:2009-2027`).**
  Through the wrapper, the first Create commit returns `CommitUnknownResult { may_still_commit:
  true }` (`crates/traits/src/lib.rs:230-248`) in three variants: (F1) the batch was applied
  before the error; (F2) it was not applied; (F3) it is held and applied **after** the recovery
  re-drive has committed (a late commit, which must then lose its own `require_absent(mpu:<id>)`).
  In each, recovery re-drives with the **same** upload id and nonce, rebuilds the `mpuctl`
  precondition from a fresh read, and answers `Created`. Afterwards there is exactly one `mpu:`
  record, for that upload id, exactly one `seggrp:` marker, and `mpuctl.count()` rose by exactly
  one. No second upload id is ever minted.
  **(G) Seeded coverage** (`AGENTS.md:188-190`: a new concurrent path lands with seeded Tier-0
  DST coverage). A NEW madsim scenario `crates/dst/tests/multipart_admission.rs` runs concurrent
  Creates and Aborts with the commit-ambiguity nemesis armed, mirroring
  `crates/dst/tests/commit_ambiguity.rs`'s 1021/1031 legs. After every seed: the number of `mpu:`
  records equals `mpuctl.count()`, every `mpu:` record has its `seggrp:` marker, and every
  `Aborting` session has its `retire:bytes:` obligation. `cargo xtask ci` sweeps it (it includes
  `cargo xtask dst`, 50 seeds), so C4-ci runs it.
- Repo + branch target: getwyrd/wyrd @ main
- Scope: `CreateMultipartUpload`'s admission and session creation (including recovery of an
  unknown commit outcome under the original upload id), and `AbortMultipartUpload`'s fence, as
  store-I/O functions in `wyrd-core` answering in the landed vocabulary; the shared fence batch
  builder in `wyrd-core`, and the one behaviour-preserving change to restore's `fence_session`
  (`crates/custodian/src/restore.rs:1045-1057`) to use it; the seeded DST scenario
  `crates/dst/tests/multipart_admission.rs`; the multipart module header and the
  living-architecture paragraph updated to say these writers now exist / out of scope:
  UploadPart and slot reservation (#657); Complete (#658); the retirement drain and the terminal
  delete that decrements `mpuctl.count` (#659); the reaper and operator abort (#625, #633); every
  gateway, S3, CLI or custodian **caller of Create or Abort** (#666–#670); the bucket
  deletion-fence key and its `require_absent` (#511, see Design); the `Completing → Aborting`
  edge, which a client Abort never takes (the table answers `409`); any edit to proposal 0016 or
  to any ADR.

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: new-feature
- Confidence: medium
- Recommendation: (set by Do)

## 3. Correctness (Check — chain)
- C1 Spec: none — brief.md
- C2 Reproduction (red pre-fix): none — (no gate configured)
- C3 Change: none — patch.diff
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): fail — xtask: `cargo deny --all-features --config deny-all-features.toml check advisories` failed with exit status: 1
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: unverifiable —                why this slice has no isolable red (the cargo output is above).
- C4 diff coverage: changed lines executed by the patch's tests: pass — diff coverage 99.7% — 362 of 363 instrumentable changed lines executed (floor 80%); 363 of 953 changed lines were instru
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): pass — 73 mutants tested in 4m: 62 caught, 11 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 2 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_656/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 16.22s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Review of #656’s multipart Create admission and Abort fencing: one reproduced Abort commit-uncertainty defect remains.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | Admission, fence atomicity, counter ownership, and original-ID recovery have concrete acceptance criteria consistent with proposal 0016; `brief.md:23`, `docs/design/proposals/draft/0016-multipart-commit-protocol.md:657`, `docs/design/proposals/draft/0016-multipart-commit-protocol.md:977`. |
| C2 Reproduction (red pre-fix) | NEEDS-HUMAN | Accept the declared net-new verification posture — the stashed base cannot compile the test because the API is absent; three independently executed behavioral mutant reds support discrimination but do not constitute a pre-fix behavioral red; `brief.md:133`, `reviewer-red.log:3`. |
| C3 Change | PASS | The change stays within the approved core API, shared restore batch, tests, and living-doc scope; existing restore behavior passes 30 tests, and no production Create/Abort caller was found; `crates/custodian/src/restore.rs:1052`, `docs/design/architecture/05-building-block-view.md:202`. |
| C4 Verification (red→green) | NEEDS-HUMAN | Resolve the sign-off disposition of the inherited all-features dependency gate outside this slice — local tests pass, but full CI remains red on unchanged rustls 0.21.12 advisories; `gate-logs/C4-ci.log:6525`, `Cargo.lock:3294`, `brief.md:356`. |
| C5 Causal adequacy | NEEDS-HUMAN [impl] | Preserve earlier in-flight uncertainty across Abort retries and add a mixed-outcome regression — a later settled error currently conceals a fence that can still land after return (R1); `crates/core/src/multipart/store.rs:751`, `reviewer-abort-probe.log:20`. |
| T1 Structure | PASS | Runtime services remain caller-owned, metadata access stays on the existing trait seam, and both fence writers share one atomic batch builder; `crates/core/src/multipart/store.rs:87`, `crates/core/src/multipart/store.rs:775`, `crates/custodian/src/restore.rs:1052`. |
| T2 Shape | PASS | Formatting, spelling, documentation rendering, and statics checks pass; creation reserves encoded size for later states without weakening canonical decoding; `reviewer-scanners.log:1`, `reviewer-statics.log:3`, `crates/core/src/multipart.rs:2368`. |
| T3 Runtime | FAIL | R1 violates the metadata outcome contract: Abort reports that one reread settles the outcome while an earlier fence can still commit; the independent real-redb probe demonstrates the late state change; `crates/traits/src/lib.rs:240`, `crates/core/src/multipart/store.rs:731`, `reviewer-abort-probe.log:20`. |
| T4 Contribution | FAIL | The batch review’s two entries are one unresolved reproduced defect, requiring one disposition; affected-path prior art was checked, and contribution-artifact auditing is N/A until its mandatory publish rerun; `gate-logs/T4-batch-review.log:10`, `gate-logs/T4-contribution.log:10`, `reviewer-rejected-paths.json:1`. |
| T5 Judgment | PASS | No additional scope or root-cause dispute was found: the prior size and identity cases pass, the #511 deferral is retained, and caller-owned timeouts remain the recorded decision; `crates/core/src/multipart/store.rs:245`, `crates/core/src/multipart/store.rs:358`, `crates/core/src/multipart/store.rs:564`, `brief.md:342`. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether this isolated API slice meets the intended milestone — core behavior is exercised, but production verbs and reclamation remain the deliberately separate #668/#659/#625 work; `brief.md:146`, `crates/core/src/multipart/store.rs:27`. |

R1: Abort must retain uncertainty from every fence that can still land. At `crates/core/src/multipart/store.rs:751`, each new unknown error replaces the previous one; exhaustion returns that last error at line 731. A later `may_still_commit: false` describes only its own transaction. It cannot settle an earlier transaction with `may_still_commit: true`, as the seam explicitly states at `crates/traits/src/lib.rs:240`.

The independent probe held the first Abort batch in flight, returned three subsequent unapplied unknown outcomes with `may_still_commit: false`, and left every recovery read at `Open`. Abort returned the last false-valued error. Applying the first held batch **after Abort returned** then committed successfully: the session became `Aborting`, its retirement obligation appeared, and the admission counter stayed unchanged (`reviewer-abort-probe.log:20`). This can lead a caller to treat a negative reread as final even though the upload is subsequently fenced. Preserve outstanding ambiguity until confirmed success or session state invalidates the earlier batch; Create already retains it at `crates/core/src/multipart/store.rs:541`. Add the mixed sequence to the regression suite. The two frozen batch-review entries describe this same defect, not two defects.

The probe runs against the unchanged reviewed implementation and real redb preconditions. From this report’s directory:

```sh
CARGO_TARGET_DIR="$PWD/reviewer-build" TMPDIR="$PWD/pdca-reviewer-656-scratch" \
  cargo test --offline --manifest-path pdca-reviewer-656-probe/Cargo.toml \
  --test abort_uncertainty reviewer_abort_preserves_earlier_in_flight_uncertainty \
  -- --nocapture
```

Its expected assertion is red on this patch. The existing DST tests run 1021 and 1031 separately (`crates/dst/tests/multipart_admission.rs:324`, `crates/dst/tests/multipart_admission.rs:339`), so their green results do not discharge R1.

Independent verification supports the rest of the scoped behavior:

- Stashing the tracked fix while retaining the new test reproduced the missing-API compile failure. Popping the stash restored a 29/29 green integration run (`reviewer-red.log`, `reviewer-restored-green.log`). This is the declared enhancement limitation, not a stale-target or compile defect in the patch.
- All four multipart DST tests passed with `RUSTFLAGS='--cfg madsim' MADSIM_TEST_NUM=50`, including their explicit seed sweeps (`reviewer-dst.log`). The three restore suites passed 30 tests (`reviewer-restore.log`).
- The three brief-required mutations were executed in a separate scratch copy. Capping contention at two attempts produced `AdmissionContention { attempts: 2 }`; decrementing on Abort failed the counter assertion with 0 versus 1; re-minting after an unknown Create produced two sessions instead of one. Each test executed and failed. Restoring that copy passed all 29 tests (`reviewer-mutant-small-contention-budget.log`, `reviewer-mutant-abort-decrement.log`, `reviewer-mutant-unknown-remint.log`, `reviewer-mutants-restored-green.log`).
- Formatting, typos, docs lint, rendering/link audit, the statics scanner, and the 5-valid/6-invalid conformance vectors passed (`reviewer-scanners.log`, `reviewer-statics.log`, `reviewer-conformance.log`). Thus both declared external tools, `typos` and `docs-renderer`, were actually exercised. No live distributed-backend run is claimed.

The frozen gates are adjudicated from their captured output where the instance-scoped wrapper is unavailable here:

| Gate | Assessment | Evidence |
|------|------------|----------|
| C4-ci | NEEDS-HUMAN | Both attempts fail the all-features advisory audit for RUSTSEC-2026-0346/0349 through tikv-client’s rustls 0.21.12; preceding workspace build/tests pass. The unchanged dependency failure is inherited and explicitly outside this slice, not a new patch defect; `gate-logs/C4-ci.log:6525`, `brief.md:356`. |
| C4-verify | NEEDS-HUMAN | Green executes 29 core and 4 DST tests; the reverted production cannot compile the test, so no behavioral pre-fix red was established; `gate-logs/C4-verify.log:12`, `gate-logs/C4-verify.log:21`. |
| C4-diff-cov | PASS | The captured run reports 362/363 instrumentable changed lines covered, 99.7%; 590 other changed lines are unscored, so this is not coverage of every changed line; `gate-logs/C4-diff-cov.log:603`. |
| C5-mutants | PASS | Captured baseline passes; 73 mutants yield 62 caught and 11 unviable, with none reported missed. This aggregate result does not test every error sequence; `gate-logs/C5-mutants.log:10`. |
| T4-batch-review | FAIL | Two duplicate reports of R1, independently reproduced here; `gate-logs/T4-batch-review.log:10`. |
| T4-contribution | N/A | PR/commit contribution artifacts are drafted after Check; the substantive audit must rerun at publish, exactly as the deferred evidence states; `gate-logs/T4-contribution.log:10`. |
| host-tikv | PASS | Captured output shows both requested feature-enabled clippy builds finishing successfully; it establishes compilation, not live TiKV behavior; `gate-logs/host-tikv.log:2`, `gate-logs/host-tikv.log:211`. |

The CI failure occurs before its conformance/statics/DST stages (`xtask/src/main.rs:1683`); those stages must not be inferred green from that log. This review separately ran conformance/statics and the affected multipart DST target, not the entire DST suite. No gate log was missing. No applicable `INTEGRATION.md` was present in the provided target.

Prior-art checking was performed by every affected path. GitHub path-history queries and closed-PR path searches covered all six changed files (`reviewer-prior-art.json`). An additional file-list audit of all 19 closed, unmerged PRs found only #647 overlapping, at `crates/custodian/src/restore.rs`; its changes concern segmented-map resolution, not this session-fence batch (`reviewer-rejected-paths.json`, `reviewer-prior-647-restore.diff`). [Issue #508](https://github.com/getwyrd/wyrd/issues/508) records the rejected monolithic attempts, and [issue #636](https://github.com/getwyrd/wyrd/issues/636) is closed as not planned. The patch’s scoped split and shared fence builder do not revive those rejected approaches. The #511 tracked deferral and the recorded timeout decision were not re-raised.

Source citations above resolve against `$PDCA_TARGET`, whose recorded base is `7bf7eb906c634d57d28f2ae26fd0b412732948d5`; local evidence paths resolve in this report’s directory. Reverse-apply checking confirms the supplied patch is restored exactly, and dependency manifests/lockfiles are unchanged. This report is advisory; it does not grant or gate acceptance.

<!-- pdca:leaf-complete -->

### Advisory — adversary

# Adversarial review: issue 656 (multipart Create/Abort), iteration 3

I tried to break the patch by re-running the evidence in a scratch copy of `$PDCA_TARGET`
(the patched tree, with throwaway probe tests added only to the copy). One reproduced defect
confirms the T4 blocker. One test gap lets a regression of the same kind slip past the suite.
One doc claim is false. One gate gap leaves part of the brief's verification claim unmet.

## Findings

- NEEDS-HUMAN [impl] — **Abort drops an in-flight commit outcome. This confirms the T4 blocker with a concrete case.** `crates/core/src/multipart/store.rs:749-751` overwrites `unknown` on every round, and `:731` returns the last one. I scripted the interposer as: round 1 `Unknown::Late` (held, `may_still_commit: true`), rounds 2-4 `Dropped { may_still_commit: false }`. `abort_session` returns `CommitUnknownResult { may_still_commit: false }`. A caller that trusts that flag re-reads once and sees `Open`. Round 1's held fence then commits, and the session becomes `Aborting`. My probe (`adv_abort_mixed_codes_must_not_report_out_of_flight`) printed `returned may_still_commit = false; settling re-read = Open` / `after the held fence lands: Aborting` and failed. `:753` has the same gap: a plain store fault after an in-flight round is returned as-is, so the in-flight fact is lost there too. `:747` (`unknown = None` on `Conflict`) is correct: the session's bytes moved, and epochs only grow, so the earlier fence can never land. Real-world impact is limited. The fence batch is atomic (session plus obligation), so C-1 holds. Only the error the caller (#668) gets is wrong. Fix: once a `may_still_commit: true` error is seen, keep it (as Create does at `:541-545`), including over a later plain fault while the session is unchanged. Add a mixed-code regression test.

- NEEDS-HUMAN [impl] — **Create's "keep the in-flight doubt" rule has no test, and C5's "0 missed" hides that.** `crates/core/src/multipart/store.rs:541-545` keeps an earlier `may_still_commit: true` outcome when a later commit reports `false`. I replaced the guard with an unconditional `*doubt = Some(unknown.clone())` (Abort's current behaviour). All 29 tests in `crates/core/tests/multipart_create_abort.rs` still passed. The concrete case that weakened code gets wrong: the tiny profile (`max_sessions = 2`), commit 1 `Unknown::Late`, commit 2 `Dropped { may_still_commit: false }`. Under the mutant the create answers `Ok(Refused(SlowDown { AdmissionContention { attempts: 2 } }))`, and the held batch then commits: an admitted, counted session that no client was told about. That is exactly the strand `0016:2009-2027` forbids. The patched code correctly answers `Unsettled` (my probe passes on the patch and fails on the mutant). cargo-mutants does not generate a "drop the `if`" mutant, so the C5 row (`73 mutants … 62 caught, 11 unviable`) says nothing about this line. The DST scenario cannot see it either: it arms one ambiguity code per run (`crates/dst/tests/multipart_admission.rs:190`), so outcomes are never mixed. It also accepts any `CommitUnknownResult` as `Exhausted` without checking the flag (`:172`), so it would not catch the Abort defect above even with mixing. Add the mixed-code Create regression next to the Abort one.

- NEEDS-HUMAN [impl] — **The rustdoc says a contention refusal is "never a lost race". It can come with room left in the ledger.** `crates/core/src/multipart/store.rs:291-296` and `:419-421` say every spent contention attempt is another session admitted. But `:395-397` also spends that budget on `Step::Unknown`. My probe (tiny profile; commit 1 `Dropped { may_still_commit: false }`; one rival admitted before commit 2) answered `SlowDown { AdmissionContention { attempts: 2 } }` with the ledger at **1/2**. At the shipped profile the same thing takes 45 rival admissions plus one unknown outcome. The behaviour itself was accepted at the iteration-1 sign-off (unknown outcomes must end the loop), and `05-building-block-view.md:202` already describes it correctly. Only these two rustdoc sentences contradict it. Doc fix only.

- NEEDS-HUMAN [human] — **C4-ci never reached the gates the brief relies on for (G) and ADR-0035.** `xtask/src/main.rs:1683-1687` runs `cargo_deny_check` before `run_conformance`, `run_statics`, `run_orchestrator_guard` and `run_dst`. Both C4-ci attempts stopped at the inherited RUSTSEC deny failure (`gate-logs/C4-ci.log:3578`, `:6603`). So the brief's claim that "`cargo xtask ci` sweeps it … so C4-ci runs it" did not happen this round. The madsim clippy leg over the new `crates/dst/tests/multipart_admission.rs`, the statics scan, and the custodian DST that drives the refactored `fence_session` were also never run by a gate. To narrow the gap I ran these by hand on the patched tree: madsim clippy on `wyrd-dst` (clean, `Checking wyrd-dst` confirmed); `multipart_admission` at `MADSIM_TEST_NUM=50` (4/4 pass); the 5 `restore_*` custodian DST tests at 50 seeds (pass); and 32 of 33 custodian DST tests (I stopped the last one, `committed_regression_seeds_stay_green`, after about 12 minutes). I did not run `cargo xtask statics` or conformance. By reading the code, `store.rs` adds no `static`, so I expect statics to pass. A human should decide whether to accept on that partial run, since the builder was told not to touch `Cargo.lock`/`deny.toml`.

## Attacks I tried that did not break the patch

- **(A′) red.** With `contention_budget` (`store.rs:429-432`) changed to return `MAX_UPLOAD_ID_ATTEMPTS`, both A′ tests and 5 others fail. So the forced-schedule red holds and does not depend on scheduler luck. The barrier interposer gives one winner per round (`multipart_create_abort.rs:499-526`, `:731-734`).
- **Create recovery orderings.** An in-doubt batch landing between `lost()`'s marker read and session read (`store.rs:594-603`), at the ledger read, after the re-drive, or on the last budgeted attempt: each ends `Created` once, or `Unsettled` naming the identity. `owns()` (`:245-254`) plus the vacant pre-read (`:517-522`) shut out the repeated-identity and squatter cases. I found no ordering that strands a session or double-counts.
- **Session-size boundary** (`multipart.rs` `widest_encoding`). It covers `Completing` naming the object twice and every number at its widest. It is measured on encoded bytes and tested at ±1 byte through a ceiling-enforcing store.
- **Restore refactor** (`crates/custodian/src/restore.rs:1051-1052`). `fence_batch` builds the same preconditions and puts as the removed inline code, and `mpu_key(parse_mpu_key(k)) == k` holds for a valid key. Restore unit tests passed in C4-ci's `cargo test` step, and the restore DST tests pass (above).
- **Abort decision table, (E) count invariance, and two racing Aborts.** I read them against `0016:977` and could not break them.

<!-- pdca:leaf-complete -->

### Advisory — code-review

- NEEDS-HUMAN [impl] — `crates/core/src/multipart/store.rs:751`: **Preserve outstanding Abort commit ambiguity.** If the first fence remains in flight (`may_still_commit: true`) and the next three attempts return unapplied unknown outcomes with `false`, while every re-read still finds `Open`, this assignment replaces the outstanding `true`. Exhaustion at `crates/core/src/multipart/store.rs:731` then returns `false`, incorrectly allowing a caller to treat another re-read as definitive although the first fence can still land afterward. A failing recovery read at `crates/core/src/multipart/store.rs:750` or a later ordinary commit error at `crates/core/src/multipart/store.rs:753` likewise discards unresolved ambiguity. Preserve the strongest outstanding outcome across retries and subsequent faults until the session state settles or invalidates the earlier fence, following Create's existing rule at `crates/core/src/multipart/store.rs:543`. Add mixed 1031→1021 and subsequent-fault regressions that release the held first batch after Abort returns.

No separate reuse, simplification, or efficiency findings. Reviewed target source and frozen gate evidence; no builds or tests were rerun.

<!-- pdca:leaf-complete -->

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C2 Reproduction (red pre-fix) — Accept the declared net-new verification posture — the stashed base cannot compile the test because the API is absent; three independently executed behavioral mutant reds support discrimination but do not constitute a pre-fix behavioral red; `brief.md:133`, `reviewer-red.log:3`.
- [ ] C4 Verification (red→green) — Resolve the sign-off disposition of the inherited all-features dependency gate outside this slice — local tests pass, but full CI remains red on unchanged rustls 0.21.12 advisories; `gate-logs/C4-ci.log:6525`, `Cargo.lock:3294`, `brief.md:356`.
- [ ] C5 Causal adequacy — Preserve earlier in-flight uncertainty across Abort retries and add a mixed-outcome regression — a later settled error currently conceals a fence that can still land after return (R1); `crates/core/src/multipart/store.rs:751`, `reviewer-abort-probe.log:20`.
- [ ] Validation — fitness-to-purpose — Decide whether this isolated API slice meets the intended milestone — core behavior is exercised, but production verbs and reclamation remain the deliberately separate #668/#659/#625 work; `brief.md:146`, `crates/core/src/multipart/store.rs:27`.
- [ ] C4-ci — Both attempts fail the all-features advisory audit for RUSTSEC-2026-0346/0349 through tikv-client’s rustls 0.21.12; preceding workspace build/tests pass. The unchanged dependency failure is inherited and explicitly outside this slice, not a new patch defect; `gate-logs/C4-ci.log:6525`, `brief.md:356`.
- [ ] C4-verify — Green executes 29 core and 4 DST tests; the reverted production cannot compile the test, so no behavioral pre-fix red was established; `gate-logs/C4-verify.log:12`, `gate-logs/C4-verify.log:21`.
- [ ] **Abort drops an in-flight commit outcome. This confirms the T4 blocker with a concrete case.** `crates/core/src/multipart/store.rs:749-751` overwrites `unknown` on every round, and `:731` returns the last one. I scripted the interposer as: round 1 `Unknown::Late` (held, `may_still_commit: true`), rounds 2-4 `Dropped { may_still_commit: false }`. `abort_session` returns `CommitUnknownResult { may_still_commit: false }`. A caller that trusts that flag re-reads once and sees `Open`. Round 1's held fence then commits, and the session becomes `Aborting`. My probe (`adv_abort_mixed_codes_must_not_report_out_of_flight`) printed `returned may_still_commit = false; settling re-read = Open` / `after the held fence lands: Aborting` and failed. `:753` has the same gap: a plain store fault after an in-flight round is returned as-is, so the in-flight fact is lost there too. `:747` (`unknown = None` on `Conflict`) is correct: the session's bytes moved, and epochs only grow, so the earlier fence can never land. Real-world impact is limited. The fence batch is atomic (session plus obligation), so C-1 holds. Only the error the caller (#668) gets is wrong. Fix: once a `may_still_commit: true` error is seen, keep it (as Create does at `:541-545`), including over a later plain fault while the session is unchanged. Add a mixed-code regression test.
- [ ] **Create's "keep the in-flight doubt" rule has no test, and C5's "0 missed" hides that.** `crates/core/src/multipart/store.rs:541-545` keeps an earlier `may_still_commit: true` outcome when a later commit reports `false`. I replaced the guard with an unconditional `*doubt = Some(unknown.clone())` (Abort's current behaviour). All 29 tests in `crates/core/tests/multipart_create_abort.rs` still passed. The concrete case that weakened code gets wrong: the tiny profile (`max_sessions = 2`), commit 1 `Unknown::Late`, commit 2 `Dropped { may_still_commit: false }`. Under the mutant the create answers `Ok(Refused(SlowDown { AdmissionContention { attempts: 2 } }))`, and the held batch then commits: an admitted, counted session that no client was told about. That is exactly the strand `0016:2009-2027` forbids. The patched code correctly answers `Unsettled` (my probe passes on the patch and fails on the mutant). cargo-mutants does not generate a "drop the `if`" mutant, so the C5 row (`73 mutants … 62 caught, 11 unviable`) says nothing about this line. The DST scenario cannot see it either: it arms one ambiguity code per run (`crates/dst/tests/multipart_admission.rs:190`), so outcomes are never mixed. It also accepts any `CommitUnknownResult` as `Exhausted` without checking the flag (`:172`), so it would not catch the Abort defect above even with mixing. Add the mixed-code Create regression next to the Abort one.
- [ ] **The rustdoc says a contention refusal is "never a lost race". It can come with room left in the ledger.** `crates/core/src/multipart/store.rs:291-296` and `:419-421` say every spent contention attempt is another session admitted. But `:395-397` also spends that budget on `Step::Unknown`. My probe (tiny profile; commit 1 `Dropped { may_still_commit: false }`; one rival admitted before commit 2) answered `SlowDown { AdmissionContention { attempts: 2 } }` with the ledger at **1/2**. At the shipped profile the same thing takes 45 rival admissions plus one unknown outcome. The behaviour itself was accepted at the iteration-1 sign-off (unknown outcomes must end the loop), and `05-building-block-view.md:202` already describes it correctly. Only these two rustdoc sentences contradict it. Doc fix only.
- [ ] **C4-ci never reached the gates the brief relies on for (G) and ADR-0035.** `xtask/src/main.rs:1683-1687` runs `cargo_deny_check` before `run_conformance`, `run_statics`, `run_orchestrator_guard` and `run_dst`. Both C4-ci attempts stopped at the inherited RUSTSEC deny failure (`gate-logs/C4-ci.log:3578`, `:6603`). So the brief's claim that "`cargo xtask ci` sweeps it … so C4-ci runs it" did not happen this round. The madsim clippy leg over the new `crates/dst/tests/multipart_admission.rs`, the statics scan, and the custodian DST that drives the refactored `fence_session` were also never run by a gate. To narrow the gap I ran these by hand on the patched tree: madsim clippy on `wyrd-dst` (clean, `Checking wyrd-dst` confirmed); `multipart_admission` at `MADSIM_TEST_NUM=50` (4/4 pass); the 5 `restore_*` custodian DST tests at 50 seeds (pass); and 32 of 33 custodian DST tests (I stopped the last one, `committed_regression_seeds_stay_green`, after about 12 minutes). I did not run `cargo xtask statics` or conformance. By reading the code, `store.rs` adds no `static`, so I expect statics to pass. A human should decide whether to accept on that partial run, since the builder was told not to touch `Cargo.lock`/`deny.toml`.
- [ ] `crates/core/src/multipart/store.rs:751`: **Preserve outstanding Abort commit ambiguity.** If the first fence remains in flight (`may_still_commit: true`) and the next three attempts return unapplied unknown outcomes with `false`, while every re-read still finds `Open`, this assignment replaces the outstanding `true`. Exhaustion at `crates/core/src/multipart/store.rs:731` then returns `false`, incorrectly allowing a caller to treat another re-read as definitive although the first fence can still land afterward. A failing recovery read at `crates/core/src/multipart/store.rs:750` or a later ordinary commit error at `crates/core/src/multipart/store.rs:753` likewise discards unresolved ambiguity. Preserve the strongest outstanding outcome across retries and subsequent faults until the session state settles or invalidates the earlier fence, following Create's existing rule at `crates/core/src/multipart/store.rs:543`. Add mixed 1031→1021 and subsequent-fault regressions that release the held first batch after Abort returns.
- [ ] C4 per-fix red->green: this patch's test red pre-fix, green post-fix unverifiable —                why this slice has no isolable red (the cargo output is above).
- [ ] C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance) FAILED (gating) — xtask: `cargo deny --all-features --config deny-all-features.toml check advisories` failed with exit status: 1
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 2 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_656/review-b
- [ ] **Create omits the bucket deletion fence.** `brief.md:162-170` enumerates the Create batch with only the bucket record required present and classifies only a gone bucket as `NoSuchBucket`; leg B (`brief.md:33-37`) tests only absence. On the target, `docs/design/proposals/draft/0016-multipart-commit-protocol.md:657` requires ADR-0046 §4's bucket-existence contract, and `docs/design/adr/0046-bucket-model-real-namespace.md:93-95` requires both exact marker bytes and absence of the deletion fence. The marker remains present while deletion is fenced (`:104-117`), so the proposed batch can admit into a deleting bucket. Specify both preconditions and add a deleting-bucket refusal/race case with no session, marker or counter writes.
- [ ] **Unknown Create outcomes lack a recoverable identity contract.** `brief.md:172` stops at “an unknown commit outcome is an `Err`,” while `:169` treats a taken upload id as a collision requiring re-minting. The normative proposal explicitly says unknown Create outcomes must recover with the **original upload id**, rebuilding the ledger precondition; erroring out or minting another id can strand an admitted session (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:2010-2027`). `crates/traits/src/lib.rs:240-247` confirms that an absent re-read cannot settle a commit that may still land. Define how the core API preserves/exposes the attempt identity and distinguishes recovery from a fresh-id collision, even if the gateway drives recovery later. Add a late-commit recovery scenario asserting one session and one increment; ordinary error propagation currently satisfies the brief without proving this property.
- [ ] **The required contention red is scheduler-dependent.** `brief.md:23-32,53-59,100-103` relies on multithreaded Tokio plus redb to demonstrate refusals when the contention budget is shortened. Target `crates/metadata-redb/src/lib.rs:113-117,208-234` implements reads and commits synchronously, without an internal await. Spawning 8/10/16 tasks does not ensure shared ledger reads, much less repeated losses by one task: a valid schedule can serialize them and let the shortened-budget mutant pass. Require a controlled read/commit schedule and assertions that a creator actually exceeds the collision-sized CAS budget, retaining the real-store checks. Include reproducible seeded coverage: target `AGENTS.md:188-190` requires it for new concurrent paths, but the brief specifies only the ordinary-runtime test.
- [ ] **The claimed shared fence batch builder is only a record builder.** `brief.md:117-118,206-207` says #841 already supplied the shared builder requested by the tracker. The first comment in `notes.json` says the client Abort and restore should use “one batch builder, not two.” Target `crates/core/src/multipart.rs:2360-2372,2426-2442` produces only a session/obligation pair; `crates/custodian/src/restore.rs:1050-1057` separately constructs the `WriteBatch`, including its preconditions. Reusing `open_teardown` alone therefore still duplicates transaction construction. Revise the scope to identify the shared batch-construction seam and the restore callsite change needed to use it, or explicitly surface the narrower record-builder interpretation for adjudication instead of claiming the tracker requirement is already implemented.
- [ ] size backstop — this slice is behaving oversized: patch is 202 KB (threshold 100 KB). Recommend answering `iterate-plan` at sign-off and authoring the split in the re-plan (`pdca split`), rather than `iterate-do`: a slice that is too big yields implementation-shaped findings every round, and splitting authors briefs, which is Plan's beat.

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
- Iteration delta (if iterating): Shape, scope and slice size are right (round 1 ruling stands; the 202 KB size backstop is not a slicing signal here). One reproduced bug and two gaps block accept: 1. R1 — Abort drops in-flight commit ambiguity (crates/core/src/multipart/store.rs:749-753). Once a CommitUnknownResult with may_still_commit: true is seen, keep it across later unknown outcomes with may_still_commit: false, a failing recovery read, and a later plain store fault, while the session is unchanged — the same rule Create already applies at store.rs:541-545. Conflict (:747, session bytes moved) may still clear it. Add mixed 1031 -> 1021 regressions and a subsequent-plain-fault regression that release the held first batch AFTER Abort returns and assert the returned error still says may_still_commit: true. 2. Create's keep-the-doubt rule (store.rs:541-545) has no test: replacing it with an unconditional assignment passes all 29 tests. Add the mixed-code Create regression (tiny profile, commit 1 Unknown::Late, commit 2 Dropped{may_still_commit:false}; must answer Unsettled, not a contention refusal). Consider making the DST scenario check the may_still_commit flag (crates/dst/tests/multipart_admission.rs:172) and mix ambiguity codes within a run. 3. Rustdoc at store.rs:291-296 and :419-421 claims a contention refusal is never a lost race / always a full ledger; an unknown outcome also spends that budget (:395-397). Fix the wording to match 05-building-block-view.md:202. Doc only, no behaviour change. C4-ci's cargo-deny failure (RUSTSEC-2026-0346/0349, rustls 0.21.12 via tikv-client) is inherited from main; the waiver goes via Act. Do not touch Cargo.lock or deny*.toml.
- By / date: Eduard Ralph / 2026-10-14

## 10. Act candidates (hints for the next Act review)
- Plan advisory: 4 finding(s); brief revised: yes (plan-advisory-*.md)
- (empty is the common case)
