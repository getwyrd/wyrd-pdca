# Build notes — issue 1018, iteration 2: `wyrd mpu abort` (FU-6 part 1)

Target: getwyrd/wyrd @ main, stacked on `origin/pdca-integration/main-r9dd123c9ed70` (6e44de9,
the worktree HEAD and the bundle's `stack-base-tip`). Every `path:line` below is on the
**patched** tree unless it says "base".

This iteration keeps iteration 1's design (one-batch fence, exact-attempt `{seg}` obligation,
`{session, all}` for both doors, accepted at sign-off) and fixes every item of the carry-forward
and all 15 entries of `review-batch.md`. No finding is recorded-rejected, so there is no
`review-rejected.md`.

## What changed since iteration 1, item by item

### CLI parsing (carry-forward; review findings at v1 `cli.rs:3094`, `:3102`, `:3126`)

- **Strict parse, token by token, before any store access.** `ParsedArgs::parse_exact`
  (`crates/server/src/cli.rs:3352`) refuses, in argv order: a flag not in `MPU_ABORT_FLAGS`
  (`cli.rs:3086`), a flag given a second time, and a value that is missing, empty or starts
  with `--`. Because it reads tokens, not the parsed map, an unknown flag can no longer hide as
  another flag's value (`--data-dir --gc-expired-pending --data-dir DIR`,
  `--data-dir --reap-abandoned-uploads`). `cmd_mpu_abort` calls it first (`cli.rs:3109`).
- **Removed** iteration 1's `ParsedArgs::repeated` field / `repeated()` / `unknown_flag()`: the two
  C5 survivors (v1 `cli.rs:3240`, `:3273`) lived there; that code no longer exists.
  `ParsedArgs::parse` is back to its base form, so no other command's parsing changes.
- **Every repeated flag is refused**, not only `--upload-id`. A destructive verb should not pick
  "the last `--data-dir`" silently. The adversary called this a convention call; I chose refusal.
- **`--data-dir` is refused on a networked backend** (`cli.rs:3134`): with
  `WYRD_METADATA_BACKEND=tikv` in the environment, an explicit `--data-dir` would otherwise be read
  and dropped, and the abort would act on the cluster. Mirrors the reaper one-shot refusing flags
  it would ignore (`REAPER_ONE_SHOT_EXCLUDES`, base `cli.rs:1679`). **Not testable in the
  default build**: `MetadataBackend` has only `Redb` there, so the branch is dead and C4 diff
  coverage will show it uncovered. It compiles under `--features tikv,etcd` and `--features fdb`
  (both clippy runs clean, below).
- A trailing flag with no value (`... --data-dir` at the end) still exits **2**, from
  `install_logging`'s generic parse (base `cli.rs:455`), before the verb runs. That is the
  existing behavior of every command, so the tests do not claim exit 1 for it.

### Operator output (carry-forward; v1 `abort.rs:464`, `:468`, `cli.rs:3173`, `:3202`)

- **Teardown hint for the same store.** `teardown_command` (`cli.rs:3189`) prints
  `wyrd custodian --reap-abandoned-uploads --metadata-backend <b>` plus, for redb,
  `--data-dir '<dir>'`, quoted by `shell_quoted` (`cli.rs:3202`, POSIX single quotes, `'` →
  `'\''`). The backend is always spelled out (`MetadataBackend::flag_value`, `cli.rs:173`), even
  when the environment chose it, so the line runs the same in another shell. The default dir is
  printed explicitly when `--data-dir` was not given.
- **No rival's fence claimed as ours.** `OperatorAbortOutcome::FencedUnattributed { epoch }`
  (`crates/core/src/multipart/abort.rs:346`): the session was found exactly as our fence leaves
  it, after an unknown outcome. `Fenced` now means only "the store acknowledged our commit"
  (`abort.rs:528`). The CLI says "this abort or a rival fence of the same bytes put it", exit 0,
  and the audit line uses a separate action, `operator-abort-fence-unattributed`
  (`cli.rs:3209-3235`), instead of `operator-abort-fenced`.
- **No "nothing written" after doubt.** `OperatorAbortOutcome::AbsentAfterDoubt`
  (`abort.rs:366`): no record, read while a fence of ours may have landed unseen. The CLI says
  "possibly fenced and already torn down, and nothing is left to abort", exit 1, and does not say
  "nothing written" (`operator_abort_answer`, `cli.rs:3246`).
