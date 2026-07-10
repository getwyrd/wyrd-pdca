# Build notes — issue 440 / server-fdb-backend-selection (iteration 2)

Base commit on the target worktree `$PDCA_WORKTREE` (`/home/eddie/wyrd/wyrd.pdca-wt`):
`779ac1d` — *"Merge pull request #492 from getwyrd/fix/438-metadata-fdb-store"*, i.e.
`origin/main` with #438 landed. Matches the brief's "Repo + branch target".

**This iteration is a rebuild of the verification record, not of the code.** Per the
carry-forward, the production diff from iteration 1 is preserved essentially verbatim; the
one code delta is a single line in the added test file (§2 below), which exists precisely to
make the C4-verify RED *assertion-driven* rather than a compile error. Everything in §3–§5
is a real, pasted terminal transcript from this session.

---

## 1. Carry-forward, item by item

| # | Sign-off finding | Disposition |
|---|---|---|
| 1 | §4 transcript was fabricated (`bytes=38` vs `bytes=17` for one file; wrong `inode`) | **§5** is the real terminal output of a script that echoes each command before running it. Nothing reconstructed. Where the numbers differ from the sign-off's own run (`inode=1`, not `inode=2`) the reason is stated. |
| 2 | C4 RED was a *compile* error, not assertion failures | **Fixed and demonstrated** (§3). RED is now `0 passed; 3 failed`, all three on message content, under the **unmodified** `run-verify.sh`. The sign-off's suggested route (workspace `check-cfg`) provably does *not* work; I ran it and show why (§2). |
| 3 | `protoc` IS installed; the `(Fdb, Etcd)` arm does compile | **Verified and recorded** (§4.3), including `cargo clean -p etcd-client` first so `etcd-client v0.14.1` really rebuilds through `protoc`. `command -v protoc` → `/usr/bin/protoc`, `libprotoc 3.21.12`. |
| 4 | A builder `NEEDS-HUMAN` never reached SUMMARY §6 | **Moot: there is no NEEDS-HUMAN this iteration.** Every external dependency the brief names (`libfdb_c`, FDB headers, Docker, `fdbcli`, and `protoc`) was checked with `command -v` / `ls` and *used*. §7 records the checks. |

---

## 2. The one code change vs iteration 1 — and why the "obvious route" fails

`crates/server/tests/fdb_backend_selection.rs:30` now carries a crate-level
`#![allow(unexpected_cfgs)]`, with the reason inline at `:18-29`.

**Why it is needed.** The workspace lint table is
`warnings = "deny"` (`Cargo.toml:195`) plus
`unexpected_cfgs = { level = "warn", check-cfg = ['cfg(madsim)'] }` (`Cargo.toml:196`);
the blanket group wins, so an undeclared cfg value is a hard error — the root manifest's own
comment says as much (*"otherwise `unexpected_cfgs` fires (now as an error)"*,
`Cargo.toml:193`). In C4-verify's RED phase the `fdb` feature is *undeclared*, because
`run-verify.sh` reverts **every modified file** (`engine/scripts/run-verify.sh:264`) —
including `crates/server/Cargo.toml`, which is where this patch declares the feature
(`crates/server/Cargo.toml:31`). rustc then rejects the test file's
`#[cfg(feature = "fdb")]` before a single assertion runs. Verbatim, reproduced this session
(fix stashed, test kept — exactly what run-verify does):

```
$ git stash push -q -- Cargo.lock crates/server/Cargo.toml crates/server/src/cli.rs
$ cargo test -p wyrd-server --test fdb_backend_selection
error: unexpected `cfg` condition value: `fdb`
  --> crates/server/tests/fdb_backend_selection.rs:44:7
   |
44 | #[cfg(feature = "fdb")]
   |       ^^^^^^^^^^^^^^^
   = note: expected values for `feature` are: `default`, `etcd`, and `tikv`
   = help: consider adding `fdb` as a feature in `Cargo.toml`
   = note: `-D unexpected-cfgs` implied by `-D warnings`
   = help: to override `-D warnings` add `#[allow(unexpected_cfgs)]`

