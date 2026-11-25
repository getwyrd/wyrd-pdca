# Build notes — issue 1018, iteration 3: `wyrd mpu abort` (FU-6 part 1)

Target: getwyrd/wyrd @ main, stacked on `origin/pdca-integration/main-r9dd123c9ed70` (6e44de9, the
worktree HEAD and the bundle's `stack-base-tip`). Every `path:line` below is on the **patched**
tree unless it says "base".

This iteration keeps the design the sign-off kept (one-batch fence, exact-attempt `{seg}`
obligation, `{session, all}` for both doors) and changes only what the iteration-2 carry-forward
and `review-batch.md` list. I started from iteration 2's `patch.diff` applied to the worktree and
changed it from there.

## Carry-forward, item by item

### 1. Cleanup hint, store selection

- **Absolute `--data-dir`.** `absolute_data_dir` (`crates/server/src/cli.rs:3166`) runs
  `std::path::absolute` once, in `cmd_mpu_abort` (`cli.rs:3150-3154`), for redb only. That one
  string is what the abort opens, what the `[store: …]` label names (`reaper_store_name`, base
  code, now fed the absolute path) and what the printed commands carry. A non-UTF-8 result is
  refused before the store is opened, since the printed line could not name it. Networked
  backends skip it: they take no directory, and resolving the default `wyrd-data` would only add
  a failure mode (no working directory) to a command that does not need one.
- **Network connection settings in the printed command.** `StoreSpelling` (`cli.rs:3220`) holds
  the backend flag, the redb directory, and for a networked backend the address variable and its
  resolved value. `store_spelling` (`cli.rs:3253`) fills it the way the open reads it: TiKV's
  `WYRD_TIKV_PD_ENDPOINTS` through `parse_endpoints` and re-joined with `,`; FoundationDB's
  `WYRD_FDB_CLUSTER_FILE` through the driver's own `config::cluster_file` (so the package default
  is spelled out when the variable is unset), then made absolute (`cli.rs:3267-3277`), since a
  relative cluster file is read against the working directory just as a relative redb directory
  is. This last step was not asked for by name; I added it after the full gate run, when a
  self-review found the same class of defect there. `StoreSpelling::command` (`cli.rs:3233`) prints
  `VAR='value' wyrd <words> --metadata-backend <b> [--data-dir '<dir>']`, every value through
  `shell_quoted`. The old `teardown_command` is gone; both printed commands come from the one
  spelling, so they cannot disagree on the store.
- An address that does not resolve (unset or empty PD list) is spelled as none. That line is
  never printed: `open_tikv_meta` refuses the same value before the abort runs.

### 2. Cleanup hint, stop condition

- The text no longer says "until it reports satisfied". `operator_abort_answer`
  (`cli.rs:3361`, text at `:3369-3375`) prints the one-shot, then: run it "pass after pass, until
  `<this abort, spelled for the store>` answers that upload X is absent (its record mpu:X
  deleted). A pass reports on the whole store: one that is BLOCKED by held lines naming only other
  uploads has not stalled this one".
- Why the probe is the abort itself: there is no read-only metadata inspector on the CLI, and the
  abort is safe to re-run here. On an `Aborting` session it reads and answers `AlreadyAborting`
  with nothing written; once the drain deletes the record it answers `NoSuchUpload`, exit 1. An
  `Aborting` session never goes back to a state the abort fences, so the probe cannot fence
  anything new.
- `usage()` (`cli.rs:514-518`) and both architecture docs say the same.

### 3. Tests for items 1 and 2

- **Leg F** (`crates/server/tests/multipart_operator_abort.rs:922`), new, one more test in the
  same file:
  - Two foreign-clocked `Open@2` sessions, X and Y, each with a committed part.
  - The abort of X runs from the temp root with a **relative** `--data-dir "it's a store"` and
    with the backend chosen by `WYRD_METADATA_BACKEND=redb` scoped to that one command, no flag.
  - The test reads both printed commands out of stderr and runs them **as printed, through
    `sh -c`**, from a different temp directory, with `WYRD_METADATA_BACKEND=no-such-backend` and
    `PATH` led by the built binary's directory (`path_with_wyrd`, `:1065`).
  - That replays the hint after a working-directory change and checks that an env-chosen backend
    survives into it: a relative path finds no store there, and a line without its own
    `--metadata-backend` fails on the bogus variable.
  - Every pass must exit 1 with `BLOCKED` naming Y. The loop stops on the probe's own absent
    answer (`:997`), bounded by `CONVERGE_RUNS`.
  - Then X is fully torn down and marked, `mpuctl` is 1, and Y's `mpu:`, `part:` and `psum:`
    records are byte for byte as seeded.
  - It also checks the `[store: redb at …]` label is absolute and canonicalizes to the store
    file.
