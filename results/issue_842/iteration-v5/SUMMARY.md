# Result — issue 842 / restore-fence-completing

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect: after child-3, restore names a resurrected `Completing` session but does not
  fence it. That session can still publish over reclaimed bytes (D-B, `0016:717-728`), and the
  `seg:<nonce>:<E>:*` records its attempt already wrote have no deleter anywhere (X57,
  `0016:880`). 0016's `Completing → Aborting` restore fence is one batch: the session goes to
  `Aborting@E+1`, with its bytes obligation **and** `retire:records:{seg:<g>:<E>}` for the
  segments that attempt wrote (`0016:665`, `:2193`).
  One payload 0016 names for that row cannot be written. `{session, parts: <set>}` for a sparse
  part set exceeds the store's value ceiling: 10,000 alternating part numbers (1, 3, …, 19,999)
  encode to 128,916 bytes against `MAX_VALUE_BYTES` = 100,000
  (`crates/core/src/metadata.rs:549`), though 0016 claims they fit (`0016:382-384`). #809
  iteration 1 measured it. FoundationDB refuses the whole batch permanently
  (`2103 value_too_large`), so every run fails and every session after it stays unfenced.
  **Decided at Plan (2026-09-29, the human, option (a)):** the restore fence's bytes obligation
  for a `Completing` session is `{session, all}`, as for `Open`. It owes the same records: no part
  can commit outside `Open` (`crates/core/src/multipart.rs:4236-4250`, `0016:1030`), so the part
  set is frozen from the Complete fence on. And its size does not grow with the part count.
  Rejected: (b) keeping `{session, parts}` and naming an oversized session instead of fencing it,
  which leaves that session able to publish.
- Success criterion: the NEW file `crates/custodian/tests/restore_completing_fence.rs`
  passes over in-memory doubles, calling the production `reconcile_after_restore`. Records are
  seeded as raw JSON, and every session record carries `segment_nonce` in child-2's spelling
  (immediately after `clock_source`, in every state). The attempt's group is `(that nonce, E)`,
  where `E` is `publish_target.epoch`. Legs:
  **(G) A `Completing` session is fenced with its segments' deleter.** A `Completing@E` session
  with `segments_written > 0`, its nonce on the record, and `seg:<nonce>:<E>:*` records present
  ends as `Aborting@E+1`. **One** commit installs `retire:bytes:s:<id>:<E>` `{session, all}` and
  `retire:records:s:<id>:<E>` naming exactly the group `(nonce, E)`. Both decode through
  `decode_retire_obligation` against their keys, and the records obligation's `segments()` names
  that group. The records obligation is installed even when `segments_written` is 0: 0016's row
  is "1 put" (`0016:665`), and a damaged cursor must not decide whether records get a deleter.
  Atomicity works as in child-3's F-atomic: a double failing the commit that carries any one of
  the three writes leaves none of them. Proving that draining empties the range is #659's job.
  **(G-collision) Neither obligation overwrites one already there.** Both obligation keys are
  installed under `require_absent` (`0016:369-373`, `:675-676`; `multipart.rs:1409-1414`), as
  child-3's F-collision already requires for the bytes key. Seed a `Completing@E` session beside a
  decodable `retire:records:s:<id>:<E>` naming a **different** segment group. After the pass,
  that obligation is byte-identical, none of the three fence writes landed (the session is still
  `Completing@E`, and no `retire:bytes:s:<id>:<E>` exists), the session is named as needing a
  human with child-3's "key taken" cause, and an `Open` session whose key sorts after it is
  still fenced.
  **(G-sparse) The fence fits the store's value ceiling whatever the part count.** Use a store
  double that refuses any value larger than `MAX_VALUE_BYTES`, as FoundationDB does. A
  `Completing` session holding 10,000 parts numbered 1, 3, …, 19,999 is fenced as in G, every
  value the pass writes is at most `MAX_VALUE_BYTES`, and an `Open` session whose key sorts after
  it is fenced too.
  **(H) What cannot be fenced cleanly is never passed off as done.**
  (i) A `Completing` record with **no** nonce (the shape before child-2) fails decode. Restore
  leaves it byte-identical (ADR-0045) and names it as needing a human.
  (ii) A `Completing` session whose `seg:` records name a chunk that none of its `part:` records
  holds is still fenced, and still named as needing a human.
  (iv) A `Completing` session with one `seg:<nonce>:<E>:*` value that will not decode is still
  fenced and named as needing a human. The records obligation would delete that record without
  marking whatever chunks it named.
  (v) A key under the group's `seg:` range that is not a well-formed segment key of that group
  is named the same way.
  (vi) A decodable `Completing` session at epoch `u64::MAX` (its `publish_target.epoch` equal,
  as decode requires, `multipart.rs:2224-2229`; decode does not refuse that epoch) has no
  `E+1`. Child-3's `u64::MAX` guard (its H(ii)) applies to `Completing` too: no transition, no
  `retire:` key for it, no wrap and no panic, the record byte-identical, the session named as
  needing a human, and an `Open` session whose key sorts after it still fenced.
  In every case `needs_human()` is true.
  **(K) A second pass is idempotent and still names what needs a human.** Re-running over the
  fenced store leaves it byte-identical, and a session named under H(ii), H(iv) or H(v) on the
  first pass is named again. "Already `Aborting`" never means "nothing to report"; #664
  iteration 1 got this wrong. A fact Do can rely on: for a session at `Aborting@E'`, only a
  `Completing → Aborting` fence files `retire:records:s:<id>:<E'-1>`. A rollback files at the
  epoch it leaves and lands in `Open@E+1` (`0016:2196`).
  **(Order)** child-3's P3 still holds with a `Completing` session in the store: the fence runs
  after Pass 3.
  **(L) `cargo xtask ci` green.**
