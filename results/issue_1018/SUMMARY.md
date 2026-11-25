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
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass — run-verify.sh: PASS — red without the fix, green with it (7 test(s) ran red).
- C4 diff coverage: changed lines executed by the patch's tests: fail — diff coverage not measured — patch.diff does not apply on origin/main
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): pass — 54 mutants tested in 10m: 41 caught, 13 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 9 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_1018/review-
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 13.70s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Review #1018: operator abort now releases foreign-clock Open/Completing uploads through the ordinary retirement path; the core checks pass, with two FDB handoff defects remaining.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The single-upload, host-authorized exit and discriminating refusals are testable; recorded decisions settle PartScope::All and trailing-value usage exit 2 (brief.md:26, brief.md:173, brief.md:188). |
| C2 Reproduction (red pre-fix) | PASS | With tracked changes stashed and the new subprocess suite retained, seven tests ran and all seven failed on unknown command mpu, exit 2; this is a behavioral red, not a compilation failure (review-red.log:7, review-red.log:22, review-red.log:393). |
| C3 Change | PASS | Both previously absorbing states have an operator exit, while terminal/absent refusal and Aborting idempotence preserve the intended boundary; expiry, alarm and custodian implementation remain outside this patch (target/crates/core/src/multipart/abort.rs:405, target/crates/server/src/cli.rs:3070). |
| C4 Verification (red→green) | PASS | Restoring the patch makes all seven subprocess tests pass; core, CLI, DST and real FDB-feature compilation/regression checks also pass; diff coverage remains unmeasured because its gate used a different base (review-green-restored.log:16, review-dst.log:249, review-fdb.log:75, gate-logs/C4-diff-cov.log:10). |
| C5 Causal adequacy | PASS | An atomic fence with the exact attempt's reclamation obligations removes the absorbing state; both-door cuts, late landing and a live Complete race exercise the causal boundary, with no capability-probe workaround added (target/crates/core/src/multipart/abort.rs:95, target/crates/core/src/multipart/abort.rs:440, target/crates/dst/tests/custodian.rs:8998, target/crates/dst/tests/custodian.rs:9432). |
| T1 Structure | PASS | The operator shares the existing fence builder and retry/error semantics through MetadataStore; concrete backend selection remains in server, preserving dependency direction (target/crates/core/src/multipart/abort.rs:486, target/crates/server/src/cli.rs:3187). |
| T2 Shape | PASS | The bounded verb retains strict one-upload parsing and introduces no persisted format or listener; both living architecture documents describe the interim surface and management-API migration (target/crates/server/src/cli.rs:3509, target/docs/design/architecture/06-runtime-view.md:96, target/docs/design/architecture/08-crosscutting-concepts.md:113). |
| T3 Runtime | PASS | Correctness remains clock-free with bounded fence rounds and adapter deadlines; the subprocess bound now covers stderr draining and shell exec prevents a surviving shell child (target/crates/core/src/multipart/abort.rs:76, target/crates/metadata-tikv/src/lib.rs:165, target/crates/server/tests/multipart_operator_abort.rs:251, target/crates/server/tests/multipart_operator_abort.rs:1023). |
| T4 Contribution | NEEDS-HUMAN | Confirm the affected-path audit of merged history and closed/rejected work — the supplied target contains one synthetic commit, no remote and no such audit evidence, so prior-art clearance cannot be established (review-prior-art.log:1); contribution-artifact inspection is separately N/A until publish (gate-logs/T4-contribution.log:10). |
| T5 Judgment | NEEDS-HUMAN [impl] | Preserve the FDB handoff's effective configuration, including an absent external-client directory, and use an absolute store label — otherwise a replay can inherit different client settings and identical labels can identify different clusters (target/crates/server/src/cli.rs:3157, target/crates/server/src/cli.rs:3312). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Accept the interim host-access CLI and operational handoff for deployment — the requested whole-fleet advice is broader than the remedies for all possible holds, and release coordination/M8 migration still require owner judgment (brief.md:203, target/crates/server/src/cli.rs:3405, target/docs/design/architecture/06-runtime-view.md:96). |