error: could not compile `wyrd-server` (test "fdb_backend_selection") due to 2 previous errors
```

**Why the sign-off's suggested route does not work — measured, not asserted.** The
carry-forward proposes *"declaring the `fdb` feature value in the workspace `check-cfg`
list"*. I built exactly that variant (root `Cargo.toml` →
`check-cfg = ['cfg(madsim)', 'cfg(feature, values("fdb"))']`, `#![allow]` removed), assembled
it into a scratch bundle, and ran the **unmodified** `run-verify.sh` against it:

```
$ PDCA_BUNDLE=/tmp/alt-bundle ./engine/scripts/run-verify.sh
run-verify.sh: GREEN — cargo test -p wyrd-server --test fdb_backend_selection (fix applied)
test result: ok. 3 passed; 0 failed; ...
run-verify.sh: RED — cargo test -p wyrd-server --test fdb_backend_selection (production reverted, test kept)
error: unexpected `cfg` condition value: `fdb`
   = note: expected values for `feature` are: `default`, `etcd`, and `tikv`
error: unexpected `cfg` condition value: `fdb`
   = note: expected values for `feature` are: `default`, `etcd`, and `tikv`
error: could not compile `wyrd-server` (test "fdb_backend_selection") due to 2 previous errors
run-verify.sh: PASS — red without the fix, green with it.
```

Still a compile-error RED. The reason is structural: `_all_files` (`run-verify.sh:67`)
harvests every `+++ b/<path>` in the patch, and the RED loop `git checkout`s each one that
is not an added test (`:259-266`). Root `Cargo.toml` is a `+++ b/Cargo.toml` hunk, so it is
reverted with the rest. An **added** non-test file (a `build.rs` emitting
`cargo::rustc-check-cfg`, a `.cargo/config.toml`) fares no better: added non-test files are
`rm -f`'d (`:262`).

So the RED tree is exactly `origin/main` + the one added test file. **The only file the
patch controls in the RED tree is the test file itself**, and rustc offers no in-source
`--check-cfg`. `#![allow(unexpected_cfgs)]` in that file is therefore not *a* route — it is
the only one, short of dropping the `#[cfg]` gating the brief mandates
(`brief.md:27-31`). Cost of each alternative, concretely:

- *workspace `check-cfg` (1 line, root `Cargo.toml:196`)* — **does not work**, transcript
  above.
- *`build.rs` in `crates/server` (new file, ~4 lines + a `Cargo.toml` `build =` key)* —
  **does not work**: `rm -f`'d at `run-verify.sh:262`.
- *drop the cfg gating and branch at runtime on `from_config(Some("fdb"))`'s `Result`
  (~6 lines, no `allow`)* — **works mechanically, violates the brief**: `brief.md:27-31`
  says assertion 1 *MUST* be `#[cfg(not(feature = "fdb"))]`-gated with the positive
  `assert_eq!(…, MetadataBackend::Fdb)` under `#[cfg(feature = "fdb")]`, "exactly the etcd
  pattern at `backend_selection.rs:47-56`", and naming `MetadataBackend::Fdb` at all is
  impossible without a cfg gate (the variant does not exist in the default build). Choosing
  this would silently discard the mandated shape to dodge one `allow` in a test file.

**What the `allow` costs, and why it cannot hide anything.** It is scoped to this one test
crate and is *inert* with the fix applied (the feature is declared, so the lint never
fires). It cannot mask a typo'd cfg value, because each gated test appears by name in
exactly one of the two runs recorded below: `fdb_without_the_feature_names_the_build_flag`
in the default run (§3), `fdb_with_the_feature_selects_the_fdb_backend` in the
`--features fdb` run (§4.2). A misspelling would compile *both* out and show up as a
missing test name in one transcript. No production file gains an `allow`.

---

## 3. Red → green through the project's own runner (C4-verify), verbatim

