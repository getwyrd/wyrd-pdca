# Build notes — issue 1015 (625.1), iteration 8: the multipart reaper, `Open`-session half

Withheld from the reviewer. For the human at sign-off.

## Summary

I started from iteration 7's patch (`iteration-v7/patch.diff`; it applies cleanly to the same
base) and made the two fixes the iteration-7 sign-off asked for, and nothing else. Each has a test
that fails on iteration 7's code and negations that turn it red again. No rebase, no change to the
reaper, the drain, the DST rows or `review-rejected.md`.

| Item | Status |
|---|---|
| F1 — a zero-byte `meta.redb` was initialized and reported "satisfied" | Fixed. New `RedbMetadataStore::open_existing` over redb's own non-creating `Database::open`. The one-shot uses it, with no look at the path first, so the check-then-open race is gone too. A missing file, an empty file, and a redb database that never got the metadata table are each refused by name, and nothing is created. |
| F2 — the outcome line named only `tikv` / `fdb` | Fixed. The `[store: …]` tail now carries the PD endpoints (TiKV) or the cluster file (FoundationDB: `WYRD_FDB_CLUSTER_FILE`, or the package default), resolved with what the open itself uses. |
| One case the sign-off did not name: a redb file with no metadata table | Refused as well. **My call — see "Decisions".** |
| A mutation survivor my first version of F2 caused | Removed by undoing a one-word edit to `open_tikv_meta`. **A trade-off — see "Decisions".** |

This round's own change over iteration 7 is 6 files, +488 / −33 lines: `metadata-redb/src/lib.rs`
+253 / −1 (about 85 lines of production code and comments, the rest its tests), `cli.rs`
+171 / −31, `multipart_reaper.rs` +59, `metadata-redb/Cargo.toml` +3, `Cargo.lock` +1, and one
sentence in `06-runtime-view.md`. The whole `patch.diff` is 48 files, 505 KB, +8,537 / −230
(46 files and 483 KB last round).

## Base and prerequisites

- **Base commit:** `3c600b6` (`pdca-integrate: issue_1013`), branch
  `pdca-integration/main-r9dd123c9ed70`, the stack base in `results/issue_1015/stack-base-tip`.
  Every `path:line` below is on the patched tree (base + `patch.diff`).
- **Prerequisites on the base** (`git log 3c600b6`, the `pdca-integrate` commits): `issue_1027`
  (49463ec), `issue_1007` (c9e236e), `issue_1008` (5a28b33), `issue_1009` (6eaf793), `issue_1010`
  (0de399f), `issue_1011` (c27cdb0), `issue_1061` (21467e6), `issue_1062` (b452551),
  `issue_1013` (3c600b6). Unchanged since last round; the worktree was clean at `3c600b6` when I
  started.
- **#659's drain is dispatched from `reconcile_step` on the base:** `retirement::reconcile(ctx)`
  at `3c600b6:crates/custodian/src/reconciliation.rs:168`.
- **#806 has not landed** (its deferral marker is still in `crates/custodian/src/gc.rs` on the
  base), so the profile comparison stays one function, `check_admission_profile`
  (`crates/custodian/src/reaper.rs:380`), with the doc comment naming #806.
- **No rebase**, per the sign-off: the patch stacks on unmerged prerequisites and cannot apply on
  `origin/main` (`57aefe0`). Diff coverage stays unmeasured until they land (accepted).

## What I read

`brief.md`. The carry-forward cites line numbers in the previous attempt and says to fix two
things in it, so I applied `iteration-v7/patch.diff` as the starting tree and read my own previous
`build-notes.md` (for the commands, the negation driver and the standing sections below). I read
the one section of the target's `AGENTS.md` I was given (the review rubric), and this project's
`pdca.toml`, `docs/INTEGRATION.md` and `engine/` scripts for the runner and the gate commands. I did **not** read
`review-batch.md` or any check or review file this round: the sign-off names the two fixes and
says not to widen scope. I left `review-rejected.md` untouched ("recorded rejections … stand");
the files its rows cite (`reaper.rs`, `reconciliation.rs`, `custodian.rs`,
`dst/tests/custodian.rs`, `06-runtime-view.md:94`) keep their line numbers this round.

For F1 I read redb 4.4.0's source in the cargo registry to learn exactly what its non-creating
open refuses (`src/db.rs:2125-2137`, `src/tree_store/page_store/page_manager.rs:1058-1086`).

## F1 — a store file that is there and empty

- **What was wrong.** Iteration 7's `open_existing_local_meta_redb` asked `path.try_exists()` and
  then called `RedbMetadataStore::open`, which is `redb::Database::create`. redb's `create`
  initializes a new database in an empty file. So `touch <data-dir>/meta.redb` followed by the
  one-shot gave a fresh store, "satisfied — nothing to reap or drain", exit 0. The same two-step
  shape also left a gap between the check and the open.