**Two implementation findings remain; neither challenges the atomic fence.**

- **Preserve the absence of WYRD_FDB_EXTERNAL_CLIENT_DIR in both follow-up commands.** An abort run with this variable unset or blank resolves it to None; path_environment drops it, so the printed command does not override a value in the destination shell. Replaying there can therefore activate an external-client directory that the abort never used. This is a configuration-preservation defect; actual connection failure was not demonstrated. Emit an explicit empty assignment for this optional setting (the adapter treats blank as None), and cover unset-at-abort/set-at-replay as well as the already-covered nonempty value. Evidence: target/crates/server/src/cli.rs:3288, target/crates/server/src/cli.rs:3312, target/crates/metadata-fdb/src/lib.rs:411, target/crates/metadata-fdb/src/lib.rs:1096. The current omission is also explicitly expected by the helper test at target/crates/server/src/cli.rs:4758.
- **Use the resolved absolute FDB cluster-file path in the abort's store label.** With WYRD_FDB_CLUSTER_FILE=cluster.file, cmd_mpu_abort obtains its label through reaper_store_name, which passes the relative value straight to fdb_store_name; only the separately constructed command spelling makes it absolute. Aborts in two directories can consequently print the same label for different cluster files, and the printed follow-up identifies the same store differently. Derive the abort label from the same resolved path as its commands. Evidence: target/crates/server/src/cli.rs:3157, target/crates/server/src/cli.rs:1758, target/crates/server/src/cli.rs:1787, target/crates/server/src/cli.rs:3281. This affects operator identification; it does not show the abort opening the wrong store.

**The nine frozen batch-review reports reduce to five distinct concerns.** The three relative-label reports and the external-client-directory report support the two findings above. The remaining dispositions are:

- **Whole-fleet advice: retain for human validation, not a new builder iteration.** The three reports correctly observe that not every hold is solved by fleet access: an undecodable part explicitly needs operator repair, and a failed commit is also recorded as a hold (target/crates/custodian/src/retirement/marks.rs:880, target/crates/custodian/src/retirement/sessions.rs:1041). However, brief.md:203 expressly requests this handoff and forbids further hint expansion. The operator must still follow the detailed hold reason. This is the scope/fitness decision captured in Validation, rather than permission to expand the implementation.
- **Late-landing coverage: reject the claimed missing case.** The cancellation sweep's sent-versus-answered counter alone does not date application, but the separate InFlight row first awaits the abort returning InDoubt, asserts the store still unfenced, explicitly lands its queued commits, and asserts reclamation. Both doors run this row (target/crates/dst/tests/custodian.rs:8986, target/crates/dst/tests/custodian.rs:8998, target/crates/dst/tests/custodian.rs:9744). The independent DST run passes. Do not mistake a local weakness in the sweep's terminology for absence of the required late-landing test.
- **Published inode traversal: reject as an additional oracle outside the specified abort-race criterion.** The row asserts Completed, the Complete's Published result, no obligation naming the attempt, unchanged segments after draining, and the competing abort's outcome; its coverage sweep specifically requires Completed after losing to the flip (target/crates/dst/tests/custodian.rs:9300, target/crates/dst/tests/custodian.rs:9432). It does not prove full inode/root integrity, and this report makes no such claim. Adding that publication-contract check is not required by brief.md:188 or its restricted final iteration.

The accepted PartScope::All choice, usage exit 2 for a missing trailing value, and unchanged proposal 0016 are not reopened. No commit-signoff finding was inferred. No INTEGRATION.md was present in the target; additional project human-only items could not be enumerated (review-prior-art.log:9).

**Independent execution supports the core verdict; frozen logs supply the remaining gate evidence.** All source citations above were checked in the supplied disposable target. After stash/pop, git apply --reverse --check against patch.diff and git diff --check both succeeded; the reviewed source is restored, with no reviewer source edits.

