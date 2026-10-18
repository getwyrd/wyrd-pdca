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
- C4 diff coverage: changed lines executed by the patch's tests: pass — diff coverage 99.7% — 382 of 383 instrumentable changed lines executed (floor 80%); 383 of 1014 changed lines were instr
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): pass — 79 mutants tested in 4m: 67 caught, 12 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 1 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_656/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 13.21s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Review #656’s multipart Create admission and Abort fencing: one independently reproduced Abort correctness defect remains.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | Admission accounting, original-identity recovery, and atomic teardown have explicit falsifiable criteria and bounded scope; `brief.md:24`, `brief.md:113`, `docs/design/proposals/draft/0016-multipart-commit-protocol.md:657`, `docs/design/proposals/draft/0016-multipart-commit-protocol.md:665`. |
| C2 Reproduction (red pre-fix) | NEEDS-HUMAN | Accept or discharge the declared net-new red-evidence exception — the independently stashed base fails to compile before any assertion, and the three specified hand-mutant reds are not individually evidenced in the supplied logs; `brief.md:133`, `reviewer-red.log:174`, `gate-logs/C5-mutants.log:13`. |
| C3 Change | PASS | The patch matches the disposable target and stays within the authorized writers, shared fence, tests, and living documentation; reverse-apply validation succeeds and restoration preserves source hashes; `reviewer-grounding.log:1`, `reviewer-verification.log:22`, `crates/custodian/src/restore.rs:1052`. |
| C4 Verification (red→green) | NEEDS-HUMAN | Resolve the inherited all-features dependency-audit failure through Act — the feature and restore reruns pass, but full CI remains red and no behavioral pre-fix red exists; `gate-logs/C4-ci.log:6531`, `brief.md:369`, `reviewer-restored-green.log:42`, `reviewer-dst.log:314`, `reviewer-restore.log:88`. |
| C5 Causal adequacy | NEEDS-HUMAN [impl] | Correct R1’s inference that an unchanged session after Conflict proves an occupied obligation — it can falsely report damage and discard a fence still able to commit; `crates/core/src/multipart/store.rs:797`, `crates/metadata-tikv/src/lib.rs:1391`, `reviewer-abort-conflict.log:13`. |
| T1 Structure | PASS | Runtime ownership and dependency direction remain at the intended seams; both teardown writers share the exact-precondition batch, and the statics scanner passes; `crates/core/src/multipart/store.rs:85`, `crates/core/src/multipart/store.rs:833`, `crates/custodian/src/restore.rs:1052`, `reviewer-scanners.log:4`. |
| T2 Shape | PASS | The six affected paths fit the accepted store-only slice; production callers and frozen design documents are untouched, while the living architecture describes the new operations; `brief.md:365`, `crates/core/src/multipart/store.rs:27`, `docs/design/architecture/05-building-block-view.md:202`. |
| T3 Runtime | FAIL | Abort’s observable error contract fails under backend lock contention: the independent tests return ObligationKeyTaken with no obligation, including before a held fence subsequently commits; `crates/core/src/multipart/store.rs:801`, `reviewer-abort-conflict.log:7`, `reviewer-abort-conflict.log:13`. |
| T4 Contribution | NEEDS-HUMAN | Confirm the affected-path merged and closed/rejected prior-art check — the brief records one, but this snapshot has only one synthetic commit and no remote to corroborate it; `brief.md:172`, `reviewer-grounding.log:5`; the publish-artifact audit itself is N/A by design (`gate-logs/T4-contribution.log:10`). |
| T5 Judgment | PASS | The accepted scope and tracked deferrals remain settled; encoded-size boundaries and mixed-outcome preservation now have executing regressions, with no capability probe masking a load-time cause; `brief.md:365`, `crates/core/tests/multipart_create_abort.rs:1280`, `crates/core/tests/multipart_create_abort.rs:2335`, `crates/core/tests/multipart_create_abort.rs:2765`, `crates/core/src/multipart/store.rs:569`. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether the evidenced admission and teardown behavior fulfills #656 after R1’s disposition — passing store tests cannot establish product fitness, and this approved slice has no production Create/Abort caller; `brief.md:146`, `crates/core/src/multipart/store.rs:27`. |