`./engine/scripts/run-verify.sh` is the project's bundle-scoped runner: it applies
`patch.diff` to a clean `../wyrd-verify` worktree off `origin/main`, runs the added test
green, then reverts the production files and re-runs it. Unmodified, this session:

```
$ PDCA_BUNDLE="$PWD/results/issue_440" ./engine/scripts/run-verify.sh
run-verify.sh: GREEN — cargo test -p wyrd-server --test fdb_backend_selection (fix applied)

running 3 tests
...
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

run-verify.sh: RED — cargo test -p wyrd-server --test fdb_backend_selection (production reverted, test kept)

running 3 tests
fdb_without_the_feature_names_the_build_flag --- FAILED
unknown_backend_message_lists_fdb_as_a_known_backend --- FAILED
usage_lists_fdb_as_a_metadata_backend --- FAILED

failures:

---- fdb_without_the_feature_names_the_build_flag stdout ----

thread 'fdb_without_the_feature_names_the_build_flag' (778076) panicked at crates/server/tests/fdb_backend_selection.rs:47:5:
expected the build-hint text (mirroring the tikv hint at cli.rs), got: "unknown metadata backend `fdb` (expected `redb` or `tikv`)"
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace

---- unknown_backend_message_lists_fdb_as_a_known_backend stdout ----

thread 'unknown_backend_message_lists_fdb_as_a_known_backend' (778077) panicked at crates/server/tests/fdb_backend_selection.rs:77:5:
the unknown-backend message must mention `fdb` among the known backends, got: "unknown metadata backend `nonsense` (expected `redb` or `tikv`)"

---- usage_lists_fdb_as_a_metadata_backend stdout ----

thread 'usage_lists_fdb_as_a_metadata_backend' (778078) panicked at crates/server/tests/fdb_backend_selection.rs:96:5:
usage on stderr must list all three metadata backends, got: "usage:\n  wyrd put <file> --key <name> [--data-dir DIR] [--chunk-size N] [--durability rs(k,m)|none] [--endpoints URL,URL,…] [--metadata-backend redb|tikv]\n  wyrd get <key> [--out <file>] [--data-dir DIR] [--endpoints URL,URL,…] [--metadata-backend redb|tikv]\n  wyrd d-server [--bind ADDR] [--advertise-addr ADDR] [--data-dir DIR] [--group NAME] [--lease-ttl-secs N] [--renew-secs N] [--coordination-backend mem|etcd]\n  wyrd custodian [--zone NAME] [--data-dir DIR] [--metadata-backend redb|tikv] [--otlp-endpoint URL] [--interval-secs N] [--connect-timeout-secs N] [--endpoints URL,URL,… --ids N,N,… --failure-domains D,D,…]\n  wyrd s3 --access-key KEY --secret-key SECRET [--s3-listen ADDR] [--data-dir DIR] [--region NAME] [--endpoints URL,URL,…] [--metadata-backend redb|tikv] [--coordination-backend mem|etcd]\n  wyrd demo\n\n  --endpoints drives a local distributed cluster: fragments fan out over gRPC\n  to the listed D servers (metadata held locally). See README \"Run a local cluster\".\n\n  custodian: --endpoints wires the reconstruction plane over the D-server fleet.\n  When --endpoints is given, --ids and --failure-domains are REQUIRED and must\n  each list one entry per endpoint (matching each D-server's own --id /\n  --failure-domain); the role never fabricates identity or topology from endpoint\n  order. Omit all three to run the leader-elected role with no reconstruction plane.\n"


failures:
    fdb_without_the_feature_names_the_build_flag
    unknown_backend_message_lists_fdb_as_a_known_backend
    usage_lists_fdb_as_a_metadata_backend

test result: FAILED. 0 passed; 3 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

error: test failed, to rerun pass `-p wyrd-server --test fdb_backend_selection`
run-verify.sh: PASS — red without the fix, green with it.
```

All three RED failures are assertion failures on message content / stderr content, exactly
as `brief.md:38-43` predicts. Carry-forward item 2 is discharged.