- How "may have landed unseen" is decided: `Doubt::may_have_landed` (`abort.rs:655`) is
  `held.is_some() || unsettled.is_some()`, asked **before** `settle` clears them
  (`abort.rs:505`). That is exact: `held` = a may-still-commit fence whose required bytes were
  still in place at the last read; `unsettled` = an out-of-flight outcome no successful read
  followed. If both are empty, every unknown fence of this run was settled by a read that found
  the session present and not our put, and an `Aborting` session can only be deleted, never
  re-opened, so a later absence is not ours. I rejected the simpler `!puts.is_empty()`: it would
  say "possibly fenced" even after a read had proven our fence never landed.
- `fence_rounds` (`abort.rs:486`) now passes `absent` that bool and `fenced` an `acknowledged`
  bool. `abort_session` (`abort.rs:302`) ignores both (`|_|`, `|_, _|`), so the client Abort's
  answers are byte-for-byte what they were. The advisory reviewer said not to widen this to the
  client Abort; I did not.
- Rename note: the helper was first called `unsettled()`, which collided with the field of the
  same name in the doc link; renamed to `may_have_landed()`.

### Tests (carry-forward; review TEST-GAPs)

- **Both subprocess helpers pass `--metadata-backend redb`** (`mpu_abort` and `reap_once`,
  `crates/server/tests/multipart_operator_abort.rs:178`, `:200`), so `WYRD_METADATA_BACKEND` in the
  environment cannot redirect them. The store directory is now `<tmp>/it's a store` (`:90-104`),
  so leg A's check that stderr contains `` `<teardown_line>` `` (`:558`) fails unless the
  printed command is quoted correctly; leg A then runs that same argv.
