# Build notes — #843 (809.5): seeded Tier-0 DST coverage for the restore session fence

Iteration 3. Target: getwyrd/wyrd @ main, stacked bundle base `0b48ab7` (origin/main plus
children 1–4; both fence shapes present: `crates/custodian/src/restore.rs:801-866`
`fence_session`, `:886` `plan_fence`). Patch: test-only, one file,
`crates/dst/tests/custodian.rs` (+930/−2). No production change. No new file under
`crates/dst/tests/`.

## What changed in this iteration (the carry-forward)

The carry-forward finding: when the writer wins after the pass read `@E`, the v2 oracle accepted
`ChangedUnderPass | LostConflict | ObligationKeyTaken { .. }` as the cause the pass names. That
let a wrong diagnosis through. If the fence checks its obligation keys before re-reading the
session, the Completing arm reports `ObligationKeyTaken { retire:records:s:<id>:3 }`. That key
is the root flip's own `{parts}` obligation, and the operator summary turns it into "whose
retirement key was already taken by another obligation" (`crates/server/src/cli.rs:1484`,
`:1494`) for an upload that simply published. All three v2 tests still passed under that change.

What I changed, all in `crates/dst/tests/custodian.rs` (line numbers on the patched file):

1. **The cause check is exact** (`:5671-5678`): `named[0].cause == SessionUnsettled::ChangedUnderPass`,
   nothing else. The comment above the arm (`:5655-5662`) says why: a lost fence commit is
   diagnosed by re-reading the session afresh (`restore.rs:842-846`), and that re-read returns
   the writer's state, so `ChangedUnderPass` is the only true cause.
2. **The tap records the pass's re-read**, so that "why" is checked per schedule instead of
   assumed. New event `FenceEvent::SessionGot(Option<Bytes>)` (`:5081-5084`), logged by
   `FenceMeta::get` when the key is the session's (`:5143-5149`); the tap doc comment says so
   (`:5092-5098`). The writer-won arm then asserts that the first point read of the session
   after the fence's refused commit returned the writer's state (`:5679-5689`). The cause check
   runs first, so a wrong diagnosis fails on the cause message itself. A mutated fence that names
   `ChangedUnderPass` without re-reading fails on the second check.
3. **D3's doc comment states the Completing arm's limit** (`:5873-5876`), from the v2 adversary's
   second point: only the Open arm's stale-preimage conflict is decided by the session-bytes
   precondition alone. The root flip also writes the `retire:records:` key the fence
   `require_absent`s (`restore.rs:825-827`), so the Completing arm's fence would lose on that key
   even without the session precondition. No assertion changed for this; it is a reader warning.

I did not take the alternative of keeping the three-way match and adding a separate negative
check ("never `ObligationKeyTaken` naming the flip's key"). That is a weaker version of the same
pin: it still lets `LostConflict` through, which is also wrong in every writer-won schedule here.

Is `ChangedUnderPass` right in **every** writer-won schedule, including the lock-conflict landing
(`stale: false`, writer at 6.5 ms, where the writer still holds its prewrite lock when the fence
prewrites)? Yes, measured, not assumed: D3 walks all 15 landings per arm deterministically, and
the seeded leg only draws from those same 15 landings. Both pass on the unmodified fence with the
exact check plus the re-read check, so the re-read saw the writer's state on every landing.

## What the patch adds overall (line numbers on the patched file)

Insertion points on the base (`0b48ab7`): imports at base `:71-72`; the property-18 section after
`prop_staged_replace_reaches_every_point_of_the_fence` (ends base `:4994`); registrations after
base `:5160`; the regression-seed calls after base `:5199`.

- Imports: `decode_retire_obligation`, `PartScope`, `RetireMode`, `RetireToken`, `MPU_PREFIX`
  (`:71-75`).