- **The fix, store side** (`crates/metadata-redb/src/lib.rs`):
  - `RedbMetadataStore::open_existing` (`:135`) calls `Database::open` (`:137`). redb refuses a
    missing file (the OS's not-found) and a zero-length file ("Database file is empty and creating
    a new database was not requested") on the one handle it opens, so there is no separate check
    for anything to slip past.
  - It then looks for the metadata table in a **read** transaction (`:144`) and refuses if the
    table is not there. It does not call `from_db`, which would create the table.
  - A typed error, `NoExistingStore` (`:73`): `Missing`, `Empty`, `Uninitialized`. Anything else
    — a lock another process holds, an I/O fault, a file that is not a redb database — comes back
    as redb's own error, unchanged (`behind`, `:86`).
  - redb reports an empty file and a foreign file with the same error kind. To tell the caller
    which, `behind` reads the file's length after redb has already refused (`:95-100`). That read
    only picks the wording. The refusal does not depend on it.
- **The fix, CLI side** (`crates/server/src/cli.rs`): `open_existing_local_meta_redb` (`:2159`)
  is now one call to `open_existing` (`:2161`). A `NoExistingStore` becomes the operator message
  "no metadata store at `<path>`: `<which>`. The one-shot … creates none … Check --data-dir."
  Any other error is passed through as it is (`:2173`), the way every other command reports a
  store it cannot open. Other commands keep `open_local_meta_redb`.
- **Tests:**
  - `an_empty_store_file_is_refused_and_left_empty`
    (`crates/server/tests/multipart_reaper.rs:1205`, new, subprocess): a data dir whose
    `meta.redb` is a zero-length file. The binary exits non-zero, names the path and "the file is
    empty", reports no pass, leaves the file at 0 bytes and puts nothing beside it. Then a
    writer's open makes a store of that same file and the one-shot runs on it and names it — so
    the refusal was the empty file's, not the directory's.
  - `the_reaper_one_shot_opens_only_a_store_that_exists` (`cli.rs:3248`, extended as the
    sign-off asked): the zero-length file is refused with its path and "the file is empty", and is
    still 0 bytes. It also now holds a store open and asserts the second open's error is **not**
    worded as a missing store.
  - Seven unit tests in `metadata-redb/src/lib.rs` (`open_existing_tests`, `:443`), each on a
    real file: missing (and a missing directory) → `Missing`, nothing created; empty → `Empty`,
    still 0 bytes, and `open` then initializes that very file; a database with no table →
    `Uninitialized`, and the table is still absent afterwards; a foreign file → redb's error,
    bytes untouched; a store held open → redb's `DatabaseAlreadyOpen`, not an absence; an
    existing store → opened, read, written through; the three messages.
  - The crate gets `tempfile` as a dev-dependency for these (it is already a workspace
    dependency; `Cargo.lock` gains the one edge).
- **Red on iteration 7's code:** I put iteration 7's `cli.rs` and the base's
  `metadata-redb/src/lib.rs` back under the final test file and ran it: **it compiled; 13 ran,
  1 failed, 12 passed.** The failure is the new leg, at its first assertion, with iteration 7
  printing `multipart reaper pass satisfied — nothing to reap or drain [store: redb at …]` over
  the empty file. Log: `v8-red-on-v7.log`. I restored both files and checked their checksums.
  Negation 63 is the same behaviour (exists-check, then a creating open) applied to the final
  code: the subprocess leg and the cli unit test both go red.

## F2 — which TiKV or FoundationDB store

- **What was wrong.** `reaper_store_name` returned `"tikv"` or `"fdb"`. Those backends take
  their address from the environment, so "nothing to reap [store: tikv]" read the same whichever
  cluster the variable pointed at.
- **The fix** (`cli.rs`):
  - `reaper_store_name` (`:1725`): the TiKV arm (`:1730`) passes `WYRD_TIKV_PD_ENDPOINTS` to
    `tikv_store_name` (`:1751`), which parses it with the same `parse_endpoints` the open uses
    and prints `tikv at PD <a>,<b>`. The FoundationDB arm (`:1733`) calls the driver's own
    `wyrd_metadata_fdb::config::cluster_file` on the driver's own `CLUSTER_FILE_ENV` — the exact
    expression `FdbMetadataStore::connect()` uses (`crates/metadata-fdb/src/lib.rs:1228-1230`) —
    and `fdb_store_name` (`:1762`) prints `fdb at cluster file <path>`. So an unset variable
    names the package default, `/etc/foundationdb/fdb.cluster`.
  - The naming code spells the TiKV variable once, `TIKV_PD_ENDPOINTS_ENV` (`:1742`).
    `open_tikv_meta` (`:301`) is **not** touched: it keeps its own literal, and its text is
    byte-identical to iteration 7's. See "Decisions" for why, and for what that costs.
  - The redb name is unchanged.
- **How "resolved" is carried.** The name is worked out from the same variable with the same
  resolver as the open, not handed over from the open. For FoundationDB there is no other way
  short of changing the driver: `connect()` resolves the cluster file inside itself and exposes
  no accessor. Nothing in the server, custodian, telemetry or either driver sets an environment
  variable (`grep set_var|remove_var`: no hits), so the two reads agree. See "Decisions" for the
  alternative.
- **Tests:**
  - `the_reaper_one_shot_names_which_networked_store_it_ran_on` (`cli.rs:3300`, runs in the
    default build): two TiKV configurations give two names; two spellings of one list give one
    name; no endpoint is said to be no endpoint, never printed as a cluster; two cluster files
    give two names. This is the test the sign-off asked for.
  - Two tests of the arms themselves, compiled only with their backends:
    `the_tikv_arm_names_the_pd_endpoints_this_process_would_connect_to` (`:3334`) and
    `the_fdb_arm_names_the_cluster_file_this_process_would_connect_with` (`:3353`). Each works
    out the expected name from the variable's literal name and the environment the test process
    was started with, then calls `reaper_store_name`. The environment is set from outside the
    process, never by the test. Runs, all green (`v8-gated-arms.log`, and the negation driver's
    entry `n00`):
    - TiKV: variable unset; blank; set to `http://pd-x:2379, http://pd-y:2379`.
    - FoundationDB: variable unset (so the package default, `/etc/foundationdb/fdb.cluster`);
      blank (the default again); set to `/srv/elsewhere/fdb.cluster`; and as the harness sets it
      (see the note below).

    **These two do not run in `cargo xtask ci`** — the default gate does not build the `tikv` or
    `fdb` feature — so their evidence is my runs here, not the gate's.

    A note on the environment: this session came with `WYRD_FDB_CLUSTER_FILE` already set, to
    `…/wyrd/target/fdb-single-node/fdb.cluster` (the conformance fixture's path; no file is there
    and no cluster is up). So a plain run here is a "set" run, and the "unset" and "blank" runs
    above remove or blank the variable explicitly (`env -u …`, `env VAR=' '`).
- **The gated code compiles.** Both of the project's own feature rows pass on the final tree:
  `cargo clippy -p wyrd-server --features fdb,etcd --tests` and `… --features tikv,etcd --tests`
  (exit 0, no warnings; `v8-clippy-fdb-etcd.log`, `v8-clippy-tikv-etcd.log`). The host has both
  toolchains (libfdb_c and headers, cmake, openssl, protoc).
- **Red on iteration 7's behaviour:** the default-build test names two functions iteration 7
  does not have, so it cannot compile against that file. Negations 68 and 69 put iteration 7's
  names (`tikv`, `fdb`) back under the new function names: the test goes red. Negations 70 and 72
  do the same in the gated arms: their tests go red.
- **Not exercised end to end:** I did not run the one-shot against a live TiKV or FoundationDB
  cluster, so nobody has yet seen the real line for those backends. The line itself is the same
  `eprintln!` the redb legs read; only the name in it differs, and that is what the tests above
  cover. See item 2 under "Items for the human".

## Tests and evidence

### Binding red→green: `crates/server/tests/multipart_reaper.rs` (new, not cfg-gated)

Thirteen plain `#[test]`s (twelve before; one new leg this round). Each one-shot leg seeds a redb
store with canonical raw records, drops it, runs the built `wyrd` binary with
`--reap-abandoned-uploads` last in argv, bounded at 120 s, and asserts on the reopened store.
Convergence bound `CONVERGE_RUNS = 6` (`:68`).

- **RED on the base** (the tree with the whole patch reverse-applied and only this file kept):
  **it compiled; 13 ran, 13 failed, 0 passed.** The one-shot printed
  ``wyrd: flag `--reap-abandoned-uploads` needs a value`` 20 times. Log:
  `$PDCA_SCRATCH/pdca-builder-1015-logs/v8-red.log`. Afterwards I moved the file out, re-applied
  the patch and checked the tree's diff checksum was the one from before (`624fc021…`).
  - A, C: "the store did not converge within 6 one-shots" (`:186`).
  - E-open: the foreign `Aborting` session not torn down (`:769`).
  - H (mismatch) and H (lost ledger): the signal assertions (`:838`, `:885`).
  - Held-session leg: the idle session not torn down (`:932`).
  - No-fleet leg: the hold is not named on stderr (`:1000`).
  - Budget leg: the first one-shot failed (`:1047`). Usage leg: no flag in the synopsis (`:1074`).
  - Refused-flag leg, data-dir leg and the new empty-file leg: the refusal message is missing
    (`:1127`, `:1168`, `:1212`).
  - B: its exit-status assertion (`:648`). **B's store assertions are vacuous on the base**
    (nothing reaps there); its teeth are negations 1 and 60.
- **RED on iteration 7's production code** (this round's "fails today"): **it compiled; 13 ran,
  1 failed** (`v8-red-on-v7.log`), as described under F1.
