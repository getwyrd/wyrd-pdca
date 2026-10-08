# Build notes — #843 (809.5): seeded Tier-0 DST coverage for the restore session fence

Target: getwyrd/wyrd @ main, stacked base `0b48ab7` (origin/main plus children 1–4; both fence
shapes present: `crates/custodian/src/restore.rs:801-866` `fence_session`, `:886-918`
`plan_fence`). Patch: test-only, one file, `crates/dst/tests/custodian.rs` (+907/−2). No
production change; no new file under `crates/dst/tests/`.

## What the patch adds (line numbers are on the patched file)

- Imports: `decode_retire_obligation`, `PartScope`, `RetireMode`, `RetireToken`, `MPU_PREFIX`
  (`custodian.rs:71-75`).
- **Property 18** section, `custodian.rs:4996-5878`:
  - `FenceMeta` (`:5094-5199`): a recording tap over `SimTikvMetadataStore`. It forwards every
    trait call unchanged (hops included). It logs each `mpu:` listing's value of the session the
    instant the store answers, and each answer to a commit that writes the session
    (`FenceAnswered`). The concurrent writer commits through `writer_commit`, which logs
    `WriterAnswered`. With `strike: Some(fate)`, the pass's first commit writing the session is
    answered `CommitUnknownResult` (1021, out of flight), applied whole or not at all, exactly as
    `AmbiguousSweepMeta` does (`:3975` on base).
  - `seed_restored_session` (`:5265`): seeds the session, one committed part (chunk on disk on
    D server 1), the attempt's `seg:` record (Completing arm), and a committed object whose only
    fragment is gone (the dangling chunk). It writes straight into the model, so the fixture is
    neither logged nor struck. The writer batches follow the brief: Complete fence
    `Open@3 → Completing@4` (attempts 1→2, nonce unchanged, no obligation), and root flip
    `Completing@3 → Completed@4` writing session + segmented inode + dirent +
    `retire:records:s:<id>:3 {"parts":[[1,1]]}`, all under `require` / `require_absent`.
  - `assert_owed` (`:5431`): asserts the WHOLE `retire:` namespace equals the expected key set,
    and checks every record by payload through the production `decode_retire_obligation`.
  - `restore_fence_under_a_concurrent_writer` (`:5509`): one run of the production
    `reconcile_after_restore` with the writer landing at `delay_micros`. It judges from the
    pass's own read: the last `mpu:` listing in the pass (the fence's, `restore.rs:789`). Core
    assertion (`:5571-5582`): exactly one transition out of `@E` (writer committed + fence
    committed from a read of `@E` == 1). Then regime-specific state, obligations, report naming,
    and publication checks.
  - D1 seeded leg `prop_restore_fence_never_shares_the_epoch` (`:5728`), D2
    `restore_fence_after_an_ambiguous_commit` + `prop_restore_fence_settles_an_ambiguous_commit`
    (`:5740`, `:5837`), D3 `prop_restore_fence_reaches_the_contested_window` (`:5854`).
- Registrations with `dst_campaign_test!` (`:6046-6062`), and the two seeded legs appended to
  `committed_regression_seeds_stay_green` (`:6103-6104`). They are appended last, so they draw from
  the shared RNG after every existing property: no existing property's draws change.

No existing constant moved. Two new ones: `RESTORE_FENCE_SPAN = 14` (`:5046`) and
`RESTORE_FENCE_DRAWS = 4` (`:5051`). No existing property's assertions changed.

## Key design decisions

**Store: `SimTikvMetadataStore` behind a tap, never `MemMeta`.** As the brief requires: `MemMeta`
never yields, so no writer could land between the pass's read and its commit.

**Which read is "the pass's read".** The pass lists `mpu:` twice: the staged-class read
(`gc.rs:1555`) and the fence's own listing (`restore.rs:789`). Nothing after the fence listing
reads `mpu:` (`check_attempt` / `recheck_fenced` read `seg:`, `part:`, `retire:`). So the fence's
read is the last `SessionRead` in the pass. I deliberately did not use the fence commit's
`require` value as the record of the read: mutation (a) removes that precondition, and the oracle
has to keep working under it.

**Landing granularity: whole milliseconds plus 500 µs**, the precedent of property 17
(`REPLACE_SPAN`, `custodian.rs:4479-4486` on base). Every step of the pass is on an integer-ms
hop, so a half-ms writer never ties with one. Its commit applies strictly between two of the
pass's steps, and the scheduler's tie-breaking never decides the regime.

Measured on seed 1 (same on every seed, since half-ms landings avoid ties): the regime per landing
(ms) was identical in both arms:

| landing | 0–4 | 5 | 6 | 7–14 |
|---|---|---|---|---|
| regime | WriterFirst | WriterWon{stale:true} | WriterWon{stale:false} (lock) | FenceWon |