**R1 — Abort mistakes a retryable conflict for permanent damage and loses commit ambiguity.** At `crates/core/src/multipart/store.rs:797`, an unchanged session after `Conflict` immediately becomes `AbortFault::ObligationKeyTaken`, without reading the obligation. TiKV can return that conflict from a locking read before comparing the precondition (`crates/metadata-tikv/src/lib.rs:1391`, `crates/metadata-tikv/src/lib.rs:934`). It therefore does not establish that the obligation exists or that an earlier fence cannot land. The existing restore path already distinguishes these cases by reading the obligation keys (`crates/custodian/src/restore.rs:1065`).

Two independent tests exercise the unchanged production functions over real redb reads and commits, with the adapter’s conflict result injected at the metadata seam:

- A single conflict with an unchanged Open session and zero retirement entries returns `ObligationKeyTaken`, rather than retrying an otherwise successful fence (`reviewer-abort-conflict.log:7`).
- A held first fence returns `CommitUnknownResult { may_still_commit: true }`; its retry returns Conflict while neither precondition changed. Abort returns `ObligationKeyTaken`. Releasing the held batch **after Abort returns** commits successfully, changes the session to Aborting, and installs its obligation (`reviewer-abort-conflict.log:13`). The returned error has lost the outstanding uncertainty.

Classify the actual session/obligation observations, bound retryable conflicts, and preserve outstanding ambiguity until evidence settles it. Add both cases to the regression suite. The existing interposer delegates conflicts to redb’s precondition checks, so it cannot exercise this adapter-specific result (`crates/core/tests/multipart_create_abort.rs:344`, `crates/core/tests/multipart_create_abort.rs:869`). This is one finding, corroborating `gate-logs/T4-batch-review.log:10`, not an additional review round’s unrelated concern. The reproduction models the adapter’s result; it is not a live TiKV-cluster test. Its source is retained as `reviewer_abort_conflict.rs`; no production fix was made.

**Verification establishes the exercised behavior, with explicit limits.** All source citations above resolve beneath `$PDCA_TARGET`; report and evidence paths resolve in the review cwd. Reverse-applying the complete patch in check mode succeeds, so there is no stale-target caveat. The production files were stashed while keeping the discriminator, then restored; their SHA256 values match the original patched target (`reviewer-verification.log:22`).

| Gate or independent check | Adjudication | Evidence |
|---|---|---|
| C4-verify | UNVERIFIABLE baseline; green reproduced | Stashed base: unresolved new API, exit 101 before tests (`reviewer-red.log:174`); restored patch: 34/34 pass (`reviewer-restored-green.log:42`), matching `gate-logs/C4-verify.log:22`. This is the declared enhancement posture, not a patch compile defect. |
| Multipart DST | PASS for the targeted scenario | Independent `RUSTFLAGS="--cfg madsim" MADSIM_TEST_NUM=50` run: 34 core tests and all six DST tests pass (`reviewer-dst.log:1`, `reviewer-dst.log:302`, `reviewer-dst.log:314`); the three explicit sweeps each cover 64 seeds (`crates/dst/tests/multipart_admission.rs:87`). |
| Restore refactor | PASS | Independent existing suites: seven Completing-fence and eleven Open-fence tests pass unchanged (`reviewer-restore.log:71`, `reviewer-restore.log:88`). |
| C4-ci | Inherited failure; reviewer-host limitation separately identified | Frozen attempts pass fmt/clippy/build/workspace tests, then fail the all-features audit on RUSTSEC-2026-0346/0349 (`gate-logs/C4-ci.log:6531`); Cargo.lock and both deny configurations equal the base (`reviewer-grounding.log:10`). CI stops before conformance/statics/DST (`xtask/src/main.rs:1683`), so the frozen CI log does not prove those later steps. |
| Independent prose/hygiene/scanners | PASS for completed checks | Typos, docs lint, 99-page render/link audit, and repository guards pass (`reviewer-ci.log:2`); statics and 5 valid + 6 invalid conformance vectors pass separately (`reviewer-scanners.log:4`, `reviewer-scanners.log:7`). Both declared external dependencies were exercised. |
| Reviewer full-CI/audit reruns | Host caveats, not patch defects | Local CI stops at missing rustfmt (`reviewer-ci.log:22`); direct offline cargo-deny cannot lock its read-only advisory cache (`reviewer-deny.log:1`). Frozen logs supply the adjudication; neither host failure is counted as a new patch failure. |
| C4-diff-cov | PASS in frozen evidence | 382/383 instrumentable changed lines, 99.7%; 631 other changed production lines are unscored, and 344 tests ran (`gate-logs/C4-diff-cov.log:594`, `gate-logs/C4-diff-cov.log:608`). Coverage does not discharge R1. |
| C5-mutants | PASS in frozen evidence | 79 tested: 67 caught, 12 unviable, none reported missed (`gate-logs/C5-mutants.log:13`). The aggregate does not identify the three hand-mutant demonstrations required by `brief.md:138`; their withheld outputs were not consulted. |
| T4-batch-review | FAIL, independently corroborated | Its sole finding is R1 (`gate-logs/T4-batch-review.log:10`); both independent discriminators fail (`reviewer-abort-conflict.log:25`). |
| T4-contribution | N/A | PR/commit artifacts are drafted after Check; the substantive audit must rerun at publish (`gate-logs/T4-contribution.log:10`). No human clearance is requested for this deferred row. |
| host-tikv | PASS in frozen evidence | Both feature-enabled clippy invocations finish (`gate-logs/host-tikv.log:112`, `gate-logs/host-tikv.log:211`); this proves compilation, not live contention behavior. |