### `cargo xtask ci` (the gating `C4-ci`), via the project wrapper

```
$ ./engine/xtask.sh ci
…
xtask ci: all checks passed
```

(exit 0). Standalone, before it: `cargo fmt --all -- --check` → clean;
`cargo clippy -p wyrd-server --all-targets` → `Finished dev profile`, no diagnostics;
`typos crates/server/tests/fdb_backend_selection.rs` → clean. No `core.hooksPath` /
`.pre-commit-config.yaml` exists in the target, so `cargo xtask ci` (fmt + clippy + deny +
typos + tests) is the target's commit gate, and it is green.

---

## 4. Feature-on evidence (brief `Verification posture` — load-bearing: no gate compiles these arms)

All three commands were run **after** `cargo clean -p …` of the crates in question, so the
transcripts show real compilation rather than a cache hit.

### 4.1 `cargo check -p wyrd-server --features fdb --tests`

```
$ cargo clean -p wyrd-server -p wyrd-coordination-etcd -p wyrd-metadata-fdb
     Removed 8403 files, 6.2GiB total
$ cargo check -p wyrd-server --features fdb --tests
    Checking wyrd-metadata-fdb v0.0.0 (/home/eddie/wyrd/wyrd.pdca-wt/crates/metadata-fdb)
    Checking wyrd-server v0.0.0 (/home/eddie/wyrd/wyrd.pdca-wt/crates/server)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.70s
```

Proves the seven non-etcd `#[cfg(feature = "fdb")]` arms plus `(Fdb, Mem)` typecheck against
the real `FdbMetadataStore`.

### 4.2 `cargo test -p wyrd-server --features fdb --test fdb_backend_selection`

```
$ cargo test -p wyrd-server --features fdb --test fdb_backend_selection
   Compiling wyrd-metadata-fdb v0.0.0 (/home/eddie/wyrd/wyrd.pdca-wt/crates/metadata-fdb)
   Compiling wyrd-server v0.0.0 (/home/eddie/wyrd/wyrd.pdca-wt/crates/server)
    Finished `test` profile [unoptimized + debuginfo] target(s) in 2.31s
     Running tests/fdb_backend_selection.rs (target/debug/deps/fdb_backend_selection-c2bfc7db68c44ed5)

running 3 tests
test fdb_with_the_feature_selects_the_fdb_backend ... ok
test unknown_backend_message_lists_fdb_as_a_known_backend ... ok
test usage_lists_fdb_as_a_metadata_backend ... ok

test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

Note the cfg gates flipping, visibly: `fdb_with_the_feature_selects_the_fdb_backend` is
present here and absent in §3; `fdb_without_the_feature_names_the_build_flag` is the reverse.
That is the mechanical check that the `#![allow(unexpected_cfgs)]` isn't masking a typo.

### 4.3 `cargo check -p wyrd-server --features fdb,etcd --tests` — the `(Fdb, Etcd)` arm (carry-forward item 3)

```
$ command -v protoc && protoc --version
/usr/bin/protoc
libprotoc 3.21.12

$ cargo clean -p etcd-client -p wyrd-coordination-etcd -p wyrd-server
$ cargo check -p wyrd-server --features fdb,etcd --tests
   Compiling etcd-client v0.14.1
    Checking wyrd-coordination-etcd v0.0.0 (/home/eddie/wyrd/wyrd.pdca-wt/crates/coordination-etcd)
    Checking wyrd-server v0.0.0 (/home/eddie/wyrd/wyrd.pdca-wt/crates/server)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 1.63s
exit=0
```

`etcd-client v0.14.1` is rebuilt from scratch — its `protoc`-driven build script runs — and
`wyrd-coordination-etcd` really compiles. So the `(Fdb, Etcd)` tuple arm at
`crates/server/src/cli.rs:1486-1492` is compile-verified. **Iteration 1's
`NEEDS-HUMAN external dependency: protoc` claim was wrong and is withdrawn.** There is no
NEEDS-HUMAN item in this bundle.

