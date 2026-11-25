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
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass — run-verify.sh: PASS — red without the fix, green with it (5 test(s) ran red).
- C4 diff coverage: changed lines executed by the patch's tests: fail — diff coverage not measured — patch.diff does not apply on origin/main
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): fail — 46 mutants tested in 12m: 2 missed, 33 caught, 11 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 9 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_1018/review-
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 14.41s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Review #1018: operator abort must free foreign-clock Open/Completing uploads through ordinary reclamation; the fence is verified, but the printed cleanup command can select a different store.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The required state transitions, discriminating refusals, scope exclusions and approved `{session, all}` choice are explicit; no scope decision remains unresolved (`brief.md:28`, `brief.md:180`). |
| C2 Reproduction (red pre-fix) | PASS | With the tracked fix stashed and the new subprocess tests retained, all five tests executed and failed on the missing `mpu` command, rather than failing compilation (`review-red.log:289`). |
| C3 Change | PASS | Both doors retain ordinary reclamation and admission accounting, and the living docs record the interim CLI authority and migration obligation (`target/crates/core/src/multipart/abort.rs:438`, `target/docs/design/architecture/06-runtime-view.md:96`). |
| C4 Verification (red→green) | PASS | Restoring the patch gives 5/5 subprocess passes, 25/25 core passes and 5/5 operator DST passes over 50 seeds; independent scanners and FDB feature type-check pass (`review-green-restored.log:14`, `review-core.log:120`, `review-dst.log:247`, `review-scanners.log:1`, `review-fdb.log:273`). |
| C5 Causal adequacy | PASS | The missing operator exit is supplied through the existing atomic fence, with both Completing obligations and truthful unknown-outcome handling; no capability probe conceals a load-time cause (`target/crates/core/src/multipart/abort.rs:405`, `target/crates/core/src/multipart/abort.rs:526`, `target/crates/dst/tests/custodian.rs:8963`). |
| T1 Structure | PASS | Backend selection stays in server; core uses the MetadataStore seam and shares fence construction and retry logic with client Abort (`target/crates/server/src/cli.rs:3156`, `target/crates/core/src/multipart/abort.rs:95`, `target/crates/core/src/multipart/abort.rs:486`). |
| T2 Shape | PASS | The one-session, flags-only command rejects duplicates and unknown flags before store access, and its documented surface includes the management-plane migration (`target/crates/server/src/cli.rs:3110`, `target/crates/server/src/cli.rs:3352`, `target/docs/design/architecture/06-runtime-view.md:96`). |
| T3 Runtime | PASS | Work is bounded by ABORT_ROUNDS and the store-operation contract; the lifecycle reads no clock, and seeded late commits remain reclaimable after the operator returns in doubt (`target/crates/core/src/multipart/abort.rs:72`, `target/crates/core/src/multipart/abort.rs:293`, `target/crates/dst/tests/custodian.rs:8973`). |
| T4 Contribution | N/A | Commit/PR artifacts are intentionally drafted after Check; the substantive contribution audit must rerun at publish, so this deferred row is neither green nor an outstanding human decision (`gate-logs/T4-contribution.log:10`); path-based prior art and batch-review dispositions are recorded below. |
| T5 Judgment | NEEDS-HUMAN [impl] | Preserve the original store in the generated cleanup command—relative redb paths and omitted network connection settings can leave the intended upload stranded or run reclamation against another store (`target/crates/server/src/cli.rs:3190`, `target/crates/server/src/cli.rs:3196`; reproduced in `review-hint.log:5`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Approve operational readiness and release ordering for the interim host-authorized abort plus subsequent drain—this must ship with the reaper and before production S3 multipart exposure (`brief.md:169`, `target/docs/design/architecture/06-runtime-view.md:96`). |

One P2 implementation correction remains: make the cleanup hint retain the store selected by the abort. The command promises a handoff to the same store, but preserves context-dependent inputs instead of the resolved location.

- **Redb reproduction:** I seeded a real store under `pdca-reviewer-1018-hint/a/store`, ran the patched binary from directory `a` with `--data-dir store`, and observed exit 0 and `Aborting@1`. Its printed command contains `--data-dir 'store'`. Replaying that command from directory `b` exits 1 looking for `b/store`, although the original store still exists (`review-hint.log:1`, `review-hint.log:7`). An existing store at that second location would instead be selected. Resolve the directory to an absolute path for the hint and test replay after a working-directory change (`target/crates/server/src/cli.rs:3196`).
- **Network selection:** the hint includes only `--metadata-backend tikv` or `fdb`, although the abort resolves `WYRD_TIKV_PD_ENDPOINTS` or `WYRD_FDB_CLUSTER_FILE`. A command-scoped override on the abort does not survive into the copied cleanup command. TiKV can then fail to connect; FDB can select its default cluster instead. Include the resolved, shell-quoted connection settings and test command-scoped overrides (`target/crates/server/src/cli.rs:314`, `target/crates/server/src/cli.rs:3190`, `target/crates/metadata-fdb/src/lib.rs:400`). This consequence is grounded in the selection code; no live network-cluster run is claimed.

The independent verification supports the storage change. I used `git stash push` without untracked files, ran `cargo test -p wyrd-server --test multipart_operator_abort --offline`, restored with `git stash pop`, and reran the same command. The result was five executed failures followed by five passes. The core abort suite passed 25 tests. `RUSTFLAGS='--cfg madsim' MADSIM_TEST_NUM=50 cargo test -p wyrd-dst --test custodian operator_abort --offline` passed all five selected tests, including cancellation on both sides of the commit and reclamation after a late landing. `cargo clippy -p wyrd-server --features fdb,etcd --tests --offline` passed. Formatting, diff whitespace, `typos`, docs lint and the real docs renderer/link audit passed; both external dependencies named in the brief were exercised. The supplied patch still passes `git apply --reverse --check` against the target after restoration.

The remaining gate evidence has explicit limits. The instance-scoped wrappers are not supplied here, so the full CI verdict is supported by `gate-logs/C4-ci.log:5099`, and the TiKV feature compile by `gate-logs/host-tikv.log`; these are frozen gate results, not independent full reruns. Diff coverage was **not measured**: its runner tried `origin/main` instead of this stacked bundle's integration base (`gate-logs/C4-diff-cov.log:10`, `brief.md:80`). That is a base-selection caveat, not evidence of a defective patch; the supplied target supports the independent red→green. The mutation log reports 46 tested, 33 caught, 11 unviable and two missed deletions of audit-event arms (`gate-logs/C5-mutants.log:13`). Those are real audit-assertion gaps, not demonstrated fence or reclamation defects; the structured event was observed in the independent successful abort (`review-hint.log:4`). No missing gate log requires escalation.

The nine batch-review entries reduce to the one retained issue above and three recorded rejections (`gate-logs/T4-batch-review.log:10`):

- **Retain and consolidate two cleanup-location entries** into the T5 correction, with the redb reproduction above.
- **Reject two missing-value exit-code entries as contract overreach.** A trailing `--upload-id` or `--data-dir` indeed exits 2 with a specific missing-value diagnostic (`review-cli.log:1`), before opening a store. The brief requires exit 1 for Completed/absent session refusals and the named unknown/repeated-flag cases; it does not impose that code on every syntax error (`brief.md:45`, `brief.md:176`, `target/crates/server/src/cli.rs:519`). The required cases pass.
- **Reject three concurrent-completer coverage entries as an additional coverage request, not a demonstrated failure of the required atomicity tests.** The new campaigns exercise a lone abort with cancellation and delayed commits; they do not purport to exercise a live completer. They meet the explicitly required two-door atomicity campaign (`brief.md:55`). The reused fence already has seeded concurrent-writer coverage (`target/crates/dst/tests/custodian.rs:5952`), and Complete has a regression for a root flip losing to Aborting (`target/crates/core/tests/multipart_complete.rs:4077`). I independently reran the three restore-fence DST tests across 50 seeds and that Complete test: all passed (`review-restore-race.log:9`, `review-complete-race.log:8`). This supports the reused mechanism without claiming an operator-specific full-pipeline race was run.
- **Reject two payload-table entries as settled by explicit sign-off.** The requested table update says all three Completing teardown doors use `{session, all}` (`brief.md:180`, `target/crates/core/src/multipart.rs:4157`). The reaper's future Completing judgment is already deferred to #1017 (`target/crates/custodian/src/reaper.rs:845`); it is not reopened here.

The prior-art check ran by all eight affected file paths, rather than issue title alone. Default-branch commit histories identify the shared Abort builder and restore Completing fence already reused here; the new operator test has no merged history (`review-prior-art.log:2`). I also inspected exact changed-file lists for all 24 closed, unmerged PRs returned by the repository query. Only #647 and #336 overlap affected paths: [#647 was closed for excessive scope](https://github.com/getwyrd/wyrd/pull/647#issuecomment-5137104571), and [#336 withdrew a duplicate ADR](https://github.com/getwyrd/wyrd/pull/336#issuecomment-4826535816). Neither is a rejected operator-abort implementation (`review-prior-art.log:10`, `review-prior-art.log:35`). No DCO inference or builder notes were used. This report is advisory; the frozen gate results are unchanged.

<!-- pdca:leaf-complete -->

### Advisory — adversary

# Adversarial review — #1018 operator abort (iteration 2)

**Verdict: I could not break the core fence.** The fence is one batch, it retires only the attempt's own segments (F18), it closes the X57 hole (fenced segments with no reclaimer), and its doubt handling is sound. I did find one concrete defect a human operator will hit: the printed teardown hint never reaches its stop condition. The other findings are small test and wording gaps. All of them are `[impl]`.

## What I tried and could not break

- **Red→green evidence** (`gate-logs/C4-verify.log`): 5 tests ran. All 5 were red on the base (`wyrd: unknown command mpu`, exit 2) and green with the patch. They drive the real binary (`CARGO_BIN_EXE_wyrd`), not a copy. Leg A's red comes at the abort step (`crates/server/tests/multipart_operator_abort.rs:556`), after its "one-shot alone moves nothing" check already passed. So the red is the defect itself, not a fixture fault. This is not a zero-test run.
- **One-batch atomicity: my own negation run** (brief's negation 2, the one leg B cannot catch). In a scratch copy I split the commit at `crates/core/src/multipart/abort.rs:526` into a session-only batch followed by an obligations batch. Then I ran `RUSTFLAGS="--cfg madsim" MADSIM_TEST_NUM=5 cargo test -p wyrd-dst --test custodian operator_abort`. Result: 4 of 5 rows went red. Both deterministic sweeps (`crates/dst/tests/custodian.rs:8843`) failed at cut 2125µs with `a fenced session carries every obligation of its door … left: []`. The `Completing` campaign row failed at 4125µs, and so did the unknown-outcome row. The `Open` campaign row passed on 5 seeds, because a random cut rarely lands in the window; the sweep row is the real proof. So the DST cases do tell one batch from two.
- **Live completer against the `Completing` door.** By code read this is safe. Every segment batch requires the session's exact bytes and moves the cursor (`crates/core/src/staged_publication.rs:17-21`, `crates/core/src/multipart.rs:2814-2822`). The flip also requires those bytes. So once the fence lands, no segment can land in `seg:<g>:<E>:` and the flip cannot win. The fence has exactly the rollback shape that caller obligation 1 asks for (`staged_publication.rs:64-68`). If the completer wins instead, `lost_any` (`abort.rs:559`) reads `Moved` and plans again, and a `Completed` result is refused with exit 1. The Complete's own release writes the same `retire:records:s:<id>:<E>` key, but it also moves the session to `Open@E+1`, so the abort re-plans through the `Open` door at a new epoch. No key collision.
- **F18.** The records group comes from `publish_target.epoch` (`crates/core/src/multipart.rs:2703-2708`), which decode pins to the session epoch. No payload can name a whole nonce, so a "whole group" negation cannot even be written without a new type. Leg B also pins `segments() == Some(&group)` and checks the decoy byte for byte.
- **Doubt handling.** I walked `fence_rounds` (`abort.rs:505`, `Doubt` at `:618-686`) through two cases:
  - unknown → re-read unchanged → `Conflict` → absent: `NoSuchUpload` comes only after the unknown outcome was settled;
  - held (`may_still_commit`) → absent: answers `AbsentAfterDoubt`.

  The client Abort's answers are unchanged, because its `absent` closure ignores the flag.
- **Command line, real binary.** Each of these exits 1 before the store opens:
  - `--upload-id=X`, `--reap-abandoned-uploads`, `--clock-source`;
  - a repeated flag, `--data-dir --bogus`, `--data-dir ""`.

  With no store at the default dir it exits 1 with "creates none".

## Findings

- NEEDS-HUMAN [impl] — `crates/server/src/cli.rs:3253`: the abort tells the operator to run the one-shot "until it reports satisfied". But any other held session in the store makes every pass `Blocked`: `Blocked` wins the merge (`crates/custodian/src/reconciliation.rs:79`), gets printed (`cli.rs:1146-1156`) and exits 1. That is exactly the case this verb exists for: many foreign-clocked sessions, aborted one per invocation (grant term ii).
  - **Reproduced through the real binary.** I seeded two foreign-clocked `Open@2` sessions, X and Y. `wyrd mpu abort --upload-id X` exited 0. I then ran the printed one-shot four times. `mpu:X` was gone after run 1, but every run printed `multipart reaper pass BLOCKED — … (NEEDS-HUMAN)` and exited 1.
  - **Effect.** An operator or script that follows the hint loops forever, and sees a NEEDS-HUMAN that is about Y, not X. Leg A passes only because its store holds one session.
  - **Fix.** Give a stop condition for this one upload, for example "until `mpu:<id>` is gone; a BLOCKED pass that names other uploads does not mean this one stalled". Add a two-session subprocess case.
- NEEDS-HUMAN [impl] — `crates/server/src/cli.rs:3216` and `:3223`: these are the two C5 survivors, and no test covers either audit arm. The lines do reach stderr: leg A's stderr holds `"action":"operator-abort-fenced"` (I printed it). So one `stderr.contains(...)` check in leg A (`crates/server/tests/multipart_operator_abort.rs:556-560`) kills the `Fenced` survivor. The `FencedUnattributed` arm can't be reached through redb, since redb never answers unknown. Either move the audit fields into a function the DST unknown-outcome row can assert on, or record why that one survivor is accepted.
- NEEDS-HUMAN [impl] — `crates/server/src/cli.rs:422-425` with `:486`: a valued flag with no value at the end of the line is refused by the global log pre-parser with **exit 2**. Every other bad `mpu abort` line gets exit 1. I reproduced it with `wyrd mpu abort --upload-id` and with `… --data-dir` last. T4 marks this blocking. Nothing is written, so it is not a safety issue. No test can catch it: the D-leg helper always appends `--metadata-backend redb --data-dir <dir>` (`multipart_operator_abort.rs:178`). The pre-parser's refusal to swallow parse errors is deliberate (#531), so the likely answer is to record a rejection (exit 2 means a usage error) rather than loosen it. Either way, it needs a recorded answer before T4 can go green.
- NEEDS-HUMAN [impl] — `crates/server/src/cli.rs:3143` and `:3189-3196`: the default `--data-dir` is the relative `wyrd-data`. Both the teardown hint and the `[store: …]` label echo it relative; my probe printed `[store: redb at store/meta.redb]`. Pasted into a shell in another directory, the line points at another store or at none. The one-shot refuses a missing store, so the worst case is reaping some other real store. The fix is one line: `std::path::absolute(data_dir)` before quoting. T4 marks this blocking too.
- NEEDS-HUMAN [impl] — `crates/core/src/multipart.rs:4157`: the row lists "the reaper's at the residency ceiling `W_session`" as a writer of the `Completing→Aborting` fence. The reaper does not write that fence yet (`crates/custodian/src/reaper.rs:845`, `// deferred: #1017`). The sign-off asked for this wording, so it is not the builder's slip, but T4 marks it blocking twice. Adding "(#1017, not yet built)" keeps the sign-off's `{session, all}` decision and makes the row true.
- NEEDS-HUMAN [impl] — `crates/dst/tests/custodian.rs:8502`: no seeded DST case races `operator_abort` against a live Complete. The operator abort is the first writer that can fence `Completing` while a completer is still running: the restore fence runs with writers stopped, and the reaper's `Completing` edge is deferred to #1017. The rubric's test-fidelity class wants a new concurrent path to land with seeded Tier-0 coverage. I could not build a failing interleaving by code read (see above), so this is missing coverage, not a refutation. What would cover it: one row that interleaves the completer's segment batches and flip with the abort. Every end state should then be one of two:
  - `Completed`, with no obligation naming `seg:<g>:<E>`;
  - `Aborting@E+1`, with `seg:<g>:<E>` named, and drained by the following passes.

  T4 raised this three times.

## Gate claims

- `C4-diff-cov` "fail" says only that the bundle does not apply on `origin/main`, which is expected for a stacked base. It is not evidence against the fix. The verify gate ran on the folded base and its red is real.
- `T4-batch-review` is gating and failing with 9 blocking items. My triage:
  - **Real, listed above:** both exit-2 items, the relative path, both `:4157` wording items, and the three DST-race items.
  - **Rejectable with a recorded reason:** "network backend env not echoed" (`cli.rs:3190`). The env supplies the address by design, and `[store: …]` names the PD endpoints or cluster file it used (`cli.rs:1748-1758`).

  Each item needs a fix or a recorded rejection. A sign-off that waves T4 through as noise would be unwarranted.

<!-- pdca:leaf-complete -->

### Advisory — code-review

- NEEDS-HUMAN [impl] — `crates/server/src/cli.rs:3190`: The printed teardown command loses the network store selection. For example, `WYRD_FDB_CLUSTER_FILE=/clusters/a.cluster wyrd mpu abort --upload-id ID --metadata-backend fdb` prints a custodian command containing only `--metadata-backend fdb`. The invocation-scoped override is gone when that command is pasted, so the connection helper (`crates/server/src/cli.rs:341`) can fall back to `/etc/foundationdb/fdb.cluster` and reap a different cluster while leaving the aborted upload undrained. TiKV likewise loses an invocation-scoped `WYRD_TIKV_PD_ENDPOINTS`. Include the effective, shell-quoted connection settings in the suggested command and cover invocation-scoped overrides in its tests.

No additional correctness or reuse/simplification/efficiency findings. Reviewed the target source and frozen gate evidence; no builds or tests were rerun.

<!-- pdca:leaf-complete -->

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] T5 Judgment — Preserve the original store in the generated cleanup command—relative redb paths and omitted network connection settings can leave the intended upload stranded or run reclamation against another store (`target/crates/server/src/cli.rs:3190`, `target/crates/server/src/cli.rs:3196`; reproduced in `review-hint.log:5`).
- [ ] Validation — fitness-to-purpose — Approve operational readiness and release ordering for the interim host-authorized abort plus subsequent drain—this must ship with the reaper and before production S3 multipart exposure (`brief.md:169`, `target/docs/design/architecture/06-runtime-view.md:96`).
- [ ] `crates/server/src/cli.rs:3253`: the abort tells the operator to run the one-shot "until it reports satisfied". But any other held session in the store makes every pass `Blocked`: `Blocked` wins the merge (`crates/custodian/src/reconciliation.rs:79`), gets printed (`cli.rs:1146-1156`) and exits 1. That is exactly the case this verb exists for: many foreign-clocked sessions, aborted one per invocation (grant term ii).
- [ ] `crates/server/src/cli.rs:3216` and `:3223`: these are the two C5 survivors, and no test covers either audit arm. The lines do reach stderr: leg A's stderr holds `"action":"operator-abort-fenced"` (I printed it). So one `stderr.contains(...)` check in leg A (`crates/server/tests/multipart_operator_abort.rs:556-560`) kills the `Fenced` survivor. The `FencedUnattributed` arm can't be reached through redb, since redb never answers unknown. Either move the audit fields into a function the DST unknown-outcome row can assert on, or record why that one survivor is accepted.
- [ ] `crates/server/src/cli.rs:422-425` with `:486`: a valued flag with no value at the end of the line is refused by the global log pre-parser with **exit 2**. Every other bad `mpu abort` line gets exit 1. I reproduced it with `wyrd mpu abort --upload-id` and with `… --data-dir` last. T4 marks this blocking. Nothing is written, so it is not a safety issue. No test can catch it: the D-leg helper always appends `--metadata-backend redb --data-dir <dir>` (`multipart_operator_abort.rs:178`). The pre-parser's refusal to swallow parse errors is deliberate (#531), so the likely answer is to record a rejection (exit 2 means a usage error) rather than loosen it. Either way, it needs a recorded answer before T4 can go green.
- [ ] `crates/server/src/cli.rs:3143` and `:3189-3196`: the default `--data-dir` is the relative `wyrd-data`. Both the teardown hint and the `[store: …]` label echo it relative; my probe printed `[store: redb at store/meta.redb]`. Pasted into a shell in another directory, the line points at another store or at none. The one-shot refuses a missing store, so the worst case is reaping some other real store. The fix is one line: `std::path::absolute(data_dir)` before quoting. T4 marks this blocking too.
- [ ] `crates/core/src/multipart.rs:4157`: the row lists "the reaper's at the residency ceiling `W_session`" as a writer of the `Completing→Aborting` fence. The reaper does not write that fence yet (`crates/custodian/src/reaper.rs:845`, `// deferred: #1017`). The sign-off asked for this wording, so it is not the builder's slip, but T4 marks it blocking twice. Adding "(#1017, not yet built)" keeps the sign-off's `{session, all}` decision and makes the row true.
- [ ] `crates/dst/tests/custodian.rs:8502`: no seeded DST case races `operator_abort` against a live Complete. The operator abort is the first writer that can fence `Completing` while a completer is still running: the restore fence runs with writers stopped, and the reaper's `Completing` edge is deferred to #1017. The rubric's test-fidelity class wants a new concurrent path to land with seeded Tier-0 coverage. I could not build a failing interleaving by code read (see above), so this is missing coverage, not a refutation. What would cover it: one row that interleaves the completer's segment batches and flip with the abort. Every end state should then be one of two:
- [ ] `crates/server/src/cli.rs:3190`: The printed teardown command loses the network store selection. For example, `WYRD_FDB_CLUSTER_FILE=/clusters/a.cluster wyrd mpu abort --upload-id ID --metadata-backend fdb` prints a custodian command containing only `--metadata-backend fdb`. The invocation-scoped override is gone when that command is pasted, so the connection helper (`crates/server/src/cli.rs:341`) can fall back to `/etc/foundationdb/fdb.cluster` and reap a different cluster while leaving the aborted upload undrained. TiKV likewise loses an invocation-scoped `WYRD_TIKV_PD_ENDPOINTS`. Include the effective, shell-quoted connection settings in the suggested command and cover invocation-scoped overrides in its tests.
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 9 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_1018/review-
- [ ] size backstop — this slice is behaving oversized: patch is 135 KB (threshold 100 KB). Recommend answering `iterate-plan` at sign-off and authoring the split in the re-plan (`pdca split`), rather than `iterate-do`: a slice that is too big yields implementation-shaped findings every round, and splitting authors briefs, which is Plan's beat.

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
- Iteration delta (if iterating): Keep the design: the one-batch fence, F18 exact-attempt reclaim and X57 closure held against the adversary again. Fix only the implementation findings: - Cleanup hint, store selection: resolve `--data-dir` to an absolute path (std::path::absolute) in both the hint and the `[store: …]` label; include the effective, shell-quoted network connection settings (WYRD_TIKV_PD_ENDPOINTS / WYRD_FDB_CLUSTER_FILE as resolved) in the printed command. Test: replay the hint after a working-directory change, and a command-scoped env override surviving into the hint. - Cleanup hint, stop condition: do not say "run until it reports satisfied". Give a per-upload stop condition (until `mpu:<id>` is gone; a BLOCKED pass naming other uploads does not mean this one stalled). Add a two-session subprocess case (abort X while Y stays held). - C5 survivors (cli.rs:3216/:3223): assert `"action":"operator-abort-fenced"` on leg A's stderr; for the unreachable-via-redb `FencedUnattributed` arm, either make it assertable from the DST unknown-outcome row or record in build-notes why that survivor is accepted. - `crates/core/src/multipart.rs:4157`: keep `{session, all}`, but mark the reaper W_session writer as "(#1017, not yet built)". - REQUIRED: add a seeded DST case racing `operator_abort` against a live Complete (interleave the completer's segment batches and flip with the abort). Every end state must be either `Completed` with no obligation naming `seg:<g>:<E>`, or `Aborting@E+1` with `seg:<g>:<E>` named and drained by the following passes. - Decided at sign-off: exit 2 for a valued flag with a missing trailing value is REJECTED as a finding — exit 2 is the usage-error code and the global pre-parser's behaviour is deliberate (#531). Do not change it; record the rejection in build-notes.md.
- By / date: Eduard Ralph / 2026-11-10

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
