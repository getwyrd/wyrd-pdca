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
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass — run-verify.sh: PASS — red without the fix, green with it (8 test(s) ran red).
- C4 diff coverage: changed lines executed by the patch's tests: fail — diff coverage not measured — patch.diff does not apply on origin/main
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): fail — 44 mutants tested in 5m: 5 missed, 15 caught, 24 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 6 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_841/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 13.01s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

One audit-verdict defect remains in #841’s fix to fence resurrected Open upload sessions atomically with obligations retiring their staged residue and committed parts.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The Open-only boundary and atomicity, race, collision, idempotence, paging, and audit requirements are falsifiable; Completing and new DST coverage have explicit downstream owners (`brief.md:21`, `brief.md:148`). |
| C2 Reproduction (red pre-fix) | PASS | Stashing the tracked fix while keeping the new test independently produced eight assertion failures, with no compile failure (`reviewer-red.log:144`, `reviewer-red.log:216`; `crates/custodian/tests/restore_open_fence.rs:498`). |
| C3 Change | PASS | The patch stays within the authorized nine files and 89,236 bytes; its conditional batch prevents both a stale-session overwrite and retirement-key clobbering (`brief.md:147`; `crates/custodian/src/restore.rs:751`). |
| C4 Verification (red→green) | PASS | Restoring the patch independently made all eight specified tests pass; broader tests and existing DST also passed, with the local cargo-deny host limitation and unmeasured diff coverage qualified below (`reviewer-green.log:16`, `reviewer-dst.log:541`, `gate-logs/C4-ci.log:3829`). |
| C5 Causal adequacy | NEEDS-HUMAN [impl] | Correct the audit verdict on an interrupted fence—an otherwise healthy store with its first upload still Open is certified `clean=true`; the executable counterexample contradicts the restore-summary guarantee (`crates/custodian/src/restore.rs:1315`, `reviewer-audit-probe.log:12`). |
| T1 Structure | PASS | The fence uses the MetadataStore seam and the shared codec; private obligation fields keep the retirement key and payload coupled, without introducing a concrete backend dependency (`crates/custodian/src/restore.rs:735`, `crates/core/src/multipart.rs:3584`). |
| T2 Shape | PASS | The API and operator documentation describe the changed restore behavior; CLI naming retains the existing 20-record bound and counts the remainder (`docs/design/architecture/06-runtime-view.md:65`, `crates/server/src/cli.rs:1374`, `crates/server/src/cli.rs:1429`). |
| T3 Runtime | PASS | Bounded paging and one conditional commit per Open session preserve the store contract; conflicts are classified once, unknown outcomes propagate, and no new clock source is introduced (`crates/custodian/src/restore.rs:720`, `crates/custodian/src/restore.rs:759`). |
| T4 Contribution | N/A | Publication artifacts are intentionally drafted after Check; the substantive contribution audit is owed at publish, as the deferred gate explicitly records (`gate-logs/T4-contribution.log:10`). |
| T5 Judgment | PASS | The production-path tests assert exact records and atomic batches, exercise races/collisions, and preserve second-pass findings; the duplicate batch-review claims and tracked deferrals are adjudicated below (`crates/custodian/tests/restore_open_fence.rs:363`, `crates/custodian/tests/restore_open_fence.rs:770`, `brief.md:149`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Accept the stopped-writer restore procedure and operator recovery instructions for this Open-only stage—resuming multipart service while named uploads remain unfenced can still publish over reclaimed data (`docs/design/architecture/06-runtime-view.md:65`, `crates/server/src/cli.rs:1383`). |

Source citations are relative to `$PDCA_TARGET` (`./target`); brief, gate-log, and reviewer-log citations are relative to this review directory. The target contained the prerequisite patches and compiled successfully. Reverse-application checking confirmed that the supplied patch remained applied after the red→green run; no production or submitted test changes were made.

The distinct implementation finding is **P2: an interrupted session fence must not emit a clean restore verdict**. On an empty fragment fleet with one canonical `Open@3` session, inject a failure in the first fence commit. The production pass returns `Err`, leaves the session byte-identical, and writes no obligation, but its summary contains `clean=true`, `needs_human=false`, and `fence_finished=false` (`reviewer-audit-probe.log:12`). `emit_summary` now runs on this error path (`crates/custodian/src/restore.rs:713`), but its `clean` field uses only the accumulated report, which is still otherwise empty (`crates/custodian/src/restore.rs:284`, `crates/custodian/src/restore.rs:1315`). Require fence completion for the clean audit verdict and cover this otherwise-healthy failure case. The CLI still exits through its error path (`crates/server/src/cli.rs:1196`); this finding concerns the contradictory structured audit verdict, not a successful CLI exit. A retryable fence error need not independently become a permanent human-repair classification.

The submitted P3 test misses this case because it seeds dangling and under-replicated chunks, which already make the report unclean, and checks the summary’s prose rather than its `clean` field (`crates/custodian/tests/restore_open_fence.rs:813`, `crates/custodian/tests/restore_open_fence.rs:840`). The independent probe is a scratch-only test linked against the unchanged production crates. Run it from this review directory:

```sh
CARGO_TARGET_DIR="$PWD/pdca-reviewer-841-build" cargo test --offline \
  --manifest-path "$PWD/pdca-reviewer-841-probe/Cargo.toml" \
  --test audit_probe reviewer_failed_first_fence_cannot_certify_clean \
  -- --exact --nocapture
```

It compiles and fails the assertion that `clean` must be false (`reviewer-audit-probe.log:8`, `reviewer-audit-probe.log:15`).

The verification evidence supports the specified fence behavior, with these limits:

- **Independent executions:** eight failures before the fix and eight passes after it; workspace formatting, clippy, build, tests (including the CLI bound/agreement tests and both existing restore suites), and cargo-machete passed. Separate conformance and statics checks passed, and `cargo xtask dst` passed its existing 50-seed campaign. Both TiKV feature clippy commands also passed (`reviewer-ci.log:18`, `reviewer-ci.log:1384`, `reviewer-ci.log:2072`, `reviewer-conformance.log:1`, `reviewer-statics.log:3`, `reviewer-dst.log:541`, `reviewer-tikv.log:200`).
- **Host caveat:** the local full CI invocation stopped at cargo-deny because the sandbox cannot acquire `/home/eddie/.cargo/advisory-dbs/db.lock` (`reviewer-ci.log:3128`). The frozen gate log shows all three deny checks succeeding and the full CI run finishing successfully (`gate-logs/C4-ci.log:3221`, `gate-logs/C4-ci.log:3232`, `gate-logs/C4-ci.log:3235`, `gate-logs/C4-ci.log:3829`). This is not a patch failure or an unmet dependency in the frozen gate run. Both external dependencies named by the brief—typos and the docs renderer—ran successfully here as well (`reviewer-ci.log:2`, `reviewer-ci.log:10`).
- **Coverage caveat:** the frozen diff-coverage wrapper could not apply the stacked patch to `origin/main` (`gate-logs/C4-diff-cov.log:10`); coverage was not measured. The supplied target includes children #839/#840 and independently compiled and passed. No patch-application or compilation defect is inferred from that base mismatch.
- **Mutation evidence:** the frozen scanner reports 44 mutants, with five missed, 15 caught, and 24 unviable (`gate-logs/C5-mutants.log:13`). The missed edits affect the Open-state check, Aborting transition, and obligation key. Each would violate the custodian test’s exact-session or exact-key assertions (`crates/custodian/tests/restore_open_fence.rs:363`); the log does not establish which downstream tests were selected. I did not rerun the unavailable instance-scoped mutation wrapper or claim the five mutants were independently killed. They do not demonstrate an additional production defect.

The frozen batch review’s six entries reduce to one retained defect and one settled deferral (`gate-logs/T4-batch-review.log:10`). Its three clean-verdict entries describe the independently reproduced defect above. Its three requests for new seeded DST coverage conflict with the brief’s explicit child-5 allocation (`brief.md:149`) and are settled under the repository’s deferral protocol (`AGENTS.md:200`). Completing-session fencing is likewise explicitly deferred to #842 (`crates/custodian/src/restore.rs:804`). No capability probe or load-time symptom guard was introduced. Tier-1 disk-fault and Tier-2 kill/reconstruct follow-up can accompany the downstream restore campaign; this stage changes metadata fencing and performs no fragment deletion.

The prior-art check ran by all nine affected paths against merged GitHub history and the latest 100 closed PRs, extending the brief’s last-40 check. File lists were inspected for all ten unmerged PRs in that window and the one post-triage merge (`reviewer-prior-art-summary.log:2`). The only unmerged overlap, #647, concerns segmented chunk-map containment, not upload fences; the new merge #844 touches none of these paths (`reviewer-prior-art-summary.log:12`, `reviewer-prior-art-summary.log:22`). Full API evidence is retained in `reviewer-prior-art-detail.json`. The rejected #809/#664 approaches documented in `brief.md:179` are addressed by placing the fence after Pass 3 and preserving second-pass findings (`crates/custodian/src/restore.rs:710`, `crates/custodian/tests/restore_open_fence.rs:770`).

### Advisory — adversary

# Adversarial review — #841 (809.3) restore fence for `Open` upload sessions

Advisory only. Re-ran the proof myself in a scratch copy of `$PDCA_TARGET` (post-fix tree):
`cargo test -p wyrd-custodian --test restore_open_fence` → 8/8 green. The red half in
`gate-logs/C4-verify.log` is real: all 8 legs fail by assertion on the base (session still
`Open@3`, no `retire:` key, `Ok` where `Err` is wanted, "the race was not exercised"). None of them
fail for a compile reason. Every leg drives the production `reconcile_after_restore`.

## Findings

- NEEDS-HUMAN [impl] — `crates/custodian/src/restore.rs:1315` (called from `:713`): when the
  fence fails, the summary line (the only audit record of the counts on that path) still prints
  `clean = report.is_clean()`, and `is_clean()` ignores `fence_finished`. **Concrete failing case,
  reproduced:** a store with nothing wrong in it except one `Open@3` session whose fence commit
  returns `CommitUnknownResult`. The pass returns `Err`, and the summary reads
  `clean=true needs_human=false fence_finished=false message="post-restore reconciliation
  INCOMPLETE — the session fence did not finish …"`. So the line certifies the run as clean while
  the message says it is not finished, and an `Open` session may still be live. The P3 leg hides
  this: `restore_open_fence.rs:815` seeds a dangling chunk, so `needs_human` is true and `clean`
  is false for a different reason. The leg checks `under_replicated`, `dangling` and the message
  (`:838`, `:844`) but never `clean`. Fix: `clean = fence_finished && report.is_clean()`, and add
  a P3 variant with no other finding that asserts `summary["clean"] == "false"`. (The T4 batch
  review found the same thing three times. This confirms it by running it.)

- NEEDS-HUMAN [impl] — `crates/core/src/multipart.rs:2265`: the "`None` unless `Open`" contract
  of the new public `SessionRecord::open_teardown` is not tested anywhere. Its only caller,
  `crates/custodian/src/restore.rs:800`, calls it from inside the `SessionState::Open {}` arm, so
  the non-`Open` branch never runs. If that guard were deleted, `open_teardown` would return a
  `{session, all}` teardown for a `Completing` session (the exact shape `0016:2187` limits to
  `Open`), and every test would still pass. Downstream callers (#656 Abort, the reaper) will rely
  on that guard. This is also why C5 reports 5 survivors (`multipart.rs:2265`, `:2272`, `:3622`).
  I applied the `delete !` and `key -> vec![]` mutants by hand, and both are killed by the
  custodian tests (9 and 7 failures). They survive only because the mutant run uses `wyrd-core`'s
  own tests, which never call the new API. Fix: a `wyrd-core` unit test that calls
  `open_teardown` on each state and at `u64::MAX`, and round-trips `obligation().key()` /
  `payload()` through `decode_retire_obligation`.

- NEEDS-HUMAN [human] — `gate-logs/T4-batch-review.log`: the gating T4 review also blocks three
  times on missing seeded Tier-0 DST coverage for the fence (rubric: "a new destructive or
  concurrent path lands with seeded Tier-0 DST coverage"). The brief puts DST out of scope on
  purpose (child-5), but the patch has no `// deferred: #N` marker and no recorded rejection. So
  under the rubric's own deferral rule the finding is not yet settled, and T4 stays red. A human
  must record the deferral against child-5's tracking issue, or decide the fence can't land
  without it.

## Not a refutation (context)

- `C4-diff-cov` "does not apply on origin/main" is expected. This bundle sits on main + child-1 +
  child-2 (brief line 5), so changed-line coverage was not measured at all. It is not evidence
  either way.
- Attempted and could **not** break:
  - Splitting the fence across two commits, in either order, is caught by F-atomic
    (`restore_open_fence.rs` run 1 / run 2) and by `assert_fenced`'s one-commit check.
  - A blind put of the obligation is caught by F-collision.
  - Dropping `require(key, read)` is caught by F-race (it overwrites `Completing@4`).
  - Fencing before Pass 3 is caught by P3's `DANGLING` line.
  - Paging: `fence_open_sessions` (`restore.rs:720`) mirrors `gc::walk_staged_range`, and
    `checked_page` refuses an empty page that still has a cursor, so a cut-short listing
    becomes an error, not a silent stop.
  - Unknown outcome that actually landed: the re-run sees `Aborting@E+1`, skips it, and writes
    nothing (probed).
  - The fence's obligation does not collide with GC's `retirement_draining` (`gc.rs:756`): that
    only reads `retire:bytes:` keys named by a mark's event, and restore marks carry none.
  - A key that names no upload is now named twice, in `unresolvable` and in
    `sessions_unsettled` (probed). The `KeyNamesNoUpload` doc says this is intended, and it is
    harmless.

### Advisory — code-review

- NEEDS-HUMAN [impl] — **A failed fence can emit `clean=true`.** `crates/custodian/src/restore.rs:713` now emits the summary on fence errors, but `crates/custodian/src/restore.rs:1315` derives `clean` solely from the partial report. With one `Open` session and otherwise healthy metadata, failure of the first fence commit leaves every report counter/list empty, so the audit reports `clean=true` while the session may remain publishable. The `INCOMPLETE` message does not correct that structured verdict. Require `fence_finished && report.is_clean()` for the audit field. Add an otherwise-clean fence-failure assertion: the existing fault test at `crates/custodian/tests/restore_open_fence.rs:815` seeds a dangling chunk, which independently makes `is_clean()` false and masks this bug.

No additional correctness or substantive reuse, simplification, or efficiency findings. Reviewed against the target source and frozen gate evidence; tests were not rerun.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C5 Causal adequacy — Correct the audit verdict on an interrupted fence—an otherwise healthy store with its first upload still Open is certified `clean=true`; the executable counterexample contradicts the restore-summary guarantee (`crates/custodian/src/restore.rs:1315`, `reviewer-audit-probe.log:12`).
- [ ] Validation — fitness-to-purpose — Accept the stopped-writer restore procedure and operator recovery instructions for this Open-only stage—resuming multipart service while named uploads remain unfenced can still publish over reclaimed data (`docs/design/architecture/06-runtime-view.md:65`, `crates/server/src/cli.rs:1383`).
- [ ] `crates/custodian/src/restore.rs:1315` (called from `:713`): when the fence fails, the summary line (the only audit record of the counts on that path) still prints `clean = report.is_clean()`, and `is_clean()` ignores `fence_finished`. **Concrete failing case, reproduced:** a store with nothing wrong in it except one `Open@3` session whose fence commit returns `CommitUnknownResult`. The pass returns `Err`, and the summary reads `clean=true needs_human=false fence_finished=false message="post-restore reconciliation INCOMPLETE — the session fence did not finish …"`. So the line certifies the run as clean while the message says it is not finished, and an `Open` session may still be live. The P3 leg hides this: `restore_open_fence.rs:815` seeds a dangling chunk, so `needs_human` is true and `clean` is false for a different reason. The leg checks `under_replicated`, `dangling` and the message (`:838`, `:844`) but never `clean`. Fix: `clean = fence_finished && report.is_clean()`, and add a P3 variant with no other finding that asserts `summary["clean"] == "false"`. (The T4 batch review found the same thing three times. This confirms it by running it.)
- [ ] `crates/core/src/multipart.rs:2265`: the "`None` unless `Open`" contract of the new public `SessionRecord::open_teardown` is not tested anywhere. Its only caller, `crates/custodian/src/restore.rs:800`, calls it from inside the `SessionState::Open {}` arm, so the non-`Open` branch never runs. If that guard were deleted, `open_teardown` would return a `{session, all}` teardown for a `Completing` session (the exact shape `0016:2187` limits to `Open`), and every test would still pass. Downstream callers (#656 Abort, the reaper) will rely on that guard. This is also why C5 reports 5 survivors (`multipart.rs:2265`, `:2272`, `:3622`). I applied the `delete !` and `key -> vec![]` mutants by hand, and both are killed by the custodian tests (9 and 7 failures). They survive only because the mutant run uses `wyrd-core`'s own tests, which never call the new API. Fix: a `wyrd-core` unit test that calls `open_teardown` on each state and at `u64::MAX`, and round-trips `obligation().key()` / `payload()` through `decode_retire_obligation`.
- [ ] `gate-logs/T4-batch-review.log`: the gating T4 review also blocks three times on missing seeded Tier-0 DST coverage for the fence (rubric: "a new destructive or concurrent path lands with seeded Tier-0 DST coverage"). The brief puts DST out of scope on purpose (child-5), but the patch has no `// deferred: #N` marker and no recorded rejection. So under the rubric's own deferral rule the finding is not yet settled, and T4 stays red. A human must record the deferral against child-5's tracking issue, or decide the fence can't land without it.
- [ ] **A failed fence can emit `clean=true`.** `crates/custodian/src/restore.rs:713` now emits the summary on fence errors, but `crates/custodian/src/restore.rs:1315` derives `clean` solely from the partial report. With one `Open` session and otherwise healthy metadata, failure of the first fence commit leaves every report counter/list empty, so the audit reports `clean=true` while the session may remain publishable. The `INCOMPLETE` message does not correct that structured verdict. Require `fence_finished && report.is_clean()` for the audit field. Add an otherwise-clean fence-failure assertion: the existing fault test at `crates/custodian/tests/restore_open_fence.rs:815` seeds a dangling chunk, which independently makes `is_clean()` false and masks this bug.
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 6 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_841/review-b
- [ ] **The fence can satisfy the criteria while overwriting an existing retirement obligation.** `brief.md:26-43` tests installation, atomicity and a changed session; `brief.md:94-104` specifies the session precondition but never requires the retirement key to be absent. The target design explicitly requires “Installation is `require_absent(retire:<mode>:<token>)`, never a blind put” and explains that overwriting loses reclamation evidence (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:369-373`). Add that precondition to the batch contract and a collision leg: seed the intended retirement key with different bytes, require both existing records to remain unchanged, and specify how the conflict is classified and reported. Key/payload agreement alone does not prevent overwrites.
- [ ] **Running after Pass 3 does not preserve every verdict on a fence fault.** P3 promises that a fence fault “never hides the pass's verdicts,” but checks only `DANGLING` (`brief.md:55-59`); the invariant repeats the broader promise at `brief.md:78-79`. Unlike dangling and misplaced chunks, under-replicated chunks only enter the returned report (`crates/custodian/src/restore.rs:568-584`); their audit count is emitted by `emit_summary` (`:1021-1032`). The proposed fence sits before that summary (`brief.md:94-97`), so propagating its error can lose this verdict, and the CLI also skips `restore_verdict` on error (`crates/server/src/cli.rs:1187-1196`). Specify an error-path reporting mechanism and extend P3 with an under-replicated chunk whose verdict must remain observable when fencing fails.
- [ ] **The CLI criterion conflicts with the required helper's bounded output.** `brief.md:62-65` requires a paragraph “naming every session it could not fence, through `named_records`.” That helper names at most 20 records and replaces the remainder with a count (`crates/server/src/cli.rs:1380-1398`); the existing test explicitly requires omitted names beyond that bound (`:2999-3023`). With 21 unsettled sessions, the stated criterion fails. Revise it to require the bounded names plus remainder count in the CLI and every name in the report/audit trail, and specify an over-limit test. Otherwise satisfying “every” invites an undeclared change to shared CLI truncation behavior.

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
- Iteration delta (if iterating): Auto-iterate (round 1): rebuilding for the implementation-level findings — C5 Causal adequacy — Correct the audit verdict on an interrupted fence—an otherwise healthy store with its first upload still Open is certified `clean=true`; the executable counterexample contradicts the restore-summary guarantee (`crates/custodian/src/restore.rs:1315`, `reviewer-audit-probe.log:12`).; `crates/custodian/src/restore.rs:1315` (called from `:713`): when the fence fails, the summary line (the only audit record of the counts on that path) still prints `clean = report.is_clean()`, and `is_clean()` ignores `fence_finished`. **Concrete failing case, reproduced:** a store with nothing wrong in it except one `Open@3` session whose fence commit returns `CommitUnknownResult`. The pass returns `Err`, and the summary reads `clean=true needs_human=false fence_finished=false message="post-restore reconciliation INCOMPLETE — the session fence did not finish …"`. So the line certifies the run as clean while the message says it is not finished, and an `Open` session may still be live. The P3 leg hides this: `restore_open_fence.rs:815` seeds a dangling chunk, so `needs_human` is true and `clean` is false for a different reason. The leg checks `under_replicated`, `dangling` and the message (`:838`, `:844`) but never `clean`. Fix: `clean = fence_finished && report.is_clean()`, and add a P3 variant with no other finding that asserts `summary["clean"] == "false"`. (The T4 batch review found the same thing three times. This confirms it by running it.); `crates/core/src/multipart.rs:2265`: the "`None` unless `Open`" contract of the new public `SessionRecord::open_teardown` is not tested anywhere. Its only caller, `crates/custodian/src/restore.rs:800`, calls it from inside the `SessionState::Open {}` arm, so the non-`Open` branch never runs. If that guard were deleted, `open_teardown` would return a `{session, all}` teardown for a `Completing` session (the exact shape `0016:2187` limits to `Open`), and every test would still pass. Downstream callers (#656 Abort, the reaper) will rely on that guard. This is also why C5 reports 5 survivors (`multipart.rs:2265`, `:2272`, `:3622`). I applied the `delete !` and `key -> vec![]` mutants by hand, and both are killed by the custodian tests (9 and 7 failures). They survive only because the mutant run uses `wyrd-core`'s own tests, which never call the new API. Fix: a `wyrd-core` unit test that calls `open_teardown` on each state and at `u64::MAX`, and round-trips `obligation().key()` / `payload()` through `decode_retire_obligation`.; **A failed fence can emit `clean=true`.** `crates/custodian/src/restore.rs:713` now emits the summary on fence errors, but `crates/custodian/src/restore.rs:1315` derives `clean` solely from the partial report. With one `Open` session and otherwise healthy metadata, failure of the first fence commit leaves every report counter/list empty, so the audit reports `clean=true` while the session may remain publishable. The `INCOMPLETE` message does not correct that structured verdict. Require `fence_finished && report.is_clean()` for the audit field. Add an otherwise-clean fence-failure assertion: the existing fault test at `crates/custodian/tests/restore_open_fence.rs:815` seeds a dangling chunk, which independently makes `is_clean()` false and masks this bug.; T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 6 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_841/review-b. 4 finding(s) needing human judgment were deferred to sign-off, not addressed here.
- By / date: auto-iterate / 2026-09-30

## 10. Act candidates (hints for the next Act review)
- Plan advisory: 3 finding(s); brief revised: yes (plan-advisory-*.md)
- (empty is the common case)