**Prior-art confirmation remains a sign-off decision.** The brief names the three modified existing paths, merged record/teardown work, and rejected #508/#636 attempts (`brief.md:172`). I queried history for all six affected paths: existing paths expose only synthetic base `067c8e8`, the three added paths expose no history, and no remote is configured (`reviewer-grounding.log:5`). Thus the check is documented but cannot be mechanically corroborated against merged history or closed/rejected work from the supplied inputs. The accepted scope/size decisions, #511 deferral, and caller-owned timeout ruling remain settled (`brief.md:342`, `brief.md:365`); none is re-raised here.

<!-- pdca:leaf-complete -->

### Advisory — adversary

# Adversarial review — issue 656 (multipart Create / Abort), round 4

Bottom line: I could refute the fix in one place. That place is the same Abort branch the
gating T4 review already blocks on, and I reproduced it. Create, the shared fence builder, the
restore refactor and the evidence all held up when I attacked them.

## Refutation that landed

- NEEDS-HUMAN [impl] — `crates/core/src/multipart/store.rs:797-802`: on `Conflict`, if the
  re-read finds the session byte-identical, Abort returns `AbortFault::ObligationKeyTaken`
  ("damage") **without reading the obligation key**. The comment at `:798-799` claims "only the
  obligation key can have failed". That holds for redb and the sim-FDB model. It does not hold
  for every backend. The TiKV adapter maps a `get_for_update` lock failure to `Conflict` when
  "the concurrent winner already holds/moved the lock"
  (`crates/metadata-tikv/src/lib.rs:1388-1393`). If that lock holder never commits, the re-read
  finds nothing changed. The peer call site this slice was told to mirror already handles this
  case. Restore reads each obligation key and reports `SessionUnsettled::LostConflict`, "a
  conflict a re-read cannot explain" (`crates/custodian/src/restore.rs:324-325`, `:1065-1075`).
  The client Abort skips that read.
  **Concrete failing cases** (my scratch probe, a wrapper over `RedbMetadataStore` that answers
  `Ok(Conflict)` once without applying anything):
  (1) one spurious `Conflict` → Abort returns `ObligationKeyTaken { retire:bytes:s:<id>:0 }`
  while that key is **absent**;
  (2) round 1 → `CommitUnknownResult { may_still_commit: true }` with the batch held, round 2
  → spurious `Conflict` → Abort returns `ObligationKeyTaken`. When the held round-1 batch is
  then released it **commits**, and the session is `Aborting`.
  So the in-doubt outcome is thrown away for a "damage" error. That breaks the patch's own
  promise at `:711-714` ("never with an error that lets its caller take the fence for one that
  installed nothing") and the rubric's *Transactions* class (`CommitUnknownResult` must outrank
  a conflict-style answer). No existing test can catch it: `d_a_taken_obligation_key_is_a_typed_fault`
  (`crates/core/tests/multipart_create_abort.rs:1981`) only plants the key, so it passes whether
  or not the key is read. The DST model is strict CAS, so it never produces this case either.
  **Fix:** read the obligation key in this branch. Report `ObligationKeyTaken` only when the key
  is present. When it is absent, treat the conflict as an unexplained lost race: keep `doubt`
  and spend a round, or end with the held `CommitUnknownResult` if `may_still_commit`. Add both
  probes above as regressions.

## Attacks that did not land