- **GREEN:** 13 ran, 13 passed on the patched tree, alone (`v8-green-2.log`) and inside the gate
  run.

I ran these with `cargo test` under `timeout`, as in earlier rounds, rather than
`engine/scripts/run-verify.sh`: that script makes its own `../wyrd-verify` worktree outside the
roots I was given. Check's C4-verify row runs it.

### Negations

Driver: `$PDCA_SCRATCH/pdca-builder-1015-scripts/neg8.py`. It applies one break, runs the legs
that must catch it under `timeout 2400`, records how many tests ran and failed and whether the
build compiled, and restores the file. It loads last round's driver for the earlier breaks. Logs:
`neg8-<name>.log`; printed results `neg8-final.out`; summary `neg8-final-summary.json`.

**What I ran this round, on the final tree:** this round's 17 breaks, the brief's seven
(1, 2, 3, 6, 10, 14, 15), and every earlier break that edits `cli.rs` (the one file of last
round's that changed). 33 breaks, 58 legs. **Every leg compiled and ran. 57 went red; 1 stayed
green, and that one is expected** (negation 63's third leg, below). The checksums of the three
files the driver edits were the same after the run as before.

**What I did not re-run:** the other 41 breaks from earlier rounds. They edit `reaper.rs`,
`reconciliation.rs`, `retirement.rs`, `sessions.rs`, `abort.rs` and the server's `custodian.rs`,
and their tests live in `reaper/tests.rs`, `custodian/tests/gc.rs`, `custodian_gc.rs` and
`dst/tests/custodian.rs`. All of those are byte-identical to iteration 7's (I compared the blob
ids of the two trees), where every one went red (iteration 7's notes, `neg7-final.out`). The
brief's seven I re-ran anyway because the brief names them.

| # | Broken variant | Leg that catches it | Result |
|---|---|---|---|
| 1 | progress from `created_at` + `psum:` only, slot stamp and lease ignored | B (server); pre-first-chunk (DST) | both **red** |
| 2 | `psum:` read before `slot:` | X60 (DST) | **red** |
| 3 | no slot pins on the idle fence | X62 (DST) | **red** |
| 6 | the drain skips a foreign-clocked session wholesale | E-open `Aborting` half (server) | **red** |
| 10 | idle arm off, ceiling kept | A; held-session leg (server) | both **red** |
| 14 | no profile comparison | H (server) | **red** |
| 15 | no reaper dispatched from `reconcile_step` | `gc.rs` GC case (at its convergence assertion, see below); deployed-step test | both **red** |
| 34 | usage synopsis without the flag | usage leg (server) | **red** |
| 55 | no flag is refused | refused-flag leg (server); cli unit test | both **red** |
| 56a–d | one of the four flags is not refused (one break per flag) | refused-flag leg (server); cli unit test | all 8 **red** |
| 57 | the one-shot opens with the writer's creating open | data-dir leg; empty-file leg (server) | both **red** |
| 59 | the outcome line names the wrong path | B (server); cli unit test | both **red** |
| 60 | the one-shot prints "satisfied" without running a pass | B (server) | **red** |
| 63 | **(this round)** iteration 7's open: an exists-check, then a creating open | empty-file leg (server); cli unit test; data-dir leg (server) | **red**, **red**, **green (expected)** |
| 64 | **(this round)** `open_existing` uses redb's creating open | redb missing-file and empty-file tests; data-dir leg and empty-file leg (server); cli unit test | all 5 **red** |
| 65 | **(this round)** `open_existing` creates the missing table | redb no-table test | **red** |
| 66a | **(this round)** an empty file is not told apart from a foreign one | redb empty-file test; cli unit test; empty-file leg (server) | all **red** |
| 66b | **(this round)** a missing file is not told apart | redb missing-file test; cli unit test; data-dir leg (server) | all **red** |
| 66c | **(this round)** any invalid file is called empty | redb foreign-file test | **red** |
| 66d | **(this round)** any I/O refusal is called missing | redb empty-file test; redb foreign-file test | both **red** |
| 66e | **(this round)** a store held open is called missing | redb held-store test; cli unit test | both **red** |
| 67 | **(this round)** the CLI words every failed open as "no metadata store" | cli unit test | **red** |
| 67b | **(this round)** the refusal does not say which absence | cli unit test; empty-file leg (server) | both **red** |
| 68 | **(this round)** the TiKV name is `tikv` alone, as in iteration 7 | cli names test | **red** |
| 68b | **(this round)** the TiKV name is the raw value, not the parsed list | cli names test | **red** |
| 69 | **(this round)** the FoundationDB name is `fdb` alone, as in iteration 7 | cli names test | **red** |
| 70 | **(this round, `tikv` build)** the TiKV arm returns `tikv` | TiKV-arm test, variable unset and set | both **red** |
| 71 | **(this round, `tikv` build)** the TiKV arm reads another variable | TiKV-arm test, variable set | **red** |
| 72 | **(this round, `fdb` build)** the FoundationDB arm returns `fdb` | FoundationDB-arm test, variable unset and set | both **red** |
| 73 | **(this round, `fdb` build)** the FoundationDB arm ignores the variable | FoundationDB-arm test, variable set | **red** |

(Last round's negation 58, "a missing store is not refused", edited the `try_exists` line that
is gone; 66b replaces it.)

**The one green leg.** Negation 63 is iteration 7's open. Its third leg is the missing-data-dir
leg, which iteration 7 already passed, so it stays green — it shows that leg cannot tell
iteration 7 from this round, which is why the empty-file leg exists.

**Negations 71 and 73 need the variable set.** With the variable unset, the right variable and
a wrong one both read as unset, so those two breaks can only be seen with it set. I ran those
legs with it set. In a run with a bare environment the two gated tests would not catch a wrong
variable name. For FoundationDB the arm uses the driver's own constant, so there is no second
spelling to get wrong. For TiKV the arm's constant and the open's literal are two spellings
("Decisions" says why), so there a rename of one is only caught with the variable set.

Negations (2) and (3) are DST-only, as the brief says; the integration test does not claim them.

**Negation 15 and the GC case** (unchanged, restated because the brief asks): with no reaper
dispatched, the GC case fails at its convergence assertion ("the session is torn down within four
passes"), the first assertion that depends on the reaper. It does not get as far as the GC
assertions. So "finds no mark and deletes nothing at mark + grace" is true of the store, but it
is not the assertion that fires.

### Unit, integration and DST tests

- `crates/metadata-redb/src/lib.rs`: 7 new unit tests.
- `crates/server/src/cli.rs`: 1 changed and 1 new unit test in the default build; 2 more compiled
  only with `tikv` / `fdb`.
- `crates/server/tests/multipart_reaper.rs`: 13 tests (1 new).
- Unchanged this round: `crates/custodian/src/reaper/tests.rs` (40 tests),
  `crates/custodian/tests/gc.rs`, `crates/server/tests/custodian_gc.rs`, and the DST rows in the
  existing `crates/dst/tests/custodian.rs` (X60, X62, X19, read fault, pre-first-chunk, Complete
  race, unknown fence outcome, and the slot-cycle row, ignored as decided). No file added under
  `crates/dst/tests/`.
- **No new DST row this round.** Neither fix touches a destructive or concurrent path: F1 is a
  refusal before anything is opened for writing, F2 is a string.

### Gate

`./engine/xtask.sh ci` (the project runner, `PDCA_WORKTREE` = this worktree), bounded with
`timeout 5400`, on the final tree: **exit 0, "xtask ci: all checks passed"**. The tree's diff
checksum after the run is the checksum of `patch.diff` (`624fc021…`), so the gate tested exactly
the patch in the bundle. Every stage ran: `typos`, `lint_docs.py`, `render_site.py --check` (the
two external dependencies the brief names), the gitlink, unsafe and blackbox guards,
`cargo fmt --check`, workspace clippy, build and test, `cargo-machete`, `cargo deny` (including
the all-features advisories check), the conformance, statics and deploy guards, and
`cargo clippy` / `cargo test -p wyrd-dst` under `--cfg madsim`. **2,605 tests passed, 0 failed,
16 ignored** across 234 test binaries (2,596 last round, plus this round's 9: seven redb unit
tests, one cli unit test, one subprocess leg). Log:
`$PDCA_SCRATCH/pdca-builder-1015-logs/ci-v14.log`.

- The two feature rows the default gate skips, run by hand on the final tree: `cargo clippy -p
  wyrd-server --features fdb,etcd --tests` and `… --features tikv,etcd --tests`, both exit 0.
- `git apply --check --whitespace=error-all` of `patch.diff` against `3c600b6` (temporary index,
  worktree untouched): passes.
- `gitleaks stdin` over `patch.diff`: no leaks.
- `cargo fmt --all -- --check`: clean. `typos` over the files this round touched: clean (and
  inside the gate).
- The bundle copy of the test, `results/issue_1015/crates/server/tests/multipart_reaper.rs`, is
  byte-identical to the patched file (`f67d8498…`).

**Mutation check** (the C5 gate's own command, `cargo mutants --in-diff patch.diff --no-shuffle
--cap-lints=true`, bounded with `timeout 5400`, output kept out of the worktree), on the final
tree against the `patch.diff` in the bundle: **233 mutants tested in 20 minutes: 207 caught,
0 missed, 0 timed out, 26 unviable** (`mutants-v8.log`; lists in
`$PDCA_SCRATCH/pdca-builder-1015-mutants-v8/mutants.out/`). An earlier run this round, on the
tree that shared the TiKV constant with `open_tikv_meta`, tested 234 and had one survivor, in
that function; "Decisions" says what I did about it.

## Refuting my own test

- **(a) Genuine red?** Yes. With the whole fix reverted (base tree, only the new test file
  kept), the subprocess test compiled and 13 of 13 failed, each on a store, signal or exit
  assertion after the one-shot was refused at parse. Against iteration 7's production code the
  new leg fails, with that code reporting "satisfied" over an empty file. Each of this round's
  changes has a break that turns its test red (63–73), and the brief's negations (1, 2, 3, 6, 10,
  14, 15) were re-run on this tree.
- **(b) Production path?** Yes. The subprocess legs run the built `wyrd` binary:
  `cmd_custodian` → `run_reaper_once_over_backend` → `open_existing_local_meta_redb` →
  `RedbMetadataStore::open_existing` → redb, on a real file. The unit tests call the production
  `open_existing`, `open_existing_local_meta_redb`, `tikv_store_name`, `fdb_store_name` and (in
  the feature builds) `reaper_store_name`. Nothing is mocked or copied.
- **(c) Fixture includes the fault?** Yes. F1's fixture is a real zero-length `meta.redb`, and
  the test shows it is the faulty input: iteration 7's code turns it into a store and reports
  "satisfied" (`v8-red-on-v7.log`), and in the redb unit test `open` initializes that very file
  right after `open_existing` refused it. The no-table fixture is a real redb database made by
  redb's own `create`. The held-store fixture really holds the lock. For F2 the default-build
  fixture is two different configurations per backend; the gated runs really set the variable in
  the test process's environment. What the fixtures do **not** include is a live TiKV or
  FoundationDB cluster (item 2 below).

## Production reach

Unchanged in shape; `cli.rs` line numbers refreshed.

- **Deployed loop:** `cmd_custodian` → `run_reconstruction_over_backend` →
  `CustodianService::run_reconstruction_until`. Each interval the reconstruction pass calls
  `reconcile_pass(…, Some(&ctx), None, Some(&reaper), …)` (`crates/server/src/custodian.rs:680`)
  → `reconcile_step` (`crates/custodian/src/reconciliation.rs:140`) → reconstruction → reaper →
  retirement leg (`:168-195`). A healthy fleet therefore reaps every interval. The loop does not
  special-case `Blocked`; only the one-shot maps it to an exit code. **The deployed loop is not
  touched this round**: it still opens the store with the creating `open_local_meta_redb`, as a
  node should.
- **One-shot:** `wyrd custodian --reap-abandoned-uploads` (`crates/server/src/cli.rs:1011`, in
  `VALUELESS_FLAGS`, `:3035`) → the flag refusal (`:1023`) → `run_reaper_once_over_backend`
  (`:1773`) → `open_existing_local_meta_redb` (`:2159`) → `RedbMetadataStore::open_existing`
  (`crates/metadata-redb/src/lib.rs:135`) → `reap_abandoned_uploads_once` (`custodian.rs:463`)
  → `reconcile_pass` → `reconcile_step` → reaper → `retirement::reconcile_record_only`
  (`reconciliation.rs:201`). The outcome line (`cli.rs:1124`) ends with `reaper_store_name`
  (`:1134`). Exits non-zero on `Blocked` (`:1138`).
- No test-only entry point.

## Operation capacity (the brief's leg-H note)

Unchanged. Equal stored profiles don't prove equal operation budgets: `B_ops` is
`KnobSet::batch_ops` (`crates/core/src/multipart.rs:6165`), and `KnobSet::profile` leaves it
out. No format change. Operation capacity rests on:

1. Every reaper batch is sized from `MAX_BATCH_OPS` (`multipart.rs:5949`) and `MAX_BATCH_BYTES`
   (`:5910`) and measured against them before it is sent (`within_budgets`, `reaper.rs:1134`;
   the cost, `fence_cost`, `:1115`). The reaper defines no other budget.
2. `knob_clamps_hold` (`multipart.rs:6292`) and `crates/core/tests/multipart_knobs.rs` are
   unmodified (`git diff 3c600b6 -- crates/core/tests/multipart_knobs.rs` is empty) and green in
   the gate run.
3. The `B_ops` calibration is 625.3's (#1017).

The largest fence this slice builds is 20 operations and 202,838 bytes, against 500 and
5,000,000 (`largest_fence_fits_both_batch_budgets`, `reaper/tests.rs:1679`). This round adds no
batch and no write.

## Profile: who plumbs a non-shipped one

Unchanged: `ReaperPolicy::default()` uses `Budget::shipped()` (`reaper.rs:315`), and the server
builds the policy from the default. The first production writer of `mpuctl` is the S3
`CreateMultipartUpload` verb (#668, child of #508). Whoever makes that profile configurable for
the gateway must hand the custodian the same value through `ReaperPolicy::profile`; until then a
store created under any other profile is refused, loudly. The deferred clean-lap age bound (F2
of iteration 5) becomes reachable with that same change; it stays deferred to #508 (doc at
`reaper.rs:253-266`, marker at `:790`).

## Decisions and what I ruled out

- **F1: a redb database with no metadata table is refused too.** The sign-off named the
  zero-length file. `RedbMetadataStore::open` does two creating things: redb's `create`, then
  `from_db`, which writes the table. If `open_existing` had kept `from_db`, a redb file with no
  table (what a crash between those two steps leaves) would still be made into a store on the
  spot and reported "satisfied" — the same false all-clear on a rarer input, and a likely next
  finding. Refusing it costs 8 lines (`lib.rs:143-150`) and one variant. No store a node has
  ever opened lacks the table, because `open` writes it before it returns, so this cannot refuse
  a real store. To drop it: replace those lines with `Self::from_db(db)` and delete the
  `Uninitialized` variant and its test.
- **F1: a typed error, not a wrapped string.** The alternative was to let the CLI label every
  `open_existing` failure "no metadata store at …" (about 12 lines less: no enum, no `behind`).
  I did not, because a store that a running node holds open would then be reported as missing,
  and the operator sent to look for a file that is there. Negations 66e and 67 are that
  alternative; the held-store assertions catch it.
- **F1: the length read after the refusal.** redb gives an empty file and a foreign file the
  same error kind, with different message text. The choices were to match on redb's message text
  (brittle across redb versions) or read the file's length once redb has refused. I read the
  length. It cannot change the outcome, only whether the message says "the file is empty" or
  carries redb's own text.
- **F1: the other failures are passed through, without the path.** A locked or damaged store
  prints redb's bare error, as `put`, `get`, `s3` and the deployed custodian already do. Adding
  the path there would be a behaviour change beyond F1 (2 lines, in the `None` arm at
  `cli.rs:2173`, if you want it).
- **F2: the name is resolved again, not handed over from the open.** Handing it over would mean
  the opens return what they resolved. For TiKV that is a small change (resolve once in
  `run_reaper_once_over_backend`, return the name with the outcome: about 15 lines and a changed
  `pub` signature). For FoundationDB it needs a change in `wyrd-metadata-fdb` — an accessor on
  the store, or a `connect` that takes the path — which is a different crate's API and outside
  "do not widen scope beyond F1/F2". So both backends use the one approach that works for both:
  the same variable and the same resolver as the open. If you want the stricter form for
  FoundationDB, it is a `metadata-fdb` change to plan.
- **F2: `open_tikv_meta` keeps its own spelling of the variable.** My first version shared one
  constant between the open and the name: a one-token edit inside `open_tikv_meta`
  (`std::env::var("WYRD_TIKV_PD_ENDPOINTS")` → `std::env::var(TIKV_PD_ENDPOINTS_ENV)`). The full
  gate passed on that tree (`ci-v13-shared-const.log`), but the mutation run reported **1 missed
  mutant** on it: `cli.rs:307:5: replace open_tikv_meta … with Ok(Default::default())`
  (`mutants-v8-shared-const.log`: 234 tested, 207 caught, 1 missed, 26 unviable).
  `cargo mutants --in-diff` mutates a function as soon as one changed line is inside it, and
  this function is only compiled with the `tikv` feature, so the default-build run the C5 gate
  does can never catch that mutant. I undid the one-token edit, which takes the function out of
  the diff. What that costs: the variable's name is now written in two places in `cli.rs` (the
  open's literal at `:302`, the naming constant at `:1742`). If someone renames one and not
  the other, only the `tikv`-build arm test with the variable set would notice (negation 71).
  To go back to one constant: that one token, and accept the unkillable mutant.
- **F2: the cluster file's path, not its contents.** The path is what the sign-off named. The
  file's first line (`description:id@coordinators`) would identify the cluster itself, but it
  carries the cluster's id, which I would not print to a log by default.
- **F2: an unset TiKV variable has its own text.** The name is only printed after a pass ran,
  and the open refuses an unset variable before that, so this arm is not reachable from the
  one-shot today. The function still has to return something; it says the variable names no
  endpoint rather than falling back to a bare `tikv`.
- **F2: no injected environment.** I considered passing the environment into
  `reaper_store_name` as a lookup so the gated arms could be tested with made-up values. It adds
  a parameter that is unused in the default build (needing a lint exemption) for two one-line
  arms. Instead the gated tests read the real environment and I ran them with the variable set
  from outside.
- **Docs:** one sentence in `docs/design/architecture/06-runtime-view.md:94` now says an empty
  file counts as no store and what the last line names for each backend. `08` already says "on a
  data directory that holds no metadata store", which is still exact, so I left it. The usage
  text (`cli.rs:494-496`) is unchanged and still true.

## Items for the human (NEEDS-HUMAN)

1. **A redb file with no metadata table is refused** (`NoExistingStore::Uninitialized`). The
   sign-off did not name this case; I added it. Confirm, or see the 8-line removal above.
2. **The TiKV and FoundationDB outcome lines have not been seen on a live cluster.** The naming
   functions are tested in the default build and the two arms in feature builds I ran here, but
   those feature tests are not part of `cargo xtask ci`, and no test runs the one-shot against a
   live networked backend. To see it: build with `--features fdb`, set `WYRD_FDB_CLUSTER_FILE`
   to a test cluster's file, run `wyrd custodian --metadata-backend fdb --reap-abandoned-uploads`,
   and check the last line ends `[store: fdb at cluster file <that path>]`; unset the variable
   and check it names `/etc/foundationdb/fdb.cluster`. The same with `--features tikv` and
   `WYRD_TIKV_PD_ENDPOINTS`.
3. **The redb refusal message changed shape**: it now reads "no metadata store at `<path>`:
   `<which>`. The one-shot …" (a colon and the reason where there was a full stop). Nothing in the
   repo parses it.
4. **The networked outcome lines changed shape**: `[store: tikv at PD …]` and
   `[store: fdb at cluster file …]` where there was `[store: tikv]` / `[store: fdb]`.
5. **Diff coverage** still can't be measured against bare `origin/main`; accepted at the last
   sign-offs. `PDCA_VERIFY_BASE=pdca-integration/main-r9dd123c9ed70` would measure it against the
   stack base.
6. Standing, unchanged: the slot-cycle follow-up issue is still to be filed (its tracker number
   is yours to add); the two #659 follow-ups in SUMMARY §10 are untouched; the clean-lap age
   bound stays deferred to #508; the rows in `review-rejected.md` stand as they were; last
   round's open calls (leg B's tally assertions, `Blocked` for an unknown fence outcome, the two
   loop-tuning flags staying accepted) are as you left them; negation 15 fires at the GC case's
   convergence assertion, not its GC assertions.

### External dependencies

The two the brief names, `typos` and the docs renderer, both ran inside the gate. The feature
toolchains the two gated tests need are on this host, so I ran those tests. One thing the brief
does not list was not available, and it is behind item 2:

NEEDS-HUMAN external dependency: fdb cluster healthy — no live FoundationDB (or TiKV) cluster was up, so the one-shot's `[store: …]` line for a networked backend was never observed end to end; the naming functions (default build) and both backend arms (feature builds, run by hand) are tested without one, but the arm tests are not part of `cargo xtask ci`

`fdb cluster healthy` is already a registered `[[doctor.checks]]` row in `pdca.toml` (it fails
on this host today: the cluster file `WYRD_FDB_CLUSTER_FILE` points at does not exist), so I
propose no new row. I propose none for TiKV either: its development is stood down (#443,
`xtask/src/dist.rs:44-45`). I did not bring a cluster up myself or look for one elsewhere: the
one-shot is a command that can write to the store it is pointed at, and a cluster I was not
given is not mine to point it at.

## Scratch

Under `$PDCA_SCRATCH` (`/var/tmp/pdca/wyrd-pdca-9c587031/issue_1015`):

- `pdca-builder-1015-scripts/neg8.py`: this round's negation driver (it loads `neg7.py` and
  what that loads);
- `pdca-builder-1015-logs/`: `ci-v14.log` (the gate run on the final tree), `v8-red.log` (the
  base), `v8-red-on-v7.log` (iteration 7's code), `v8-green-2.log`, `v8-redb-unit.log`,
  `v8-cli-unit.log`,
  `v8-clippy-fdb-etcd.log`, `v8-clippy-tikv-etcd.log`, `v8-gated-arms.log`, `mutants-v8.log`,
  `neg8-*.log`, `neg8-final.out`, `neg8-final-summary.json`; from the first pass of this round,
  before the one-token revert described under "Decisions": `ci-v13-shared-const.log`,
  `mutants-v8-shared-const.log`, `neg8-run1.out`, `neg8-run1-summary.json`; and earlier rounds'
  logs;
- `pdca-builder-1015-neg/`: `final-v8.diff` (identical to `patch.diff`), `v7-to-v8.diff` (this
  round's change alone), `v7-src/` and `v8-final/` (the files I swapped for the red run, and
  checksums), temporary git index files (`v7.index`, `v8*.index`);
- `pdca-builder-1015-mutants-v8/`: this round's mutation run on the final tree;
  `pdca-builder-1015-mutants-v8-shared-const/`: the earlier run with the one survivor;
- earlier rounds' directories, untouched (the `--cfg madsim` cargo target among them, about
  4 GB).

Left in place for the harness to reclaim, as in earlier rounds: everything is inside the
per-issue root the harness made, and the logs are the evidence these notes point at. I ran no
`rm`.

The worktree holds the change uncommitted on top of `3c600b6`; `patch.diff` is the diff of that
tree against `3c600b6`, new files included (built through a temporary index, so the worktree's
own index is as the harness left it). I made no commit, no stash, and pushed nothing.