- Repo + branch target: getwyrd/wyrd @ main
- Scope: the `Completing` arm of the restore fence: one commit per session installing the
  bytes and records obligations beside the transition. Reading the attempt's `seg:` range to name
  what cannot be fenced cleanly (H). The second-pass re-check (K). Update `multipart.rs`'s retire
  rows table (`:3141-3150`) so the restore `Completing` fence is listed under `{session, all}`,
  with the reason. Extend the fence paragraph child-3 wrote in `06-runtime-view.md` §6.5 and the
  m4 blueprint's step 7 to `Completing` sessions. A `deferred: #659` marker belongs wherever the
  re-check could misread a range the future drain has half-deleted. Size budget: at most 7 files
  and under 80 KB of diff. / out of scope: the `Open` fence, and report fields beyond what G/H
  need (child-3); DST (child-5; the same rule as child-3 for `crates/dst/tests/custodian.rs`); the
  `{session, parts}` rows of other writers; #810; #659; #656, #658; #508; any edit to 0016 or an
  ADR.

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: likely-fix
- Confidence: medium
- Recommendation: (set by Do)

## 3. Correctness (Check — chain)
- C1 Spec: none — brief.md
- C2 Reproduction (red pre-fix): none — (no gate configured)
- C3 Change: none — patch.diff
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): unverifiable — gate exceeded its 7200s timeout
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass — run-verify.sh: PASS — red without the fix, green with it (7 test(s) ran red).
- C4 diff coverage: changed lines executed by the patch's tests: fail — diff coverage not measured — patch.diff does not apply on origin/main
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): pass — 36 mutants tested in 3m: 8 caught, 28 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): pass — review-branch: 0 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_842/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 13.10s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