- Red→green: cargo test -p wyrd-server --test multipart_operator_abort --offline ran 7/7 failures on the stashed base and 7/7 passes after restoration (review-red.log:393; review-green-restored.log:16).
- Core/CLI: cargo test -p wyrd-core --test multipart_abort --offline passed 25 tests; server CLI unit tests passed 34 without FDB and 36 with FDB (review-core.log:120; review-cli-unit.log:42; review-fdb-unit.log:44).
- Concurrency: RUSTFLAGS='--cfg madsim' cargo test -p wyrd-dst --test custodian --offline operator_abort passed all seven relevant tests, using a separate build directory inside the review sandbox (review-dst.log:249).
- FDB: the real --features fdb build linked and passed abort_refuses_an_fdb_path_it_could_not_print_before_connecting; the feature's spelling test also passed with command-scoped relative cluster/client paths (review-fdb.log:75; review-fdb-scoped-environment.log:7). These exercise refusal/formatting, not a live-cluster abort. No tool alias or replacement backend was used for these builds.
- Scanners: cargo clippy -p wyrd-core -p wyrd-server --all-targets --features wyrd-server/fdb --offline, cargo fmt --all -- --check, typos, documentation lint, and the renderer/link audit all passed (review-clippy.log; review-fmt.log; review-typos.log; review-docs_lint.log:1; review-docs_render.log:3). The brief's typos/docs-renderer dependencies were actually exercised.

Frozen gate adjudication:

- **C4-ci — PASS from captured evidence:** full workspace checks, conformance and DST conclude all checks passed (gate-logs/C4-ci.log:4463, gate-logs/C4-ci.log:5106). The full instance wrapper was not rerun; the independent checks above are the reproduced subset.
- **C4-verify — PASS:** the log's seven behavioral reds and seven greens agree with the independent run (gate-logs/C4-verify.log:406).
- **C4-diff-cov — unmeasured, base caveat:** the gate fails applying this stacked patch to origin/main before measuring coverage (gate-logs/C4-diff-cov.log:10). The supplied target builds and reproduces red→green; this is not evidence of a patch compilation defect or a coverage percentage.
- **C5-mutants — PASS from captured evidence:** 54 attempted, 41 caught, 13 unviable, no survivors; mutation testing was not independently repeated (gate-logs/C5-mutants.log:13).
- **T4-batch-review — FAIL as recorded:** nine raw reports, with the independent dispositions above; the red review row is not automatically a C4 defect (gate-logs/T4-batch-review.log:10).
- **T4-contribution — N/A:** artifacts are drafted after Check; the substantive audit must rerun at publish (gate-logs/T4-contribution.log:10).
- **host-tikv — PASS from captured evidence:** the actual TiKV crate and server feature-arm clippy commands finish successfully; this establishes compilation, not live-cluster execution (gate-logs/host-tikv.log:2, gate-logs/host-tikv.log:211).

<!-- pdca:leaf-complete -->

### Advisory — adversary

# Adversarial review — issue 1018 (operator abort, FU-6 part 1), iteration 4

**Bottom line:** I could not refute the fix. The one-batch fence (X57), the exact-attempt `{seg}` range (F18) and the refusals all held, including one claim I tested by building a mutation myself. Two small items remain: a label/doc mismatch on FoundationDB (`[impl]`) and hint wording the sign-off already shaped (`[human]`).

## Refutation attempts that failed (the fix held)