---

## 5. The live round-trip against a real FoundationDB cluster — verbatim

The brief requires this and forbids substituting a code-read. It gates nothing (ADR-0042,
*"selection is not deployment"*), but it is the only check that drives `open_fdb_meta` →
`FdbMetadataStore::connect()` → a real `fdbserver`.

Method: a script (`target/fdb-440-demo.sh`, deleted afterwards — it is scratch, not part of
the patch) that prints each command as `$ …` immediately before running it and lets the real
output follow. `[exit N]` lines are printed by the script for non-zero exits. **The block
below is the script's actual stdout, copied unedited.** The cluster was brought up and the
database created with the `run_fdb_conformance` recipe (`xtask/src/main.rs:292`,
`configure_fdb_database` at `:334`, `write_fdb_cluster_file` at `:382`), under compose
project `wyrd-fdb-440`:

```
$ docker compose -p wyrd-fdb-440 -f deploy/fdb-single-node/docker-compose.yml up -d
 Container wyrd-fdb-440-fdb-1 Created
 Container wyrd-fdb-440-fdb-1 Starting
 Container wyrd-fdb-440-fdb-1 Started
$ docker compose -p wyrd-fdb-440 -f deploy/fdb-single-node/docker-compose.yml exec -T fdb fdbcli --exec "configure new single memory"
Database created
```

then, from the script:

```
$ docker compose -p wyrd-fdb-440 -f deploy/fdb-single-node/docker-compose.yml ps --format 'table {{.Name}}\t{{.Status}}'
NAME                 STATUS
wyrd-fdb-440-fdb-1   Up 36 seconds

$ cat target/fdb-single-node/fdb.cluster; echo
docker:docker@127.0.0.1:4500

$ fdbcli -C /home/eddie/wyrd/wyrd.pdca-wt/target/fdb-single-node/fdb.cluster --exec 'status minimal'
The database is available.

$ cargo build -p wyrd-server --bin wyrd --features fdb
   Compiling wyrd-metadata-fdb v0.0.0 (/home/eddie/wyrd/wyrd.pdca-wt/crates/metadata-fdb)
   Compiling wyrd-server v0.0.0 (/home/eddie/wyrd/wyrd.pdca-wt/crates/server)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 2.38s

$ echo $WYRD_FDB_CLUSTER_FILE
/home/eddie/wyrd/wyrd.pdca-wt/target/fdb-single-node/fdb.cluster

$ rm -rf target/fdb-440-data target/fdb-440-io; mkdir -p target/fdb-440-io

$ printf 'hello from the fdb backend, issue 440\n' > target/fdb-440-io/input.txt

$ wc -c target/fdb-440-io/input.txt
38 target/fdb-440-io/input.txt

$ ./target/debug/wyrd put target/fdb-440-io/input.txt --key demo/fdb-440 --data-dir target/fdb-440-data --metadata-backend fdb
put ok: key=demo/fdb-440 inode=1 chunks=1 bytes=38 durability=rs(6,3) version=1

$ ./target/debug/wyrd get demo/fdb-440 --data-dir target/fdb-440-data --out target/fdb-440-io/output.txt --metadata-backend fdb

$ diff target/fdb-440-io/input.txt target/fdb-440-io/output.txt && echo 'ROUND-TRIP OK: byte-identical'
ROUND-TRIP OK: byte-identical

# a second put of the same key must hit the require_absent/CAS path on the real cluster:
$ ./target/debug/wyrd put target/fdb-440-io/input.txt --key demo/fdb-440 --data-dir target/fdb-440-data --metadata-backend fdb
wyrd: key `demo/fdb-440` already exists
[exit 1]

$ ./target/debug/wyrd get demo/no-such-key-440 --data-dir target/fdb-440-data --metadata-backend fdb
wyrd: key `demo/no-such-key-440` not found
[exit 1]

# a SECOND key proves alloc_inode advances against the live cluster:
$ ./target/debug/wyrd put target/fdb-440-io/input.txt --key demo/fdb-440-second --data-dir target/fdb-440-data --metadata-backend fdb
put ok: key=demo/fdb-440-second inode=3 chunks=1 bytes=38 durability=rs(6,3) version=1

# the metadata really lives in FoundationDB, not in the local --data-dir:
$ fdbcli -C /home/eddie/wyrd/wyrd.pdca-wt/target/fdb-single-node/fdb.cluster --exec 'getrangekeys "" \xff 8'

Range limited to 8 keys
`dirent:0/demo/fdb-440'
`dirent:0/demo/fdb-440-second'
`inode:1'
`inode:3'
`meta:next_inode'
`pending:36893488147419103232'


$ find target/fdb-440-data -type f | head
target/fdb-440-data/chunks/00000000000000030000000000000000/00005.frag
target/fdb-440-data/chunks/00000000000000030000000000000000/00002.frag
target/fdb-440-data/chunks/00000000000000030000000000000000/00003.frag
target/fdb-440-data/chunks/00000000000000030000000000000000/00004.frag
target/fdb-440-data/chunks/00000000000000030000000000000000/00001.frag
target/fdb-440-data/chunks/00000000000000030000000000000000/00008.frag
target/fdb-440-data/chunks/00000000000000030000000000000000/00007.frag
target/fdb-440-data/chunks/00000000000000030000000000000000/00006.frag
target/fdb-440-data/chunks/00000000000000030000000000000000/00000.frag
target/fdb-440-data/chunks/00000000000000010000000000000000/00005.frag

# the DEFAULT build (no fdb feature) still rejects the value, with the build hint:
$ cargo build -q -p wyrd-server --bin wyrd

$ ./target/debug/wyrd put target/fdb-440-io/input.txt --key x --data-dir target/fdb-440-data --metadata-backend fdb
wyrd: metadata backend `fdb` requires building `wyrd` with `--features fdb`
[exit 1]

$ ./target/debug/wyrd 2>&1 | head -2
usage:
  wyrd put <file> --key <name> [--data-dir DIR] [--chunk-size N] [--durability rs(k,m)|none] [--endpoints URL,URL,…] [--metadata-backend redb|tikv|fdb]
[exit 2]
```

