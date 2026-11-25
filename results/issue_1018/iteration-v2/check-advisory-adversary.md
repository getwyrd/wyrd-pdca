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