- **One batch per door (X57): tested, held.** I copied the source to scratch and split the `Completing` fence into two commits at `crates/core/src/multipart/abort.rs:526-528`: the first carried only `retire:bytes:s:<id>:<E>`, the second the `retire:records:` obligation. Then I ran the two Completing-door DST rows under `--cfg madsim`. `operator_abort_completing_door_cut_reaches_both_sides` went red at `crates/dst/tests/custodian.rs:8699-8701` ("a fenced session carries every obligation of its door", cut at 2125µs, only the bytes key present). So brief negation 2 is caught by a test, not just by reading the code. One caveat, not a defect: against the same mutation, the random-cut row `operator_abort_completing_door_cut_never_strands` (`custodian.rs:8848`) passed for the seed it drew. Only the deterministic sweep row (`:8859`) reliably catches the split, so that row must stay in the suite.
- **Can a `Completing` session be stuck because its obligation key is already taken? No.** Nothing writes `retire:bytes:s:<id>:<E>` or `retire:records:s:<id>:<E>` while the session stays `Completing@E`. The Complete's release moves the session to `Open@E+2` in the same batch as its `{seg}` obligation (`crates/core/src/multipart/complete.rs:1309-1320`). The flip moves it to `Completed`. So `ObligationKeyTaken` (refusal E) means real damage, not a reachable dead end. A live completer cannot add segments after the fence, because every segment batch requires the exact session bytes (`complete.rs:149`). I found no segment that could leak past the drain.
- **The stop condition "until `mpu:<id>` is gone" is sound.** The terminal delete happens only after both `retire:bytes:s:<id>:` and `retire:records:s:<id>:` are read empty (`crates/custodian/src/retirement/sessions.rs:124-126`, `:931-938`). A missing record therefore means this upload's obligations are drained. It cannot stop the operator early.
- **The red→green evidence.** All 7 tests fail on the base for one reason: exit 2, `unknown command mpu`. On its own, that shows only that the verb is new. Whether the tests can tell a correct abort from a wrong one rests on the negation runs. The exact key-diff assertions catch negations 1, 3 and 5 (`crates/server/tests/multipart_operator_abort.rs:590-593`, `:656-659`, `:674-687`). Leg C (`:730-741`) catches 4. I re-ran 2 above. The tests drive the real binary and the production `operator_abort` / `complete`. I found no parallel re-implementation.
- **CLI parsing.** I tried `--upload-id=X`, a bare `--`, `--data-dir --gc-expired-pending`, a stray positional, `wyrd mpu` with no verb, repeated log flags, and `--clock-source`. `ParsedArgs::parse_exact` (`crates/server/src/cli.rs:3508-3528`) refuses each one before the store opens, with no way through. A trailing valued flag with no value exits 2 from the global pre-parse, which the iteration-2 sign-off already settled.
- **The T4 test gaps at `custodian.rs:8839` and `:9322`.** The `late` flag in the cut row counts "sent but not answered", so a commit that applied just before the cut also counts as late. That is true, but the stronger schedule the iteration-1 sign-off asked for is the `Fate::InFlight` row (`custodian.rs:8989`). That row holds every fence, returns `InDoubt { may_still_commit }`, lets the fences land after the operator gave up, and asserts the session is reclaimed. The completed-race row not reading the published inode does not touch what the abort claims, which is that no obligation names the published attempt. I raise neither.
- **C4 diff coverage "fail"** says "patch.diff does not apply on origin/main". That is the stacked-bundle base problem, not evidence against the fix.

## Findings

- NEEDS-HUMAN [impl] — **On FoundationDB, the `[store: …]` label keeps a relative cluster-file path, which contradicts the new architecture text.** `crates/server/src/cli.rs:3157` builds the label with `reaper_store_name` (`cli.rs:1757-1760`), which prints `WYRD_FDB_CLUSTER_FILE` exactly as given. `store_spelling` (`cli.rs:3284`, through `path_environment`) prints the same path made absolute. Concrete case: from `/a`, `WYRD_FDB_CLUSTER_FILE=fdb.cluster wyrd mpu abort --upload-id X --metadata-backend fdb` prints `[store: fdb at cluster file fdb.cluster]`. The one-shot it prints carries `WYRD_FDB_CLUSTER_FILE='/a/fdb.cluster'`, and that one-shot's own last line names `/a/fdb.cluster`. `docs/design/architecture/06-runtime-view.md:96` says "The abort's `[store: …]` label names the same absolute path", which is false for fdb. No wrong store is reachable, because the printed command is right. Fix it either by building the fdb label from the path `store_spelling` already made absolute, or by limiting the doc sentence to redb. The T4 gate flagged this too.
- NEEDS-HUMAN [human] — **The "whole fleet" advice covers every hold that names this upload, but only a partial-fleet hold fits it.** `cli.rs:3405-3408` says a pass whose held lines name upload X "will not clear by repeating that metadata-only one-shot: stop it, and run the custodian … with --endpoints". That is right for `Hold::PartialFleet` (`crates/custodian/src/retirement.rs:342`). It is wrong for `Hold::CommitFailed` (`:316`), which clears on the next visit, so repeating the one-shot does help. It is also wrong for `RecordUnreadable` (`:336`), `ListingPastStep`, `AdmissionLedger` (`:352`) and `ReclaimingRetained`, which the code marks as needing a human: a whole-fleet pass will not clear them either, and the hint gives no next step. Nothing is lost either way, since the whole-fleet custodian runs the same drain. This is the F3 text the iteration-3 sign-off asked for, with an instruction not to grow the hint, so it is a human call. The options are to narrow "held lines name upload X" to "a partial-fleet hold of upload X" (about the same length), or to accept the wording as it is.