Teardown:

```
$ docker compose -p wyrd-fdb-440 -f deploy/fdb-single-node/docker-compose.yml down -v --remove-orphans
 Container wyrd-fdb-440-fdb-1 Stopping
 Container wyrd-fdb-440-fdb-1 Stopped
 Container wyrd-fdb-440-fdb-1 Removing
 Container wyrd-fdb-440-fdb-1 Removed
```

### Reading the numbers (so nothing is taken on trust)

- `bytes=38` on **both** puts, and `wc -c` says `38` — the same file has one size. (Iteration
  1's transcript claimed 38 and 17 for the same file; that is the fabrication the sign-off
  caught.)
- `inode=1` for the first key, `inode=3` for the second. The sign-off's own run reported
  `inode=2` for its first put; the difference is simply a fresh database here
  (`configure new single memory` on a new container) and does not indicate anything about the
  patch. The gap 1 → 3 is `alloc_inode` consuming inode 2 on the **rejected** duplicate put in
  between — the CAS `require_absent` fails *after* the inode is allocated. That matches
  `ALLOC_INODE_BUDGET`'s conditional-batch contract (`cli.rs:75-80`, brief "out of scope"),
  and is why the brief says not to "fix" it.
- The FDB keyspace dump is the decisive one: `dirent:0/demo/fdb-440`,
  `dirent:0/demo/fdb-440-second`, `inode:1`, `inode:3`, `meta:next_inode` live **in the
  cluster**; the `--data-dir` holds only `chunks/**/*.frag`. The metadata plane really is
  FoundationDB. The `pending:…` key is the driver's own in-flight marker, from #438.