No implementation defect found in #842’s restore fence for resurrected `Completing` uploads: it must prevent publication over reclaimed bytes, install both deleters atomically, and retain damage warnings on rerun.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The accepted `{session, all}` decision makes the sparse-part requirement achievable; G/H/K specify observable fencing and damage outcomes without reopening Plan (`brief.md:25`, `brief.md:34`). |
| C2 Reproduction (red pre-fix) | PASS | With tracked changes stashed and the new test retained, all seven tests compiled and failed by assertion, independently confirming the unfenced-session defect (`reviewer-evidence/red.log:74`). |
| C3 Change | PASS | The seven-file, 81,892-byte patch stays within restore fencing, necessary reporting/tests, and living documentation; all patched source blobs match the supplied diff (`reviewer-evidence/target-integrity.log:1`, `brief.md:114`). |
| C4 Verification (red→green) | PASS | Restoring the patch makes all seven regression tests and nine Open-fence tests pass; the complete CI rerun also exits zero with writable caches (`reviewer-evidence/green-restored.log:15`, `reviewer-evidence/green-restored.log:30`, `reviewer-evidence/ci-writable-cache.log:3908`). |
| C5 Causal adequacy | PASS | The publication fence and both absence-protected deleters address the cause; absent/foreign obligations cannot silently clear rerun warnings, and no optional-capability workaround is introduced (`target/crates/custodian/src/restore.rs:824`, `target/crates/custodian/src/restore.rs:949`). |
| T1 Structure | PASS | Domain constructors preserve validated record/key relationships, while restore uses the MetadataStore seam; there is no new backend dependency or lifecycle clock (`target/crates/core/src/multipart.rs:2291`, `target/crates/custodian/src/restore.rs:801`). |
| T2 Shape | PASS | The teardown preserves session identity and emits decodable obligations without a part-count-dependent payload; the core unit test checks both keys and ineligible states (`target/crates/core/src/multipart.rs:5197`, `target/crates/custodian/tests/restore_completing_fence.rs:467`). |
| T3 Runtime | PASS | Tests exercise atomic failure, both key collisions, page-two damage, late-page part membership, epoch exhaustion, and repeated reporting after obligation removal (`target/crates/custodian/tests/restore_completing_fence.rs:399`, `target/crates/custodian/tests/restore_completing_fence.rs:527`, `target/crates/custodian/tests/restore_completing_fence.rs:635`). |
| T4 Contribution | N/A | Contribution artifacts are intentionally drafted after Check; their substantive audit is owed to the mandatory publish rerun (`gate-logs/T4-contribution.log:10`). |
| T5 Judgment | NEEDS-HUMAN | Confirm merged and closed/rejected prior art for all seven affected paths — the supplied single-commit target has no remote/history to establish whether equivalent work already exists (`reviewer-evidence/prior-art.log:1`, `brief.md:145`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Approve the stopped-writer recovery workflow and operator handling of named segment damage before service resumes — automated state-transition evidence does not establish operational fitness (`target/docs/design/architecture/06-runtime-view.md:65`, `target/docs/design/architecture/m4-first-deployment-blueprint.md:633`). |

The independent reruns support the implementation; the two human decisions above remain advisory sign-off items. Source citations beginning `target/` resolve inside `$PDCA_TARGET`; other citations refer to this review bundle.

- **Red→green is independently reproduced.** `git stash push` retained the untracked regression file; seven assertion failures occurred against the base, and `git stash pop` restored the fix. The restored run passed 7 Completing and 9 Open tests. All seven resulting blobs match `patch.diff`, and no stash remains (`reviewer-evidence/target-integrity.log:1`).
- **Full CI passes with the real tools.** The frozen run timed out after 7,200 seconds in `custodian_day_one` (`gate-logs/C4-ci.log:7`, `gate-logs/C4-ci.log:2316`). My first run completed workspace tests but hit a read-only cargo-deny cache lock (`reviewer-evidence/ci.log:3215`). Moving caches into this sandbox allowed the unchanged `cargo xtask ci` command and repository policies to finish successfully, including dependency walls, conformance, guards, and DST (`reviewer-evidence/ci-writable-cache.log:3908`). No compiler/tool shim or weakened policy was used. Both declared external dependencies, `typos` and the docs renderer, also passed direct reruns; 99 pages rendered with a clean link audit (`reviewer-evidence/scanners.log:5`).
- **Coverage remains unmeasured.** The frozen coverage gate could not apply the patch to `origin/main` (`gate-logs/C4-diff-cov.log:10`). The brief requires a base including predecessor patches (`brief.md:5`); the supplied target matches the patch and passes compilation/tests. This is a base/evidence caveat, not a demonstrated patch defect. No coverage percentage is claimed.
- **Other frozen evidence has narrower claims.** The instance-scoped mutation wrapper’s log reports 36 mutants: 8 caught and 28 unviable, with none reported missed (`gate-logs/C5-mutants.log:13`); this is not proof of exhaustive behavioral coverage. The batch-review log reports zero blocking findings (`gate-logs/T4-batch-review.log:10`). The TiKV log shows both requested feature clippy invocations completing, not a live backend exercise (`gate-logs/host-tikv.log:110`, `gate-logs/host-tikv.log:209`). These logs were read; those instance-scoped review/mutation wrappers were not independently rerun.

The missing-deleter carry-forward is resolved: the repeat-pass test removes foreign, malformed, and parts-only obligations, then requires their surviving segment records to remain named; a genuinely empty Aborting control stays unnamed (`target/crates/custodian/tests/restore_completing_fence.rs:635`). The previously requested second-page, multi-chunk, bytes-key collision, and core teardown regressions are present and passed. The root rubric was applied; the existing `#659` half-drain and `#843` fence-specific DST deferrals remain settled (`target/crates/custodian/src/restore.rs:932`, `target/crates/custodian/src/restore.rs:785`). Existing DST passing is not presented as new fence-specific DST coverage. Live FoundationDB recovery was not exercised; the production-path test uses the value-ceiling double explicitly required by the brief (`brief.md:132`). No `INTEGRATION.md` exists in the supplied target (`reviewer-evidence/prior-art.log:7`).

### Advisory — adversary

# Adversarial review — #842 restore fence for `Completing` sessions

Verdict: I could not break the production fence. Two test gaps let real regressions through
(bullets 1–2), and two gate rows claim more than they proved (bullets 3–4). All work below was
done in a scratch copy of `$PDCA_TARGET`, with the patch applied as found there.

- NEEDS-HUMAN [impl] — `crates/custodian/src/restore.rs:1491` (`emit_segments_unaccounted`) is
  pinned by no test. Running cargo-mutants on the diff with `--cap-lints=true`, the mutant
  `replace emit_segments_unaccounted with ()` **survives** the whole wyrd-custodian suite. A
  second mutant survives too: `restore.rs:266` (`Display for SegmentFault` → empty string). Why
  it matters: the CLI names only 20 sessions, then says "and N more (the audit log names every
  one)" (`crates/server/src/cli.rs:1402-1416`, `named_records` at `:1463-1474`). The blueprint
  sends operators to `action=session-segments-unaccounted` too. Failing case under the mutant:
  21 fenced `Completing` sessions, each holding one undecodable `seg:` record. The 21st is then
  named nowhere: not on stdout, and not in the audit log. Under the second mutant, the
  "and why: …" text prints blank. Fix: the test file already captures the audit seam (`AUDIT`
  and `audited()`, `crates/custodian/tests/restore_completing_fence.rs:336-371`), but only for
  `dangling` and `summary`. In leg H or K, assert that each named case emits a
  `session-segments-unaccounted` event carrying its `session`, its `record` and a non-empty
  `fault`. (The subscriber is installed once per binary, by the Order test, so the new leg must
  install it too, idempotently.)

