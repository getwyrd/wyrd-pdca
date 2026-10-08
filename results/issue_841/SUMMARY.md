# Result — issue 841 / restore-fence-open

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect: the post-restore pass fences no upload session. A restored metadata image can
  resurrect a session that was torn down after the restore point, with its bytes already
  reclaimed, and nothing stops a retried Complete from publishing over them (D-B,
  `0016:717-728`; F13). `reconcile_after_restore` (`crates/custodian/src/restore.rs:312-589`)
  reads each session only by key, for staged protection (`crates/custodian/src/gc.rs:1505-1510`:
  "A session record's value is never decoded"), and never writes to it. 0016's restore fence
  moves every session open in the image to `Aborting@E+1`, in one batch with the retirement
  obligation that owes its records (`0016:664`, `:823`, `:717-728`), and the report counts it as
  `sessions_fenced` (`0016:823`, `:879`).
- Success criterion: the NEW file `crates/custodian/tests/restore_open_fence.rs` passes over
  in-memory doubles, with records seeded as raw JSON, calling the production
  `reconcile_after_restore`. Every seeded session record carries `segment_nonce` in child-2's
  spelling (immediately after `clock_source`, in every state): on this base a record without it
  does not decode, and would be taken for H(i). Legs:
  **(F) An `Open` session is fenced, whole.** An `Open@E` session with committed parts ends as
  `Aborting@E+1`. The same commit installs `retire:bytes:s:<id>:<E>` owing the session's staged
  residue **and every part**: `{session, all}`, the one payload shape that owes both
  (`crates/core/src/multipart.rs:3141-3142`, `0016:2187`). `{session}` alone would leave the
  committed parts with no deleter. The obligation decodes through `decode_retire_obligation`
  (`multipart.rs:3455`) against the key it sits under. The `Debug` rendering contains
  `sessions_fenced: 1`. A session already `Aborting` or `Completed` in the image is left
  byte-identical and not counted.
  **(F-atomic) The fence lands whole or not at all.** Use a store double that fails the commit
  carrying the session's write in one run, and the commit carrying the obligation's write in
  another. After either run, **neither** write is present and the pass returns `Err`. A commit
  whose outcome is unknown is never read as `Conflict` (`crates/traits/src/lib.rs:205-212`,
  `AGENTS.md:178-180`).
  **(F-race) A session that changes under the pass is not fenced blind.** A write lands on the
  session record between the pass's read of it and the fence's commit: a Complete fence moving it
  from `Open@E` to `Completing@E+1` (every fence bumps the epoch, `0016:704-708`,
  `multipart.rs:2173`). The fence then writes nothing: no `retire:` key names that session, and the
  concurrent write's bytes are intact. The session is named as needing a human, and the pass
  still fences every other session and returns `Ok`.
  **(F-collision) The fence never overwrites an obligation.** Installation is
  `require_absent(retire:<mode>:<token>)`, never a blind put (`0016:369-373`, and the note under
  the batch table, `0016:675-676`; `multipart.rs:1409-1414`). Seed an `Open@E` session beside a
  `retire:bytes:s:<id>:<E>` value that decodes but differs from what the fence would write (a
  `{session}` payload). After the pass, the session and that obligation are both byte-identical,
  nothing else was written for that session, the session is named as needing a human with a
  cause that says the obligation key was taken (not that the session changed, as in F-race),
  and the pass still fences every other session and returns `Ok`. A collision is classified
  once and named, never retried in a loop: the token grammar makes it impossible without damage
  (`0016:358-373`).
  **(H) What this pass cannot fence is named, never passed off as done.** Each of these is left
  byte-identical, has no `retire:` key written for it, and is named as needing a human
  (`needs_human()` true, `restore.rs:212`):
  (i) a session whose value will not decode (the staged class still protects its records exactly
  as on the base);
  (ii) an `Open` session at epoch `u64::MAX`, which has no `E+1`;
  (iii) a decodable `Completing` session. Its fence must also retire its segment records and is
  child-4's; until that lands, this pass must say it did not fence it.
  **(K) A second pass is idempotent.** Re-running over the fenced store leaves the whole store
  byte-identical to how the first pass left it (no second obligation, nothing fenced again), and
  every session H named is named again.
  **(P3) A fence fault never hides the pass's verdicts.** The store holds one dangling committed
  chunk, one under-replicated committed chunk, and one `Open` session whose fence commit fails.
  The pass returns `Err`, **and** before it the audit trail carries both verdicts: the chunk's
  `DANGLING` line (`emit_dangling`, `restore.rs:906`), and the pass's summary counts with
  `under_replicated` at 1. An under-replicated chunk has no line of its own; its only audit
  record is the summary (`emit_summary`, `restore.rs:1021-1032`), which the base emits only on
  the `Ok` path (`:587`). So on a fence fault the summary is still emitted, and it must not read
  "complete" (the fence did not finish). The CLI prints no verdict on `Err`
  (`crates/server/src/cli.rs:1187-1196`); that stays, and the audit trail is where the operator
  finds the counts. Do chooses how the error path emits them. The fence runs after Pass 3
  (`restore.rs:534-585`), never before it. This is the "no report AT ALL" class #651 fixed
  (`restore.rs:324-329`).
  **(Paging)** Sessions listed across more than one page of the `mpu:` listing are all fenced. A
  scan cap on the double forces the paging, as `staged_protection.rs:1067` does.
  **(CLI)** `restore_verdict` (`crates/server/src/cli.rs:1256`) counts fenced sessions in its
  summary, and prints a `NEEDS-HUMAN` paragraph for the sessions it could not fence, through
  `named_records` (`cli.rs:1389`) unchanged: the first 20 by name
  (`NAMED_UNREADABLE_RECORDS`, `cli.rs:1380`) and the rest as a count. `RestoreReport` and the
  audit trail name every one. Cover this in `cli.rs`'s report tests (green-only), including an
  over-limit case with 21 unsettled sessions, mirroring
  `restore_verdict_names_the_blocking_records_and_counts_the_ones_it_cannot_fit`
  (`cli.rs:2999-3023`); and extend the agreement test (from `cli.rs:2898`) to the new paragraph.
  Do not change `named_records`' bound.
  **(L) `cargo xtask ci` green.** This includes the existing restore legs in
  `staged_protection.rs` (among them the three writes-during-restore legs, `:1511-1540`) and the
  restore DST properties in `crates/dst/tests/custodian.rs:1954-2200`, unchanged.
