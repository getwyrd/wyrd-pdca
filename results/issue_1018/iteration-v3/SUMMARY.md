# Result — issue 1018 / mpu-operator-abort

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect: #625's reaper deliberately **skips** a session whose `clock_source` it does not
  own (`0016:1889-1907`, `:1984-1998`). For a foreign-clocked `Open` or `Completing` session, no
  actor in the system can move it. It keeps its records, its staged bytes and its admission slot
  forever. `MAX_SESSIONS` of them is a permanent `503 SlowDown` on `CreateMultipartUpload` with
  no exit. 0016 names the operator as the exit (`0016:255-262`), but no operator verb exists.
  `crates/server/src/cli.rs` has no multipart or session-management subcommand
  (`:412-428`).
- Success criterion: one new test file (see `Test file`), every leg asserted on durable
  store state or on the subprocess's exit status and stderr.
  **(A) Operator abort of an `Open` session.** Seed an `Open@E` session with staged parts.
  - `wyrd mpu abort --upload-id <ID> --data-dir <DIR>` exits 0.
  - The session is left `Aborting@E+1` with `retire:bytes:{session}` installed (the Open
    door's payload, `0016:664`).
  - A following pass of #625's operator one-shot, invoked as argv, tears it down to zero:
    records gone, `orphan:` marks written, `mpuctl.count` decremented. This shows the verb hands
    off to the ordinary clock-free path instead of growing a second teardown.
  **(B) Abort of a `Completing` session ALSO retires that attempt's segments.** Seed a
  `Completing@E` session that has already written `seg:<g>:<E>:*` records, plus a decoy
  `seg:<g>:<E+2>:*` range.
  - The verb leaves the session `Aborting@E+1` **and** `retire:records:{seg:<g>:<E>}` present.
  - `retire:bytes:{session, parts}` is installed too (`0016:665`).
  - The decoy later-epoch range is **not** named by any obligation (F18).
  - A following one-shot pass drains the named range empty and leaves the decoy intact.
  **(C) Abort refuses where 0016 says it must, and the refusal is DISCRIMINATING.**
  - A `Completed` session (tombstone) and an absent session are **not** abortable.
  - An already-`Aborting` session is idempotent: exit 0, no second fence, no double obligation,
    epoch unchanged.
  - **"Store unchanged + non-zero" is not an oracle here.** On the base, an unknown command
    also leaves the store untouched and exits non-zero (**2**, `cli.rs:419-423`).
  - Each refusal therefore asserts the **handled** exit status **1** (`ExitCode::FAILURE`,
    `cli.rs:429-433`) **and** a stderr message naming the session's state. For `Completed`, the
    message says abort does not apply to a terminal session. (The pointer to the expiry verb is
    added and asserted by the sibling child.)
  **One-batch atomicity is REQUIRED but deliberately NOT in the binding criterion.**
  - Both doors MUST commit their state change and their obligation(s) in one batch
    (`0016:664-665`). For the `Completing` door, no interleaving may leave a fenced session whose
    segments have no reclaimer (the X57 hole).
  - An end-state scan cannot tell one batch from two, so Do owes atomicity outside C4, in two
    parts:
    - (i) seeded DST cases **appended to the existing** `crates/dst/tests/custodian.rs`, one per
      door, that interrupt between the two commits and show no reachable unreclaimed state;
    - (ii) the batch construction, stated in `build-notes.md` for the reviewer.
  - If Do cannot construct the interleaving, say so and record the property as a code-read
    obligation. Do not claim a test proves it.
- Repo + branch target: getwyrd/wyrd @ main
- Scope: 
  - **The `wyrd mpu` subcommand group** in `crates/server/src/cli.rs`: a dispatch arm, a
    `usage()` line, `ParsedArgs` / `VALUELESS_FLAGS` conventions (`:2244-2284`), and
    `cmd_custodian`'s stray-positional refusal and its "a value after a valueless flag does NOT
    disable it" rule (`:913-930`).
  - **The `abort` verb.** Exact argv, verbatim: `wyrd mpu abort --upload-id <ID> --data-dir
    <DIR>`, plus the role's standard `--metadata-backend <B>` and log flags. Flags only, exactly
    one `--upload-id`, no clock-source flag.
  - **The fence.** It fences `Open@E → Aborting@E+1` or `Completing@E → Aborting@E+1` in **one**
    batch preconditioned on the session record's exact bytes (`WriteBatch::require`,
    `crates/traits/src/lib.rs:825-843`). **The two doors carry different payloads and must not
    be conflated:**
    - `Open` gets 1 put + `retire:bytes:{session}` (`0016:664`);
    - `Completing` gets 1 put + `retire:bytes:{session, parts}` + `retire:records:{seg:<g>:<E>}`
      for **that fence epoch only** (`0016:665`).
  - **Reuse, don't rewrite.** The `Completing` door is "one shape for all three doors" with the
    restore fence and the `W_session` edge: **reuse** the path #658 landed. Do not write a third
    copy.
  - **Refusals.** Refuse `Completed` / absent sessions with the handled error. Be idempotent
    against an already-`Aborting` session.
  - **Tests and docs:**
    - the DST cases appended to `crates/dst/tests/custodian.rs`;
    - the living architecture docs (`docs/design/architecture/06-runtime-view.md`,
      `08-crosscutting-concepts.md`), updated for the `mpu` group and the abort verb, including
      grant term (iii). This is a merge requirement (`AGENTS.md` "Docs currency").

  **Out of scope:**
  - the `expire` verb, the session-record field, and **any** edit under `crates/custodian/src/`
    (the sibling child);
  - the alarm;
  - the reaper's windows, clock guard and teardown (#625);
  - the record classes and state machine (#693/#658/#659);
  - any S3 wire-visible verb (#508);
  - the 0008 management plane;
  - FU-2 / FU-3 / FU-4;
  - any file under `docs/design/adr/` or `docs/design/specs/`, and any edit to 0016.

  Anything else is a Check §6 item, not a silent edit to a layer below.

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: Fixed
- Confidence: medium
- Recommendation: (set by Do)

## 3. Correctness (Check — chain)
- C1 Spec: none — brief.md
- C2 Reproduction (red pre-fix): none — (no gate configured)
- C3 Change: none — patch.diff
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): pass — xtask ci: all checks passed
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass — run-verify.sh: PASS — red without the fix, green with it (6 test(s) ran red).
- C4 diff coverage: changed lines executed by the patch's tests: fail — diff coverage not measured — patch.diff does not apply on origin/main
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): pass — 49 mutants tested in 12m: 36 caught, 13 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 12 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_1018/review
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 30.94s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Review of #1018’s single-upload operator abort: the atomic fence passes independent red→green verification, but recovery-command and subprocess-bound defects need correction.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The brief defines discriminating durable-state and exit-status criteria, an explicit interim-CLI grant, and settled carry-forward decisions; scope and acceptance are assessable (`brief.md:26`, `brief.md:95`, `brief.md:188`). |
| C2 Reproduction (red pre-fix) | PASS | Independently stashing the production changes while retaining the new test produced six executed failures, each reaching the base’s unknown `mpu` command; this is behavioral red, not a compile failure (`reviewer-evidence/red.log:7`, `reviewer-evidence/red.log:339`). |
| C3 Change | PASS | The change stays within the operator-abort work, reuses the existing fence/teardown seams, and updates both living architecture documents; the sibling expiry/alarm and custodian implementation are untouched (`crates/core/src/multipart/abort.rs:411`, `docs/design/architecture/06-runtime-view.md:96`, `patch.diff:3435`). |
| C4 Verification (red→green) | PASS | Restoring the patch produced six passing subprocess tests; 25 core tests and seven focused DST tests also passed, with the full CI result supported by its frozen log; coverage remains unmeasured on the older comparison base (`reviewer-evidence/restored-green.log:15`, `reviewer-evidence/core-abort.log:120`, `reviewer-evidence/dst-abort.log:249`, `gate-logs/C4-ci.log:5100`). |
| C5 Causal adequacy | PASS | The missing operator transition is supplied without weakening the foreign-clock rule: exact-byte fencing and every reclamation obligation share one commit, and the Completing range stays attempt-specific; no capability-probe symptom guard was introduced (`crates/core/src/multipart/abort.rs:95`, `crates/core/src/multipart/abort.rs:440`, `crates/core/src/multipart/abort.rs:526`). |
| T1 Structure | PASS | Backend wiring remains in server and lifecycle logic uses the existing MetadataStore seam; no new crate, shared mutable global, or independent teardown subsystem is introduced (`crates/server/src/cli.rs:3184`, `crates/core/src/multipart/abort.rs:486`). |
| T2 Shape | PASS | The command rejects unknown/repeated flags and option-shaped values before opening a store, preserves intentional trailing-value usage exit 2, and documents the interim management-API migration (`crates/server/src/cli.rs:3110`, `crates/server/src/cli.rs:3473`, `docs/design/architecture/06-runtime-view.md:96`). |
| T3 Runtime | FAIL | Recovery commands can lose FoundationDB cluster identity or required client configuration, and the unconditional retry instruction cannot resolve a target held for whole-fleet repair; these affect an operator’s ability to complete recovery (F1–F3; `crates/server/src/cli.rs:3268`, `crates/server/src/cli.rs:3276`, `crates/server/src/cli.rs:3370`). |
| T4 Contribution | NEEDS-HUMAN | Establish prior art by affected path against merged history and closed/rejected work — all eight path queries reach only the synthetic base or no history, with no remote or prior-art evidence supplied (`reviewer-evidence/prior-art.log:1`); the contribution-artifact audit is deferred to publish, N/A here (`gate-logs/T4-contribution.log:10`). |
| T5 Judgment | NEEDS-HUMAN [impl] | Correct the recovery instructions/configuration and bound subprocess output collection — the reproduced cases can misdirect recovery, retain an admission slot indefinitely, or hang the verification harness; add regressions for F1–F4 (`crates/server/src/cli.rs:3276`, `crates/server/src/cli.rs:3370`, `crates/server/tests/multipart_operator_abort.rs:267`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Confirm operational recovery on the deployment’s backend/fleet and the required release-stack sign-off — live FoundationDB multiversion connectivity and whole-fleet recovery were not exercised, so that portion rests on compilation, isolated spelling probes, and redb evidence (`brief.md:169`, `reviewer-evidence/fdb-check.log:251`, `reviewer-evidence/fdb-spelling-probe.log:1`). |

Source citations without an input/evidence prefix are relative to `$PDCA_TARGET`; input and log citations are relative to this review directory. The supplied target is usable and contains the patch. Its tracked diff was identical before stashing and after restoration; no implementation changes were made.

Four implementation concerns remain:

1. **F1 — Reject an unrepresentable absolute FoundationDB path instead of retaining a relative one.** At `crates/server/src/cli.rs:3273`, absolute-path conversion can succeed while UTF-8 conversion fails; `:3276` then substitutes the original relative cluster-file path. A relative UTF-8 filename remains usable from a non-UTF-8 working directory, so successful backend opening does not exclude this case. Executing the printed commands elsewhere can select another cluster file. The extracted production spelling functions printed `WYRD_FDB_CLUSTER_FILE='cluster.conf'` and `cluster_file_absolute=false` from such a directory (`reviewer-evidence/fdb-spelling-probe.log:6`). Resolve this before store access, as the redb branch already does, and add the non-UTF-8-directory regression.

2. **F2 — Preserve the external FoundationDB client directory in follow-up commands.** `StoreSpelling` captures only the cluster-file assignment (`crates/server/src/cli.rs:3227`, `:3268`), while the driver consumes `WYRD_FDB_EXTERNAL_CLIENT_DIR` when booting its network (`crates/metadata-fdb/src/lib.rs:1096`). A command-scoped setting used for a multiversion cluster therefore disappears from both printed commands. The spelling probe had that variable set but produced `external_client_present=false` (`reviewer-evidence/fdb-spelling-probe.log:5`). Carry its effective, shell-quoted value, making a relative directory independent of the invocation directory. The probe checks generated text; it does not claim a live FDB connection test.

3. **F3 — Tell the operator to stop repeating a metadata-only pass when this upload requires fleet repair.** The new instruction says to repeat until the upload disappears (`crates/server/src/cli.rs:3370`), but an aborted part with untrusted placement requires whole-fleet listings (`crates/custodian/src/retirement/marks.rs:82`). A scratch test using the supplied fixtures and actual built CLI seeded that condition: abort exited 0, then three one-shots each exited 1 with `hold="partial-fleet"`, leaving the target present and admission count 1 (`reviewer-evidence/damaged-placement.log:3`, `:16`, `:36`). Qualify the retry instruction with a concrete escalation for a held obligation naming this upload. This requires CLI guidance/tests, not a change to the out-of-scope custodian policy.

4. **F4 — Include stderr draining and descendants in the subprocess deadline.** The timer ends when the immediate child exits; `drained.join()` has no deadline, and cleanup kills only that child (`crates/server/tests/multipart_operator_abort.rs:257`, `:267`, `:278`). The exact helper, extracted with a 100 ms bound, returned successfully after 1003 ms when an immediately exiting shell left a one-second child holding stderr (`reviewer-evidence/subprocess-bound-probe.log:2`). A longer-lived descendant can therefore hang the suite despite the stated bound. Bound the full subprocess lifecycle and ensure timeout cleanup covers the launched command tree; test a descendant retaining the pipe.

The independent verification supports the fence, with explicit limits:

- `cargo test --offline -p wyrd-server --test multipart_operator_abort`: six failures on stashed production, six passes after restoration. The new test remained present on both legs. The red failures were `unknown command mpu`, exit 2; the restored tests distinguish handled refusals and inspect durable records.
- `cargo test --offline -p wyrd-core --test multipart_abort`: 25 passes. `RUSTFLAGS='--cfg madsim' MADSIM_TEST_NUM=32 cargo test --offline -p wyrd-dst --test custodian operator_abort`: seven passes. The in-flight outcome test explicitly observes the unfenced store after abort returns, then lands the queued fence and checks reclamation (`crates/dst/tests/custodian.rs:8986`, `:8998`). Atomic batch construction was also inspected directly.
- `cargo check --offline -p wyrd-server --features fdb --tests` passed. Typos, formatting, documentation lint, and rendering/link audit were rerun successfully; rendering produced 101 pages (`reviewer-evidence/docs-lint.log:1`, `reviewer-evidence/docs-render.log:2`). Thus the brief’s `typos` and `docs-renderer` dependencies were exercised.
- Full CI, the TiKV feature clippy gate, and mutation testing are adjudicated from their supplied logs, not claimed as independently replayed in full. Those logs show full CI completion, successful feature compilation, and 49 mutants tested: 36 caught, 13 unviable (`gate-logs/C4-ci.log:5100`, `gate-logs/host-tikv.log:211`, `gate-logs/C5-mutants.log:13`). No named gate log is missing.

The frozen gate exceptions have the following disposition:

| Gate | Verdict | Basis |
|------|---------|-------|
| C4-diff-cov | N/A | No coverage measurement was produced: the wrapper tried `origin/main` rather than the brief’s folded verification base; this is a comparison-base caveat, not a patch compilation or applicability defect (`gate-logs/C4-diff-cov.log:10`). |
| T4-batch-review | FAIL | The frozen gate reports 12 blocking entries; independent review confirms four unique concerns F1–F4, consolidating repeated path-fallback claims (`gate-logs/T4-batch-review.log:10`). |
| T4-contribution | N/A | The artifacts are intentionally drafted after Check; their substantive audit must rerun at publish (`gate-logs/T4-contribution.log:10`). |

The remaining batch-review claims do not establish additional implementation defects. The cut test’s “late” counter alone is weaker than its wording, but the separate in-flight row proves post-return landing and reclamation (`crates/dst/tests/custodian.rs:8998`). The Completed race checks the brief’s required absence of a reclaim obligation naming the published attempt and preserves the segment snapshot through drain passes (`:9315`, `:9327`); a full publication-integrity audit is not established by that test. Its acceptance of round exhaustion matches the expressly allowed fail-closed behavior against a live completer (`crates/core/src/multipart/abort.rs:72`). The unknown-outcome wrapper injects the MetadataStore seam’s ambiguity outcomes over a deterministic store; this is not evidence of a real FDB adapter run, and none is claimed. These limitations do not invalidate the independently passing binding red→green criteria.

The accepted `{session, all}` decision, the tracked proposal update, and deliberate missing-trailing-value exit 2 remain settled (`brief.md:180`, `brief.md:196`). No DCO finding is raised. No additional INTEGRATION.md human-only checklist was present in the supplied target. This report is advisory; it does not alter deterministic gate results or grant release approval.
<!-- pdca:leaf-complete -->

### Advisory — adversary

# Adversarial review — issue 1018 (`wyrd mpu abort`, FU-6 part 1)

**Bottom line: I could not refute the fix.** I re-ran the proof, ran the one negation that leg B
cannot catch (fence split over two batches), tried the race and doubt paths by reading the code,
and threw hostile command lines at the built binary. The one-batch fence, the exact-epoch segment
obligation (F18) and the X57 closure all held. What is left is two small operator-hint gaps
(both already among T4's items) and some notes on how far each piece of evidence reaches.

## The evidence

- **The red→green proof is real, but on its own it only shows the verb exists.** GREEN reproduced
  here: `cargo test -p wyrd-server --test multipart_operator_abort`, 6 passed. RED
  (`gate-logs/C4-verify.log`): 6 ran, 6 failed, so this is not a zero-test run. But all six RED
  failures are the same one: exit 2 `unknown command mpu` at each test's first exit-code
  assertion (`crates/server/tests/multipart_operator_abort.rs:564`, `:637`, `:716`, `:795`, `:902`,
  `:960`). The checks that actually test behaviour are elsewhere: the exact changed-key sets
  (`:575-579`, `:641-645`), the decoy check (`:665-672`, `:683-687`), C5 (49 mutants, 0 missed)
  and the DST sweep. This is a note on what the C4 row proves, not a refutation.
- **Negation 2 (records obligation in a second batch) is caught. I ran it myself.** In a scratch
  copy I patched `fence_rounds` (`crates/core/src/multipart/abort.rs:526-528`) so that the
  `Completing` door commits the session plus the bytes obligation first, then the records
  obligation in a separate batch. Two tests then failed at `crates/dst/tests/custodian.rs:8699-8701`
  ("a fenced session carries every obligation of its door"):
  `operator_abort_completing_door_cut_reaches_both_sides` and
  `operator_abort_outcome_unknown_claims_no_more_than_it_saw`. With the file restored, all 7 DST
  rows pass. Two limits on what this shows:
  - The seeded single-cut row (`custodian.rs:8848-8852`) still **passed** under the negation with
    `MADSIM_TEST_NUM=1`, because one random cut rarely lands in the ~1 ms gap. The deterministic
    sweep (`:8859-8882`) is what catches the split.
  - Both live-Complete race rows (`custodian.rs:9229-9432`) **passed** under the negation, because
    they judge only end states. They are no evidence for atomicity, and build notes or the PR text
    should not cite them for it.

## The fix: what I tried and could not break

- **Wrong segment epoch (F18).** The decoder refuses a `Completing` record whose publish-target
  epoch differs from its own epoch (`crates/core/src/multipart.rs:2963-2966`), and
  `completing_teardown` builds the group from that epoch (`:2703-2708`, `:2767-2778`). The records
  obligation can only name `seg:<g>:<E>:`.
- **Racing the root flip.** The flip moves the session in the same batch that writes
  `retire:bytes:s:<id>:<E>`. `lost` reads the session again after it finds the key
  (`abort.rs:602-613`), so the abort answers from the new state (`Completed`) rather than
  reporting a false `ObligationKeyTaken`.
- **Racing a Complete-fence release** (`Completing@E → Open@E+1` plus `retire:records:s:<id>:<E>`).
  The abort's fence loses, the session reads as moved, and it fences `Open@E+1` under a different
  key. Nothing is stranded.
- **A false "nothing written" after an unknown outcome.** The drain never rewrites an `Aborting`
  record; it only deletes it (`crates/custodian/src/retirement/sessions.rs:947`, `:974-976`). So the
  byte-for-byte match in `Doubt::landed` (`abort.rs:643-649`) cannot miss our own landed fence and
  answer `AlreadyAborting`. Absence while in doubt answers `AbsentAfterDoubt` (`abort.rs:505-508`).
- **Two operator aborts at once.** The loser reads `Aborting` and answers `AlreadyAborting`, with
  no second obligation.
- **Regression in the client Abort.** `fence_rounds` with one obligation does the same steps as
  the old loop, and `lost_any` over one key is the same as `lost`. C4-ci passed the existing
  `multipart_abort.rs` suite.
- **Hostile command lines** against the built binary: `--upload-id=ID`, `-- extra`, a trailing
  `--log-level`, an uppercase id, `--data-dir -x`, a directory with no store, bare `mpu`, and
  `mpu ABORT`. Each was refused (exit 1, or exit 2 from the usage / pre-parse path) and no store
  file was created. `parse_exact` (`crates/server/src/cli.rs:3473-3493`) and `parse` (`:3443-3465`)
  split the arguments the same way, because every flag the verb accepts takes a value and no value
  may start with `--`.

## Findings

- NEEDS-HUMAN [impl] — `crates/server/src/cli.rs:3273-3276`: when the absolute cluster-file path
  is not valid UTF-8, the FDB spelling quietly falls back to the **relative** cluster file. A
  concrete case: `WYRD_FDB_CLUSTER_FILE=fdb.cluster` set for the one command, run from a non-UTF-8
  working directory. The printed follow-up commands then carry
  `WYRD_FDB_CLUSTER_FILE='fdb.cluster'`. Replayed from another directory (exactly the case the
  v2 sign-off asked to close), they read a different cluster file, or none. The redb path refuses
  the same situation (`cli.rs:3166-3178`). Fix: refuse it the same way. This one bug is 4 of T4's
  12 blocking items (`cli.rs:3276`, reported four times). It needs the fdb feature, a relative
  cluster file and a non-UTF-8 working directory all at once.
- NEEDS-HUMAN [impl] — `crates/server/src/cli.rs:3369-3374`: the stop-condition hint gives no way
  out for an upload the record-only one-shot can never finish. If an `Aborting` session has a
  staging entry that fails the placement rule or will not decode, every pass that does not see the
  whole fleet holds it (`crates/custodian/src/retirement/sessions.rs:731-745`), and
  `--reap-abandoned-uploads` is metadata-only. An operator following the hint loops for ever, each
  pass `BLOCKED` and naming this upload. The hint only says that a pass blocked on *other* uploads
  is not a stall. It should also say what to do when the blocked line names *this* upload (for
  example, run the custodian with `--endpoints`). Same as T4's `cli.rs:3371` item. This only
  happens after damage, so it is low severity.
- `crates/server/src/cli.rs:3268` (T4): the printed FDB command leaves out
  `WYRD_FDB_EXTERNAL_CLIENT_DIR`. A follow-up run without it fails at connect (the version check),
  so it fails safe and never acts on the wrong store. It is reasonable to fold this into the
  `:3276` fix, or to record it as rejected with that reason.
- My read on T4's DST and test-gap items: none of them hides a defect in the fix.
  - `custodian.rs:8839`: the `late` count can include a commit that applied before the cut. True,
    but the "lands after the operator gave up" property is shown separately by the `InFlight` row
    (`:8989-9007`), which lands fences only after `operator_abort` has returned.
  - `:9308`: accepting `RoundsSpent` is dead tolerance here (`ABORT_ROUNDS` is 4, and the Complete
    moves the session at most twice after the abort starts). A "never re-reads" mutant would fail
    the sweep's "fenced after losing a round" check (`:9422-9424`), and leg C catches a wrong
    answer on `Completed`.
  - `:9320`: segments cannot be deleted without an obligation naming them, and the row checks
    there is none.
  - `:8959`: `operator_abort` works on any store and FDB does emit 1021, so this is a fidelity nit
    on the TiKV model.
  - `multipart_operator_abort.rs:267`: a surviving grandchild process only matters on a timeout,
    when the test has already failed.

  Recording these as rejected, with these reasons, is defensible.
- The C4 diff-coverage failure ("patch.diff does not apply on origin/main",
  `gate-logs/C4-diff-cov.log`) is a harness fault: the gate used `origin/main` rather than the
  stacked base. It is no evidence against the fix; coverage is simply unmeasured.
- The `check-gates.json` claims I checked hold: C4-verify (6 ran red, all 6 green here), C5 (0
  missed), C4-ci green. The overall `fail` comes from T4 alone (gating), and the items above are
  my triage of it.

<!-- pdca:leaf-complete -->

### Advisory — code-review

- NEEDS-HUMAN [impl] — `crates/server/src/cli.rs:3276`: The FoundationDB hint silently falls back to the original cluster-file path when absolute-path conversion fails. A relative UTF-8 filename can open successfully from a working directory containing non-UTF-8 bytes, yet replaying this hint elsewhere can select a different cluster. Propagate resolution/encoding errors before opening the store, following the existing `absolute_data_dir` approach.

- NEEDS-HUMAN [impl] — `crates/server/src/cli.rs:3277`: The generated FoundationDB commands preserve only `WYRD_FDB_CLUSTER_FILE`. An abort using command-scoped `WYRD_FDB_EXTERNAL_CLIENT_DIR` can connect successfully during a version transition, while both printed follow-up commands fail without that setting (`crates/metadata-fdb/src/lib.rs:1096`). Preserve the effective external-client directory too, made absolute and shell-quoted.

- NEEDS-HUMAN [impl] — `crates/dst/tests/custodian.rs:9307`: The completed-race oracle accepts `Stop::RoundsSpent`, and the coverage assertion at `crates/dst/tests/custodian.rs:9427` checks only `Completed` durable state plus a lost commit. It can therefore credit “lost to the flip, then refused the Completed upload” even when abort never returns that refusal. Retain the actual abort outcome and require `OperatorAbortOutcome::Completed` for this coverage row; keep exhaustion coverage separate.

- NEEDS-HUMAN [impl] — `crates/server/tests/multipart_operator_abort.rs:986`: The printed-command test runs `sh -c`, but the timeout guard at `crates/server/tests/multipart_operator_abort.rs:278` kills only its immediate child. If `wyrd` hangs, killing the shell can leave `wyrd` and the stderr reader alive after the test fails. This shell/child behavior was reproduced locally. Use shell `exec` for these commands or terminate and reap the process group, and bound stderr draining too.

No additional reuse/simplification or efficiency findings. Review used the target source and frozen gate evidence; target builds were not rerun.
<!-- pdca:leaf-complete -->

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] T4 Contribution — Establish prior art by affected path against merged history and closed/rejected work — all eight path queries reach only the synthetic base or no history, with no remote or prior-art evidence supplied (`reviewer-evidence/prior-art.log:1`); the contribution-artifact audit is deferred to publish, N/A here (`gate-logs/T4-contribution.log:10`).
- [ ] T5 Judgment — Correct the recovery instructions/configuration and bound subprocess output collection — the reproduced cases can misdirect recovery, retain an admission slot indefinitely, or hang the verification harness; add regressions for F1–F4 (`crates/server/src/cli.rs:3276`, `crates/server/src/cli.rs:3370`, `crates/server/tests/multipart_operator_abort.rs:267`).
- [ ] Validation — fitness-to-purpose — Confirm operational recovery on the deployment’s backend/fleet and the required release-stack sign-off — live FoundationDB multiversion connectivity and whole-fleet recovery were not exercised, so that portion rests on compilation, isolated spelling probes, and redb evidence (`brief.md:169`, `reviewer-evidence/fdb-check.log:251`, `reviewer-evidence/fdb-spelling-probe.log:1`).
- [ ] `crates/server/src/cli.rs:3273-3276`: when the absolute cluster-file path is not valid UTF-8, the FDB spelling quietly falls back to the **relative** cluster file. A concrete case: `WYRD_FDB_CLUSTER_FILE=fdb.cluster` set for the one command, run from a non-UTF-8 working directory. The printed follow-up commands then carry `WYRD_FDB_CLUSTER_FILE='fdb.cluster'`. Replayed from another directory (exactly the case the v2 sign-off asked to close), they read a different cluster file, or none. The redb path refuses the same situation (`cli.rs:3166-3178`). Fix: refuse it the same way. This one bug is 4 of T4's 12 blocking items (`cli.rs:3276`, reported four times). It needs the fdb feature, a relative cluster file and a non-UTF-8 working directory all at once.
- [ ] `crates/server/src/cli.rs:3369-3374`: the stop-condition hint gives no way out for an upload the record-only one-shot can never finish. If an `Aborting` session has a staging entry that fails the placement rule or will not decode, every pass that does not see the whole fleet holds it (`crates/custodian/src/retirement/sessions.rs:731-745`), and `--reap-abandoned-uploads` is metadata-only. An operator following the hint loops for ever, each pass `BLOCKED` and naming this upload. The hint only says that a pass blocked on *other* uploads is not a stall. It should also say what to do when the blocked line names *this* upload (for example, run the custodian with `--endpoints`). Same as T4's `cli.rs:3371` item. This only happens after damage, so it is low severity.
- [ ] `crates/server/src/cli.rs:3276`: The FoundationDB hint silently falls back to the original cluster-file path when absolute-path conversion fails. A relative UTF-8 filename can open successfully from a working directory containing non-UTF-8 bytes, yet replaying this hint elsewhere can select a different cluster. Propagate resolution/encoding errors before opening the store, following the existing `absolute_data_dir` approach.
- [ ] `crates/server/src/cli.rs:3277`: The generated FoundationDB commands preserve only `WYRD_FDB_CLUSTER_FILE`. An abort using command-scoped `WYRD_FDB_EXTERNAL_CLIENT_DIR` can connect successfully during a version transition, while both printed follow-up commands fail without that setting (`crates/metadata-fdb/src/lib.rs:1096`). Preserve the effective external-client directory too, made absolute and shell-quoted.
- [ ] `crates/dst/tests/custodian.rs:9307`: The completed-race oracle accepts `Stop::RoundsSpent`, and the coverage assertion at `crates/dst/tests/custodian.rs:9427` checks only `Completed` durable state plus a lost commit. It can therefore credit “lost to the flip, then refused the Completed upload” even when abort never returns that refusal. Retain the actual abort outcome and require `OperatorAbortOutcome::Completed` for this coverage row; keep exhaustion coverage separate.
- [ ] `crates/server/tests/multipart_operator_abort.rs:986`: The printed-command test runs `sh -c`, but the timeout guard at `crates/server/tests/multipart_operator_abort.rs:278` kills only its immediate child. If `wyrd` hangs, killing the shell can leave `wyrd` and the stderr reader alive after the test fails. This shell/child behavior was reproduced locally. Use shell `exec` for these commands or terminate and reap the process group, and bound stderr draining too.
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 12 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_1018/review
- [ ] size backstop — this slice is behaving oversized: patch is 176 KB (threshold 100 KB). Recommend answering `iterate-plan` at sign-off and authoring the split in the re-plan (`pdca split`), rather than `iterate-do`: a slice that is too big yields implementation-shaped findings every round, and splitting authors briefs, which is Plan's beat.

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
- Iteration delta (if iterating): Keep the design: the one-batch fence, F18 exact-attempt reclaim and X57 closure held for a third round (adversary could not refute). Fix only these four items, nothing else, and do not grow the printed hint further: - F1 (crates/server/src/cli.rs:3273-3276): when the absolute FDB cluster-file path is not valid UTF-8, REFUSE before store access (same as the redb `absolute_data_dir` path does) instead of falling back to the relative path. Add the non-UTF-8-working-directory regression. - F2 (cli.rs:3268/3277): carry the effective WYRD_FDB_EXTERNAL_CLIENT_DIR, made absolute and shell-quoted, into both printed follow-up commands, alongside the cluster file. - F3 (cli.rs:3369-3374): the stop-condition hint must say what to do when a BLOCKED pass names THIS upload (e.g. a partial-fleet hold): stop repeating the metadata-only one-shot and run the custodian with `--endpoints` (whole fleet). CLI text + test only; no custodian change. - F4 (crates/server/tests/multipart_operator_abort.rs:257-278, :986): bound the whole subprocess lifecycle including stderr draining; use `exec` in the `sh -c` commands or kill+reap the process group on timeout. Optional, small: in the completed-race DST row (crates/dst/tests/custodian.rs:9307/:9427) keep the actual abort outcome and require OperatorAbortOutcome::Completed for that coverage row. Keep the patch from growing further — no new features, no extra hint detail beyond the above.
- By / date: Eduard Ralph / 2026-11-10

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
