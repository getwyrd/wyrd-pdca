# Build notes — issue 1018: `wyrd mpu abort` (FU-6 part 1)

Target: getwyrd/wyrd @ main, stacked on `origin/pdca-integration/main-r9dd123c9ed70`
(6e44de9, the worktree HEAD and the bundle's `stack-base-tip`). Every `path:line` below is on
the **patched** tree unless it says "base".

## What the patch does

1. **`crates/core/src/multipart/abort.rs`** — a new `operator_abort(store, upload_id)`
   (`abort.rs:381`) and its answer type `OperatorAbortOutcome` (`abort.rs:330`).
   - `Open@E` → `SessionRecord::open_teardown` (the client Abort's / reaper's teardown).
   - `Completing@E` → `SessionRecord::completing_teardown` (`abort.rs:403`), the restore
     fence's teardown that #658/#842 landed (base `crates/core/src/multipart.rs:2767`), with
     both of its obligations.
   - `Aborting` → `AlreadyAborting { epoch }`; `Completed` → `Completed { epoch }`; absent →
     `NoSuchUpload`. No write in any of those three.
   - Both fences are built by the existing `fence_batch` (base `abort.rs:83`) and committed in
     one `store.commit` (`abort.rs:487`).
   - To avoid a second copy of `abort_session`'s round/doubt loop, that loop moved into a
     private `fence_rounds` (`abort.rs:448`) that both Aborts call. `abort_session`
     (`abort.rs:297`) now passes a plan closure that keeps its old answers exactly.
   - `lost_any` (`abort.rs:520`) runs the existing `lost` classifier over each obligation key,
     because the `Completing` fence requires two keys absent and `lost` takes one.
   - `Doubt.puts` now keeps the `SessionRecord` beside its bytes, so a fence found landed after
     an unknown outcome can answer with the epoch it put (`abort.rs:604`). `Fenced` carries
     only that epoch, and `Round::Fence` boxes its session: both to satisfy clippy's
     `large_enum_variant`, which the first gate run failed on.
   - Doc text of `Stop::RoundsSpent` / `Stop::EpochExhausted` no longer says "Open" only.
2. **`crates/server/src/cli.rs`** — the `wyrd mpu` group.
   - Dispatch arm `cli.rs:421`; usage lines `cli.rs:500-508`.
   - `cmd_mpu` (`cli.rs:3056`) → `cmd_mpu_abort` (`cli.rs:3093`).
   - Flags only: stray positional refused (`cli.rs:3095`), as `cmd_custodian` does
     (`cli.rs:978`, base `:967`); unknown flags refused against `MPU_ABORT_FLAGS` (`cli.rs:3073`), so there
     is no clock-source flag and a typo is not silently ignored; a second `--upload-id`
     refused (`cli.rs:3109`). `UploadId::new` refuses `*` and any non-id, so no wildcard.
   - `ParsedArgs` now records repeated flags (`repeated`, `cli.rs:3266`) and can name an
     unknown flag (`unknown_flag`, `cli.rs:3272`). No other command's parse changes.
   - Backend open mirrors `run_reaper_once_over_backend` (`operator_abort_over_backend`,
     `cli.rs:3137`). redb uses a new `open_existing_redb_or` (`cli.rs:2185`), factored out of
     `open_existing_local_meta_redb` so the abort never creates a store either; the reaper's
     refusal text is unchanged.
   - `report_operator_abort` (`cli.rs:3168`): exit 0 for fenced / already aborting; `Err` →
     the dispatcher's handled `ExitCode::FAILURE` (1) for `Completed` and absent, with stderr
     naming the state ("Completed@N, a terminal session: abort does not apply to a terminal
     session" / "is absent"). A `tracing::warn!` audit line on `wyrd.mpu` for a fence.
3. **`crates/server/tests/multipart_operator_abort.rs`** (new) — legs A–D, argv only.
4. **`crates/dst/tests/custodian.rs`** — `mod operator_abort_rows` (`custodian.rs:8495`),
   four campaign rows (`custodian.rs:8985` onward) and the regression-seed replay line
   (`custodian.rs:9078`).
5. **Docs** — `docs/design/architecture/06-runtime-view.md:96` (new paragraph in 6.7, with
   grant term (iii)) and `08-crosscutting-concepts.md:113` (appended sentences).
6. **`crates/core/src/multipart.rs:4157-4158`** — doc-only: the `RetirePayload` table said the
   operator abort's `Completing` fence installs `{session, parts: <set>}`; it now says it
   installs the restore fence's `{session, all}`, which is what the code does.

## Scope calls the human should look at (Check §6 candidates)

- **An edit one layer below the CLI (`crates/core/src/multipart/abort.rs`).** The brief puts
  the verb in `cli.rs` and says anything else is a §6 item, so here it is. The reason: the
  brief also requires DST cases in `crates/dst/tests/custodian.rs` that drive the fence, and
  `wyrd-dst` cannot reach `wyrd-server`: its dev-deps (`crates/dst/Cargo.toml:19-50`) have no
  `wyrd-server`, and adding it would pull the whole server binary crate (tokio runtime, the S3
  gateway, gRPC) into the madsim build. I did not try that build, so I am not claiming it
  fails — only that it is a much larger change than a function in core. Putting the fence in `cli.rs` would have left atomicity as a code-read
  obligation only. In core it sits beside the one `fence_batch` builder, which is where the
  brief's "reuse, don't rewrite" points. No record class, state, or the state machine changed:
  only a new caller of `open_teardown` / `completing_teardown` / `fence_batch` / `lost`.
  Nothing under `crates/custodian/src/` is touched.
- **Payload shape differs from the brief's wording, on purpose.** The brief says the `Open`
  door installs `retire:bytes:{session}` and the `Completing` door `{session, parts}`
  (`0016:664-665`). The patch reuses the existing constructors, which mint `{session, all}`
  for both (`multipart.rs:4157`, base `:2741`, `:2767`). That is the brief's own "reuse the
  path #658 landed; do not write a third copy". `all` is also why the restore fence uses it:
  a part set can outgrow the value ceiling for a sparse session (#842, `multipart.rs:4157`),
  and no part commits outside `Open`, so `all` names exactly the parts. Leg A/B assert
  `session() && parts() == Some(&PartScope::All)`; the drain then marks every part's bytes
  (legs A and B assert the marks).
- **The client `abort_session` was refactored**, not only extended. Behaviour should be
  identical: same read/plan/round/commit/classify order, `EpochExhausted` still checked before
  the round budget, one obligation key passed to `lost`. Its existing tests ran in the gate
  (see "Gates run"). Rejected alternative: copy the loop into `operator_abort`. Cost: the loop
  is `abort.rs` base lines 288-359 (72 lines) plus the `Doubt` helper, so a copy is ~70 lines
  of duplicated doubt handling — the "third copy" the brief forbids.
- **Not bounded by a timeout in the CLI.** `operator_abort` is not cancel-safe once a commit is
  sent (same as `abort_session`, `abort.rs` doc), so wrapping it in a timeout could drop a
  fence with no answer. It relies on the store's own per-operation deadlines, like the reaper
  one-shot does. Rubric "await discipline" reviewers may raise it; that is the reason.
- **A rival fence with identical bytes.** After an unknown outcome, if a restore pass fenced
  the same `Completing@E` to the same `Aborting@E+1` bytes, `operator_abort` reports
  `Fenced`. The obligations a rival writes are the same keys and payloads, so nothing is
  lost. This is the same property `abort_session` already has against the reaper.

## Atomicity (brief's "required but not in the binding criterion")

**(ii) The batch, for the reviewer.** Both doors build exactly one `WriteBatch` through
`fence_batch(session_key, read, &session, &obligations)` (`abort.rs:487`; builder base
`abort.rs:83-99`) and send it in one `store.commit`:

| Door | Preconditions | Mutations |
|---|---|---|
| `Open@E` | `require(mpu:<id> == bytes read)`, `require_absent(retire:bytes:s:<id>:<E>)` | put `mpu:<id>` = `Aborting@E+1`; put `retire:bytes:s:<id>:<E>` = `{session, all}` |
| `Completing@E` | `require(mpu:<id> == bytes read)`, `require_absent(retire:bytes:s:<id>:<E>)`, `require_absent(retire:records:s:<id>:<E>)` | put `mpu:<id>` = `Aborting@E+1`; put `retire:bytes:s:<id>:<E>` = `{session, all}`; put `retire:records:s:<id>:<E>` = `{seg: <nonce, E>}` |

The `{seg}` group's epoch is the session's `attempt_segment_group()` epoch, which decode pins to
the `Completing` epoch `E` (publish target epoch), so only `seg:<nonce>:<E>:` is named (F18).

**(i) The DST cases** (`crates/dst/tests/custodian.rs`, `mod operator_abort_rows`). Over the
simulated-TiKV model (every read and commit is a real madsim await; commit has a prewrite and a
mid-commit hop), seed one door's session, run `operator_abort` under
`madsim::time::timeout(cut)` — the operator process dying at `cut` — and read the store. A
one-batch fence can leave only two states: the seed with no `retire:` key, or `Aborting@E+1`
with every obligation of the door (decoded and checked). Anything else panics.
- `operator_abort_{open,completing}_door_cut_reaches_both_sides`: every cut from 125µs to
  7875µs in 250µs steps (half-ms offsets, so no tie with a 1ms hop); asserts both states are
  reached and exactly one flip, so the commit point is inside the sweep.
- `operator_abort_{open,completing}_door_cut_never_strands`: one cut drawn from the seed; also
  in the regression-seed replay.
- This is the interleaving the brief asked for: **negation 2 (second batch) is caught by it**
  (below), at cut 3125µs, "a fenced session carries every obligation of its door".

## Negation runs (brief's verification posture)

Each was a temporary edit to `crates/core/src/multipart/abort.rs` (pristine copy kept in the
scratch dir and restored after; `diff -q` confirmed pristine and `git apply --check -R
patch.diff` confirmed the tree equals the shipped patch). Run twice: once before the clippy fix
and once on the final code, with the same results.

| # | Mutation | Caught by | Result |
|---|---|---|---|
| 1 | `Completing` door installs only `retire:bytes`, no `retire:records:{seg}` | leg B (and both `completing` DST rows) | leg B FAILED; DST `completing_door_*` FAILED (3125µs/5625µs cuts) |
| 2 | `{seg}` obligation in a **second** batch | DST only | leg B **passed** (as the brief predicted); DST `completing_door_cut_reaches_both_sides` and `..._never_strands` FAILED: "a fenced session carries every obligation of its door" at cut 3125µs (sweep) and 4375µs / 4625µs (seeded), `Err(Elapsed)`. The sweep row catches it on every run; the seeded row catches it only when the seed draws a cut inside the ~2 ms gap between the two commits (about a quarter of draws), so over `cargo xtask dst`'s 50 seeds it is near-certain but not guaranteed |
| 3 | `{seg}` names epoch `E+2` (the decoy's group) instead of `E` | leg B | FAILED |
| 4 | `Completed` answered as `AlreadyAborting` (abort applies) | leg C | FAILED |
| 5 | after the fence, the verb deletes the upload's `part:` records itself | legs A and B (the "abort writes exactly these keys and deletes nothing" check before the one-shot) | FAILED |

Server legs run with `cargo test -p wyrd-server --test multipart_operator_abort` (bounded by
`timeout 900`); DST with `RUSTFLAGS="--cfg madsim" MADSIM_TEST_NUM=5 cargo test -p wyrd-dst
--test custodian operator_abort` (bounded by `timeout 1500`) — the flags `xtask::run_dst` uses.

On negation 3: a literal "whole `seg:<g>:` range" obligation is not expressible — a
`SegmentGroup` always carries an epoch and `RetireObligation` has no group-wide row. The nearest
wrong-range mutation is naming a different epoch's range, which is what #3 does.

## Red→green through the project's runner

`engine/scripts/run-verify.sh`, run by hand as C4-verify runs it:
`PDCA_BUNDLE=<bundle> PDCA_VERIFY_BASE=origin/pdca-integration/main-r9dd123c9ed70
PDCA_LANE=b1018 WYRD_VERIFY=$PDCA_SCRATCH/pdca-builder-1018-verify`, bounded by `timeout 5400`.
The lane suffix and scratch `WYRD_VERIFY` keep it off the harness's own `wyrd-verify*`
checkouts (the sibling builder for 1016 did the same; its worktree is registered as
`pdca-verify-lb1016`). This one registers `pdca-verify-lb1018` in the primary repo's worktree
list; the next `git worktree prune` drops it once the scratch dir is reclaimed.

- **GREEN (fix applied):** `cargo test -p wyrd-server --test multipart_operator_abort` —
  `test result: ok. 4 passed; 0 failed`.
- **RED (production reverted, test kept):** `test result: FAILED. 0 passed; 4 failed` —
  **4 tests ran, 4 failed**. Not a zero-test run and not a compile failure: the test names only
  base symbols and compiled on the base. Each leg failed on the verb's exit status, with
  stderr `wyrd: unknown command \`mpu\`` (exit 2): leg A at `multipart_operator_abort.rs:518`
  (after its pre-abort one-shot assertions had passed on the base), B at `:583`, C at `:662`
  ("a Completed session is refused, handled": got 2, wants 1 — the discriminating refusal), D
  at `:730`.
- Verdict line: `run-verify.sh: PASS — red without the fix, green with it (4 test(s) ran red).`

## Refuting my own test

- **(a) Genuine red? Yes.** `run-verify.sh` reverted every production file (core, CLI, docs)
  and kept the test: 4 ran, 4 failed (above). The base's "unknown command" path also leaves
  the store untouched and exits non-zero, which is why every refusal in leg C/D asserts exit
  **1** exactly plus a state-naming message: on the base they get 2 and fail. Mutation-level
  red as well: negations 1, 3, 4, 5 each turn a leg red; negation 2 turns the DST rows red.
- **(b) Production path? Yes.** The subprocess legs run the built `wyrd` binary
  (`env!("CARGO_BIN_EXE_wyrd")`) through the real `run` dispatch, the real `cmd_mpu_abort`,
  the real redb store open, and `wyrd_core::multipart::abort::operator_abort` committing real
  batches; the teardown is the real `wyrd custodian --reap-abandoned-uploads` one-shot. The
  test file calls no symbol the patch adds. The DST rows call the same production
  `operator_abort` over `SimTikvMetadataStore`, the DST tier's existing second
  `MetadataStore` model (`crates/dst/tests/support/mod.rs`), not a copy of the fence.
- **(c) Fixture includes the fault? Yes.** The sessions are stamped `clock_source: "logical"`,
  a clock the reaper does not own — the exact absorbing case. Leg A first runs the one-shot
  alone and asserts it exits non-zero and leaves the store byte-identical (the defect, on the
  fixture), then aborts. Leg B seeds the attempt's `seg:<g>:3:*` **and** a decoy
  `seg:<g>:5:*` range plus the `seggrp:` marker, and the parts with real chunk placements, so
  the drain has real bytes to mark. The DST rows inject the fault the atomicity claim is about
  — the operator process dying — at every 250µs across the run, and assert both the "before"
  and "after" states are reached, so the sweep is not vacuous.

## Gates run

- `cargo xtask ci` (typos, lint_docs, render_site `--check`, gitlink/unsafe/blackbox guards,
  `cargo fmt --check`, workspace clippy, build, `cargo test --workspace --exclude wyrd-dst`,
  cargo-deny incl. the all-features advisories config, conformance vectors, statics,
  deploy-guard, DST clippy and `cargo test -p wyrd-dst` at 50 seeds): **all checks passed** on
  the final tree. The first run failed only on clippy `large_enum_variant`
  (`OperatorAbortOutcome::Fenced` and `Round::Fence`); fixed as described above, then rerun.
- `cargo fmt --all` run over every touched file; `cargo fmt --all -- --check` clean.
- External dependencies the brief names: `typos` 1.48.0 present, clean on every touched file;
  `docs-renderer` (`markdown_it`, `yaml`) present, `render_site.py --check`: link audit OK.
- No NEEDS-HUMAN external dependency.

## Things I did not do

- No push, no PR.
- The `expire` verb, the session-record field, the alarm, and anything under
  `crates/custodian/src/` are the sibling child's; untouched.
- No test for the tikv/fdb arms of `operator_abort_over_backend`: they are `#[cfg]`-gated out of
  the default build, as the reaper one-shot's are.