- Repo + branch target: getwyrd/wyrd @ main
- Scope: the restore fence for `Open` sessions: one commit per session, installing the
  transition and its obligation together, holding only if the session record is still exactly
  what the pass read (the fence row's own precondition, `0016:664`) **and** the obligation key is
  absent (`require_absent`, `0016:369-373`, `:675-676`). It runs after Pass 3, before
  `emit_summary` (`restore.rs:587`); a fence fault still emits the summary (leg P3).
  `sessions_fenced` and `sessions_unsettled` go on `RestoreReport`. `needs_human()` counts the
  second, and `is_clean()` is false whenever a session was fenced, because that predicate counts
  the work the pass did (`restore.rs:189-192`). Both get audit lines.
  In `multipart.rs`, add the writer-side API the fence needs. This is the first writer of the
  session record and of a retire obligation, so update the "no writer-side constructor" notes
  (`multipart.rs:2127-2130`, `:3180-3182`) and the retire rows table (`:3141-3142`) to match what
  you add, and keep an obligation's key and payload unable to disagree.
  Update `restore_verdict` and its tests.
  One existing test changes by design: `staged_protection.rs:1742-1830` asserts the pass never
  names an undecodable session on the audit seam, which the fence makes false. Keep its
  protection assertions, and repoint its naming assertion at the unsettled line.
  One fixture changes because this child is the first to decode session values: child-1's
  `crates/custodian/tests/restore_staged_report.rs` ran in parallel with child-2, so its seeded
  sessions lack `segment_nonce`. Add the field to that file's session builder, and nothing else
  in it; if any of its legs then fails for another reason, say so in `build-notes.md` rather
  than editing around it.
  Docs: a new fence paragraph in `docs/design/architecture/06-runtime-view.md` §6.5, after `:63`
  (not §6.7: its lines are 3–6 KB each). Update the m4 blueprint's step 7
  (`m4-first-deployment-blueprint.md:581-625`), including its claim that the pass never decodes
  an `mpu:` value (`:610-612`), and `cli.rs`'s matching comment (`:1339-1345`).
  `gc.rs` only to widen the visibility of its paging helpers (`walk_staged_range` `:1714`,
  `staged_page` `:1733`) if the fence reuses them.
  Size budget: at most 9 files and under 90 KB of diff.
  / out of scope: fencing `Completing` sessions, and anything about `seg:` records (child-4;
  here a `Completing` session is only named); DST coverage (child-5; leave
  `crates/dst/tests/custodian.rs` untouched unless an existing property stops passing, and then
  say so in `build-notes.md`); any durable record that the fence ran, and any "generation"
  (#810); the retire drain (#659); Abort and Complete (#656, #658); the gateway (#508);
  `scrub.rs`, `reconstruction.rs`; any edit to 0016 or an ADR.

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: likely-fix
- Confidence: medium
- Recommendation: (set by Do)

## 3. Correctness (Check — chain)
- C1 Spec: none — brief.md
- C2 Reproduction (red pre-fix): none — (no gate configured)
- C3 Change: none — patch.diff
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): pass — xtask ci: all checks passed
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass — run-verify.sh: PASS — red without the fix, green with it (9 test(s) ran red).
- C4 diff coverage: changed lines executed by the patch's tests: fail — diff coverage not measured — patch.diff does not apply on origin/main
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): pass — 44 mutants tested in 5m: 20 caught, 24 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): pass — review-branch: 0 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_841/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 13.05s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