- **Property 18**, `:4996-5918`:
  - `FenceMeta` (`:5099-5230`): a recording tap over `SimTikvMetadataStore`. Every call is
    forwarded unchanged, hops included. It logs each `mpu:` listing's value of the session
    (`SessionRead`), each point read of the session (`SessionGot`, new), and each answer to a
    commit that writes the session (`FenceAnswered`). The concurrent writer commits through
    `writer_commit`, which logs `WriterAnswered`. With `strike: Some(fate)`, the pass's first
    commit writing the session is answered `CommitUnknownResult` out of flight and applied whole
    or not at all, as `AmbiguousSweepMeta` does (base `:3975`).
  - `seed_restored_session` (`:5274`): seeds the session, one committed part (chunk on disk on D
    server 1), the attempt's `seg:` record (Completing arm), and a committed object whose only
    fragment is gone (the dangling chunk). It writes straight into the model, so the fixture is
    neither logged nor struck. Writer batches follow the brief: Complete fence `Open@3 →
    Completing@4` (attempts 1→2, nonce unchanged, no obligation); root flip `Completing@3 →
    Completed@4` writing session + segmented inode + dirent + `retire:records:s:<id>:3
    {"parts":[[1,1]]}`, all under `require` / `require_absent`.
  - `assert_owed` (`:5440`): the WHOLE `retire:` namespace must equal the expected key set, and
    each record is checked by payload through the production `decode_retire_obligation`.
  - `restore_fence_under_a_concurrent_writer` (`:5518`): one run of the production
    `reconcile_after_restore` with the writer landing at `delay_micros`. It judges from the pass's
    own read: the last `mpu:` listing in the pass (the fence's, `restore.rs:789`). Core assertion
    (`:5585-5591`): exactly one transition out of `@E`. Then per-regime state, obligations, the
    report's naming and cause, and publication checks.
  - D1 seeded leg `prop_restore_fence_never_shares_the_epoch` (`:5748`); D2
    `restore_fence_after_an_ambiguous_commit` + `prop_restore_fence_settles_an_ambiguous_commit`
    (`:5760`, `:5857`); D3 `prop_restore_fence_reaches_the_contested_window` (`:5877`).
- Registrations with `dst_campaign_test!` (`:6069-6085`), and the two seeded legs appended to
  `committed_regression_seeds_stay_green` (`:6126-6127`). Appended last, so they draw from the
  shared RNG after every existing property: no existing property's draws change.

No existing constant moved. Two new ones: `RESTORE_FENCE_SPAN = 14` (`:5046`) and
`RESTORE_FENCE_DRAWS = 4` (`:5051`). No existing property's assertions changed.

## Key design decisions (unchanged from v2)

**Store: `SimTikvMetadataStore` behind a tap, never `MemMeta`.** `MemMeta` never yields, so no
writer could land between the pass's read and its commit.

**Which read is "the pass's read".** The pass lists `mpu:` twice: the staged-class read and the
fence's own listing (`restore.rs:789`). Nothing after the fence listing lists `mpu:`, so the
fence's read is the last `SessionRead`. I did not use the fence commit's `require` value as the
record of the read: mutation (a) removes that precondition, and the oracle has to keep working
under it.

**Landing granularity: whole milliseconds plus 500 µs**, as property 17 does (`REPLACE_SPAN`,
base `:4479-4486`). Every step of the pass is on an integer-ms hop, so a half-ms writer never ties
with one, and the scheduler's tie-breaking never decides the regime. Regime per landing (ms), the
same in both arms:

| landing | 0–4 | 5 | 6 | 7–14 |
|---|---|---|---|---|
| regime | WriterFirst | WriterWon{stale:true} | WriterWon{stale:false} (lock) | FenceWon |

**Why 4 draws per arm per seed (`RESTORE_FENCE_DRAWS`).** The stale window is 1 landing of 15.
With one draw per seed, a 50-seed run misses it about 3% of the time ((14/15)^50); with 4,
(14/15)^200 ≈ 1e-6. D3 walks every landing deterministically, so the coverage claim never rests
on the draws.

**Ruled out:** a reply hop on every read as `HandoffMeta` does (lengthens every read equally, so
the window's share of the span stays ~1/reads, for more tap code); expected values minted by
production code (expected session bytes and `retire:` keys are spelled out by hand so the oracle
does not share code with what it judges; payloads go through the production decoder because the
brief asks for that); a new test file (forbidden by the brief's Verification posture).

## D4 — demonstrated falsifiability, re-run this iteration on the updated test

Runner for the mutation runs: `RUSTFLAGS="--cfg madsim" MADSIM_TEST_NUM=50
MADSIM_TEST_SEED=843000 timeout 1500 cargo test -p wyrd-dst --test custodian restore_fence` from
the worktree. That is the same build and seed count `cargo xtask dst` uses (`xtask/src/main.rs:1721-1758`),
filtered to the new properties, with a timeout. Every mutation was applied alone, run, then
reverted with `git checkout -- crates/custodian/src/restore.rs`. Panic line numbers below are on
the patched test file as shipped.

### (a) Drop the fence's precondition on the session bytes

Mutation at `crates/custodian/src/restore.rs:824`:
`WriteBatch::new().require(key.to_vec(), read.to_vec())` → `WriteBatch::new()`.

- D1 seeded: **FAILED**, first at **`MADSIM_TEST_SEED=843008`**.
- D3: **FAILED** (deterministic; Open arm, writer at 5.5 ms).
- D2: passed (no concurrent writer, so not what (a) breaks).
- Completing arm alone does not catch (a): the flip installs `retire:records:s:<id>:3`, which the
  fence's `require_absent` still refuses. The Open arm is where (a) is visible (now stated in D3's
  doc comment).

```
thread '<unnamed>' panicked at crates/dst/tests/custodian.rs:5585:5:
assertion `left == right` failed: the fence (read {"parent":0,"object":"fenced","created_at_millis":100,"clock_source":"wall","segment_nonce":"84308430843084308430843084308430","epoch":3,"attempts":1,"state":{"kind":"Open"}}, answered Some((4, Committed))) and the writer (Committed) — exactly one transition out of @3 may land — Open arm, writer at 5500 µs: [Pass, SessionRead(Some(..."Open"...)), SessionRead(Some(..."Open"...)), WriterAnswered(Committed), FenceAnswered(Committed)]
  left: 2
 right: 1
note: run with `MADSIM_TEST_SEED=843008` environment variable to reproduce this error
```

### (b) Split the transition and its obligations into two commits

Mutation at `crates/custodian/src/restore.rs:824-832`: commit 1 = `require(session bytes)` + put
session; only if commit 1 is `Committed`, commit 2 = `require_absent(obligation keys)` + put
obligations; the existing `match` over the combined outcome.

- D2 seeded: **FAILED** at **`MADSIM_TEST_SEED=843000`** (every arm drawn with fate `Landed`).
- D1 and D3: passed, as expected. Once the session commit lands the session is no longer `@E`, so
  no writer's precondition can hold between the two commits.

```
thread '<unnamed>' panicked at crates/dst/tests/custodian.rs:5455:5:
assertion `left == right` failed: the `retire:` namespace holds an obligation without its transition, or misses one — Open arm, Landed: [Pass, SessionRead(Some(..."Open"...)), SessionRead(Some(..."Open"...)), FenceAnswered(Unknown(Landed))]
  left: []
 right: ["retire:bytes:s:84384384384384384384384384384384:3"]
note: run with `MADSIM_TEST_SEED=843000` environment variable to reproduce this error
```

The struck commit landed the session as `Aborting@4` with no obligation. The re-run would not
repair it: `plan_fence` maps `Aborting` to `Plan::Fenced` and `recheck_fenced` only reads.

### (c) New this iteration: diagnose a lost commit by obligation keys before the session

The carry-forward's concrete failing case. Mutation at `crates/custodian/src/restore.rs:841-856`:
look up each obligation key first and report `ObligationKeyTaken` for the first one present; only
if none is present, re-read the session (`ChangedUnderPass` if it changed, else `LostConflict`).

- With the **updated** test: D1 seeded **FAILED**, first at **`MADSIM_TEST_SEED=843009`**; D3
  **FAILED** (deterministic; Completing arm, writer at 5.5 ms). D2 passed (no writer).

```
thread '<unnamed>' panicked at crates/dst/tests/custodian.rs:5671:13:
the fence's commit lost to the writer, so the pass names the session it could not fence, as changed under it: RestoreReport { ..., sessions_fenced: 0, sessions_unsettled: [UnsettledSession { session: "mpu:84384384384384384384384384384384", cause: ObligationKeyTaken { key: "retire:records:s:84384384384384384384384384384384:3" } }], segments_unaccounted: [] } — Completing arm, writer at 6500 µs: [...]
note: run with `MADSIM_TEST_SEED=843009` environment variable to reproduce this error
```

- With the **v2** oracle put back temporarily (three-way match, no re-read check), same mutation,
  same seeds: **all three tests passed** (`3 passed; 0 failed`). So the narrowed check is what
  catches (c). The test file was then restored from a saved copy.

After the three mutations: `git status` shows only `crates/dst/tests/custodian.rs` modified, and
`patch.diff` contains no production change.

## Refuting my own test (forced)

- **(a) Genuine red?** Yes. There is no production fix to revert: the patch adds tests only, and
  the base's "red" is the properties' absence (on base no property runs `reconcile_after_restore`
  over a store holding an `mpu:` key). The red is D4: mutations (a), (b) and (c) of the production
  fence each turn at least one of the new properties red (seeds and panics above), and reverting
  them turns the properties green. For this iteration's change in particular, I "reverted" it
  the other way round: under mutation (c), the v2 oracle is green and the v3 oracle is red.
- **(b) Production path?** Yes. Every run calls `wyrd_custodian::reconcile_after_restore`, which
  runs the production `fence_open_sessions` → `fence_session` → `plan_fence`; decode checks use
  the production `decode_retire_obligation` / `decode_session_record`. The tap only forwards and
  logs. D4 is the proof: editing `restore.rs` flips the result.
- **(c) Fixture includes the fault?** Yes. The concurrent writer is a real madsim task committing
  to the same simulated-TiKV store. D3 asserts from the recorded order that the writer's commit
  applied between the fence's read and the fence's commit (the stale-preimage `Conflict`) in BOTH
  arms. In the Completing arm the flip really holds the fence's `retire:records:s:<id>:3` key,
  which is what makes mutation (c) visible. D2's nemesis really strikes the fence's commit: it
  asserts exactly one `FenceAnswered(Unknown(fate))` and that the pass's `Err` is a
  `CommitUnknownResult`. The dangling chunk is really missing from the fleet.

## Gates run

- `cargo fmt --all` then `cargo fmt --all -- --check`: clean.
- `typos crates/dst/tests/custodian.rs`: clean. The patch touches no docs (`docs-renderer` not
  exercised by this file; it runs inside `cargo xtask ci`).
- `./engine/xtask.sh dst` (the project's runner): **exit 0**, `custodian.rs` 31/31 (28 existing +
  3 new) at `MADSIM_TEST_NUM=50`, the regression-seed test included. (Run before I swapped the
  order of the two writer-won assertions; the full `ci` run below re-ran DST after it.)
- `./engine/xtask.sh ci` (criterion L), run on the final tree: **exit 0, "xtask ci: all checks
  passed"**. It ran typos, docs lint and render `--check` (link audit OK), the gitlink / unsafe /
  blackbox guards, `cargo fmt --check`, workspace clippy/build/test, cargo-machete, cargo-deny
  (three `*-not-encountered` config warnings, not failures), conformance vectors, the ADR-0035
  statics gate, the deploy guard, then `wyrd-dst` clippy and tests under `--cfg madsim` at 50
  seeds: `custodian.rs` 31/31, every existing property passing unchanged.
- The three new properties on the unmodified fence, 50 seeds each, seed ranges starting at 1,
  1000, 5000 and 843000: all `3 passed; 0 failed`.

## Out of scope, for the human

- `crates/custodian/src/restore.rs:785` carries `// deferred: #843 — seeded Tier-0 DST coverage
  of this fence (809.5).` Once this lands that marker is stale, but the brief limits this child to
  the test file. Already deferred to sign-off in round 1; not touched here.
- No real defect in the fence turned up: on the unmodified fence the three new properties pass on
  every seed range I ran (see Gates run).
