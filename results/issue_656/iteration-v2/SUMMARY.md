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
- C4 diff coverage: changed lines executed by the patch's tests: pass — diff coverage 99.7% — 297 of 298 instrumentable changed lines executed (floor 80%); 298 of 809 changed lines were instru
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): pass — 56 mutants tested in 4m: 45 caught, 11 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 2 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_656/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 23.66s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Review of #656’s multipart Create admission and Abort fencing: two implementation defects reproduced.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The amended brief defines observable admission, ambiguity recovery and teardown invariants, with gateway wiring excluded; brief.md:24, brief.md:115, brief.md:336. |
| C2 Reproduction (red pre-fix) | NEEDS-HUMAN | Accept the declared net-new API exception — independently stashing production changes produces E0432 before any test executes, although all three required behavioral mutations fail; review-red.log:2, review-mutants.log:16, review-mutants.log:61, review-mutants.log:84. |
| C3 Change | PASS | All six changed paths serve the agreed core/store, shared restore-fence, tests and living-doc scope; no production Create/Abort caller was found; crates/custodian/src/restore.rs:1052, docs/design/architecture/05-building-block-view.md:202. |
| C4 Verification (red→green) | NEEDS-HUMAN | Resolve the inherited optional-TiKV advisory failure and obtain a completed full gate run before claiming green verification — normal workspace tests pass, but the unchanged dependency graph still fails its all-features audit; review-ci-final.log:2936, review-deny.log:7, gate-logs/C4-ci.log:6516. |
| C5 Causal adequacy | NEEDS-HUMAN [impl] | Fix admission’s missing transition-size reserve and recovery’s unsupported ownership inference — an admitted upload can become unfenceable or a new request can receive an existing upload’s ID; crates/core/src/multipart/store.rs:493, crates/core/src/multipart/store.rs:404. |
| T1 Structure | PASS | The narrow MetadataStore seam, caller-owned time/randomness/wait, and single fence-batch builder preserve the architecture; crates/core/src/multipart/store.rs:85, crates/core/src/multipart/store.rs:694, crates/custodian/src/restore.rs:1052. |
| T2 Shape | FAIL | An accepted 100,000-byte Open record needs 100,004 bytes for its first Abort, exceeding the backend value ceiling and preventing normal teardown; crates/core/src/multipart/store.rs:493, crates/core/src/multipart.rs:2435, review-probes.log:62. |
| T3 Runtime | FAIL | A repeated identity followed by an unapplied unknown commit returns the old upload as Created with the ledger still at one; crates/core/src/multipart/store.rs:400, review-probes.log:54. |
| T4 Contribution | FAIL | The two frozen batch-review findings remain actionable and both reproduce independently; prior art was checked by affected path, while the contribution-artifact audit is deferred to publish; gate-logs/T4-batch-review.log:10, review-prior-art-summary.txt:1. |
| T5 Judgment | NEEDS-HUMAN [impl] | Add transition-size and collision-plus-ambiguity regressions — the current passing suite and mutation score do not cover these combined failure conditions; crates/core/tests/multipart_create_abort.rs:1157, crates/core/tests/multipart_create_abort.rs:1761, review-probes.log:75. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether this API-only slice is fit to hand to later callers after the defects are fixed — Redb and simulated-FDB behavior was exercised, but live FDB/TiKV behavior and user-visible S3 integration were not; brief.md:147, crates/core/src/multipart/store.rs:27. |

Source citations above and below are relative to the supplied `$PDCA_TARGET`; evidence files are relative to this report. The target matches the patch, the stashed changes were restored byte-for-byte, and `git apply --reverse --check ../patch.diff` succeeds. Findings are advisory; deterministic gates and human sign-off retain their own authority.