The fence's listing is the 7th hop. A writer starting at 5.5 ms prewrites at 6.5 and applies at
7.5. The listing at 7.0 still reads `@E`; the fence's prewrite at 8.0 then misses its precondition.
That is a genuine stale-preimage `Conflict`, and the recorded order shows it:
`SessionRead(@E) < WriterAnswered(Committed) < FenceAnswered(Conflict)`. At 6.5 ms the writer
still holds its prewrite lock when the fence prewrites, so the fence gets a lock `Conflict` and
the writer applies after. D3 counts that as `stale: false`, not as the window.

**Why 4 draws per arm per seed (`RESTORE_FENCE_DRAWS`).** The stale window is 1 landing of 15
per arm. On my first try with one draw per seed, mutation (a) was not caught by the 50-seed
range starting at 843000. Ranges 1, 1000 and 5000 all caught it (seeds 37, 1024, 5013).
(14/15)^50 ≈ 3% per 50-seed run. With 4 draws, (14/15)^200 ≈ 1e-6, and range 843000 then
fails at seed 843008. Each run takes about 1 ms of real time. D3 walks every landing
deterministically, so the coverage claim never rests on the draws.

**Ruled out:**
- *A reply hop on every read, as `HandoffMeta` does* (`HANDOFF_REPLY_MILLIS`, base `:2643-2648`).
  It widens the stale window to ~4 landings, but it lengthens every read equally. The pass becomes
  ~4× longer and the window's share of the span stays the same (~1/reads). It adds sleeps to the
  tap for no gain in density. A reply hop on the `mpu:` listing alone would raise density but
  would be an unexplained asymmetry in the model. Half-ms landings keep the tap a pure forwarder,
  as `RecordingMeta` is.
- *Expected values minted by production code* (`open_teardown`, `retire_key`): expected session
  bytes and `retire:` keys are spelled out by hand (`fence_session`, `fence_retire_key`), so the
  oracle does not share code with what it judges. Obligation payloads are decoded through the
  production decoder, because the brief asks for exactly that.
- *A new test file*: forbidden by the brief's Verification posture.

## D4 — demonstrated falsifiability (both mutations, one at a time, then restored)

Runner for these: `timeout … env RUSTFLAGS="--cfg madsim" MADSIM_TEST_NUM=… MADSIM_TEST_SEED=…
cargo test -p wyrd-dst --test custodian restore_fence` (the same build `cargo xtask dst` makes,
filtered to the new properties, with an explicit timeout). Line numbers in the pasted panics are
from before `cargo fmt` reflowed the file; the assertion messages identify them: the D1 check is
now `custodian.rs:5571-5582`, the D2 namespace check is now `:5446-5451`.

### (a) Drop the fence's precondition on the session bytes

Mutation at `crates/custodian/src/restore.rs:824`:
`WriteBatch::new().require(key.to_vec(), read.to_vec())` → `WriteBatch::new()`.

- D1 seeded (`MADSIM_TEST_NUM=50`): **FAILED**. Range 843000 first fails at
  **`MADSIM_TEST_SEED=843008`**; range 1 fails at seed 1 (with one draw per seed, before the
  4-draw change, ranges 1/1000/5000 failed at seeds 37/1024/5013).
- D3: **FAILED** (deterministic, Open arm, landing 5.5 ms).
- D2: passed (it has no concurrent writer, so it is not what (a) breaks).
- Completing arm under (a): not caught, as expected. The flip installs
  `retire:records:s:<id>:3`, so the fence's `require_absent` on that key still refuses it. The
  Open arm is where (a) is visible.

Failure (seed 843008, `restore_fence_never_shares_the_epoch_with_a_session_writer`; D3 printed the
same panic):

```
panicked at crates/dst/tests/custodian.rs:5573:5:
assertion `left == right` failed: the fence (read {"parent":0,"object":"fenced","created_at_millis":100,"clock_source":"wall","segment_nonce":"84308430843084308430843084308430","epoch":3,"attempts":1,"state":{"kind":"Open"}}, answered Some((4, Committed))) and the writer (Committed) — exactly one transition out of @3 may land — Open arm, writer at 5500 µs: [Pass, SessionRead(Some(b"{...\"epoch\":3,\"attempts\":1,\"state\":{\"kind\":\"Open\"}}")), SessionRead(Some(b"{...\"epoch\":3,\"attempts\":1,\"state\":{\"kind\":\"Open\"}}")), WriterAnswered(Committed), FenceAnswered(Committed)]
  left: 2
 right: 1
note: run with `MADSIM_TEST_SEED=843008` environment variable to reproduce this error
```

Both the Complete fence and the precondition-less restore fence landed out of `@3`.

### (b) Split the transition and its obligations into two commits

Mutation at `crates/custodian/src/restore.rs:824-832`: commit 1 = `require(session bytes)` + put
session; then, only if commit 1 was `Committed`, commit 2 = `require_absent(obligation keys)` +
put obligations; the existing `match` over the combined outcome.