- NEEDS-HUMAN [impl] — every fixture sets the session's `segment_nonce` equal to its upload id
  (`restore_completing_fence.rs:182-196`; `group()` at `:219`). So no test can tell the two
  apart. Concrete survivor, which I ran: change `restore.rs:911` from
  `SegmentGroup::from_nonce(record.segment_nonce().clone(), attempt)` to
  `SegmentGroup::new(upload.as_str(), attempt).expect(..)`. All 7 tests in
  `restore_completing_fence.rs` and all 9 in `restore_open_fence.rs` still pass. Under that
  mutant, a fenced session whose nonce differs from its id (`77…` vs `a7…`) is named
  `NotOfAttempt` on **every** re-run, because its own correct `{seg:(77…,3)}` obligation no
  longer matches. Its real `seg:77…:3:` range is never read, so a junk `seg:77…:3:000001` goes
  unnamed. That is a false alarm on every clean fence, and it hides the real fault (leg K). The
  production code is correct today: my probe with a distinct nonce passes on the patch as built.
  Fix: seed K's clean control and at least one damaged H/K case with a nonce that is not the
  upload id.

- NEEDS-HUMAN [human] — the C5 row ("36 mutants tested: 8 caught, 28 unviable", 0 missed) is
  not evidence of adequacy. The workspace sets `warnings = "deny"` (`Cargo.toml:230`), and
  `.cargo/mutants.toml` sets no `cap_lints`. So any mutant that stubs a body and leaves a
  parameter unused fails to compile, and is counted "unviable". Re-running the same 36 mutants
  with `--cap-lints=true` gives **25 caught, 2 missed, 9 unviable**; the 2 missed are the ones in
  bullet 1. Whether the gate (or `.cargo/mutants.toml`) should cap lints is a harness/repo
  decision outside this diff. The two survivors themselves are bullet 1's [impl] work.