The scratch mutation and its build (`$PDCA_SCRATCH/pdca-adversary-1018-twobatch*`) were deleted after the run.

<!-- pdca:leaf-complete -->

### Advisory — code-review

- NEEDS-HUMAN [impl] — `crates/server/src/cli.rs:3157`: The FoundationDB `[store: …]` label still uses the raw relative cluster-file path. With `WYRD_FDB_CLUSTER_FILE=cluster/fdb.cluster`, invocations from two directories can report the same label for different stores, while their follow-up commands correctly carry different absolute paths. `reaper_store_name` preserves that relative value (`crates/server/src/cli.rs:1758`); the absolute resolution only reaches `StoreSpelling`. Build the abort's label from the same resolved configuration as its commands and cover a relative FDB cluster-file path.

- NEEDS-HUMAN [impl] — `crates/server/src/cli.rs:3405`: The new hint incorrectly says any hold naming this upload cannot clear on another metadata-only pass and requires the whole fleet. A transient commit failure also produces a named hold (`crates/custodian/src/retirement/sessions.rs:1042`), while an undecodable part explicitly requires operator repair even with the whole fleet (`crates/custodian/src/retirement/marks.rs:880`). This sends operators to unnecessary fleet work or a remedy that cannot resolve their hold. Narrow the existing whole-fleet instruction to the partial-fleet case and make other holds follow their reported reason; update the assertions that currently require the blanket claim.