- D2 seeded (`MADSIM_TEST_NUM=50`): **FAILED**. Range 843000 fails at
  **`MADSIM_TEST_SEED=843000`**; range 1 fails at seed 1. It fails on every arm drawn with fate
  `Landed`.
- D1 and D3: passed, as expected. Once the session commit lands the session is no longer `@E`,
  so no writer's precondition can hold between the two commits.

Failure (seed 843000, `restore_fence_settles_an_ambiguous_commit_on_the_next_pass`):

```
panicked at crates/dst/tests/custodian.rs:5443:5:
assertion `left == right` failed: the `retire:` namespace holds an obligation without its transition, or misses one — Open arm, Landed: [Pass, SessionRead(Some(b"{...\"epoch\":3,\"attempts\":1,\"state\":{\"kind\":\"Open\"}}")), SessionRead(Some(b"{...}")), FenceAnswered(Unknown(Landed))]
  left: []
 right: ["retire:bytes:s:84384384384384384384384384384384:3"]
note: run with `MADSIM_TEST_SEED=843000` environment variable to reproduce this error
```

The struck commit landed the session as `Aborting@4` with no obligation, which is a transition
without its obligations. The re-run would not repair it: `plan_fence` maps `Aborting` to
`Plan::Fenced` (`restore.rs:907-913`), and `recheck_fenced` (`:935-967`) only reads.

After each mutation: `git checkout -- crates/custodian/src/restore.rs`. `git status` then shows
only `crates/dst/tests/custodian.rs` modified, and `patch.diff` contains no production change.

## Refuting my own test (forced)

- **(a) Genuine red?** Yes. This patch adds tests only, so there is no fix to revert. The base's
  "red" is the property's absence: on base, `reconcile_after_restore` is called only by property
  11 (`custodian.rs:2031`, `:2123` on base), over a store with no `mpu:` key. The demonstrated
  red is D4: breaking the production fence two ways turns D1+D3 (a) and D2 (b) red, with seeds
  and failures pasted above. Restoring the fence turns them green.
- **(b) Production path?** Yes. Every run calls `wyrd_custodian::reconcile_after_restore`, which
  calls the production `fence_open_sessions` → `fence_session` → `plan_fence`, and the decode
  checks use the production `decode_retire_obligation` / `decode_session_record`. The tap only
  forwards. The proof that it is the production fence being exercised is D4 itself: editing
  `restore.rs` flips the result.
- **(c) Fixture includes the fault?** Yes. The concurrent writer is a real madsim task committing
  to the same simulated-TiKV store. D3 asserts on the recorded order that its commit applied
  between the fence's read and the fence's commit (the stale-preimage `Conflict`) in BOTH arms,
  so the race is present, not curated out. D2's nemesis really strikes the fence's commit: it
  asserts exactly one `FenceAnswered(Unknown(fate))`, and the pass's `Err` downcasts to
  `CommitUnknownResult`. The dangling chunk is really missing from the fleet (D2 asserts the
  `dangling` audit line came before `session-fence-failed`).

## Gates run

- `cargo fmt --all` then `cargo fmt --all -- --check`: clean.
- `typos crates/dst/tests/custodian.rs` (typos-cli 1.48.0): clean. `docs-renderer` deps present
  (`python3 -c "import markdown_it, yaml"`); the patch touches no docs.
- `./engine/xtask.sh dst` (the project's runner, `cargo xtask dst` in the worktree):
  **exit 0**. `cargo clippy -p wyrd-dst --all-targets` under `--cfg madsim` is clean, and every
  `wyrd-dst` test passes at `MADSIM_TEST_NUM=50`, `custodian.rs` 31/31 (28 existing + 3 new), the
  regression-seed test included.
- `./engine/xtask.sh ci` (criterion L): **exit 0, "xtask ci: all checks passed"**. It ran typos,
  docs lint and render `--check`, the gitlink/unsafe/blackbox guards, `cargo fmt --check`,
  workspace clippy/build/test, cargo-machete, cargo-deny (two `*-not-encountered` config
  warnings, not failures), the ADR-0035 statics gate, the deploy guard, then `wyrd-dst` clippy and
  tests under `--cfg madsim` at 50 seeds. Every existing property in `custodian.rs` passes
  unchanged.

## Out of scope, for the human

- `crates/custodian/src/restore.rs:785` carries `// deferred: #843 — seeded Tier-0 DST coverage of
  this fence (809.5).` Once this lands that marker is stale, but the brief limits this child to
  the test file ("Nothing outside that file"). Removing it needs a one-line follow-up, or the
  human may fold it into this PR.
- No real defect in the fence turned up: on the unmodified fence the three new properties are
  green on seed ranges 1, 1000, 5000 and 843000 at `MADSIM_TEST_NUM=50` each (run against the
  `cargo xtask dst` build, after the restore), and in `cargo xtask dst`'s own time-seeded sweep.