- **Leg D** (`:758`) adds, each asserting exit 1, the refusal text, and a byte-identical store:
  `--clock-source logical` (unknown flag); `--data-dir X --data-dir X --upload-id A --upload-id B`
  and `--log-level info --log-level info --upload-id A --upload-id B` (repeated non-id flags beside
  a repeated id); `--data-dir ""`; `--data-dir --gc-expired-pending --data-dir DIR`; and
  `--data-dir --bogus` run from a root where `--bogus/meta.redb` really holds the `Open` session
  (the reviewer's probe: on iteration 1 this exited 0 and fenced it).
- **Leg E** (`:877`): a `Completing@3` session whose `retire:records:s:<id>:3` key alone is taken.
  The fence conflicts, `lost_any` finds the bytes key free (Unexplained) and the records key taken
  under an unchanged session (Taken), so the verb exits 1 naming the **records** key, store
  unchanged.
- **CLI unit tests** (`cli.rs:4509`, `:4522`): exact teardown strings and quoting; every
  `operator_abort_answer` variant's exit and wording, including that `FencedUnattributed` does not
  say "fenced to" and `AbsentAfterDoubt` does not say "nothing written".
- **Core test** (`crates/core/tests/multipart_abort.rs:1397`, new `OnCommit::UnknownTornDown` at
  `:400`, `:555`): over a real redb store, a fence applied then answered 1021 →
  `FencedUnattributed { epoch: 1 }`; a fence applied, the session record deleted, answered 1021 →
  `AbsentAfterDoubt`; a later abort with nothing in doubt → `NoSuchUpload`. Added so the new
  `abort.rs` logic is covered by plain `cargo test` (what C5's `cargo mutants` runs), not only
  under `--cfg madsim`. Deleting the `mpu:` key stands in for the drain's terminal delete here;
  the DST row below runs the real drain.
- **DST, late landing after the operator gives up** (`crates/dst/tests/custodian.rs`,
  `mod operator_abort_rows`, `:8502`):
  - `Sent` (`:8754`) wraps the simulated-TiKV store: a commit request spends one hop in transit
    (`:8780`), then runs on its own madsim task, deliberately left running when the caller drops it (the rubric's "abort spawned tasks on drop" does not apply: that task is the store's side of the commit, the fault being injected). A cut during transit drops the request; once
    delivered, it lands even though the abort was cut before the answer. This fixes the v1 model,
    where the cut dropped the commit future and its write could never land.
  - `abort_cut_at` (`:8795`) waits `LAND_MILLIS` after the cut, judges the two-state invariant
    (`judge`, `:8672`), and when a sent-but-unanswered fence landed, runs the real reaper +
    record-only drain (`reaper_rows::reap`, made `pub(super)` at `:7427`; `ledger` at `:7092`)
    until the session is reclaimed (`assert_reclaimed`, `:8719`: record gone, both `retire:`
    ranges and the attempt's `seg:` range empty, `mpuctl.count` back to 0).
  - `prop_operator_abort_cut_reaches_both_sides` (`:8843`) now also requires at least one such
    late landing. Measured sweep (Completing door): cuts 125–1875 µs → untouched; 2125–3875 µs →
    landed after the cut, reclaimed (8 cuts); 4125 µs on → finished normally.
  - Why the transit hop: madsim sleeps at least 1 ms and `timeout` polls the inner future first
    (`madsim-0.2.34/src/sim/time/mod.rs:111-140`), so the earliest cut always falls after the
    abort's first read. Without the hop, every cut sent the commit and no "untouched" state was
    reachable. I recorded this in the `CUT_SPAN_MICROS` doc; iteration 1's "every cut at a
    half-millisecond" comment overstated the precision below 1 ms.
  - `prop_operator_abort_outcome_unknown` (`:8963`, campaign row `:9303`), per door, with the
    `Unknown` wrapper (`:8891`): **InFlight** — every fence answered 1031 and held; the abort
    spends all `ABORT_ROUNDS` and returns `InDoubt { may_still_commit: true, RoundsSpent }`, store
    untouched; then the held fences land in send order: first `Committed`, the rest `Conflict`;
    then the drain reclaims it. **Landed** → `FencedUnattributed`. **LandedAndTornDown** (the real
    drain runs inside the commit, before the abort's re-read) → `AbsentAfterDoubt`.

### Docs

- `ABORT_ROUNDS` (`abort.rs:72-75`): one sentence that the budget is sized for the `Open` door; a
  live completer on the `Completing` door can spend it, which ends `RoundsSpent`, fails closed,
  and a re-run starts over.
- `RetirePayload` table (`crates/core/src/multipart.rs:4157-4158`), as decided at sign-off: the
  `{session, all}` row names all three `Completing→Aborting` fences (restore, reaper at
  `W_session`, operator abort); the `{session, parts: <set>}` row is now "0016's spelling; no
  writer installs it". 0016 itself is not edited.
- `docs/design/architecture/06-runtime-view.md:96`: strict command line, `--data-dir` refused on
  networked backends, the printed one-shot, and the two unknown-outcome answers.
  `08-crosscutting-concepts.md:113`: the printed one-shot. Grant term (iii) text unchanged.

## Atomicity (brief's "required but not in the binding criterion")

Unchanged from iteration 1. Both doors build one `WriteBatch` with `fence_batch`
(`abort.rs:526`; builder `abort.rs:95-111`) and send it in one `store.commit`:

| Door | Preconditions | Mutations |
|---|---|---|
| `Open@E` | `require(mpu:<id> == bytes read)`, `require_absent(retire:bytes:s:<id>:<E>)` | put `mpu:<id>` = `Aborting@E+1`; put `retire:bytes:s:<id>:<E>` = `{session, all}` |
| `Completing@E` | `require(mpu:<id> == bytes read)`, `require_absent(retire:bytes:s:<id>:<E>)`, `require_absent(retire:records:s:<id>:<E>)` | put `mpu:<id>` = `Aborting@E+1`; put `retire:bytes:s:<id>:<E>` = `{session, all}`; put `retire:records:s:<id>:<E>` = `{seg: <nonce, E>}` |

The `{seg}` group's epoch is `attempt_segment_group()`'s, which decode pins to the `Completing`
epoch `E`, so only `seg:<nonce>:<E>:` is named (F18). The DST cut sweep is the interleaving the
brief asked for, and negation 2 below shows it tells one batch from two.

## Negation runs

Each mutation was applied by a script to the worktree's `abort.rs` / `cli.rs`, the tests run, and
the file restored from a pristine copy (`cmp` confirmed after each batch). Server legs:
`cargo test -p wyrd-server --test multipart_operator_abort`; CLI units:
`cargo test -p wyrd-server --lib`; core: `cargo test -p wyrd-core --test multipart_abort`; DST:
`RUSTFLAGS="--cfg madsim" MADSIM_TEST_NUM=3 cargo test -p wyrd-dst --test custodian
operator_abort` — each bounded by `timeout`.

| # | Mutation | Red tests |
|---|---|---|
| 1 | `Completing` door installs only `retire:bytes`, no `retire:records:{seg}` (X57) | leg B; leg E; DST completing sweep, completing seeded row, unknown-outcome row |
| 2 | fence batch carries only `obligations[..1]`; `{seg}` committed in a **second** batch | **leg B stays green** (as the brief predicted); DST `operator_abort_completing_door_cut_reaches_both_sides` (fenced without its records obligation) and the unknown-outcome row; leg E also goes red, by side effect (the first batch no longer requires the records key absent). The seeded `..._cut_never_strands` row caught it in none of 3 seeds: it needs a cut inside the 2–5 ms gap, so over `cargo xtask dst`'s 50 seeds it is likely, not certain. The sweep row is the guaranteed catch. |
| 3 | `{seg}` names epoch `E+2` (the decoy's) instead of `E` (F18) | leg B (decoy assertion); leg E; DST completing rows |
| 4 | `Completed` answered as `AlreadyAborting` (abort applies) | leg C |
| 5 | the fence batch also deletes the upload's `part:` records | legs A and B ("writes exactly these keys and deletes nothing") |
| 6 | a value starting `--` is taken as the value | leg D |
| 7 | a repeated flag is not refused | leg D |
| 8 | an unknown flag is not refused | leg D |
| 9 | the teardown `--data-dir` is printed unquoted | leg A; CLI unit `the_teardown_command_names_the_store_the_abort_acted_on` |
| 10 | a fence found after an unknown outcome answered `Fenced` | core `o_...`; DST unknown-outcome row |
| 11 | an absence after doubt answered `NoSuchUpload` | core `o_...`; DST unknown-outcome row |
| 12 | `may_have_landed` always `true` | leg C (absent must say "nothing written"); core `o_...` |

My first build of negation 2 left both obligations in the first batch, so nothing was split and
everything stayed green; that was a broken mutation, not a test gap. Negations 8 and 9 first
failed to **compile** (the workspace denies unused code, and the mutation left a helper unused);
rewritten as `false && …` and an unquoting `.replace`, they compile and go red as shown.

## Red→green through the project's runner

`engine/scripts/run-verify.sh`, run by hand as C4-verify runs it:
`PDCA_BUNDLE=<bundle> PDCA_VERIFY_BASE=origin/pdca-integration/main-r9dd123c9ed70 PDCA_LANE=b1018
WYRD_VERIFY=$PDCA_SCRATCH/pdca-builder-1018-verify`, bounded by `timeout 5400`.

- **GREEN (fix applied):** `running 5 tests` … `test result: ok. 5 passed; 0 failed`.
- **RED (production reverted, test kept):** `running 5 tests` … `test result: FAILED. 0 passed;
  5 failed` — **5 tests ran, 5 failed**; not a zero-test run and not a compile failure. Each
  failed on the exit-status assertion with stderr `wyrd: unknown command \`mpu\`` (exit 2):
  leg A at `multipart_operator_abort.rs:556` (after its pre-abort one-shot assertions passed on
  the base), B at `:625`, C at `:704` ("a Completed session is refused, handled": wants 1, got 2),
  D at `:783`, E at `:890`.
- Verdict line: `run-verify.sh: PASS — red without the fix, green with it (5 test(s) ran red).`

## Refuting my own test

- **(a) Genuine red? Yes.** `run-verify.sh` reverted every non-test file and kept the new test:
  5 ran, 5 failed (above). Every refusal leg asserts exit **1** plus a state-naming message, so the
  base's exit-2 "unknown command" cannot pass them. At the mutation level, each of the 12
  negations turns at least one test red (table above).
- **(b) Production path? Yes.** The subprocess legs run the built `wyrd` binary through the real
  `run` dispatch, `cmd_mpu_abort`, `parse_exact`, the real redb open and
  `wyrd_core::multipart::abort::operator_abort` committing real batches; the teardown is the real
  `wyrd custodian --reap-abandoned-uploads` one-shot. The test file names no symbol the patch adds.
  The core and DST cases call the production `operator_abort`; their wrappers (`Scripted`, `Sent`,
  `Unknown`) only change what the store answers or when a commit runs, never the fence.
- **(c) Fixture includes the fault? Yes.** Sessions are stamped `clock_source: "logical"`, which
  the reaper does not own (leg A first shows the one-shot alone leaves the store byte-identical).
  Leg B seeds the attempt's segments and a decoy later epoch. Leg D's `--bogus` case puts a real
  store with the session at `--bogus`. Leg E puts a real record at the records key. The DST sweep
  injects the operator's death at every 250 µs and asserts both untouched and landed states, plus
  at least one fence that landed after the cut; the unknown-outcome row injects 1031/1021 answers
  on every fence commit.

## Gates run

- `cargo fmt --all` over the tree; `cargo fmt --all -- --check` clean.
- `cargo clippy -p wyrd-core -p wyrd-server --all-targets -- -D warnings`: clean.
- `RUSTFLAGS="--cfg madsim" cargo clippy -p wyrd-dst --all-targets -- -D warnings`: clean.
- `WYRD_TIKV_TOOLCHAIN=1 cargo clippy -p wyrd-server --features tikv,etcd --tests` and
  `cargo clippy -p wyrd-server --features fdb --tests` (after touching `cli.rs` so they re-check
  it): clean.
- `cargo test -p wyrd-server --test multipart_operator_abort` 5/5, `cargo test -p wyrd-server
  --lib` 81/81, `cargo test -p wyrd-core --test multipart_abort` 25/25, DST `operator_abort` rows
  5/5.
- `./engine/xtask.sh ci` (the C4 gate, `cargo xtask ci` in the worktree, bounded by
  `timeout 7000`): **`xtask ci: all checks passed`** — typos, `lint_docs`, `render_site --check`,
  the gitlink/unsafe/blackbox guards, fmt, workspace clippy/build/test, cargo-machete, cargo-deny
  (incl. the all-features advisories config), statics, deploy-guard, DST clippy and the DST suite
  at the gate's seed count, where all five `operator_abort_*` rows passed. After it, one doc-comment
  edit to `Sent` only; re-checked with fmt, DST clippy, the five DST rows and `typos`.
- `patch.diff` checked to apply cleanly on 6e44de9 (`git apply --cached --check` on a scratch
  index).
- External dependencies the brief names: `typos` and `docs-renderer` run inside `cargo xtask ci`
  (typos, `lint_docs`, `render_site --check`).
- No NEEDS-HUMAN external dependency.

## Things to look at (Check §6 candidates)

- **Patch size: ~138 KB**, over the 100 KB backstop iteration 1 already tripped. Growth over
  iteration 1 (~35 KB): DST rows +13.7 KB, CLI +6.8 KB, subprocess test +6.7 KB, core test
  +4.4 KB, `abort.rs` +3.2 KB. About 26 KB of the total is the two architecture-doc paragraphs,
  which are single long lines, so any edit re-sends the whole paragraph.
- **An edit one layer below the CLI** (`crates/core/src/multipart/abort.rs`), as in iteration 1:
  the DST cases must reach the fence, and `wyrd-dst` has no `wyrd-server` dependency. This
  iteration adds two outcome variants there and one helper.
- **Two visibility changes in existing DST code**: `reaper_rows::ledger` and `reaper_rows::reap`
  became `pub(super)` (`custodian.rs:7092`, `:7427`) so the operator-abort rows reuse the real
  reaper/drain pass rather than a second copy.
- **`--data-dir` refusal on tikv/fdb is compiled but not exercised** by any test (default build
  has no such backend).
- **No CLI-level timeout** around the abort: as in iteration 1, `operator_abort` is not
  cancel-safe once a commit is sent; it relies on the store's own deadlines, which come back as
  `CommitUnknownResult`.
- **C4 diff coverage** failed in iteration 1 with "patch.diff does not apply on origin/main": it
  used `origin/main` instead of the stack base. That is the gate's base selection, not the patch;
  it will recur unless the gate honors `PDCA_VERIFY_BASE` the way `run-verify.sh` does.

## Scratch

Working files are under `$PDCA_SCRATCH` with the `pdca-builder-1018-` prefix (pristine copies,
the negation script and results, the verify checkout and logs). The harness owns that root; I did
not delete them. The verify run registers a `pdca-verify-lb1018` worktree in the primary repo,
which `git worktree prune` drops once the scratch dir is reclaimed.

## Not done

- No push, no PR.
- The `expire` verb, the session-record field, the alarm, and anything under
  `crates/custodian/src/` are the sibling child's; untouched.