- NEEDS-HUMAN [human] — leg (L), "`cargo xtask ci` green", is unverified, yet `check-gates.json`
  reports `overall: "pass"`. The gating C4-ci row is `unverifiable`: it hit the 7200 s timeout
  inside `crates/server/tests/custodian_day_one.rs`, after fmt, clippy, build and most tests
  had passed. It never ran machete, deny, conformance, statics, the orchestrator guard or DST.
  The hang is not this patch: here, with the patch applied, `custodian_day_one` passes 15/15 in
  0.19 s. What I re-ran myself, all green: `cargo xtask statics`;
  `cargo clippy -p wyrd-dst --all-targets` under `--cfg madsim`; and the DST `restore*` tests.
  Still not run: machete, deny, conformance, the orchestrator guard, the full DST seed sweep,
  and the server tests after `custodian_day_one`. Provisional (toolchain/time), not a
  refutation.

- Attacks I tried that did not land:
  - **Red→green.** It is real and runs the production `reconcile_after_restore`. With the
    production files reverted and the test kept, 7/7 fail by assertion. With the fix, 7/7 pass.
  - **One commit.** All three writes go in one batch (`restore.rs:807-840`). A split batch
    would fail G-atomic.
  - **`require_absent`.** It is on both keys. G-collision covers the records key and the bytes
    key separately. Its "key taken" cause names whichever key is taken.
  - **Value ceiling.** The written payloads are minimal, `{"session":true,"parts":"all"}` and
    `{"seg":{"nonce":…,"epoch":3}}`, with no default fields emitted. G-sparse would refuse
    `{session, parts}`.
  - **Last epoch.** `Completing@u64::MAX` takes the guard with no wrap.
  - **Paging.** Both the `seg:` and `part:` reads page correctly. The "paged" case and G-sparse
    each kill a stop-after-page-one mutant.
  - **The re-check.** It handles a foreign, undecodable, parts-only or absent obligation.
  - **Designed flows.** I traced rollback (lands `Open@E+1`), the Open-fence `Aborting`
    sessions and the reaper/operator rows: none of them gives a false `NoDeleter` or
    `NotOfAttempt`.
  - **Unknown commit outcome.** `CommitUnknownResult` is never read as a `Conflict`
    (`:859-863`).

### Advisory — code-review

No findings on either lens: no introduced correctness bugs or material reuse, simplification, or efficiency issues found in this diff.

Reviewed the atomic fence, collision handling, epoch bounds, paginated segment/part checks, repeat-pass reporting, tests, and CLI changes against the read-only target. All 41 diff hunks match the target source.