No introduced fence-logic defect or additional reuse, simplification, or efficiency finding identified. Validation was assessed from the supplied frozen gate evidence; no builds were rerun.
<!-- pdca:leaf-complete -->

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [x] T4 Contribution — Confirm the affected-path audit of merged history and closed/rejected work — the supplied target contains one synthetic commit, no remote and no such audit evidence, so prior-art clearance cannot be established (review-prior-art.log:1); contribution-artifact inspection is separately N/A until publish (gate-logs/T4-contribution.log:10).
- [x] T5 Judgment — Preserve the FDB handoff's effective configuration, including an absent external-client directory, and use an absolute store label — otherwise a replay can inherit different client settings and identical labels can identify different clusters (target/crates/server/src/cli.rs:3157, target/crates/server/src/cli.rs:3312).
- [x] Validation — fitness-to-purpose — Accept the interim host-access CLI and operational handoff for deployment — the requested whole-fleet advice is broader than the remedies for all possible holds, and release coordination/M8 migration still require owner judgment (brief.md:203, target/crates/server/src/cli.rs:3405, target/docs/design/architecture/06-runtime-view.md:96).
- [x] **On FoundationDB, the `[store: …]` label keeps a relative cluster-file path, which contradicts the new architecture text.** `crates/server/src/cli.rs:3157` builds the label with `reaper_store_name` (`cli.rs:1757-1760`), which prints `WYRD_FDB_CLUSTER_FILE` exactly as given. `store_spelling` (`cli.rs:3284`, through `path_environment`) prints the same path made absolute. Concrete case: from `/a`, `WYRD_FDB_CLUSTER_FILE=fdb.cluster wyrd mpu abort --upload-id X --metadata-backend fdb` prints `[store: fdb at cluster file fdb.cluster]`. The one-shot it prints carries `WYRD_FDB_CLUSTER_FILE='/a/fdb.cluster'`, and that one-shot's own last line names `/a/fdb.cluster`. `docs/design/architecture/06-runtime-view.md:96` says "The abort's `[store: …]` label names the same absolute path", which is false for fdb. No wrong store is reachable, because the printed command is right. Fix it either by building the fdb label from the path `store_spelling` already made absolute, or by limiting the doc sentence to redb. The T4 gate flagged this too.
- [x] **The "whole fleet" advice covers every hold that names this upload, but only a partial-fleet hold fits it.** `cli.rs:3405-3408` says a pass whose held lines name upload X "will not clear by repeating that metadata-only one-shot: stop it, and run the custodian … with --endpoints". That is right for `Hold::PartialFleet` (`crates/custodian/src/retirement.rs:342`). It is wrong for `Hold::CommitFailed` (`:316`), which clears on the next visit, so repeating the one-shot does help. It is also wrong for `RecordUnreadable` (`:336`), `ListingPastStep`, `AdmissionLedger` (`:352`) and `ReclaimingRetained`, which the code marks as needing a human: a whole-fleet pass will not clear them either, and the hint gives no next step. Nothing is lost either way, since the whole-fleet custodian runs the same drain. This is the F3 text the iteration-3 sign-off asked for, with an instruction not to grow the hint, so it is a human call. The options are to narrow "held lines name upload X" to "a partial-fleet hold of upload X" (about the same length), or to accept the wording as it is.
- [x] `crates/server/src/cli.rs:3157`: The FoundationDB `[store: …]` label still uses the raw relative cluster-file path. With `WYRD_FDB_CLUSTER_FILE=cluster/fdb.cluster`, invocations from two directories can report the same label for different stores, while their follow-up commands correctly carry different absolute paths. `reaper_store_name` preserves that relative value (`crates/server/src/cli.rs:1758`); the absolute resolution only reaches `StoreSpelling`. Build the abort's label from the same resolved configuration as its commands and cover a relative FDB cluster-file path.
- [x] `crates/server/src/cli.rs:3405`: The new hint incorrectly says any hold naming this upload cannot clear on another metadata-only pass and requires the whole fleet. A transient commit failure also produces a named hold (`crates/custodian/src/retirement/sessions.rs:1042`), while an undecodable part explicitly requires operator repair even with the whole fleet (`crates/custodian/src/retirement/marks.rs:880`). This sends operators to unnecessary fleet work or a remedy that cannot resolve their hold. Narrow the existing whole-fleet instruction to the partial-fleet case and make other holds follow their reported reason; update the assertions that currently require the blanket claim.
- [x] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 9 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_1018/review-
- [x] size backstop — this slice is behaving oversized: patch is 188 KB (threshold 100 KB). Recommend answering `iterate-plan` at sign-off and authoring the split in the re-plan (`pdca split`), rather than `iterate-do`: a slice that is too big yields implementation-shaped findings every round, and splitting authors briefs, which is Plan's beat.

## 7. Proven / not proven
- Proven by which oracle: gates overall = fail (stub oracles).
- Unproven / needs manual run: anything flagged in §6.

## 8. Ready-to-ship attachments
- patch.diff
- tracker-comment.md     (ALWAYS, every tracker item)
- build-notes.md         (builder rationale — for the human, not the reviewer)

## 9. Check sign-off                     ← human completes Check here
- Disposition confirmed / overridden:
- Outcome: merged-wider
- Iteration delta (if iterating):
- By / date: Eduard Ralph / 2026-11-10

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
- Follow-up to #1018 (file as an issue): FDB `[store: …]` label should use the absolute cluster-file path, matching the printed command, or limit `06-runtime-view.md:96` to redb. Print an explicit empty `WYRD_FDB_EXTERNAL_CLIENT_DIR=` when it was unset at abort. Narrow the `cli.rs:3405` whole-fleet hint to partial-fleet holds, so other holds follow their reported reason.