- **Red→green evidence.** C4-verify is UNVERIFIABLE (exit 77, `gate-logs/C4-verify.log`). The
  brief declared that in advance: the new API does not exist on the base. build-notes is withheld,
  so I re-ran the three required reds myself in a scratch copy. (A′) with the contention budget
  set to `MAX_UPLOAD_ID_ATTEMPTS` fails both `a_prime_*` legs, and (A) free-running fails too.
  (E) with an `mpuctl` decrement added to the Abort batch fails 8 Abort legs. (F) with a re-mint
  on an unknown outcome fails 9 F/C legs. The unmodified code passes 34/34. The reds come from
  the real `create_session` / `abort_session` paths over a real redb store, not from a copy of
  them.
- **Gates C4-ci never reached.** C4-ci fails on the inherited cargo-deny advisory
  (RUSTSEC-2026-0346/0349, rustls 0.21.12 via tikv-client). Earlier sign-off already routed that
  waiver to Act. Because the gate stopped there, its later steps never ran: conformance, the
  ADR-0035 statics guard, and `cargo xtask dst` (madsim clippy plus the 50-seed sweep). I ran them
  by hand. `xtask statics` and `xtask conformance` pass. `cargo clippy -p wyrd-dst --all-targets`
  under `--cfg madsim` is clean. The full `cargo test -p wyrd-dst` with `MADSIM_TEST_NUM=50`
  exits 0: `multipart_admission` 6/6, and `custodian` 33/33, which covers the refactored restore
  fence.
- **Create** (`store.rs:351-616`). I tried each of these and found no failing case under the
  strict-CAS contract:
  - an unknown outcome landing between the recovery's reads;
  - a spurious `Conflict` while in doubt (`lost()` falls through to `Contended` and keeps the
    identity);
  - a bucket deleted while a batch is in doubt (ends `Unsettled`, and the held batch can no
    longer land);
  - a repeated id/nonce (`vacant()` at `:522-527` runs before the first batch);
  - a profile skew or a full ledger under doubt (`settle()` at `:444-466`);
  - backoff overflow at large profiles (`admission_backoff_millis` saturates).
- **Size headroom** (`multipart.rs:2368` `widest_encoding`). It covers every state, with every number
  the decoder allows at its widest, measured by the real encoder, so JSON escapes are counted.
  The decoder (`multipart.rs:2581-2638`) checks nothing that `SessionRecord::open` could violate.
- **Restore refactor** (`restore.rs:1051-1052`). `mpu_key(parse_mpu_key(key)) == key`, because
  the parse is strict (`multipart.rs:1236-1244`). The preconditions and puts come out in the same
  order as the inline code they replace. Conflict classification is unchanged.

<!-- pdca:leaf-complete -->

### Advisory — code-review

- NEEDS-HUMAN [impl] — `crates/core/src/multipart/store.rs:797`: **Abort mistakes a write conflict for a taken retirement key and can discard an unresolved fence.** An unchanged session after `Conflict` does not prove the obligation exists: TiKV also maps locking write conflicts to `Conflict` (`crates/metadata-tikv/src/lib.rs:1391`, `crates/metadata-tikv/src/lib.rs:935`). With the session unchanged and obligation absent, this branch nevertheless returns `ObligationKeyTaken`. If an earlier attempt returned `CommitUnknownResult { may_still_commit: true }`, it also drops that uncertainty even though the held fence can still land after Abort returns. Verify the obligation and session state before declaring damage; retry transient conflicts within the round budget while preserving unresolved outcomes. Add a regression covering both an ordinary unchanged-state conflict and a held unknown fence followed by that conflict, with the held batch released after the call. The existing redb tests only produce conflicts through failed value preconditions, so they miss this adapter behavior. This corroborates the frozen T4 finding.

No additional actionable findings from the reuse, simplification, or efficiency pass. Review used target source and frozen gate evidence; no builds were rerun.