No patch defects found in #841's restore fence for resurrected `Open` upload sessions; operational fitness remains for human sign-off.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The Open-only recovery scope has falsifiable atomicity, race, collision, idempotency and reporting criteria; its prerequisites and exclusions are explicit (`brief.md:23`, `brief.md:111`, `brief.md:120`). |
| C2 Reproduction (red pre-fix) | PASS | Stashing the tracked fix while retaining the new test produces nine assertion failures, including a session remaining Open; compilation succeeds (`reviewer-red.log:49`, `reviewer-red.log:85`). |
| C3 Change | PASS | All nine target postimages match the supplied patch; its 89,646 bytes fit the nine-file/90 KB limit and the authorized surfaces (`reviewer-integrity.log:1`, `reviewer-integrity.log:10`, `brief.md:147`). |
| C4 Verification (red→green) | PASS | Independent stash/restore reproduces nine assertion failures then nine passes; full CI and feature checks also pass, while diff coverage remains unmeasured (`reviewer-red-green.log:26`, `reviewer-ci-final.log:3819`, `reviewer-tikv.log:409`, `gate-logs/C4-diff-cov.log:11`). |
| C5 Causal adequacy | PASS | The atomic fence removes resurrected-session publishability and owes every part; interruption cannot certify a clean restore. Independent mutation testing catches all 20 viable mutants (`crates/custodian/src/restore.rs:754`, `crates/custodian/src/restore.rs:1320`, `reviewer-mutants.log:48`). |
| T1 Structure | PASS | Recovery stays on the MetadataStore seam, while the codec owns the validated transition and inseparable obligation key/payload; no backend dependency or clock source is added (`crates/core/src/multipart.rs:2264`, `crates/core/src/multipart.rs:3584`, `crates/custodian/src/restore.rs:738`). |
| T2 Shape | PASS | Operators retain bounded CLI names and complete audit attribution, with current architecture/runbook guidance and a 21-session regression (`crates/server/src/cli.rs:1383`, `crates/server/src/cli.rs:3172`, `docs/design/architecture/06-runtime-view.md:64`). |
| T3 Runtime | PASS | Checked pagination and one conditional batch per Open session preserve progress and atomicity; conflicts are classified once and unknown outcomes remain errors (`crates/custodian/src/restore.rs:723`, `crates/custodian/src/restore.rs:768`, `crates/traits/src/lib.rs:1333`). |
| T4 Contribution | N/A | The contribution artifacts are intentionally absent at Check; their substantive audit must rerun at publish, so there is no current artifact verdict to reproduce (`gate-logs/T4-contribution.log:10`). |
| T5 Judgment | PASS | Affected-path history and the latest 40 closed PRs were checked; no matching closed-unmerged work was found, and the prior audit/API defects are addressed without reopening tracked deferrals (`reviewer-prior-art.json:2`, `reviewer-prior-art.json:13`, `crates/core/src/multipart.rs:5084`, `crates/custodian/tests/restore_open_fence.rs:816`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Confirm that the documented writers-stopped recovery procedure and handling of unfenced uploads meet operator recovery needs — automated state-transition tests cannot establish operational fitness (`docs/design/architecture/m4-first-deployment-blueprint.md:627`, `docs/design/architecture/m4-first-deployment-blueprint.md:634`). |

Source citations resolve under `$PDCA_TARGET`; brief, patch and evidence citations resolve in this review directory. The disposable target contains the intended prerequisite base: the pre-fix test compiled, and every patched file matches `patch.diff`. No implementation files were changed by this review.

The previously reported implementation defects are resolved. The otherwise-clean failed-fence test checks both `clean=false` and `needs_human=true` (`crates/custodian/tests/restore_open_fence.rs:830`). The core test exercises all non-Open states, epoch exhaustion and the obligation's decode against its own key (`crates/core/src/multipart.rs:5084`). Mutation results independently match the frozen gate: 44 tested, 20 caught, 24 unviable, none missed (`reviewer-mutants.log:48`; `gate-logs/C5-mutants.log:13`). No optional-capability probe or load-time workaround was introduced; the Open-state check enforces the public writer API's domain.

Full `cargo xtask ci` passed independently, including the unchanged restore/DST tests, dependency audits, conformance and static guards (`reviewer-ci-final.log:3819`). Verification also exercised the real `typos`, docs lint/render and TiKV/server feature checks successfully (`reviewer-validation.log:3`, `reviewer-docs-render.log:2`, `reviewer-tikv.log:409`). Both external dependencies named by the brief were present and exercised. The frozen batch-review log reports zero blocking findings (`gate-logs/T4-batch-review.log:10`).

The earlier local CI attempts hit review-environment failures: a missing test executable while mutation testing shared its cache, then a read-only advisory-database lock. Separate build output and a writable copy of the advisory cache resolved both; the final full run used the real tools and unchanged repository configuration.

Diff coverage remains unmeasured. Its frozen log establishes only that the patch did not apply to `origin/main` (`gate-logs/C4-diff-cov.log:10`); the brief explicitly stacks this change on #839 and #840 (`brief.md:111`). This is a base-selection caveat, not a patch compilation defect or a coverage percentage. The instance-scoped gate wrappers are not part of the target; their captured logs were read.

The prior-art check queried merged history by all nine affected paths and inspected file lists for the latest 40 closed PRs, with no query errors or truncated file lists (`reviewer-prior-art.json:2`). Matching work was merged predecessor functionality; no matching closed-unmerged PR appeared in that window. The rejected approaches documented in `brief.md:177` are not repeated: the fence follows Pass 3, and the second-pass test preserves both the store and existing staged findings (`crates/custodian/src/restore.rs:714`, `crates/custodian/tests/restore_open_fence.rs:730`).

Completing-session fencing (#842) and dedicated seeded DST coverage (#843) remain settled deferrals (`crates/custodian/src/restore.rs:722`, `crates/custodian/src/restore.rs:807`). A new Tier-1 disk-fault run is not indicated by this metadata-only change; observing the existing Tier-2 recovery campaign remains useful when the split restore work is integrated. Neither is a new merge condition.

### Advisory — adversary

# Adversarial review — #841 (809.3) restore fence for Open upload sessions

Verdict: **I could not refute the fix.** One test-strength regression the patch causes in a
neighbouring file (a judgment call because the brief froze that file), and two non-blocking notes.

## Findings

- NEEDS-HUMAN [human] — `crates/custodian/tests/restore_staged_report.rs:574-578` (child-1's
  leg `an_untrusted_staged_record_is_named_needs_no_human_and_is_not_a_clean_bill`): the leg's
  assertion (b), `!report.is_clean()` "over a chunk held on an untrusted record", **now passes
  for the wrong reason.** Its fixture seeds two `Open` sessions (`:504`, `:524`), which the
  new fence moves to `Aborting`. That sets `sessions_fenced: 2`, and `is_clean()` is false from
  that alone (`crates/custodian/src/restore.rs:286`). The premise the leg states at `:490` and
  checks at `:546-553` ("the held record is the run's ONLY finding") is no longer true.
  **Reproduced:** in a scratch copy I removed `&& self.staged_untrusted.is_empty()` from
  `is_clean()` (`restore.rs:288`). `cargo test -p wyrd-custodian` (every test binary, this leg
  included) still passes 100%. On the base the same mutant fails this leg, because nothing else
  makes that report unclean. The predicate is still pinned, but only by unit tests over
  hand-built reports in another crate (`crates/server/src/cli.rs:3224`, `:3068`). No test
  through the production `reconcile_after_restore` checks it any more. The fix is small: seed
  that leg's sessions as `Aborting`, or assert `sessions_fenced: 2` and check the untrusted
  verdict another way. But the brief allowed only the `segment_nonce` change in this file
  ("nothing else in it"), so a human has to approve the edit or a follow-up. That is why this is
  tagged `[human]`, not `[impl]`.

- Note (no action needed) — `docs/design/architecture/m4-first-deployment-blueprint.md:627-631`
  and `docs/design/architecture/06-runtime-view.md:65` list five reasons a session can be "NOT
  FENCED". The code has seven (`crates/custodian/src/restore.rs`, `SessionUnsettled`). The
  lists leave out `KeyNamesNoUpload` and `LostConflict`. The CLI paragraph prints each reason in
  words (`cli.rs` `unsettled_causes`), so an operator is not left stuck. This is a small docs
  mismatch, not worth a rebuild on its own.

- Note (not a refutation) — the `C4-diff-cov` row failed because "patch.diff does not apply on
  origin/main" (`gate-logs/C4-diff-cov.log`). The real base is main plus child-1 and child-2, so
  the gate ran against the wrong base and diff coverage was never measured. That is a harness
  problem, not evidence against the fix. The `LostConflict` branch (`restore.rs:777-779`) is the
  only fence branch I found that no test reaches.

## What I tried to refute, and could not

- **Red→green is real.** `gate-logs/C4-verify.log`: all 9 legs fail by assertion on the reverted
  base, then pass. The red run's `Debug` output shows child-1's fields, and the seeded sessions
  decode with the nonce, so the red base is the right one (main + child-1 + child-2). Every leg
  calls the production `reconcile_after_restore`. Nothing is re-implemented in the test.
- **F-atomic would catch a split fence in either order.** With the session written first, run
  `a2` leaves `Aborting` and no obligation. With the obligation written first, run `a1` leaves an
  orphan obligation. `assert_fenced` also requires a single applied batch to carry both writes.
- **The double's preconditions do not hide a missing guard.** Drop `require(key, read)`
  (`restore.rs:755`) and F-race sees `Aborting` over `Completing@4`. Drop
  `require_absent(obligation)` (`:756`) and F-collision sees the foreign `{session}` overwritten.
  The double's `apply` checks preconditions exactly as `Precondition` defines them
  (`crates/traits/src/lib.rs:1490-1498`).
- **Iteration 2's `clean=true` defect is fixed and pinned.** `restore.rs:1320` now
  uses `fence_finished && report.is_clean()`. The new otherwise-clean P3 variant asserts
  `clean=false`, `needs_human=true`, and it ran red on the base.
- **Iteration 2's untested `open_teardown` contract is now tested.** A `wyrd-core` unit test
  covers every non-`Open` state and `u64::MAX`, and round-trips key and payload through
  `decode_retire_obligation` (CI log line 900). C5 now reports 0 missed mutants.
- **The fenced record always decodes.** `open_teardown` skips `TryFrom` validation, but the only
  cross-field rule applies to `Completing` (`crates/core/src/multipart.rs:2312-2337`). So
  `Aborting@E+1`, built from a record that already decoded, is always valid, and its encoding is
  canonical. The test's `aborting()` helper asserts the byte-for-byte round trip.
- **An unknown commit outcome is never read as a conflict.** An `Err` from `commit` propagates
  as-is (`restore.rs:783-786`). `Conflict` arrives as `Ok(CommitOutcome::Conflict)` per the seam
  contract. Re-running after a commit that actually landed sees `Aborting@E+1` and skips it.
- **Paging** follows `walk_staged_range` exactly (`gc.rs:1714-1730`). The fence's writes go to
  keys already listed, or to `retire:`, so they cannot shift the cursor.
- **The writes-during-restore legs in staged_protection still exercise their race.** Their hooks
  fire on reads of the staged ranges, which happen before the fence, and each leg asserts its
  concurrent batch `Committed` (e.g. `staged_protection.rs:1268-1272`).
- **Rubric: not raised.** Bounded awaits: the comment follows the repo's existing #508/#636
  convention (`reconstruction.rs:611-613`, `restore.rs:922-923`). Tier-0 DST coverage: settled
  by the `// deferred: #843` marker. No new clock read.

### Advisory — code-review

No findings. The diff is clean on both advisory lenses: no introduced correctness bugs or actionable reuse, simplification, or efficiency issues found.

Reviewed the atomic fence and conflict handling (`crates/custodian/src/restore.rs:754`), shared paging (`crates/custodian/src/gc.rs:1734`), teardown construction and codec compatibility (`crates/core/src/multipart.rs:2264`), and bounded CLI reporting (`crates/server/src/cli.rs:1389`). The earlier incomplete-run verdict defect is fixed (`crates/custodian/src/restore.rs:1320`) and covered by an otherwise-clean failure test (`crates/custodian/tests/restore_open_fence.rs:816`); the public teardown API now directly tests non-Open states and epoch exhaustion (`crates/core/src/multipart.rs:5084`).

Validation used the frozen gate evidence: CI passed, all nine regression tests ran red before the fix and green afterward, and mutation testing reported 20 caught and 24 unviable mutants, with none surviving. Diff coverage was not measured because its gate could not apply the patch to origin/main. No tests were re-run and no target files were changed.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [x] Validation — fitness-to-purpose — Confirm that the documented writers-stopped recovery procedure and handling of unfenced uploads meet operator recovery needs — automated state-transition tests cannot establish operational fitness (`docs/design/architecture/m4-first-deployment-blueprint.md:627`, `docs/design/architecture/m4-first-deployment-blueprint.md:634`).
- [x] `crates/custodian/tests/restore_staged_report.rs:574-578` (child-1's leg `an_untrusted_staged_record_is_named_needs_no_human_and_is_not_a_clean_bill`): the leg's assertion (b), `!report.is_clean()` "over a chunk held on an untrusted record", **now passes for the wrong reason.** Its fixture seeds two `Open` sessions (`:504`, `:524`), which the new fence moves to `Aborting`. That sets `sessions_fenced: 2`, and `is_clean()` is false from that alone (`crates/custodian/src/restore.rs:286`). The premise the leg states at `:490` and checks at `:546-553` ("the held record is the run's ONLY finding") is no longer true. **Reproduced:** in a scratch copy I removed `&& self.staged_untrusted.is_empty()` from `is_clean()` (`restore.rs:288`). `cargo test -p wyrd-custodian` (every test binary, this leg included) still passes 100%. On the base the same mutant fails this leg, because nothing else makes that report unclean. The predicate is still pinned, but only by unit tests over hand-built reports in another crate (`crates/server/src/cli.rs:3224`, `:3068`). No test through the production `reconcile_after_restore` checks it any more. The fix is small: seed that leg's sessions as `Aborting`, or assert `sessions_fenced: 2` and check the untrusted verdict another way. But the brief allowed only the `segment_nonce` change in this file ("nothing else in it"), so a human has to approve the edit or a follow-up. That is why this is tagged `[human]`, not `[impl]`.
- [x] **The fence can satisfy the criteria while overwriting an existing retirement obligation.** `brief.md:26-43` tests installation, atomicity and a changed session; `brief.md:94-104` specifies the session precondition but never requires the retirement key to be absent. The target design explicitly requires “Installation is `require_absent(retire:<mode>:<token>)`, never a blind put” and explains that overwriting loses reclamation evidence (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:369-373`). Add that precondition to the batch contract and a collision leg: seed the intended retirement key with different bytes, require both existing records to remain unchanged, and specify how the conflict is classified and reported. Key/payload agreement alone does not prevent overwrites.
- [x] **Running after Pass 3 does not preserve every verdict on a fence fault.** P3 promises that a fence fault “never hides the pass's verdicts,” but checks only `DANGLING` (`brief.md:55-59`); the invariant repeats the broader promise at `brief.md:78-79`. Unlike dangling and misplaced chunks, under-replicated chunks only enter the returned report (`crates/custodian/src/restore.rs:568-584`); their audit count is emitted by `emit_summary` (`:1021-1032`). The proposed fence sits before that summary (`brief.md:94-97`), so propagating its error can lose this verdict, and the CLI also skips `restore_verdict` on error (`crates/server/src/cli.rs:1187-1196`). Specify an error-path reporting mechanism and extend P3 with an under-replicated chunk whose verdict must remain observable when fencing fails.
- [x] **The CLI criterion conflicts with the required helper's bounded output.** `brief.md:62-65` requires a paragraph “naming every session it could not fence, through `named_records`.” That helper names at most 20 records and replaces the remainder with a count (`crates/server/src/cli.rs:1380-1398`); the existing test explicitly requires omitted names beyond that bound (`:2999-3023`). With 21 unsettled sessions, the stated criterion fails. Revise it to require the bounded names plus remainder count in the CLI and every name in the report/audit trail, and specify an over-limit test. Otherwise satisfying “every” invites an undeclared change to shared CLI truncation behavior.
- [x] `gate-logs/T4-batch-review.log`: the gating T4 review also blocks three times on missing seeded Tier-0 DST coverage for the fence (rubric: "a new destructive or concurrent path lands with seeded Tier-0 DST coverage"). The brief puts DST out of scope on purpose (child-5), but the patch has no `// deferred: #N` marker and no recorded rejection. So under the rubric's own deferral rule the finding is not yet settled, and T4 stays red. A human must record the deferral against child-5's tracking issue, or decide the fence can't land without it.

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
- Plan advisory: 3 finding(s); brief revised: yes (plan-advisory-*.md)
- (empty is the common case)
- C4 diff-coverage gate applies patch.diff to bare origin/main, so stacked bundles (here #841 on #839+#840, see `stack-base`) never get coverage measured ("patch.diff does not apply on origin/main"); decide how the gate should pick up the stack base — harness issue, upstream to eduralph/pdca-harness.
- Follow-up: `restore_staged_report.rs` leg `an_untrusted_staged_record_is_named_needs_no_human_and_is_not_a_clean_bill` passes for the wrong reason since #841 (its Open sessions get fenced, so `is_clean()` is false regardless); seed them `Aborting` or assert `sessions_fenced: 2` so `staged_untrusted` is pinned through `reconcile_after_restore` again.