- The last two commands are the default-feature binary (rebuilt over the same path by
  `cargo build -q -p wyrd-server --bin wyrd`), showing the feature-hint rejection and the
  three-backend usage line — i.e. §3's assertions 1 and 3 also hold for the shipped binary.

---

## 6. What changed in production, and why (unchanged from iteration 1 — cited against `$PDCA_WORKTREE` @ `779ac1d` + patch)

- **Enum + `from_config`** — `Fdb` variant at `crates/server/src/cli.rs:97-102`; the
  `Some("fdb")` arms at `:119-124`. Mirrors `Tikv` / `Some("tikv")` (`:92-96`, `:113-118`)
  exactly: `Ok` under `#[cfg(feature = "fdb")]`, a build-hint `Err` under
  `#[cfg(not(feature = "fdb"))]`. Unknown-value text now names all three (`:126`).
- **`open_fdb_meta()`** — `cli.rs:155-172`. Mirrors `open_tikv_meta()` (`:131-141` pre-patch,
  `:143-153` post-patch) structurally but **not** on env resolution, and is a plain `fn`, not
  `async fn`: `FdbMetadataStore::connect()` is `pub fn connect() -> Result<Self>`
  (`crates/metadata-fdb/src/lib.rs:898`) and owns `WYRD_FDB_CLUSTER_FILE` resolution with a
  deliberate fallback to `/etc/foundationdb/fdb.cluster` (`:423-433`). The
  `WYRD_FDB_CLUSTER_FILE` hint is attached as **error context on a failed connect**
  (`cli.rs:169-171`), never as a pre-check. Copying TiKV's `env::var(...)?` pre-check
  (`cli.rs:135-137` pre-patch) would reject a healthy stock install; the brief resolves this
  and forbids it (`brief.md:82-91`).
- **The 8 dispatch sites**, each gaining exactly one arm shaped like the adjacent `Tikv` arm,
  swapping `open_tikv_meta().await?` → `open_fdb_meta()?`: `cmd_put` (`cli.rs:372-376`),
  `cmd_get` (`:458-462`), the custodian fencing message (`:713-718`),
  `run_reconstruction_over_backend` (`:839-847`), `serve_s3_dispatch`'s two tuple arms
  (`:1479-1484` `(Fdb, Mem)`, `:1485-1492` `(Fdb, Etcd)` under
  `#[cfg(all(feature = "fdb", feature = "etcd"))]`), `cluster_put` (`:1550-1554`),
  `cluster_get` (`:1598-1602`).
- **Usage strings** — `cli.rs:268, 269, 271, 272` now read `redb|tikv|fdb`.
- **Manifest** — `crates/server/Cargo.toml:25-31` (the `fdb` feature) and `:49-52` (the
  optional dep), mirroring the `tikv` block's shape and comment style. Workspace dep already
  existed (root `Cargo.toml:51`).
- **`Cargo.lock`** — one line, `"wyrd-metadata-fdb"` in `wyrd-server`'s `dependencies` array,
  mirroring the optional `wyrd-coordination-etcd` entry. No new third-party crate enters the
  graph.

**Invariant restored, not merely a small diff.** `brief.md:54-63`: *selecting a metadata
backend is passing a different concrete behind the unchanged `MetadataStore` seam — never a
refactor of any consumer.* No consumer changed: `local_store_put`, `local_store_get`,
`cluster_store_put/get`, `alloc_inode`, `run_reconstruction_until`, `Gateway::new`,
`serve_s3` are byte-for-byte identical. Every added line is at the composition root
(`crates/server`, ADR-0010). `crates/metadata-fdb` is untouched.

### Ruled out