<!-- pdca:leaf-complete -->

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C2 Reproduction (red pre-fix) — Accept or discharge the declared net-new red-evidence exception — the independently stashed base fails to compile before any assertion, and the three specified hand-mutant reds are not individually evidenced in the supplied logs; `brief.md:133`, `reviewer-red.log:174`, `gate-logs/C5-mutants.log:13`.
- [ ] C4 Verification (red→green) — Resolve the inherited all-features dependency-audit failure through Act — the feature and restore reruns pass, but full CI remains red and no behavioral pre-fix red exists; `gate-logs/C4-ci.log:6531`, `brief.md:369`, `reviewer-restored-green.log:42`, `reviewer-dst.log:314`, `reviewer-restore.log:88`.
- [ ] C5 Causal adequacy — Correct R1’s inference that an unchanged session after Conflict proves an occupied obligation — it can falsely report damage and discard a fence still able to commit; `crates/core/src/multipart/store.rs:797`, `crates/metadata-tikv/src/lib.rs:1391`, `reviewer-abort-conflict.log:13`.
- [ ] T4 Contribution — Confirm the affected-path merged and closed/rejected prior-art check — the brief records one, but this snapshot has only one synthetic commit and no remote to corroborate it; `brief.md:172`, `reviewer-grounding.log:5`; the publish-artifact audit itself is N/A by design (`gate-logs/T4-contribution.log:10`).
- [ ] Validation — fitness-to-purpose — Decide whether the evidenced admission and teardown behavior fulfills #656 after R1’s disposition — passing store tests cannot establish product fitness, and this approved slice has no production Create/Abort caller; `brief.md:146`, `crates/core/src/multipart/store.rs:27`.
- [ ] `crates/core/src/multipart/store.rs:797-802`: on `Conflict`, if the re-read finds the session byte-identical, Abort returns `AbortFault::ObligationKeyTaken` ("damage") **without reading the obligation key**. The comment at `:798-799` claims "only the obligation key can have failed". That holds for redb and the sim-FDB model. It does not hold for every backend. The TiKV adapter maps a `get_for_update` lock failure to `Conflict` when "the concurrent winner already holds/moved the lock" (`crates/metadata-tikv/src/lib.rs:1388-1393`). If that lock holder never commits, the re-read finds nothing changed. The peer call site this slice was told to mirror already handles this case. Restore reads each obligation key and reports `SessionUnsettled::LostConflict`, "a conflict a re-read cannot explain" (`crates/custodian/src/restore.rs:324-325`, `:1065-1075`). The client Abort skips that read. **Concrete failing cases** (my scratch probe, a wrapper over `RedbMetadataStore` that answers `Ok(Conflict)` once without applying anything): (1) one spurious `Conflict` → Abort returns `ObligationKeyTaken { retire:bytes:s:<id>:0 }` while that key is **absent**; (2) round 1 → `CommitUnknownResult { may_still_commit: true }` with the batch held, round 2 → spurious `Conflict` → Abort returns `ObligationKeyTaken`. When the held round-1 batch is then released it **commits**, and the session is `Aborting`. So the in-doubt outcome is thrown away for a "damage" error. That breaks the patch's own promise at `:711-714` ("never with an error that lets its caller take the fence for one that installed nothing") and the rubric's *Transactions* class (`CommitUnknownResult` must outrank a conflict-style answer). No existing test can catch it: `d_a_taken_obligation_key_is_a_typed_fault` (`crates/core/tests/multipart_create_abort.rs:1981`) only plants the key, so it passes whether or not the key is read. The DST model is strict CAS, so it never produces this case either. **Fix:** read the obligation key in this branch. Report `ObligationKeyTaken` only when the key is present. When it is absent, treat the conflict as an unexplained lost race: keep `doubt` and spend a round, or end with the held `CommitUnknownResult` if `may_still_commit`. Add both probes above as regressions.
- [ ] `crates/core/src/multipart/store.rs:797`: **Abort mistakes a write conflict for a taken retirement key and can discard an unresolved fence.** An unchanged session after `Conflict` does not prove the obligation exists: TiKV also maps locking write conflicts to `Conflict` (`crates/metadata-tikv/src/lib.rs:1391`, `crates/metadata-tikv/src/lib.rs:935`). With the session unchanged and obligation absent, this branch nevertheless returns `ObligationKeyTaken`. If an earlier attempt returned `CommitUnknownResult { may_still_commit: true }`, it also drops that uncertainty even though the held fence can still land after Abort returns. Verify the obligation and session state before declaring damage; retry transient conflicts within the round budget while preserving unresolved outcomes. Add a regression covering both an ordinary unchanged-state conflict and a held unknown fence followed by that conflict, with the held batch released after the call. The existing redb tests only produce conflicts through failed value preconditions, so they miss this adapter behavior. This corroborates the frozen T4 finding.
- [ ] C4 per-fix red->green: this patch's test red pre-fix, green post-fix unverifiable —                why this slice has no isolable red (the cargo output is above).
- [ ] C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance) FAILED (gating) — xtask: `cargo deny --all-features --config deny-all-features.toml check advisories` failed with exit status: 1
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 1 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_656/review-b
- [ ] **Create omits the bucket deletion fence.** `brief.md:162-170` enumerates the Create batch with only the bucket record required present and classifies only a gone bucket as `NoSuchBucket`; leg B (`brief.md:33-37`) tests only absence. On the target, `docs/design/proposals/draft/0016-multipart-commit-protocol.md:657` requires ADR-0046 §4's bucket-existence contract, and `docs/design/adr/0046-bucket-model-real-namespace.md:93-95` requires both exact marker bytes and absence of the deletion fence. The marker remains present while deletion is fenced (`:104-117`), so the proposed batch can admit into a deleting bucket. Specify both preconditions and add a deleting-bucket refusal/race case with no session, marker or counter writes.
- [ ] **Unknown Create outcomes lack a recoverable identity contract.** `brief.md:172` stops at “an unknown commit outcome is an `Err`,” while `:169` treats a taken upload id as a collision requiring re-minting. The normative proposal explicitly says unknown Create outcomes must recover with the **original upload id**, rebuilding the ledger precondition; erroring out or minting another id can strand an admitted session (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:2010-2027`). `crates/traits/src/lib.rs:240-247` confirms that an absent re-read cannot settle a commit that may still land. Define how the core API preserves/exposes the attempt identity and distinguishes recovery from a fresh-id collision, even if the gateway drives recovery later. Add a late-commit recovery scenario asserting one session and one increment; ordinary error propagation currently satisfies the brief without proving this property.
- [ ] **The required contention red is scheduler-dependent.** `brief.md:23-32,53-59,100-103` relies on multithreaded Tokio plus redb to demonstrate refusals when the contention budget is shortened. Target `crates/metadata-redb/src/lib.rs:113-117,208-234` implements reads and commits synchronously, without an internal await. Spawning 8/10/16 tasks does not ensure shared ledger reads, much less repeated losses by one task: a valid schedule can serialize them and let the shortened-budget mutant pass. Require a controlled read/commit schedule and assertions that a creator actually exceeds the collision-sized CAS budget, retaining the real-store checks. Include reproducible seeded coverage: target `AGENTS.md:188-190` requires it for new concurrent paths, but the brief specifies only the ordinary-runtime test.
- [ ] **The claimed shared fence batch builder is only a record builder.** `brief.md:117-118,206-207` says #841 already supplied the shared builder requested by the tracker. The first comment in `notes.json` says the client Abort and restore should use “one batch builder, not two.” Target `crates/core/src/multipart.rs:2360-2372,2426-2442` produces only a session/obligation pair; `crates/custodian/src/restore.rs:1050-1057` separately constructs the `WriteBatch`, including its preconditions. Reusing `open_teardown` alone therefore still duplicates transaction construction. Revise the scope to identify the shared batch-construction seam and the restore callsite change needed to use it, or explicitly surface the narrower record-builder interpretation for adjudication instead of claiming the tracker requirement is already implemented.
- [ ] size backstop — this slice is behaving oversized: patch is 233 KB (threshold 100 KB). Recommend answering `iterate-plan` at sign-off and authoring the split in the re-plan (`pdca split`), rather than `iterate-do`: a slice that is too big yields implementation-shaped findings every round, and splitting authors briefs, which is Plan's beat.

## 7. Proven / not proven
- Proven by which oracle: gates overall = fail (stub oracles).
- Unproven / needs manual run: anything flagged in §6.

## 8. Ready-to-ship attachments
- patch.diff
- tracker-comment.md     (ALWAYS, every tracker item)
- build-notes.md         (builder rationale — for the human, not the reviewer)

## 9. Check sign-off                     ← human completes Check here
- Disposition confirmed / overridden:
- Outcome: iterated-to-Plan
- Iteration delta (if iterating): Slice is oversized and growing every round (136 → 173 → 207 → 238 KB vs 100 KB backstop); split it in Plan (`pdca split 656`), e.g. Create admission and Abort fence as separate slices. - Carry into the Abort slice: on Conflict with an unchanged session, read the obligation key before reporting ObligationKeyTaken; if absent, treat as an unexplained lost race — keep doubt and spend a round, or end with the held CommitUnknownResult when may_still_commit (mirror restore.rs:1065-1075). Add both reviewer probes (spurious single Conflict; held round-1 batch + Conflict) as regressions, with a store wrapper that can return Conflict without applying. - DST test gap (manual review-branch): multipart_admission.rs:158 keys fault histories by session+operation, merging the two concurrent Abort calls; track each invocation separately. - What held up and can carry over: Create admission/recovery, the shared fence batch, restore refactor, encoded-size headroom.
- By / date: Eduard Ralph / 2026-10-14

## 10. Act candidates (hints for the next Act review)
- Plan advisory: 4 finding(s); brief revised: yes (plan-advisory-*.md)
- (empty is the common case)