- **The networked spelling.** The default build has only redb, so the TiKV/FDB arms cannot run in
  the default suite (an open to a real cluster would fail before the hint prints anyway). Covered
  three ways:
  - `the_printed_commands_name_the_store_the_abort_acted_on` (`cli.rs:4634`, default build)
    checks the exact `VAR='…' wyrd …` strings, quoting included.
  - `the_tikv_spelling_carries_the_pd_endpoints_this_process_connects_to` (`:4683`,
    `cfg(feature = "tikv")`) and `the_fdb_spelling_…` (`:4712`, `cfg(feature = "fdb")`) mirror
    the existing `the_tikv_arm_names_…` tests: the expected value is worked out from the
    variable's literal name.
  - I ran both with a variable scoped to the command:
    - `WYRD_TIKV_TOOLCHAIN=1 WYRD_TIKV_PD_ENDPOINTS=' http://pd-a:2379 , http://pd-b:2379,' cargo test -p wyrd-server --features tikv,etcd --lib -- the_tikv_`
    - `WYRD_FDB_CLUSTER_FILE=<v> cargo test -p wyrd-server --features fdb --lib -- the_fdb_`, for
      `v` = `clusters/a.cluster` (relative), `/clusters/b.cluster` and empty (the package default)
  - All passed (2/2 each run).
  - Limit: with the variable **unset**, as in CI, the tikv arm test cannot tell "resolved to
    nothing" from "dropped the address". Negation N17 below was therefore run with the variable
    set.
- `the_abort_resolves_its_redb_directory_to_an_absolute_path` (`cli.rs:4669`).

### 4. C5 survivors (`report_operator_abort`'s two match arms)

- Leg A now asserts `"action":"operator-abort-fenced"` on stderr (`:570`).
- The `FencedUnattributed` arm: I took the "make it assertable" option, not "accept the
  survivor". The audit choice moved into a pure function, `operator_abort_audit` (`cli.rs:3334`,
  returning `AbortAudit` at `:3321`). `report_operator_abort` (`cli.rs:3294`) now only emits what
  it returns, and has no match arm left to delete. The unit test
  `the_operator_abort_audits_only_a_fence_of_its_own_that_landed` (`cli.rs:4796`) asserts the
  action and epoch for `Fenced` and `FencedUnattributed`, and `None` for the four other answers.
- The DST unknown-outcome row cannot assert it: `wyrd-dst` has no `wyrd-server` dependency, and
  the audit line is the CLI's.
- `operator_abort_audit` names every variant explicitly, with no `_` arm, so cargo-mutants has no
  "delete match arm" mutant to make there. Its "replace with `None`" mutant is caught by leg A and
  the unit test (negations N16a/N16b).
- Both `RUST_LOG`-sensitive helpers now `env_remove("RUST_LOG")` (`:120`), so an inherited filter
  cannot hide the audit line from leg A.

### 5. `crates/core/src/multipart.rs:4157`