- **A hard `WYRD_FDB_CLUSTER_FILE` pre-check** (the issue text's suggestion). Same ~5 lines
  as what shipped, so it is not a cost trade — it is simply wrong: `connect()` falls back to
  the package default (`metadata-fdb/src/lib.rs:423-433`, unit-tested
  `an_absent_or_blank_value_falls_back_to_the_package_default`, `:479-482`), so a pre-check
  would fail a stock FDB install where `/etc/foundationdb/fdb.cluster` is present and healthy.
- **Editing `crates/server/tests/backend_selection.rs`** instead of adding a file.
  `_added_files` only recognises `--- /dev/null` hunks (`run-verify.sh:68`); a modification
  leaves `ADDED_TESTS` empty, `:244` fires, the gate degrades to `PASS (green-only)`, and the
  RED phase would revert the edited test itself (`:264`). Verified against the actual
  `run-verify.sh`, not the brief's paraphrase.
- **Adding an `fdb` row to xtask's `feature_gated_checks()`** — brief defers it to a
  follow-up (`brief.md:109-110`); out of scope here.

---

## 7. External dependencies — every one checked, none missing

```
$ command -v protoc && protoc --version
/usr/bin/protoc
libprotoc 3.21.12
$ command -v docker && docker --version
/usr/bin/docker
Docker version 29.6.1, build 8900f1d
$ command -v fdbcli && fdbcli --version
/usr/bin/fdbcli
FoundationDB CLI 7.3 (v7.3.77)
$ ls -l /usr/lib/libfdb_c.so
-rwxr-xr-x 1 root root 23991600 Apr 18 12:13 /usr/lib/libfdb_c.so
$ ls -d /usr/include/foundationdb
/usr/include/foundationdb
```

Nothing blocks the binding success criterion or `C4-ci` (default build links no FDB, compiles
no FDB code). Every supplementary dependency the brief names was present and **used**. **No
`NEEDS-HUMAN` item.**

---

## 8. The three refutation questions

**(a) Genuine red?** **Yes** — and this time the red is the *right kind*. Not asserted:
the project's own `run-verify.sh` reverted `Cargo.lock`, `crates/server/Cargo.toml`, and
`crates/server/src/cli.rs`, kept the added test, and re-ran it. Result, pasted in §3:
`test result: FAILED. 0 passed; 3 failed`, each with a panic message showing the pre-fix
string (`"unknown metadata backend \`fdb\` (expected \`redb\` or \`tikv\`)"`, the `nonsense`
message lacking `fdb`, and the `redb|tikv` usage line). I also reproduced the revert by hand
with `git stash push -- Cargo.lock crates/server/Cargo.toml crates/server/src/cli.rs` (§2)
and restored with `git stash pop`. Note the assertions bind on **message content**, never on
`is_err()` — pre-fix, `from_config(Some("fdb"))` is already `Err`, so an `is_err()` assertion
would be green before the fix and prove nothing (`brief.md:33-37`).

**(b) Production path?** **Yes.** `fdb_without_the_feature_names_the_build_flag` and
`unknown_backend_message_lists_fdb_as_a_known_backend` call
`wyrd_server::cli::MetadataBackend::from_config` — the production function at
`crates/server/src/cli.rs:110`, re-exported by `pub mod cli` (`crates/server/src/lib.rs:16`). `usage_lists_fdb_as_a_metadata_backend` executes
`env!("CARGO_BIN_EXE_wyrd")`, the binary Cargo builds from `crates/server/src/main.rs` (the
idiom `cli_roundtrip.rs:9-14` uses), and reads its real stderr. No mock, no copy, no
re-implementation. Beyond the test, §5 drives the same `cli.rs` dispatch arms through
`open_fdb_meta` into a live `fdbserver`.

**(c) Fixture includes the fault?** **Yes.** The RED fixture is not curated: `run-verify.sh`
enumerates the patch's own `+++ b/` list (`:67`) and reverts *every* modified file (`:264`) —
all three production files (`Cargo.lock`, `crates/server/Cargo.toml`,
`crates/server/src/cli.rs`) — keeping only the added test (`:260`). The failing element (the
missing `Fdb` selection arm) is exactly what is restored to its broken state; nothing is
excluded. In §5 the fixture is a *real* single-node `fdbserver` in Docker, and the FDB
keyspace dump proves the writes landed in it, not in a local store.
