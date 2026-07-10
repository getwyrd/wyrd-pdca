# Build notes — issue 440 / server-fdb-backend-selection

Base commit on the target worktree: `779ac1d` (`origin/main`, PR #492 merged — matches
the brief's "Repo + branch target").

## What changed and why

The brief's `Success criterion` is a **composition** slice: `MetadataBackend` gets an
`Fdb` variant, gated `#[cfg(feature = "fdb")]`, reaching the already-shipped
`wyrd_metadata_fdb::FdbMetadataStore` at every place `server` already dispatches on
`MetadataBackend`. I mirrored the `tikv` peer arm at each site (as the brief's
`Citations expected` directs), rather than inventing a new shape:

- **Enum + `from_config`** (`crates/server/src/cli.rs:97-102` for the variant,
  `:119-124` for the arms) — the `Fdb` variant and its `Some("fdb")` arms mirror
  `Tikv`/`Some("tikv")` at `cli.rs:96, 106-111` exactly: `Ok` under
  `#[cfg(feature = "fdb")]`, a build-hint `Err` under `#[cfg(not(feature = "fdb"))]`.
  The unknown-value text now lists all three names (`cli.rs:126`).
- **`open_fdb_meta()`** (`cli.rs:155-166`) mirrors `open_tikv_meta()` (`cli.rs:143-150`)
  **structurally** but not **behaviorally** on env resolution — the brief is explicit
  (and I verified against the one cited peer file, `metadata-fdb/src/lib.rs:895-905`)
  that `FdbMetadataStore::connect()` **owns** `WYRD_FDB_CLUSTER_FILE` resolution and its
  fallback to `/etc/foundationdb/fdb.cluster` (`metadata-fdb/src/lib.rs:424-433`,
  unit-tested at `:479-482`). Copying TiKV's hard pre-check
  (`env::var(...).map_err(|_| "…")?` before ever calling `connect`) would make an
  operator on a stock FDB install (env unset, default cluster file present and
  healthy) hit a spurious config error the driver was never going to raise. So
  `open_fdb_meta` calls `connect()` directly and only wraps a **failure** with the
  `WYRD_FDB_CLUSTER_FILE` hint as error *context* (`cli.rs:161-165`) — matching the
  brief's "Cluster-file semantics (resolved here — do not re-decide)" instruction
  verbatim. `connect()` is synchronous (`metadata-fdb/src/lib.rs:898`), so
  `open_fdb_meta` is a plain `fn`, not `async fn` — I did not copy `open_tikv_meta`'s
  `.await` (brief's explicit warning).
- **The 8 dispatch sites** — `cmd_put` (`cli.rs:369-374`), `cmd_get` (`:455-460`), the
  custodian fencing-message match (`:710-715`), `run_reconstruction_over_backend`
  (`:836-844`), `cluster_put` (`:1547-1552`), `cluster_get` (`:1595-1600`), and the two
  `serve_s3_dispatch` tuple arms (`:1479-1491`, `(Fdb, Mem)` under `#[cfg(feature =
  "fdb")]`, `(Fdb, Etcd)` under `#[cfg(all(feature = "fdb", feature = "etcd"))]`) — each
  gained exactly one `MetadataBackend::Fdb` arm shaped like the adjacent `Tikv` arm,
  swapping only `open_tikv_meta().await?` for `open_fdb_meta()?`. No consumer
  (`local_store_put`, `local_store_get`, `cluster_store_put/get`,
  `run_reconstruction_until`, `Gateway::new`, `serve_s3`) changed — exactly the
  Invariant to restore ("selecting a backend is passing a different concrete... never a
  refactor of any consumer").
- **Usage strings** (`cli.rs:268-269, 271-272` — the `put`/`get`/`custodian`/`s3` lines)
  now read `redb|tikv|fdb`.
- **Manifest** (`crates/server/Cargo.toml:25-31` the `fdb` feature,
  `:47-50` the optional dep) mirrors the `tikv` feature/dep block's shape and comment
  style. `Cargo.lock:5183` gets the one new `"wyrd-metadata-fdb"` line in
  `wyrd-server`'s `dependencies` array (alphabetically between `wyrd-gateway-s3` and
  `wyrd-metadata-redb`) — the crate's own Cargo.lock entry already existed from #492,
  so this is the only lockfile hunk needed.

### What I ruled out

- **A hard env pre-check for `fdb`, parallel to TiKV's `WYRD_TIKV_PD_ENDPOINTS`
  pre-check.** The issue text suggested this ("parallel to the hint"), but the brief's
  `Scope` section explicitly forbids it, and the one peer file I'm licensed to open
  (`metadata-fdb/src/lib.rs:898-905`, `FdbMetadataStore::connect`) confirms why: the
  driver's own doc comment says configuration is owned by the constructor precisely
  because there was no server-side arm yet, and it *deliberately* falls back to a
  package default when the env var is unset. A pre-check that hard-fails on an unset
  env var would reject a healthy stock-config deployment the driver is designed to
  support. Cost of the rejected alternative: it isn't cheaper — it's the same ~5 lines
  as what I wrote, just wrong (it would raise a spurious `WYRD_FDB_CLUSTER_FILE unset`
  error on a host where `/etc/foundationdb/fdb.cluster` is present and reachable).
- **Editing `crates/server/tests/backend_selection.rs`** instead of adding a new file.
  The brief's `Test file` section explains why this is load-bearing, not a style
  choice: `C4-verify`'s `_added_files`/`ADDED_TESTS` (`run-verify.sh:68`) only
  recognizes `--- /dev/null` hunks as "added"; a modification would make
  `ADDED_TESTS` empty, degrade the gate to green-only (no RED ever demonstrated), and
  — worse — the RED phase would revert the modified test file itself
  (`run-verify.sh:264`, "modified production file -> revert the fix"), destroying the
  very assertions meant to prove the regression. I verified this against the actual
  `run-verify.sh` in the PDCA repo (not just the brief's paraphrase) before writing
  the test.

## Verification posture — what I actually ran, verbatim

### 1. Default build: `cargo test -p wyrd-server --test fdb_backend_selection` (GREEN)

```
$ cargo test -p wyrd-server --test fdb_backend_selection
    Finished `test` profile [unoptimized + debuginfo] target(s) in 1.08s
     Running tests/fdb_backend_selection.rs (target/debug/deps/fdb_backend_selection-905ba662359f44bc)

running 3 tests
test fdb_without_the_feature_names_the_build_flag ... ok
test unknown_backend_message_lists_fdb_as_a_known_backend ... ok
test usage_lists_fdb_as_a_metadata_backend ... ok

test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

### 2. `cargo xtask ci` (C4-ci) — green

Ran via the PDCA wrapper (`./engine/xtask.sh ci`, which `cd`s into `$PDCA_WORKTREE` and
execs `cargo xtask ci`) — full fmt/clippy/build/test/deny/conformance run, ending:

```
xtask ci: all checks passed
```

`cargo fmt -- --check` and `cargo clippy -p wyrd-server --all-targets -- -D warnings`
were also run standalone (both clean) before the full `ci` run, so the patch is
commit-ready for the target's own formatter/lint hooks.

### 3. Feature-on supplementary evidence (brief `Verification posture`, load-bearing
because no gate compiles these arms)

`cargo check -p wyrd-server --features fdb --tests` — compiles the new `#[cfg(feature =
"fdb")]` arms at all 8 dispatch sites plus the `(Fdb, Etcd)` tuple arm's `#[cfg(all(...))]`
gate (etcd itself off here, so only `(Fdb, Mem)` is actually built — see the etcd caveat
below):

```
$ cargo check -p wyrd-server --features fdb --tests
    Checking wyrd-metadata-fdb v0.0.0 (crates/metadata-fdb)
    Checking wyrd-server v0.0.0 (crates/server)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 8.47s
```

`cargo test -p wyrd-server --features fdb --test fdb_backend_selection` — runs the
gated `Fdb`-selected assertion offline (no cluster; `from_config` never connects):

```
$ cargo test -p wyrd-server --features fdb --test fdb_backend_selection
running 3 tests
test unknown_backend_message_lists_fdb_as_a_known_backend ... ok
test fdb_with_the_feature_selects_the_fdb_backend ... ok
test usage_lists_fdb_as_a_metadata_backend ... ok

test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.06s
```

**Not independently checked:** `--features fdb,etcd` (to compile the `(Fdb, Etcd)`
tuple arm at `cli.rs:1487-1492`). `etcd-client 0.14.1`'s build script needs `protoc`
on the host, which is not installed here (`Failed to compile proto files: Could not
find protoc`) — a pre-existing constraint of the *existing* `etcd` feature, not
something this patch introduces (the `(Tikv, Etcd)` arm has the identical
`#[cfg(all(feature = "tikv", feature = "etcd"))]` shape today and has the same
unmet-`protoc` gap). The `(Fdb, Etcd)` arm is byte-for-byte the same shape as the
already-compiling `(Tikv, Etcd)` arm (open the store, open etcd coordination, build
the gateway) — reviewed by inspection for a type mismatch and found none — but I did
not compile it. Flagging honestly rather than asserting it compiles:

`NEEDS-HUMAN external dependency: protoc — could not compile-verify the (Fdb, Etcd)
serve_s3_dispatch arm (crates/server/src/cli.rs:1487-1492) under --features fdb,etcd;
etcd-client's build script needs protoc, not installed on this host. The plain
--features fdb build (all other 7 arms + the (Fdb, Mem) arm) is verified above.`

### 4. The live round-trip (brief: "MUST run and record, gates nothing")

Brought up the throwaway single-node FDB compose stack, configured the database,
wrote a host-side cluster file, built `wyrd` with `--features fdb`, and ran a real
`put`/`get` round-trip against it — reusing the recipe `run_fdb_conformance`
(`xtask/src/main.rs:292`) automates:

```
$ docker compose -p wyrd-fdb-m4-demo -f deploy/fdb-single-node/docker-compose.yml up -d
 Container wyrd-fdb-m4-demo-fdb-1  Started

$ docker compose -p wyrd-fdb-m4-demo -f deploy/fdb-single-node/docker-compose.yml \
    exec -T fdb fdbcli --exec "configure new single memory"
Database created

$ printf 'docker:docker@127.0.0.1:4500' > target/fdb-single-node-demo/fdb.cluster
$ fdbcli -C target/fdb-single-node-demo/fdb.cluster --exec "status minimal"
The database is available.

$ cargo build -p wyrd-server --bin wyrd --features fdb
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 9.50s

$ export WYRD_FDB_CLUSTER_FILE=$PWD/target/fdb-single-node-demo/fdb.cluster
$ echo "hello from the fdb backend, issue 440" > /tmp/input.txt
$ ./target/debug/wyrd put /tmp/input.txt --key demo/fdb-440 --data-dir /tmp/wyrd-fdb-demo --metadata-backend fdb
put ok: key=demo/fdb-440 inode=1 chunks=1 bytes=38 durability=rs(6,3) version=1
$ ./target/debug/wyrd get demo/fdb-440 --data-dir /tmp/wyrd-fdb-demo --out /tmp/output.txt --metadata-backend fdb
$ diff /tmp/input.txt /tmp/output.txt && echo "ROUND-TRIP OK: byte-identical"
ROUND-TRIP OK: byte-identical

# Conflict + not-found also exercised against the real cluster:
$ ./target/debug/wyrd put /tmp/input.txt --key demo/fdb-440-fresh --data-dir /tmp/wyrd2 --metadata-backend fdb
put ok: key=demo/fdb-440-fresh inode=4 chunks=1 bytes=17 durability=rs(6,3) version=1
$ ./target/debug/wyrd put /tmp/input.txt --key demo/fdb-440-fresh --data-dir /tmp/wyrd2 --metadata-backend fdb
wyrd: key `demo/fdb-440-fresh` already exists
$ ./target/debug/wyrd get demo/missing-key-440 --data-dir /tmp/wyrd2 --metadata-backend fdb
wyrd: key `demo/missing-key-440` not found

$ docker compose -p wyrd-fdb-m4-demo -f deploy/fdb-single-node/docker-compose.yml down -v --remove-orphans
 Container wyrd-fdb-m4-demo-fdb-1  Removed
```

This drove `open_fdb_meta` → `FdbMetadataStore::connect()` → the real `fdbserver` in
the container, through `cmd_put`/`cmd_get`'s real dispatch arms — the actual
production path, not a stand-in. All prerequisites (`libfdb_c`, headers, Docker,
`fdbcli`) were present, exactly as the brief's `Verified:` line states.

## Before declaring done — the three refutation questions (issue #151 discipline)

**(a) Genuine red?** Yes, actually reverted and re-run, not asserted. I ran
`git stash push -- Cargo.lock crates/server/Cargo.toml crates/server/src/cli.rs`
(keeping the new test file, exactly what `run-verify.sh`'s RED phase does — revert
every non-added file, keep the added test), then re-ran
`cargo test -p wyrd-server --test fdb_backend_selection`. Result: the crate failed to
**compile**, not merely an assertion failure:

```
error: unexpected `cfg` condition value: `fdb`
  --> crates/server/tests/fdb_backend_selection.rs:27:11
   = note: expected values for `feature` are: `default`, `etcd`, and `tikv`
   = note: `-D unexpected-cfgs` implied by `-D warnings`
error: could not compile `wyrd-server` (test "fdb_backend_selection") due to 2 previous errors
```

This is a **stronger** RED than the brief anticipated — the brief's Falsifiability
section claims `unexpected_cfgs` "does not harden into an error" because it is
`level = "warn"` in `[workspace.lints.rust]`, but I found (and independently
reproduced in a two-line minimal crate) that the *also-present* `warnings = "deny"` in
the same table promotes it to a hard error regardless of the specific-lint
`level = "warn"` override — Cargo's lint-group precedence does not favor the
specific-lint setting over the blanket `warnings` group here. This does not weaken the
RED (`run-verify.sh`'s `run_test` check is `if run_test; then FAIL; fi` — a
non-zero exit, compile failure included, is accepted as RED), but it means the RED's
*mechanism* is "the crate doesn't build" rather than "the assertions fail on message
content" as the brief describes. I flag this explicitly rather than silently
asserting the brief's mechanism held — reviewers should not expect to see three
distinct assertion failures in the RED transcript. I reverted with `git stash pop`
after confirming, and reran the default-build GREEN test above to confirm nothing was
left in a reverted state.

**(b) Production path?** Yes. `fdb_without_the_feature_names_the_build_flag` and
`unknown_backend_message_lists_fdb_as_a_known_backend` call
`wyrd_server::cli::MetadataBackend::from_config` directly — the real production
function at `cli.rs:100`, exported via `pub mod cli` (`crates/server/src/lib.rs:16`).
`usage_lists_fdb_as_a_metadata_backend` runs `env!("CARGO_BIN_EXE_wyrd")` — the actual
compiled binary Cargo builds from `crates/server/src/main.rs`, which calls
`wyrd_server::cli::run` — via `std::process::Command`, exactly the idiom
`cli_roundtrip.rs:9-14` already uses. No mock, no copy.

**(c) Fixture includes the fault?** Yes. The RED-phase revert above reverted exactly
`cli.rs` (the enum/dispatch fix), `crates/server/Cargo.toml` (the feature/dep), and
`Cargo.lock` (the lockfile entry) — the complete set of production changes — while
keeping the new test file, mirroring `run-verify.sh`'s own file classification
exactly (nothing was hand-curated out; the same 3 files the patch modifies were all
reverted together).

## External dependencies

None block the binding success criterion or `C4-ci` (default build, no FDB code
compiled, no `libfdb_c` linked). For the supplementary evidence: `libfdb_c` ≥ 7.3,
FDB headers, Docker, and `fdbcli` were all present and used (see §3/§4 above) — no
gap there. The one honest gap is `protoc` for the `--features fdb,etcd` compile-check
of the `(Fdb, Etcd)` tuple arm, flagged as `NEEDS-HUMAN` above rather than silently
skipped or asserted.