- The `{session, all}` row now reads "the restore fence's, the operator abort's, and the reaper's
  at the residency ceiling `W_session` (#1017, not yet built)".
- The `{session, parts: <set>}` row (`:4158`) now says no writer installs it, and that the two
  that are built install `{session, all}`.
- 0016 is not edited.

### 6. REQUIRED: seeded DST race of `operator_abort` against a live Complete

In `crates/dst/tests/custodian.rs`, `mod operator_abort_rows`, section at `:9029`:

- **Fixture** (`race_store`, `:9074`):
  - The production `create_session` on the simulated-TiKV model, in a bucket.
  - Two parts of `MAX_MAP_CHUNKS / 2 + 1` one-fragment chunks, planted as the part commit writes
    them, in the codec's own spelling.
  - Together they are one chunk past a flat map, so the production `complete_upload` writes the
    attempt's `seg:` records in a segment batch, reads them back, then flips.
  - Reused from `reaper_rows`, made `pub(super)` (`:8149`, `:8153`, `:8213`, `:8217`):
    `OneIdentity`, `BUCKET`, `MARKER`, `COMPLETE_PARENT`, `complete`.
- **The race** (`race_at`, `:9229`):
  - The Complete runs from the start. The abort waits on a permit the Complete's first landed
    commit (its fence) adds (`Writer`, `:9142`), then sleeps `start_micros` and runs the
    production `operator_abort`.
  - Both run as one `futures_util::future::join` over the same store. Each writer has its own
    logging handle, so the row knows how many of the abort's fence commits answered `Conflict`.
  - Starts are 1.5 ms + k ms (`RACE_FIRST_START_MICROS`, `:9063`): off the model's whole-ms hop
    grid, so no hop of the abort ties with one of the Complete and every run is deterministic;
    and past madsim's 1 ms minimum sleep.
  - The abort starts after the fence, so it always reads `Completing@1` or later. That keeps the
    end states to the two the sign-off named.
- **Judging** (`judge_race`, `:9270`): every run must end in exactly one of the two states.
  - **`Completed`**: the Complete answered `Published`, and the abort answered `Completed@2`, or
    `Stopped{RoundsSpent}` with none of its fences landed. **No obligation names
    `seg:<g>:1:`**. The segments and the dirent exist. Three drain passes later, the segments are
    byte-identical and the object is still published.
  - **`Aborting@2`**: the abort answered `Fenced{2}` and the Complete `Refused(NoSuchUpload)`;
    nothing is published. The store holds exactly `retire:bytes:s:<id>:1` `{session, all}` and
    `retire:records:s:<id>:1`, which names `seg:<g>:1:`. Then `assert_reclaimed_of` (`:8730`,
    iteration 2's `assert_reclaimed` with the upload and group as parameters) runs the real
    reaper and drain until the record is gone, and requires both `retire:` ranges and the
    attempt's segment range empty and `mpuctl` at 0.
  - Anything else panics.
- **Rows**, registered at `:9724` and `:9730`:
  - `operator_abort_races_a_live_complete_and_strands_nothing`: the start is drawn from the
    seed, k in `0..=RACE_SPAN_MILLIS`.
  - `operator_abort_race_with_a_live_complete_reaches_every_end`: a sweep over every k. It
    requires five things to be reached, each asserted:
    - fenced before the segment batch;
    - fenced after the segment batch and before the flip, so the flip loses;
    - fenced after losing a round to a Complete commit (the CAS input changed; re-read and
      re-planned);
    - lost to the flip, then refused the `Completed` upload;
    - started after the flip.
- **The sweep as measured** (debug print, removed before the patch):

  | Start | Outcome | Abort rounds lost |
  |---|---|---|
  | 1.5–4.5 ms | Aborted before the segment batch | 0 |
  | 5.5 ms | Aborted after the segment batch: lost a round to it, re-read cursor 2, fenced, flip lost | 1 |
  | 6.5 ms | Completed: lost to the segment batch, then to the flip | 2 |
  | 7.5 ms | Aborted between the segment batch and the flip | 0 |
  | 8.5–9.5 ms | Completed: lost to the flip | 1 |
  | 10.5 ms on | Completed: started after the flip | 0 |

  Every Aborted run had the Complete answer `NoSuchUpload`.
- **One segment batch, on purpose.** The core suite's deep fixture shows what a second batch
  costs: 250 parts at `MAX_PART_CHUNKS` (about 39,000 planted chunks a run) to pass
  `MAX_BATCH_OPS`. A second batch adds no new transition for the abort: every segment batch
  moves the cursor the abort's fence requires, which this one already does (the 5.5 ms and 6.5 ms
  runs). The row's doc comment says this, so the reviewer sees the reason.
- `RACE_SPAN_MILLIS` is 12: the flip lands about 9 ms after the fence, so the last starts are
  past it.
- **Cost:** the two rows ran in 0.20 s together at `MADSIM_TEST_NUM=3`.

### 7. Exit 2 for a valued flag with a missing trailing value

- **Rejected, as decided at the iteration-2 sign-off**: exit 2 is the usage-error code, and the
  logging pre-parse refusing a dangling value flag before any command runs is deliberate (#531).
  The behaviour is unchanged.
- To make it no longer look like a gap, `cmd_mpu_abort`'s doc now says it (`cli.rs:3109-3111`).
- Both T4 findings are recorded in `review-rejected.md` in the bundle, at `cli.rs:3116`.

### `review-batch.md` (9 entries)

- `cli.rs:3110` ×2 (exit 2): recorded-rejected (item 7).
- `cli.rs:3196` (relative path): fixed (item 1).
- `cli.rs:3190` (network env): fixed (item 1).
- `custodian.rs:9275` / `:8832` / `:8788` (no race against a live completer): fixed (item 6).
- `multipart.rs:4157` ×2: fixed (item 5).

## Atomicity (brief: required, not in the binding criterion)

Unchanged from iterations 1 and 2. Both doors build one `WriteBatch` with `fence_batch`
(`crates/core/src/multipart/abort.rs:95-111`) and send it in one `store.commit` (`abort.rs:526-527`):

| Door | Preconditions | Mutations |
|---|---|---|
| `Open@E` | `require(mpu:<id> == bytes read)`, `require_absent(retire:bytes:s:<id>:<E>)` | put `mpu:<id>` = `Aborting@E+1`; put `retire:bytes:s:<id>:<E>` = `{session, all}` |
| `Completing@E` | `require(mpu:<id> == bytes read)`, `require_absent(retire:bytes:s:<id>:<E>)`, `require_absent(retire:records:s:<id>:<E>)` | put `mpu:<id>` = `Aborting@E+1`; put `retire:bytes:s:<id>:<E>` = `{session, all}`; put `retire:records:s:<id>:<E>` = `{seg: <nonce, E>}` |

The `{seg}` group is `attempt_segment_group()`'s (`multipart.rs:2768`), pinned by decode to the
`Completing` epoch, so only `seg:<nonce>:<E>:` is named (F18). The DST cut sweep (iteration 2) is
the interleaving that tells one batch from two, and negation N2 shows it does.

## Negation runs (this iteration, all re-run on the final tree)

- **How they ran:** a script (`$PDCA_SCRATCH/pdca-builder-1018-neg/neg.py`) applied each mutation
  as an exact-text replacement, ran the named tests and restored the files. Afterwards `git diff`
  of the worktree was byte-identical to the saved pre-run diff.
- **Test commands:**
  - server: `cargo test -p wyrd-server --test multipart_operator_abort <filter>`;
  - lib: `cargo test -p wyrd-server --lib <filter>`;
  - core: `cargo test -p wyrd-core --test multipart_abort`;
  - DST: `RUSTFLAGS="--cfg madsim" MADSIM_TEST_NUM=3 cargo test -p wyrd-dst --test custodian <filter>`.
- The brief's five are N1–N5.

| # | Mutation | Red | Green |
|---|---|---|---|
| N1 | `Completing` door installs only `retire:bytes`, no `{seg}` obligation (X57) | leg B, leg E, DST `completing_door_cut_reaches_both_sides`, both race rows | — |
| N2 | fence batch carries only the bytes obligation; `{seg}` committed in a **second** batch | DST `completing_door_cut_reaches_both_sides` | **leg B** (as the brief predicted), both race rows (the race cuts nothing, so both batches land) |
| N3 | `{seg}` names epoch `E+2` instead of `E` (F18) | leg B (decoy), DST completing sweep, both race rows | — |
| N4 | `Completed` answered as `AlreadyAborting` | leg C, both race rows | — |
| N5 | the fence batch also deletes the upload's `part:` records | legs A and B | — |
| N13 | the abort keeps the relative `--data-dir` | **leg F** | the `absolute_data_dir` unit test (it tests the helper, not its use) |
| N14 | printed commands omit `--metadata-backend` | leg F, leg A, unit `the_printed_commands_name_…` | — |
| N15 | stop condition back to "until it reports satisfied" | leg F, unit `the_operator_abort_answers_…` | — |
| N16a | no audit line for `Fenced` | leg A, unit `…audits_only_…` | — |
| N16b | no audit line for `FencedUnattributed` | unit `…audits_only_…` | — |
| N17 | TiKV spelling drops the address (variable set for the run) | tikv-feature unit `the_tikv_spelling_…` | — |
| N19 | a `Conflict` that moved the session retried with the stale bytes | core `d_*`/`h4_*` (4 tests), **race sweep** | legs A–E, DST unknown-outcome row |
| N20 | FDB cluster file left relative (run with `WYRD_FDB_CLUSTER_FILE=clusters/a.cluster`) | fdb-feature unit `the_fdb_spelling_…` | — |

- **N19 explained.** It is a regression only a concurrent writer shows. Every DST row before this
  iteration stays green under it, and the race sweep goes red: the "fenced after losing a round"
  end is no longer reached.
- **N2 and the race.** The race does not see N2 because it never kills the operator between the
  two commits. The cut sweep is the row that catches it, as in iteration 2.
- **A filter miss in my script.** N19's DST filter `operator_abort_cut_reaches` matched 0 tests
  (the rows are `operator_abort_{open,completing}_door_cut_reaches_both_sides`). I report it as
  what it was, not as green. The race sweep and the core tests are what catch N19.

## Red→green through the project's runner

- **Command:** `engine/scripts/run-verify.sh`, run by hand as C4-verify runs it, from the PDCA
  repo:
  - `PDCA_BUNDLE=<bundle> PDCA_VERIFY_BASE=origin/pdca-integration/main-r9dd123c9ed70`
  - `PDCA_LANE=b1018 WYRD_VERIFY=$PDCA_SCRATCH/pdca-builder-1018-verify`
  - bounded by `timeout 5400`.
- **GREEN (fix applied):** `running 6 tests` … `test result: ok. 6 passed; 0 failed`.
- **RED (production reverted, test kept):** `running 6 tests` … `test result: FAILED. 0 passed;
  6 failed`. **6 tests ran, 6 failed.** That is neither a zero-test run nor a compile failure.
  Each failed on an exit-status assertion, with stderr `wyrd: unknown command \`mpu\`` (exit 2):

  | Leg | Failed at | Note |
  |---|---|---|
  | A | `multipart_operator_abort.rs:564` | after its "the one-shot alone moves nothing" checks passed on the base |
  | B | `:637` | |
  | C | `:716` | "a Completed session is refused, handled" |
  | D | `:795` | |
  | E | `:902` | |
  | F | `:960` | |

- **Verdict line:** `run-verify.sh: PASS — red without the fix, green with it (6 test(s) ran red).`
- I ran it twice, the second time on the final `patch.diff` (after the FDB cluster-file change),
  with the same counts and the same failure lines both times.

## Refuting my own test

- **(a) Genuine red? Yes.**
  - `run-verify.sh` reverted every non-test file and kept the test file: 6 ran, 6 failed.
  - Every refusal leg asserts exit **1** plus a message naming the state. The base's exit-2
    "unknown command" cannot pass them.
  - At the mutation level, each of the 13 negations above turns at least one test red. The
    expected exception is N2, which only the DST cut sweep sees, as the brief predicted.
- **(b) Production path? Yes.**
  - The subprocess legs run the built `wyrd` through the real `run` dispatch, `cmd_mpu_abort`,
    the real redb open and `operator_abort`. The teardown is the real
    `wyrd custodian --reap-abandoned-uploads`.
  - Leg F runs the exact lines the binary printed, through a real shell.
  - The test file names no symbol the patch adds.
  - The race row runs the production `create_session`, `complete_upload`, `operator_abort` and
    reaper/drain. Its `Writer` wrapper only logs commits and signals the fence; it changes no
    answer.
- **(c) Fixture includes the fault? Yes.**
  - Sessions are stamped under a clock the reaper does not own. Leg A first shows the one-shot
    alone leaves the store byte-identical.
  - Leg F includes the second held session that makes every pass `BLOCKED` (the adversary's
    reproduction), plus a working-directory change and a hostile `WYRD_METADATA_BACKEND` at
    replay.
  - The race has a live Complete writing real segment batches and a real flip against the abort.
    The sweep proves every interleaving class was reached, not just tried.

## Gates run

- `cargo fmt --all -- --check`: clean.
- `cargo clippy -p wyrd-core -p wyrd-server --all-targets -- -D warnings`: clean.
- `RUSTFLAGS="--cfg madsim" cargo clippy -p wyrd-dst --all-targets -- -D warnings`: clean.
- `WYRD_TIKV_TOOLCHAIN=1 cargo clippy -p wyrd-server --features tikv,etcd --tests -- -D warnings`
  and `cargo clippy -p wyrd-server --features fdb --tests -- -D warnings`, after touching
  `cli.rs`: clean.
- **Tests:**
  - `cargo test -p wyrd-server --test multipart_operator_abort`: 6/6.
  - `cargo test -p wyrd-server --lib`: 83/83.
  - Tikv- and fdb-feature spelling tests with the variables set: 2/2 each run.
  - DST `operator_abort` rows at `MADSIM_TEST_NUM=5`: 7/7.
- `./engine/xtask.sh ci` (the C4 gate, `cargo xtask ci` in the worktree, bounded by
  `timeout 7000`): **`xtask ci: all checks passed`**. That covers typos, `lint_docs`,
  `render_site --check`, the repo guards, fmt, workspace clippy/build/test, machete, deny,
  statics, deploy-guard, DST clippy and the DST suite at the gate's 50 seeds.
  - In that run the custodian DST target was 49 passed, 1 ignored (the known-gap row), in 158 s,
    all seven `operator_abort_*` rows included.
  - **After it** I changed one thing in `cli.rs`: the `cfg(feature = "fdb")` arm of
    `store_spelling`, its `cfg(feature = "fdb")` test, and one doc line. The default build does
    not compile that arm. Re-checked after the change: `cargo fmt --check`; `typos` over every
    touched file; default, tikv and fdb clippy; the server's 6 integration and 83 lib tests; the
    fdb spelling test with three variable values; and `run-verify.sh` on the final patch. I did
    not re-run the whole `xtask ci`.
- The final `patch.diff` (180,517 bytes) applies cleanly on 6e44de9 (`git apply --cached --check` on a scratch index).
- External dependencies the brief names: `typos` and `docs-renderer` run inside `cargo xtask ci`.
- No NEEDS-HUMAN external dependency.

## Things to look at (Check §6 candidates)

- **Patch size: about 180 KB**, up from about 138 KB.
  - Growth: DST +20.9 KB (the race), `cli.rs` +12.1 KB (`StoreSpelling`, the audit function,
    their unit tests), subprocess test +7.3 KB (leg F), docs +0.7 KB.
  - About 27 KB of the total is the two architecture-doc paragraphs. Each is one long line, so
    any edit re-sends the whole paragraph.
- **Visibility changes in existing DST code**: `reaper_rows::{OneIdentity, BUCKET, MARKER,
  COMPLETE_PARENT, complete}` are now `pub(super)`, on top of iteration 2's `ledger` and `reap`.
  The race reuses the real Create/Complete drivers rather than a second copy.
- **Leg F needs `sh`.** It runs the printed lines through `sh -c`, the honest replay of a shell
  line. Any Linux CI host has it. A Windows runner would not, but nothing in this workspace targets
  Windows.
- **The TiKV/FDB arms of `store_spelling` and the `--data-dir`-on-networked refusal** are
  compiled but not exercised in the default suite. I ran the spelling arms by hand under their
  features (item 3).
- **No CLI-level timeout** around the abort, unchanged from iterations 1 and 2: `operator_abort`
  is not cancel-safe once a commit is sent, so it relies on the store's own deadlines, which come
  back as `CommitUnknownResult`.
- **C4 diff coverage** failed in iterations 1 and 2 with "patch.diff does not apply on
  origin/main". That is the gate choosing `origin/main` over the stack base, not this patch, and
  it will recur until the gate honours `PDCA_VERIFY_BASE` as `run-verify.sh` does.

## Scratch

- Working files are under `$PDCA_SCRATCH` with the `pdca-builder-1018-` prefix: the negation
  script and its results, a pre-negation diff, a copy of the DST file, the verify checkout and
  its log, and the xtask log.
- The harness owns that root, and its filesystem rule says cleanup is not the builder's, so I
  did not delete them.
- The verify run registers a `pdca-verify-lb1018` worktree in the primary repo. `git worktree
  prune` drops it once the scratch dir is reclaimed.
- The worktree's index has an intent-to-add entry for the new test file (`git add -N`), so that
  `git diff HEAD` includes it.

## Not done

- No push, no PR.
- The `expire` verb, the session-record field, the alarm, and anything under
  `crates/custodian/src/` belong to the sibling child, and are untouched.