Validation used the frozen gate evidence: all seven new tests passed with the patch and failed without it; mutation testing reported eight caught and no surviving mutants (28 unviable). Full CI timed out in `custodian_day_one`; diff coverage was not measured because the patch did not apply to `origin/main`. No builds were rerun or target files changed.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] T5 Judgment — Confirm merged and closed/rejected prior art for all seven affected paths — the supplied single-commit target has no remote/history to establish whether equivalent work already exists (`reviewer-evidence/prior-art.log:1`, `brief.md:145`).
- [ ] Validation — fitness-to-purpose — Approve the stopped-writer recovery workflow and operator handling of named segment damage before service resumes — automated state-transition evidence does not establish operational fitness (`target/docs/design/architecture/06-runtime-view.md:65`, `target/docs/design/architecture/m4-first-deployment-blueprint.md:633`).
- [ ] `crates/custodian/src/restore.rs:1491` (`emit_segments_unaccounted`) is pinned by no test. Running cargo-mutants on the diff with `--cap-lints=true`, the mutant `replace emit_segments_unaccounted with ()` **survives** the whole wyrd-custodian suite. A second mutant survives too: `restore.rs:266` (`Display for SegmentFault` → empty string). Why it matters: the CLI names only 20 sessions, then says "and N more (the audit log names every one)" (`crates/server/src/cli.rs:1402-1416`, `named_records` at `:1463-1474`). The blueprint sends operators to `action=session-segments-unaccounted` too. Failing case under the mutant: 21 fenced `Completing` sessions, each holding one undecodable `seg:` record. The 21st is then named nowhere: not on stdout, and not in the audit log. Under the second mutant, the "and why: …" text prints blank. Fix: the test file already captures the audit seam (`AUDIT` and `audited()`, `crates/custodian/tests/restore_completing_fence.rs:336-371`), but only for `dangling` and `summary`. In leg H or K, assert that each named case emits a `session-segments-unaccounted` event carrying its `session`, its `record` and a non-empty `fault`. (The subscriber is installed once per binary, by the Order test, so the new leg must install it too, idempotently.)
- [ ] every fixture sets the session's `segment_nonce` equal to its upload id (`restore_completing_fence.rs:182-196`; `group()` at `:219`). So no test can tell the two apart. Concrete survivor, which I ran: change `restore.rs:911` from `SegmentGroup::from_nonce(record.segment_nonce().clone(), attempt)` to `SegmentGroup::new(upload.as_str(), attempt).expect(..)`. All 7 tests in `restore_completing_fence.rs` and all 9 in `restore_open_fence.rs` still pass. Under that mutant, a fenced session whose nonce differs from its id (`77…` vs `a7…`) is named `NotOfAttempt` on **every** re-run, because its own correct `{seg:(77…,3)}` obligation no longer matches. Its real `seg:77…:3:` range is never read, so a junk `seg:77…:3:000001` goes unnamed. That is a false alarm on every clean fence, and it hides the real fault (leg K). The production code is correct today: my probe with a distinct nonce passes on the patch as built. Fix: seed K's clean control and at least one damaged H/K case with a nonce that is not the upload id.
- [ ] the C5 row ("36 mutants tested: 8 caught, 28 unviable", 0 missed) is not evidence of adequacy. The workspace sets `warnings = "deny"` (`Cargo.toml:230`), and `.cargo/mutants.toml` sets no `cap_lints`. So any mutant that stubs a body and leaves a parameter unused fails to compile, and is counted "unviable". Re-running the same 36 mutants with `--cap-lints=true` gives **25 caught, 2 missed, 9 unviable**; the 2 missed are the ones in bullet 1. Whether the gate (or `.cargo/mutants.toml`) should cap lints is a harness/repo decision outside this diff. The two survivors themselves are bullet 1's [impl] work.
- [ ] leg (L), "`cargo xtask ci` green", is unverified, yet `check-gates.json` reports `overall: "pass"`. The gating C4-ci row is `unverifiable`: it hit the 7200 s timeout inside `crates/server/tests/custodian_day_one.rs`, after fmt, clippy, build and most tests had passed. It never ran machete, deny, conformance, statics, the orchestrator guard or DST. The hang is not this patch: here, with the patch applied, `custodian_day_one` passes 15/15 in 0.19 s. What I re-ran myself, all green: `cargo xtask statics`; `cargo clippy -p wyrd-dst --all-targets` under `--cfg madsim`; and the DST `restore*` tests. Still not run: machete, deny, conformance, the orchestrator guard, the full DST seed sweep, and the server tests after `custodian_day_one`. Provisional (toolchain/time), not a refutation.
- [ ] C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance) unverifiable — gate exceeded its 7200s timeout
- [ ] **Atomicity alone does not protect the new retirement obligation from overwrite.** G requires three writes in one commit and a failing-commit witness (`brief.md:35-43`), but never requires a collision witness for the newly added `retire:records:s:<id>:<E>` key. The target explicitly requires `require_absent` on obligation installation and classification of collisions, because overwriting a payload permanently loses reclamation evidence (`crates/core/src/multipart.rs:1409-1414`; `docs/design/proposals/draft/0016-multipart-commit-protocol.md:369-373`). A blind put of the records obligation can satisfy the listed fresh-store and atomicity cases. Revise G to require the absence guard and seed an existing, decodable same-epoch obligation naming a different segment group: its bytes must survive, none of the three fence writes may land, and the unresolved collision must be reported.
- [ ] **The promised epoch transition has an accepted input with no successor.** G says `Completing@E` ends at `Aborting@E+1` (`brief.md:35-38`), and the invariant is unconditional (`brief.md:76-79`). On the supplied target, the session epoch is any `u64` (`crates/core/src/multipart.rs:2085`); Completing validation checks cursor bounds and matching target identity/epoch, but does not reject `u64::MAX` (`crates/core/src/multipart.rs:2204-2240`). Such a decodable record cannot take the required transition. Revise H to cover a Completing record at that boundary: checked increment, no wrap/panic or partial obligations, unchanged session reported as needing a human, and continued fencing of a later eligible session. Explicitly apply any inherited child-3 guard to Completing, and qualify the invariant for sessions left unresolved instead of promising universal fencing.
- [ ] T5 Judgment — Confirm the path-based prior-art disposition against merged history and closed/rejected work — the brief records that check, but this one-commit snapshot has no remote or historical review evidence to corroborate it (`brief.md:145`, `pdca-reviewer-842-evidence/source-audit.log:10`).
- [ ] T5 Judgment — Confirm merged-history and closed/rejected-work coverage by affected path on the publication base: the brief documents restore.rs/multipart.rs prior art, but this target exposes only one synthetic commit and no remote, so broader coverage cannot be independently settled (`brief.md:145`; `reviewer-evidence/target-grounding.txt:2`).

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
- Iteration delta (if iterating): Auto-iterate (round 4): rebuilding for the implementation-level findings — `crates/custodian/src/restore.rs:1491` (`emit_segments_unaccounted`) is pinned by no test. Running cargo-mutants on the diff with `--cap-lints=true`, the mutant `replace emit_segments_unaccounted with ()` **survives** the whole wyrd-custodian suite. A second mutant survives too: `restore.rs:266` (`Display for SegmentFault` → empty string). Why it matters: the CLI names only 20 sessions, then says "and N more (the audit log names every one)" (`crates/server/src/cli.rs:1402-1416`, `named_records` at `:1463-1474`). The blueprint sends operators to `action=session-segments-unaccounted` too. Failing case under the mutant: 21 fenced `Completing` sessions, each holding one undecodable `seg:` record. The 21st is then named nowhere: not on stdout, and not in the audit log. Under the second mutant, the "and why: …" text prints blank. Fix: the test file already captures the audit seam (`AUDIT` and `audited()`, `crates/custodian/tests/restore_completing_fence.rs:336-371`), but only for `dangling` and `summary`. In leg H or K, assert that each named case emits a `session-segments-unaccounted` event carrying its `session`, its `record` and a non-empty `fault`. (The subscriber is installed once per binary, by the Order test, so the new leg must install it too, idempotently.); every fixture sets the session's `segment_nonce` equal to its upload id (`restore_completing_fence.rs:182-196`; `group()` at `:219`). So no test can tell the two apart. Concrete survivor, which I ran: change `restore.rs:911` from `SegmentGroup::from_nonce(record.segment_nonce().clone(), attempt)` to `SegmentGroup::new(upload.as_str(), attempt).expect(..)`. All 7 tests in `restore_completing_fence.rs` and all 9 in `restore_open_fence.rs` still pass. Under that mutant, a fenced session whose nonce differs from its id (`77…` vs `a7…`) is named `NotOfAttempt` on **every** re-run, because its own correct `{seg:(77…,3)}` obligation no longer matches. Its real `seg:77…:3:` range is never read, so a junk `seg:77…:3:000001` goes unnamed. That is a false alarm on every clean fence, and it hides the real fault (leg K). The production code is correct today: my probe with a distinct nonce passes on the patch as built. Fix: seed K's clean control and at least one damaged H/K case with a nonce that is not the upload id.. 8 finding(s) needing human judgment were deferred to sign-off, not addressed here.
- By / date: auto-iterate / 2026-09-30

## 10. Act candidates (hints for the next Act review)
- Plan advisory: 2 finding(s); brief revised: yes (plan-advisory-*.md)
- (empty is the common case)