1. **Reserve encoded space for every required session transition before admission.** `create_batch` writes the unrestricted request strings without measuring the future record (`crates/core/src/multipart/store.rs:493`; construction at `crates/core/src/multipart.rs:2338`). I sized `content_type` so the actual encoded Open session was exactly `MAX_VALUE_BYTES` (100,000, `crates/core/src/metadata.rs:549). Create succeeded through a Redb wrapper enforcing that ceiling. Its normal `open_teardown` replacement encoded to 100,004 bytes, so Abort failed and the persisted record remained Open with admission count one (`review-probes.log:62`). Restore uses the same fence builder and cannot repair this by fencing either. This measures real encodings and enforces the repository’s documented backend bound; it is not a claim of running a live FoundationDB service. Reject requests that cannot retain transition headroom before writing anything, and cover the encoded boundary rather than just character length.

2. **An unknown result does not establish ownership of a matching pre-existing session.** Recovery claims a found record using nonce, parent and object alone (`crates/core/src/multipart/store.rs:200`, `:404`; the settling read has the same assumption at `:374`). I first created an upload, repeated its ID and nonce for the same object, and returned an unapplied `CommitUnknownResult { may_still_commit: true }` for the second invocation’s first commit. The second invocation returned the first upload’s ID, minted no replacement, and left the ledger at one; its distinct requested content type was never installed (`review-probes.log:54`). This is a valid seam outcome: TiKV’s deadline wraps `commit_inner`, including transaction start and precondition reads, so it can report unknown before checking the colliding key (`crates/metadata-tikv/src/lib.rs:1363`, `:1378`). Resolve existing collisions before recovery can claim ownership, and add this combination to the separate collision and ambiguity tests.

The independent checks establish substantial coverage, but not complete correctness:

- `cargo test --offline -p wyrd-core --test multipart_create_abort`: 26 passed; after stash/pop, 26 passed again (`review-green-restored.log:33`). The retained test on the pre-fix production tree did not compile; that is not a behavioral red.
- In an isolated copy, a collision-sized contention budget failed forced admission, re-minting after an unknown result produced two sessions, and a canonical decrement on Abort tripped the explicit count assertion, 0 versus 1 (`review-mutants.log:16`, `:61`, `:84`). The first decrement experiment changed JSON field order and failed canonical decoding; only the corrected canonical experiment is counted as evidence for leg E. The judged source was not changed by these mutations.
- `RUSTFLAGS="--cfg madsim" MADSIM_TEST_NUM=50 cargo test --offline -p wyrd-dst --test multipart_admission`: all four tests passed, including the two explicit 64-seed fate sweeps (`review-dst.log:240`). Conformance passed all 5 valid and 6 invalid vectors, and the statics scanner passed (`review-conformance.log:1`, `review-statics.log:3`).
- The additional full `cargo xtask dst` run passed its DST clippy step and earlier suites, then remained in the pre-existing custodian regression campaign. I interrupted it after about seven minutes (exit 130); the whole-repository DST sweep is therefore **not verified**, and this interruption is not a patch failure (`review-dst-full.log:269`, `:301`). The four new multipart tests had already completed separately with the required seed setting.
- `cargo xtask ci` passed typos, docs lint/render/link audit, repository guards, formatting, workspace clippy, build, tests and cargo-machete. The final local run stopped at a read-only advisory-cache lock (`review-ci-final.log:2945`). Copying the two policies unchanged except for a writable advisory database location discharged that host issue: the default dependency audit passed, and the all-features audit reproduced RUSTSEC-2026-0346 and RUSTSEC-2026-0349 through rustls 0.21.12/TiKV (`review-deny.log:3`, `:7`). Cargo manifests and lockfile are unchanged by this patch. This is an inherited gate failure, not a new C4 patch defect; searching the repository tracker by both advisory IDs found no existing issue (`review-advisory-tracking.json:1`).

The brief’s named external tools, `typos` and `docs-renderer`, were both exercised; neither was skipped (`review-ci-final.log:2`, `:7`). To repeat the two independent defect probes from this review directory, run `cargo test --offline --manifest-path pdca-reviewer-656-probes/Cargo.toml reviewer_ -- --nocapture`; both assertions currently fail.

The frozen rows were adjudicated from their captured output, not from their result labels alone:

| Gate | Verdict | Evidence |
|------|---------|----------|
| C4-ci | NEEDS-HUMAN | The captured run stops at the inherited rustls advisories, before conformance/statics/DST; the local rerun confirms the advisory failure. gate-logs/C4-ci.log:6515; xtask/src/main.rs:1683. |
| C4-verify | NEEDS-HUMAN | Green executes 26 core and 4 DST tests; reverted production fails compilation and exits UNVERIFIABLE, matching the independently repeated result. gate-logs/C4-verify.log:14, :23, :31. |
| C4-diff-cov | PASS | Captured coverage is 297/298 instrumentable changed lines (99.7%); 511 changed lines are unscored, so this does not prove the missing boundary cases. gate-logs/C4-diff-cov.log:584, :598. |
| C5-mutants | PASS | Captured run reports 45 caught and 11 unviable out of 56; no survivors are reported. Three required targeted mutations were also independently caught. gate-logs/C5-mutants.log:10. |
| T4-batch-review | FAIL | Both reported defects ground on current source and reproduce; no rejected or noise finding is being revived. gate-logs/T4-batch-review.log:10. |
| T4-contribution | N/A | PR description is absent by design at Check; the substantive contribution audit reruns at publish. gate-logs/T4-contribution.log:10. |
| host-tikv | PASS | Captured compiler output completes both feature-enabled clippy commands; it demonstrates compilation, not a live TiKV service test. gate-logs/host-tikv.log:2, :211. |

Prior art was checked against all six affected paths at the specified base, including merged history and closed/rejected work. The merged budget/knob and restore-fence predecessors are present; the three added paths have no earlier base history. Closed PR file lists were matched to the same paths, and #508’s rejected monoliths, closed/not-planned #636, and closed/unmerged #647 were examined (`review-prior-art-summary.txt:1`; full query results in `review-prior-art.json`). The separate retry budgets and limited scope address those recorded rejection reasons. The #511 bucket-fence deferral and the prior rejection of core-owned timeouts remain settled (`crates/core/src/multipart/store.rs:483`; brief.md:342). No optional-capability probe or load-time symptom guard was introduced.

<!-- pdca:leaf-complete -->

### Advisory — adversary

# Adversarial review — #656 multipart admission (Create) and Abort fence

I applied the patch in a scratch copy of the target and ran my own probe tests over
`RedbMetadataStore`. Both blocking findings from T4-batch-review reproduce as concrete failing
cases (first two bullets). The required reds hold up, and so do the parts I could not break
(last bullet).

- NEEDS-HUMAN [impl] — **An identity repeated under an unknown outcome is handed another upload's id.** `crates/core/src/multipart/store.rs:196-199` claims a caller whose identity source repeats an id and nonce "must be re-minted, never handed another upload's id". That claim fails as soon as the repeat's first commit is in doubt. `owns()` (`store.rs:200-206`) compares only nonce, parent and object, so the doubt branch (`store.rs:400-407`) takes the *other* client's session as its own. Probe: client A creates `photos/same` under (X, N1) at t=1000. Client B, a different request at t=2000, mints the same (X, N1), and its first commit returns `CommitUnknownResult { may_still_commit: true }` without applying (a TiKV RPC timeout). Result: `B = Ok(Created { upload_id: …a1 })`, the same id as A, with `mpuctl.count = 1`. B was never admitted, and the two clients now share one upload. The existing tests miss it: `c_an_identity_repeated_for_the_same_object_…` covers only the case with nothing in doubt, and `f_in_doubt_a_session_differing_in_one_field_…` only covers squatters that differ in nonce, object or parent. Cheap fix that I checked: make `owns()` also compare the fields a request fixes and no transition changes (`created_at_millis`, `clock_source`, `content_type`). B then ends `Unsettled(Collided)` instead of taking A's id, and all 26 of the patch's tests stay green. A fuller fix is to read `mpu:<id>` once before a fresh identity's first commit.

- NEEDS-HUMAN [human] — **Create can admit a session that can never be fenced, which leaks its admission slot and stops the restore pass.** Create puts the encoded session with no size headroom (`store.rs:493`, built by `SessionRecord::open`, `crates/core/src/multipart.rs:2338`). Probe: an object name padded so the `Open@0` record is exactly `MAX_VALUE_BYTES` (100,000 bytes) is admitted. Its `Aborting@1` replacement is 100,004 bytes. Through a wrapper that refuses puts over 100,000 bytes (what FoundationDB's `2103 value_too_large` does), every Abort returns the store fault (`store.rs:672`), and the count stays at 2 for good. The restore fence uses the same teardown and propagates that fault with `?` (`crates/custodian/src/restore.rs:1018`). So one such session halts the whole post-restore session fence, and every session listed after it stays unfenced. Complete (#658) needs even more headroom than Abort: the `Completing` state repeats the object name in `publish_target` (`multipart.rs:2538`), so an object name over about 50 KB fits as `Open` but can never be Completed. The repo's own rule (`crates/core/src/metadata.rs:555-562`) is that a record which fits today but cannot be re-written is a durability hazard, which is why roots reserve half the ceiling. Sessions reserve nothing. No caller exists before #668, so a human has to pick one: bound the request strings in core now (no `Refusal` variant fits, so this needs a new fault or a new refusal), or record a tracked deferral that obliges #668 to cap key, content-type and clock-source lengths.

- NEEDS-HUMAN [impl] — **Two docs now overstate the contention bound.** `crates/core/src/multipart.rs:5078-5079` still says a create that spends its whole contention budget waits at most `MAX_ADMISSION_CAS_ATTEMPTS × 50 ms` (2.3 s). The patch made the budget the profile's own `max_sessions` (`store.rs:291`), and the clamp allows up to `SCAN_CAP/2`, so the wait bound now scales with the profile. Separately, `docs/design/architecture/05-building-block-view.md:202` says "a refusal for contention means the ledger filled". But an unknown outcome also spends that budget (`store.rs:322-323`). A store that answers every commit with an unapplied 1021 therefore refuses `AdmissionContention` on an empty ledger. Both are wording fixes.

- NEEDS-HUMAN [human] — **C4-ci is red for a reason outside this diff, and it stopped before steps this diff needs.** The failure is `cargo deny … check advisories`: RUSTSEC-2026-0349 in `rustls 0.21.12`, pulled in through `tikv-client 0.4.0` (`gate-logs/C4-ci.log`, last 50 lines). The patch does not touch `Cargo.lock`. Because `run_ci` stops at deny, CI never ran `statics`, the madsim `clippy -p wyrd-dst`, or the DST sweep. I ran all three on the patched copy. `xtask statics` reports no shared global state, the madsim clippy pass over `wyrd-dst --all-targets` is clean, and `multipart_admission` passes 4/4 at `MADSIM_TEST_NUM=50`. A human still has to decide how to clear the advisory gate (a separate dependency bump or a `deny.toml` ignore), since that is out of this slice's scope. I did not run `conformance` (chunk-format vectors, which this diff does not touch).

- **Attempted to refute these and could not:**
  - **(A′) red.** With the contention budget set to `MAX_UPLOAD_ID_ATTEMPTS` (`store.rs:291`), both `a_prime_*` tests fail. Restored, they pass. The barrier forces the losing order, so this red does not depend on luck.
  - **(F) red.** With a re-mint after `Step::Unknown`, all six `f_*` tests fail.
  - **Restore refactor.** It keeps restore's behaviour. `fence_batch` rebuilds the key as `mpu_key(parse_mpu_key(key))`, which gives back the same bytes because `UploadId` stores the string as given (`multipart.rs:1236-1244`). The preconditions and puts are in the same order as before.
  - **Abort's damage verdict.** `ObligationKeyTaken` is concluded from "the session bytes did not change" without reading the obligation key. That is sound: `Conflict` means exactly "a precondition failed" (`crates/metadata-fdb/src/lib.rs:17`), and epochs only increase, so session bytes cannot change and then change back.
  - **No production caller.** Nothing outside tests calls Create or Abort; `restore.rs` imports only `fence_batch`.
<!-- pdca:leaf-complete -->

### Advisory — code-review

- NEEDS-HUMAN [impl] — `crates/core/src/multipart/store.rs:493`: Create persists unrestricted request strings without reserving room for later session states. Padding `content_type` until the canonical `Open@0` value is exactly 100,000 bytes produces an admissible FoundationDB value, but `open_teardown` makes its `Aborting@1` replacement 100,004 bytes, exceeding `MAX_VALUE_BYTES` (`crates/core/src/metadata.rs:549`). Both Abort and restore fencing then fail, leaving the session counted and unable to enter teardown. Check the encoded admission size with lifecycle headroom and add a boundary regression using a size-enforcing store.

- NEEDS-HUMAN [impl] — `crates/core/src/multipart/store.rs:404`: An unknown commit outcome does not establish ownership of a matching session. If the identity source repeats an existing upload's ID and nonce for the same object, TiKV can time out before checking preconditions: its deadline wraps all of `commit_inner` (`crates/metadata-tikv/src/lib.rs:1363`). Recovery then returns the existing upload as `Created`, without creating or counting a new session; it can even return an already-aborted upload. Distinguish pre-existing collisions from this invocation's committed session, and combine the repeated-identity regression with an unapplied unknown outcome.

No additional reuse, simplification, or efficiency findings.
<!-- pdca:leaf-complete -->

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C2 Reproduction (red pre-fix) — Accept the declared net-new API exception — independently stashing production changes produces E0432 before any test executes, although all three required behavioral mutations fail; review-red.log:2, review-mutants.log:16, review-mutants.log:61, review-mutants.log:84.
- [ ] C4 Verification (red→green) — Resolve the inherited optional-TiKV advisory failure and obtain a completed full gate run before claiming green verification — normal workspace tests pass, but the unchanged dependency graph still fails its all-features audit; review-ci-final.log:2936, review-deny.log:7, gate-logs/C4-ci.log:6516.
- [ ] C5 Causal adequacy — Fix admission’s missing transition-size reserve and recovery’s unsupported ownership inference — an admitted upload can become unfenceable or a new request can receive an existing upload’s ID; crates/core/src/multipart/store.rs:493, crates/core/src/multipart/store.rs:404.
- [ ] T5 Judgment — Add transition-size and collision-plus-ambiguity regressions — the current passing suite and mutation score do not cover these combined failure conditions; crates/core/tests/multipart_create_abort.rs:1157, crates/core/tests/multipart_create_abort.rs:1761, review-probes.log:75.
- [ ] Validation — fitness-to-purpose — Decide whether this API-only slice is fit to hand to later callers after the defects are fixed — Redb and simulated-FDB behavior was exercised, but live FDB/TiKV behavior and user-visible S3 integration were not; brief.md:147, crates/core/src/multipart/store.rs:27.
- [ ] C4-ci — The captured run stops at the inherited rustls advisories, before conformance/statics/DST; the local rerun confirms the advisory failure. gate-logs/C4-ci.log:6515; xtask/src/main.rs:1683.
- [ ] C4-verify — Green executes 26 core and 4 DST tests; reverted production fails compilation and exits UNVERIFIABLE, matching the independently repeated result. gate-logs/C4-verify.log:14, :23, :31.
- [ ] **An identity repeated under an unknown outcome is handed another upload's id.** `crates/core/src/multipart/store.rs:196-199` claims a caller whose identity source repeats an id and nonce "must be re-minted, never handed another upload's id". That claim fails as soon as the repeat's first commit is in doubt. `owns()` (`store.rs:200-206`) compares only nonce, parent and object, so the doubt branch (`store.rs:400-407`) takes the *other* client's session as its own. Probe: client A creates `photos/same` under (X, N1) at t=1000. Client B, a different request at t=2000, mints the same (X, N1), and its first commit returns `CommitUnknownResult { may_still_commit: true }` without applying (a TiKV RPC timeout). Result: `B = Ok(Created { upload_id: …a1 })`, the same id as A, with `mpuctl.count = 1`. B was never admitted, and the two clients now share one upload. The existing tests miss it: `c_an_identity_repeated_for_the_same_object_…` covers only the case with nothing in doubt, and `f_in_doubt_a_session_differing_in_one_field_…` only covers squatters that differ in nonce, object or parent. Cheap fix that I checked: make `owns()` also compare the fields a request fixes and no transition changes (`created_at_millis`, `clock_source`, `content_type`). B then ends `Unsettled(Collided)` instead of taking A's id, and all 26 of the patch's tests stay green. A fuller fix is to read `mpu:<id>` once before a fresh identity's first commit.
- [ ] **Create can admit a session that can never be fenced, which leaks its admission slot and stops the restore pass.** Create puts the encoded session with no size headroom (`store.rs:493`, built by `SessionRecord::open`, `crates/core/src/multipart.rs:2338`). Probe: an object name padded so the `Open@0` record is exactly `MAX_VALUE_BYTES` (100,000 bytes) is admitted. Its `Aborting@1` replacement is 100,004 bytes. Through a wrapper that refuses puts over 100,000 bytes (what FoundationDB's `2103 value_too_large` does), every Abort returns the store fault (`store.rs:672`), and the count stays at 2 for good. The restore fence uses the same teardown and propagates that fault with `?` (`crates/custodian/src/restore.rs:1018`). So one such session halts the whole post-restore session fence, and every session listed after it stays unfenced. Complete (#658) needs even more headroom than Abort: the `Completing` state repeats the object name in `publish_target` (`multipart.rs:2538`), so an object name over about 50 KB fits as `Open` but can never be Completed. The repo's own rule (`crates/core/src/metadata.rs:555-562`) is that a record which fits today but cannot be re-written is a durability hazard, which is why roots reserve half the ceiling. Sessions reserve nothing. No caller exists before #668, so a human has to pick one: bound the request strings in core now (no `Refusal` variant fits, so this needs a new fault or a new refusal), or record a tracked deferral that obliges #668 to cap key, content-type and clock-source lengths.
- [ ] **Two docs now overstate the contention bound.** `crates/core/src/multipart.rs:5078-5079` still says a create that spends its whole contention budget waits at most `MAX_ADMISSION_CAS_ATTEMPTS × 50 ms` (2.3 s). The patch made the budget the profile's own `max_sessions` (`store.rs:291`), and the clamp allows up to `SCAN_CAP/2`, so the wait bound now scales with the profile. Separately, `docs/design/architecture/05-building-block-view.md:202` says "a refusal for contention means the ledger filled". But an unknown outcome also spends that budget (`store.rs:322-323`). A store that answers every commit with an unapplied 1021 therefore refuses `AdmissionContention` on an empty ledger. Both are wording fixes.
- [ ] **C4-ci is red for a reason outside this diff, and it stopped before steps this diff needs.** The failure is `cargo deny … check advisories`: RUSTSEC-2026-0349 in `rustls 0.21.12`, pulled in through `tikv-client 0.4.0` (`gate-logs/C4-ci.log`, last 50 lines). The patch does not touch `Cargo.lock`. Because `run_ci` stops at deny, CI never ran `statics`, the madsim `clippy -p wyrd-dst`, or the DST sweep. I ran all three on the patched copy. `xtask statics` reports no shared global state, the madsim clippy pass over `wyrd-dst --all-targets` is clean, and `multipart_admission` passes 4/4 at `MADSIM_TEST_NUM=50`. A human still has to decide how to clear the advisory gate (a separate dependency bump or a `deny.toml` ignore), since that is out of this slice's scope. I did not run `conformance` (chunk-format vectors, which this diff does not touch).
- [ ] `crates/core/src/multipart/store.rs:493`: Create persists unrestricted request strings without reserving room for later session states. Padding `content_type` until the canonical `Open@0` value is exactly 100,000 bytes produces an admissible FoundationDB value, but `open_teardown` makes its `Aborting@1` replacement 100,004 bytes, exceeding `MAX_VALUE_BYTES` (`crates/core/src/metadata.rs:549`). Both Abort and restore fencing then fail, leaving the session counted and unable to enter teardown. Check the encoded admission size with lifecycle headroom and add a boundary regression using a size-enforcing store.
- [ ] `crates/core/src/multipart/store.rs:404`: An unknown commit outcome does not establish ownership of a matching session. If the identity source repeats an existing upload's ID and nonce for the same object, TiKV can time out before checking preconditions: its deadline wraps all of `commit_inner` (`crates/metadata-tikv/src/lib.rs:1363`). Recovery then returns the existing upload as `Created`, without creating or counting a new session; it can even return an already-aborted upload. Distinguish pre-existing collisions from this invocation's committed session, and combine the repeated-identity regression with an unapplied unknown outcome.
- [ ] C4 per-fix red->green: this patch's test red pre-fix, green post-fix unverifiable —                why this slice has no isolable red (the cargo output is above).
- [ ] C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance) FAILED (gating) — xtask: `cargo deny --all-features --config deny-all-features.toml check advisories` failed with exit status: 1
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 2 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_656/review-b
- [ ] **Create omits the bucket deletion fence.** `brief.md:162-170` enumerates the Create batch with only the bucket record required present and classifies only a gone bucket as `NoSuchBucket`; leg B (`brief.md:33-37`) tests only absence. On the target, `docs/design/proposals/draft/0016-multipart-commit-protocol.md:657` requires ADR-0046 §4's bucket-existence contract, and `docs/design/adr/0046-bucket-model-real-namespace.md:93-95` requires both exact marker bytes and absence of the deletion fence. The marker remains present while deletion is fenced (`:104-117`), so the proposed batch can admit into a deleting bucket. Specify both preconditions and add a deleting-bucket refusal/race case with no session, marker or counter writes.
- [ ] **Unknown Create outcomes lack a recoverable identity contract.** `brief.md:172` stops at “an unknown commit outcome is an `Err`,” while `:169` treats a taken upload id as a collision requiring re-minting. The normative proposal explicitly says unknown Create outcomes must recover with the **original upload id**, rebuilding the ledger precondition; erroring out or minting another id can strand an admitted session (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:2010-2027`). `crates/traits/src/lib.rs:240-247` confirms that an absent re-read cannot settle a commit that may still land. Define how the core API preserves/exposes the attempt identity and distinguishes recovery from a fresh-id collision, even if the gateway drives recovery later. Add a late-commit recovery scenario asserting one session and one increment; ordinary error propagation currently satisfies the brief without proving this property.
- [ ] **The required contention red is scheduler-dependent.** `brief.md:23-32,53-59,100-103` relies on multithreaded Tokio plus redb to demonstrate refusals when the contention budget is shortened. Target `crates/metadata-redb/src/lib.rs:113-117,208-234` implements reads and commits synchronously, without an internal await. Spawning 8/10/16 tasks does not ensure shared ledger reads, much less repeated losses by one task: a valid schedule can serialize them and let the shortened-budget mutant pass. Require a controlled read/commit schedule and assertions that a creator actually exceeds the collision-sized CAS budget, retaining the real-store checks. Include reproducible seeded coverage: target `AGENTS.md:188-190` requires it for new concurrent paths, but the brief specifies only the ordinary-runtime test.
- [ ] **The claimed shared fence batch builder is only a record builder.** `brief.md:117-118,206-207` says #841 already supplied the shared builder requested by the tracker. The first comment in `notes.json` says the client Abort and restore should use “one batch builder, not two.” Target `crates/core/src/multipart.rs:2360-2372,2426-2442` produces only a session/obligation pair; `crates/custodian/src/restore.rs:1050-1057` separately constructs the `WriteBatch`, including its preconditions. Reusing `open_teardown` alone therefore still duplicates transaction construction. Revise the scope to identify the shared batch-construction seam and the restore callsite change needed to use it, or explicitly surface the narrower record-builder interpretation for adjudication instead of claiming the tracker requirement is already implemented.
- [ ] size backstop — this slice is behaving oversized: patch is 169 KB (threshold 100 KB). Recommend answering `iterate-plan` at sign-off and authoring the split in the re-plan (`pdca split`), rather than `iterate-do`: a slice that is too big yields implementation-shaped findings every round, and splitting authors briefs, which is Plan's beat.

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
- Iteration delta (if iterating): Shape, scope and slice size are right (most of the 172 KB patch is tests); round 1's five bugs are fixed. Two reproduced bugs in crates/core/src/multipart/store.rs still block accept. Fix each and keep a regression test for it: 1. Transition-size headroom (store.rs:493): Create admits an Open record up to MAX_VALUE_BYTES, but its Aborting@1 replacement is 4 bytes larger, so Abort and the restore fence fail forever and the slot leaks. Complete's publish_target needs more again. Reject at Create, before writing anything, any request whose encoded session cannot fit every later state (Aborting, Completing) under MAX_VALUE_BYTES. Add a boundary test through a size-enforcing store wrapper, measured on encoded bytes, not character length. 2. Ownership under an unknown outcome (store.rs:196-206, :374, :400-407): owns() compares only nonce, parent and object, so a repeated id+nonce whose first commit returns an unapplied CommitUnknownResult is handed the existing upload's id as Created, with no count increment. Make owns() also compare created_at_millis, clock_source and content_type (or read mpu:<id> before a fresh identity's first commit). Add a collision-plus-unknown-outcome regression. Also fix two doc wordings: multipart.rs:5078-5079 (the contention wait bound now scales with the profile's max_sessions) and 05-building-block-view.md:202 (a contention refusal can come from repeated unknown outcomes, not only a full ledger). C4-ci's cargo-deny failure (RUSTSEC-2026-0346/0349, rustls 0.21.12 via tikv-client) is inherited and outside this slice; do not touch Cargo.lock or deny.toml here.
- By / date: Eduard Ralph / 2026-10-14

## 10. Act candidates (hints for the next Act review)
- Plan advisory: 4 finding(s); brief revised: yes (plan-advisory-*.md)
- (empty is the common case)
