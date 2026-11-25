# Build notes — issue 1018, iteration 4: `wyrd mpu abort` (FU-6 part 1)

Target: getwyrd/wyrd @ main, stacked on `origin/pdca-integration/main-r9dd123c9ed70` (6e44de9, the
worktree HEAD and the bundle's `stack-base-tip`). Every `path:line` below is on the **patched**
tree unless it says otherwise.

I started from iteration 3's `patch.diff` applied to the worktree and changed only what the
iteration-3 sign-off asked for: F1–F4 and the optional race-row item. The design is untouched
(one-batch fence, exact-attempt `{seg}` obligation, `{session, all}` for both doors), and so are
`crates/core/**` and the DST cut rows.

## The four items and the optional one

### F1 — a non-UTF-8 FDB cluster-file path is refused before store access

- **What changed.** Iteration 3's `absolute_data_dir` is now `absolute_path(what, path)`
  (`crates/server/src/cli.rs:3170`). It makes a path absolute and refuses one with no UTF-8
  spelling, naming `what` (a flag or a variable). The redb `--data-dir` goes through it as before
  (`cli.rs:3153`).
- The FDB arm of `store_spelling` (`cli.rs:3276-3292`) now sends the cluster file through the same
  function, via `path_environment` (`cli.rs:3307`), and no longer falls back to the relative path.
- `store_spelling` now returns `Result` (`cli.rs:3257`). `cmd_mpu_abort` calls it with `?` at
  `cli.rs:3158`, before the runtime is built and before `operator_abort_over_backend` opens the
  store. So the refusal comes before any store access, as the redb one does.
- **Regression (the one asked for): "G, FoundationDB"**,
  `abort_refuses_an_fdb_path_it_could_not_print_before_connecting`
  (`crates/server/tests/multipart_operator_abort.rs:1124`, `#[cfg(all(unix, feature = "fdb"))]`).
  - It runs the real binary from a working directory named `not-utf8-\xff`, with
    `--metadata-backend fdb` and a relative `WYRD_FDB_CLUSTER_FILE=fdb.cluster`.
  - It asserts exit 1, stderr `WYRD_FDB_CLUSTER_FILE resolves to … which is not UTF-8`, and no
    `fdb backend` text (that prefix is what `open_fdb_meta` puts on a connect failure,
    `cli.rs:345`).
  - Without F1 it fails at that message assertion, because the abort went on to connect. Stderr
    from that run: `wyrd: fdb backend: set WYRD_FDB_CLUSTER_FILE to the cluster file path:
    FoundationDB metadata store: cluster unreachable after waiting 1.001358418s …`. So the unfixed
    code really reached FDB.
- **The same regression for redb (default build): "G"**,
  `abort_refuses_a_data_dir_it_could_not_print_before_opening_the_store`
  (`multipart_operator_abort.rs:1084`, `#[cfg(unix)]`).
  - It seeds a real store at `<non-UTF-8 dir>/store` with an `Open` session and runs the abort
    there with `--data-dir store`.
  - It asserts exit 1, the `--data-dir resolves to … not UTF-8` message, and the store byte for
    byte as seeded.
  - This one runs in CI and in `run-verify.sh`, which build without `fdb`. It covers the shared
    `absolute_path` refusal.

### F2 — `WYRD_FDB_EXTERNAL_CLIENT_DIR` carried, absolute and quoted, into both printed commands

- `StoreSpelling.address: Option<(var, value)>` became `environment: Vec<(var, value)>`
  (`cli.rs:3231`). `StoreSpelling::command` prints every entry as a quoted assignment before
  `wyrd`. Both printed commands (the one-shot and the probe) come from that one spelling, so both
  carry it.
- The FDB arm passes two entries to `path_environment`: the cluster file, resolved as the driver
  resolves it (`config::cluster_file`), and the external-client directory, resolved as the driver
  resolves it (`config::external_client_dir`: trimmed, `None` when unset or blank).
  `path_environment` drops a `None` entry and makes each remaining path absolute through
  `absolute_path`. A non-UTF-8 one is refused, as in F1.
- **Only printed when in effect.** An unset or blank variable prints nothing. I did not print
  `WYRD_FDB_EXTERNAL_CLIENT_DIR=''` to pin "none": a follow-up that picks up the operator's own
  external-client directory still reaches the same cluster (the cluster file is pinned). The
  defect was the opposite case, an abort that needed the directory and follow-ups that lacked it.
  The sign-off also said no extra hint detail.
- **Why a feature-independent helper.** cargo-mutants generates mutants inside
  `#[cfg(feature = …)]` functions. I checked: `cargo mutants --list -f crates/server/src/cli.rs`
  lists `cli.rs:342:5: replace open_fdb_meta … with Ok(Default::default())`. The C5 gate builds
  default features, so such a mutant is never compiled and is scored **missed**.
  - My first version, a `#[cfg(feature = "fdb")] fn fdb_environment`, would have left about three
    certain survivors.
  - `path_environment` holds the logic (drop `None`, make absolute, refuse non-UTF-8) in every
    build. The default-build unit test `the_printed_environment_carries_each_path_in_effect_absolute`
    (`cli.rs:4753`) kills its mutants.
  - The cfg'd FDB arm is a call with two arguments, inside a match arm, where cargo-mutants makes
    no mutants.
  - **Cost:** only the `fdb` build calls it outside tests, so it carries
    `#[cfg_attr(not(feature = "fdb"), allow(dead_code))]` (`cli.rs:3306`) with a doc line saying
    why. No other site in `crates/server/src` has such an allowance. A reviewer may dislike it. The
    other option was the guaranteed C5 survivors.
- **Tests:**
  - Default build: `the_printed_commands_name_the_store_the_abort_acted_on` (`cli.rs:4669`).
    Its hand-built FDB spelling now has both variables, and it checks the exact line
    `WYRD_FDB_CLUSTER_FILE='/clusters/a'\''s.cluster' WYRD_FDB_EXTERNAL_CLIENT_DIR='/opt/fdb clients' wyrd mpu abort …`.
  - Default build: `the_printed_environment_carries_each_path_in_effect_absolute` (above).
  - `fdb` build: `the_fdb_spelling_carries_the_cluster_file_and_client_directory_it_connects_with`
    (`cli.rs:4783`). It works the expected spelling out from the variables' literal names and the
    process environment, and compares it with `store_spelling(Fdb)`. I ran it three ways (all 4/4
    passed in the filtered run):
    1. variables unset;
    2. `WYRD_FDB_CLUSTER_FILE=clusters/a.cluster WYRD_FDB_EXTERNAL_CLIENT_DIR=fdb-clients`;
    3. `WYRD_FDB_CLUSTER_FILE=/c/b.cluster WYRD_FDB_EXTERNAL_CLIENT_DIR=' '`.
  - `fdb` build: the "G, FoundationDB" subprocess test's second case. It uses an absolute
    (non-existent) cluster file and a relative `WYRD_FDB_EXTERNAL_CLIENT_DIR=clients` from the
    non-UTF-8 directory, and must be refused naming `WYRD_FDB_EXTERNAL_CLIENT_DIR`. That shows the
    directory goes through the same absolute-and-refuse path.
- **Not testable here:** a successful FDB abort whose printed lines are replayed. That needs a live
  FDB cluster with a seeded session, which this host does not have and the brief does not list. The
  printed text is covered by the unit tests above.

### F3 — what to do when a BLOCKED pass names this upload

- `operator_abort_answer` (`cli.rs:3393`, the text at `:3401-3409`) now continues the stop condition with: "…has not
  stalled this one, but one whose held lines name upload X (a partial-fleet hold, say) will not
  clear by repeating that metadata-only one-shot: stop it, and run the custodian over the same
  store without --reap-abandoned-uploads, with --endpoints, --ids and --failure-domains for the
  whole fleet".
- "Without --reap-abandoned-uploads" is needed: the one-shot refuses the fleet flags
  (`reaper_one_shot_conflict`, base code). Without it, an operator who adds `--endpoints` to the
  printed line gets a refusal.
- No new command is printed. The sign-off said not to grow the hint further, and the operator
  already has the store's spelling from the one-shot line.
- `usage()` (`cli.rs:518-519`, the new line at `:519`) and both architecture docs (06 §6.7 paragraph, 08 "Multipart
  sessions") say the same in short form. No custodian change.
- **Tests:**
  - `the_operator_abort_answers_claim_only_what_the_store_showed` (`cli.rs:4820`, `held_here` at
    `:4830`) requires the sentence on all three exit-0 answers.
  - Leg F (`multipart_operator_abort.rs:998-1007`) requires it, with X's id, on the real abort's
    stderr.

### F4 — the whole subprocess lifecycle is bounded

- `run_bounded` (`multipart_operator_abort.rs:251`):
  - One deadline is set at spawn (`:253`), read from the test's own monotonic clock. A comment
    says so; it judges nothing in the store.
  - The stderr reader thread sends its result on a channel, and the test waits with
    `recv_timeout` against the same deadline (`:280`). It used to `join()` with no limit.
  - The child is still killed and reaped on every way out (`Killed`).
- Leg F's `sh` closure runs `sh -c "exec <line>"` (`:1023`). The shell becomes `wyrd`, so killing
  the child kills `wyrd` itself and leaves no grandchild holding the pipe.
  - `exec` is valid only for a line that starts with the command name. A line with a leading
    `VAR=…` assignment would make `exec` try to run `VAR=…`.
  - Leg F's lines are redb lines, so they have no assignment. The closure asserts
    `line.starts_with("wyrd ")` (`:1014`) so a future change to that cannot pass silently.
- I chose `exec` over killing a process group. Killing a group needs `kill(-pgid)`: `libc` is
  `unsafe` and the file is `#![forbid(unsafe_code)]`, and shelling out to `kill` is clumsier than
  `exec`.
- **Demonstration** (a one-off scratch test, not shipped; restored afterwards). `COMMAND_BOUND` was
  set to 3 s and `run_bounded` was run on `sh -c 'sleep 20 & exit 0'`, whose background `sleep`
  holds stderr open after `sh` exits:
  - this round's harness: panicked with `the command's stderr must close within 3s (exit status:
    0)` after **3.000 s**;
  - iteration 3's harness: returned normally after **20.003 s**. The `join()` waited for the
    `sleep`, with no bound.

### Optional — the completed-race coverage row requires the abort's own `Completed` answer

- `Race` keeps `answered: Option<OperatorAbortOutcome>` (`crates/dst/tests/custodian.rs:9218`,
  set at `:9257`). The sweep's debug list prints it.
- The "lost to the flip, then refused the Completed upload" row now also requires
  `answered == Some(Completed { epoch: 2 })` (`:9432`). A `Stopped { RoundsSpent }` no longer
  satisfies it. `judge_race` still accepts `RoundsSpent` as an end state; only the coverage row is
  stricter.
- `Race` lost `Copy`: `OperatorAbortOutcome` is `Clone` only.
- Negation N26 below shows the row now binds where it did not before.

## Review batch (iteration 3's T4, `review-batch.md`)

| Line(s) in the batch | Finding | This round |
|---|---|---|
| 3 | external-client dir omitted | fixed (F2) |
| 4, 6, 7 | relative cluster file through a non-UTF-8 directory | fixed (F1) |
| 5 | repeat the record-only pass for a target that needs the whole fleet | fixed (F3) |
| 12 | timeout covers only the immediate child | fixed (F4) |
| 14 | live-Complete race accepts `RoundsSpent` | fixed (optional item) |
| 8, 10, 13 | cut sweep counts an unanswered commit as a late landing | **not taken** |
| 9 | completed-race oracle checks only dirent + some segments | **not taken** |
| 11 | TiKV fault fixture returns FDB's 1021 | **not taken** |

The five "not taken" lines are the findings the iteration-3 sign-off left out ("Fix only these four
items, nothing else"). I recorded each in `review-rejected.md` as declined by that sign-off, so
T4 does not block on them again. **Human, please confirm:** I wrote those rows on the strength of
the sign-off's wording, not as my own technical rejection. Strike any you meant to defer, not
decline.

## Negation runs (this round)

- **How they ran:** a script (`$PDCA_SCRATCH/pdca-builder-1018-neg4/neg.py`) applied each mutation
  as an exact-text replacement, ran the named tests, and restored the files. Afterwards `git diff`
  was byte-identical to the saved pre-run diff.
- **Test commands:**
  - `it`: `cargo test -p wyrd-server --test multipart_operator_abort <filter>`;
  - `fdb-it`: the same with `--features fdb`;
  - `lib` / `fdb-lib`: `cargo test -p wyrd-server [--features fdb] --lib <filter>`. The `fdb-lib`
    runs set `WYRD_FDB_CLUSTER_FILE=clusters/a.cluster WYRD_FDB_EXTERNAL_CLIENT_DIR=fdb-clients`.
  - `dst`: `CARGO_TARGET_DIR=target/madsim RUSTFLAGS="--cfg madsim" MADSIM_TEST_NUM=3 cargo test
    -p wyrd-dst --test custodian operator_abort_race`.

| # | Mutation | Red | Green |
|---|---|---|---|
| N21 | F1 reverted: FDB cluster file falls back to the relative path when not UTF-8 | fdb-it `abort_refuses_an_fdb_path…` (0/1) | fdb-lib (UTF-8 values, by design) |
| N22 | F2 reverted: external-client dir not carried | fdb-it (0/1), fdb-lib `the_fdb_spelling…` (0/1) | — |
| N22b | paths carried as given, not made absolute | lib `the_printed_environment…` (0/1), fdb-it (0/1), fdb-lib (0/1) | — |
| N23 | F3 reverted: no instruction for a pass holding this upload | lib `the_operator_abort_answers…` (0/1), it leg F (0/1) | — |
| N24 | shared refusal made lossy (U+FFFD spelling) | it leg G redb (0/1), fdb-it (0/1) | — |
| N26 | a `Completed` read answered `Stopped{RoundsSpent}` | dst `…race_with_a_live_complete_reaches_every_end` (1 passed, 1 failed), it leg C (0/1) | — |
| N26-old | N26, with the race row's iteration-3 predicate | — | **dst 2/2 green**: the old row did not bind |

- The brief's five negations (N1–N5) and iteration 3's N13–N20 test code this round did not
  touch. I did not re-run them.
- F4 is a fix to the test harness, not to production, so it has no production negation. The
  demonstration under F4 shows the old and new behaviour side by side.

## Red → green through the project's runner

- **Command:** `engine/scripts/run-verify.sh`, run as C4-verify runs it, from the PDCA repo:
  - `PDCA_BUNDLE=<bundle> PDCA_VERIFY_BASE=origin/pdca-integration/main-r9dd123c9ed70`
  - `PDCA_LANE=b1018 WYRD_REPO=<worktree> WYRD_VERIFY=$PDCA_SCRATCH/pdca-builder-1018-verify`
  - bounded by `timeout 5400`.
- **GREEN (fix applied):** `running 7 tests` … `test result: ok. 7 passed; 0 failed`.
- **RED (production reverted, test kept):** `running 7 tests` … `test result: FAILED. 0 passed; 7
  failed`. **7 tests ran, 7 failed.** That is neither a zero-test run nor a compile failure.
  - Every red is an exit-status assertion with stderr `wyrd: unknown command \`mpu\`` (exit 2).
  - Failure points: leg A `:579` (after its "the one-shot alone moves nothing" checks passed on
    the base), B `:652`, C `:731`, D `:810`, E `:917`, F `:975`, G `:1110`.
- **Verdict line:** `run-verify.sh: PASS — red without the fix, green with it (7 test(s) ran red).`
- That run used the patch from before the one comment line added to the FDB subprocess test (see
  Gates run). The `fdb`-gated test is not compiled by `run-verify.sh`, so the verdict is the same.
- The `fdb`-only test is not compiled by `run-verify.sh`, which builds default features. Its
  red→green is N21/N22 above, run by hand under `--features fdb` (libfdb_c is installed on this
  host).

## Refuting my own test

- **(a) Genuine red? Yes.**
  - `run-verify.sh` reverted every non-test file and kept the test file: 7 ran, 7 failed.
  - At the item level, each fix this round was reverted on its own (N21–N24, N26) and a test went
    red each time.
  - For F4, the old harness ran 20 s past a 3 s bound without failing, and the new one fails at
    the bound.
- **(b) Production path? Yes.**
  - Legs A–G run the built `wyrd` through the real `run` dispatch and `cmd_mpu_abort`.
  - Leg G's FDB case runs a real `--features fdb` binary, and the refusal it checks comes from the
    production `store_spelling` → `path_environment` → `absolute_path`.
  - Leg F runs the exact lines the binary printed, through a real shell.
  - The unit tests call the production functions (`path_environment`, `store_spelling`,
    `operator_abort_answer`).
  - The race row runs the production `complete_upload` and `operator_abort`.
  - The test file names no symbol the patch adds.
- **(c) Fixture includes the fault? Yes.**
  - Leg G's working directory really has a non-UTF-8 name (`not-utf8-\xff`), and the paths are
    really relative to it.
  - Leg G's redb case has a real store with a live `Open` session at the path the relative flag
    names, so a wrong "open it anyway" would have something to act on.
  - The N21 run shows the unfixed binary reaching FDB from that fixture.
  - Leg F has the second held session that makes every pass `BLOCKED`.
  - The race has a live Complete racing the abort.

## Gates run

- `cargo fmt --all -- --check`: clean.
- `typos` over every touched file: clean.
- Clippy, each with `-D warnings`, all clean:
  - `cargo clippy -p wyrd-server --all-targets`;
  - `cargo clippy -p wyrd-server --features fdb --tests`;
  - `WYRD_TIKV_TOOLCHAIN=1 cargo clippy -p wyrd-server --features tikv,etcd --tests` (the TiKV arm
    changed shape, `.into_iter().collect()`);
  - `RUSTFLAGS="--cfg madsim" cargo clippy -p wyrd-dst --all-targets`.
- **Tests:**
  - `cargo test -p wyrd-server --test multipart_operator_abort`: 7/7; with `--features fdb`: 8/8.
  - `cargo test -p wyrd-server --lib`: 84/84.
  - DST `operator_abort` rows (`MADSIM_TEST_NUM=3`): 7/7.
- `./engine/xtask.sh ci` (the C4 gate, `cargo xtask ci` in the worktree, bounded by
  `timeout 7000`) **exited 1 on one step**: `cargo test --workspace --exclude wyrd-dst`, where
  `wyrd-gateway-s3`'s `tests::a_bodyless_response_is_recorded_complete_not_aborted`
  (`crates/gateway-s3/src/lib.rs:4919`) failed with an empty capture (`got: `).
  - **It is a flake that predates this patch.** The patch touches no gateway code.
  - Re-run 6 times on the patched worktree: failed once.
  - Re-run 12 times on the unpatched base (the verify checkout at 6e44de9, which holds only the new
    test file): failed once.
  - A second full `cargo test --workspace --exclude wyrd-dst` on the patched tree: exit 0, every
    test binary `ok`.
  - Every other step passed: typos, `lint_docs: OK`, `render_site --check` (link audit OK), the
    gitlink/unsafe/blackbox guards, fmt, workspace clippy and build, machete, deny (all three),
    statics, deploy-guard, DST clippy, and the DST suite at 50 seeds (custodian target: 49 passed,
    1 ignored, all seven `operator_abort_*` rows included).
- After that run I added one comment line to the FDB subprocess test (why it sets
  `WYRD_FDB_TRANSACTION_TIMEOUT_MS`). I re-checked fmt, typos, default clippy, and the subprocess
  file in both builds (7/7, 8/8).
- The final `patch.diff` (192,265 bytes) applies cleanly on 6e44de9 (`git apply --cached --check`
  on a scratch index).
- No NEEDS-HUMAN external dependency. `typos` and `docs-renderer`, the two the brief names, run
  inside `xtask ci`. libfdb_c, used for the hand-run `fdb` tests, is installed here and is not
  needed by any gate.

## Things to look at (Check §6 candidates)

- **Patch size: 192 KB** (192,265 bytes), up from 180 KB.
  - `cli.rs` +4.6 KB: `path_environment`, the FDB arm, the F3 sentence, tests.
  - Subprocess test +6.0 KB: leg G (both cases), the bounded drain, the F3 assertion.
  - Docs +0.6 KB (each paragraph is one long line, so any edit re-sends it whole).
  - DST +0.3 KB.
- **`#[cfg_attr(not(feature = "fdb"), allow(dead_code))]` on `path_environment`.** This is the
  price of keeping its mutants killable in the default-feature C5 gate (see F2).
- **The `[store: fdb at cluster file …]` label still prints the cluster file as given**, possibly
  relative (`reaper_store_name`, shared with the one-shot). The printed commands carry the
  absolute path. I left the label alone: it is base code shared with the one-shot, and the
  sign-off said nothing else.
- **The `fdb`-only tests do not run in CI.** `xtask ci` only clippy-checks the `fdb` build, behind
  `WYRD_FDB_TOOLCHAIN`. I ran them by hand. The logic they guard lives in `path_environment` and
  `absolute_path`, which the default build tests.
- **Five review-batch findings recorded as declined by the sign-off.** See the table above, and
  confirm or strike them.
- **A flaky gateway test can fail C4-ci.** `a_bodyless_response_is_recorded_complete_not_aborted`
  failed 1 in 12 on the unpatched base (see Gates run). If C4-ci goes red on it, re-run; it is not
  this patch.
- **C4 diff coverage** failed in iterations 1–3 with "patch.diff does not apply on origin/main".
  That is the gate choosing `origin/main` over the stack base. It will recur until the gate honours
  `PDCA_VERIFY_BASE` as `run-verify.sh` does.

## Scratch

- Working files are under `$PDCA_SCRATCH` with the `pdca-builder-1018-` prefix:
  - `neg4/`: the negation script, its output, the pre-run diff and file copies, and the F4 demo
    script;
  - `applycheck/`: the scratch index;
  - `verify-iter4.log`, `xtask-ci-iter4.log` and `wstest-iter4.log`.
- The verify checkout `pdca-builder-1018-verify` from iteration 3 was reused.
- The harness owns that root, and the filesystem rule says cleanup is not the builder's, so I did
  not delete anything.
- The worktree's index has an intent-to-add entry for the new test file (`git add -N`), so that
  `git diff HEAD` includes it.
- `target/madsim` in the worktree is a separate cargo target dir for the simulator build. I used it
  so the `--cfg madsim` build would not throw away the normal build cache.

## Not done

- No push, no PR.
- The `expire` verb, the session-record field, the alarm, and anything under
  `crates/custodian/src/` belong to the sibling child, and are untouched.
