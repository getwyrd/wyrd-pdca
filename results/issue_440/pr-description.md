# PR description

## Summary
**User impact:** An operator who wants to run `wyrd` on FoundationDB cannot. The backend
itself already ships, but asking for it — `wyrd put … --metadata-backend fdb` — fails with
``unknown metadata backend `fdb` ``, as though the backend did not exist. The help text
offers only `redb` and `tikv`, so there is no discoverable way in, and the error gives no
hint that the backend is real and merely needs to be built in. Anyone following the
FoundationDB documentation hits a dead end.

This PR makes `fdb` a selectable metadata backend, behind a new off-by-default `fdb` build
feature, everywhere `redb` and `tikv` are already selectable.

## What to look at
Everything is in the `server` crate — the one place that knows about concrete backends.
`--metadata-backend fdb` now resolves to the FoundationDB store, and each command that
opens a metadata store gained one arm for it.

A default build is unchanged and stays FoundationDB-free: it compiles no FoundationDB code
and links no `libfdb_c`. What changes for a default build is the *diagnosis* — `fdb` is now
rejected with a hint naming the build flag, exactly as `tikv` already is, rather than
reported as an unknown name.

Two cheap ways to exercise it:

```
# default build: an actionable rejection, and all three backends in the usage text
cargo run -p wyrd-server --bin wyrd -- put /etc/hostname --key k --metadata-backend fdb
cargo run -p wyrd-server --bin wyrd

# with the feature and a cluster (deploy/fdb-single-node/docker-compose.yml):
WYRD_FDB_CLUSTER_FILE=… ./wyrd put file --key demo --metadata-backend fdb
WYRD_FDB_CLUSTER_FILE=… ./wyrd get demo --out out.txt --metadata-backend fdb
```

## Root cause
`MetadataBackend` (`crates/server/src/cli.rs:88-97`) enumerates only `Redb` and `Tikv`, so
`from_config` has no `fdb` arm and the value falls through to the catch-all unknown-value
error (`crates/server/src/cli.rs:112-115`). The FoundationDB driver landed with no
server-side selection arm to reach it — its own constructor says so
(`crates/metadata-fdb/src/lib.rs:895-896`).

## Fix
An `Fdb` variant, compiled only under a new off-by-default `fdb` feature on `wyrd-server`
that forwards to `wyrd-metadata-fdb/fdb`, mirroring how the `tikv` feature is arranged
(`crates/server/Cargo.toml:23-24`, `:39-41`). Concretely:

- `MetadataBackend::Fdb` plus the paired `from_config` arms — `Ok(Fdb)` with the feature, a
  build-hint `Err` without it — mirroring the TiKV pair at `crates/server/src/cli.rs:106-111`.
  The unknown-value message now names all three backends.
- `open_fdb_meta()`, mirroring `open_tikv_meta()` (`crates/server/src/cli.rs:131-141`) but
  **synchronous**, since `FdbMetadataStore::connect()` is
  (`crates/metadata-fdb/src/lib.rs:898`). It has **no** env pre-check: `connect()` owns
  cluster-file resolution and deliberately falls back to `/etc/foundationdb/fdb.cluster`
  when `WYRD_FDB_CLUSTER_FILE` is unset, so a pre-check would reject a healthy stock
  install. The variable is attached as error context on a failed connect instead.
- One arm at each of the eight store-construction sites, beside the existing TiKV arm:
  `crates/server/src/cli.rs:336`, `:417`, `:666`, `:783`, `:1476`, `:1519`, and the two
  tuple arms of the S3 dispatch at `:1403` and `:1416` (`(Fdb, Etcd)` gated on both features).
- The four usage strings (`crates/server/src/cli.rs:236-240`) now read `redb|tikv|fdb`.
- The optional dependency, and its `Cargo.lock` entry. No new third-party crate enters the
  graph — the FoundationDB crates are already locked and allowlisted.

No consumer changed. `local_store_put`, `local_store_get`, `cluster_store_put/get`,
`alloc_inode`, `run_reconstruction_until`, `Gateway::new` and `serve_s3` are byte-for-byte
identical; selecting a backend remains "pass a different concrete behind the unchanged
`MetadataStore` seam".

## Verification
- **Claim:** In a default build (no `fdb` feature, no `libfdb_c`, no cluster),
  `--metadata-backend fdb` is rejected with a message naming `--features fdb`.
  **Checked:** `crates/server/src/cli.rs:108-111` on `main` — the shape TiKV uses; `fdb`
  previously took the catch-all at `:112-115` instead.
- **Claim:** The unknown-backend message lists all three backends.
  **Checked:** `crates/server/src/cli.rs:113` on `main` reads ``(expected `redb` or
  `tikv`)``. The test probes with `nonsense`, not `fdb`, so the echoed value cannot satisfy
  the assertion.
- **Claim:** The binary's usage lists `redb|tikv|fdb`.
  **Checked:** `crates/server/src/cli.rs:236, 237, 239, 240` on `main` print `redb|tikv`.
- **Claim:** A default build compiles no FoundationDB code and links no `libfdb_c`.
  **Checked:** `crates/server/Cargo.toml:23` (`default = []`) on `main`; the new dep is
  `optional`, mirroring `wyrd-metadata-tikv` at `:39-41`. `cargo xtask ci` is green.
- **Claim:** The feature-gated arms compile and select the real store.
  **Checked:** `cargo check -p wyrd-server --features fdb --tests` and
  `cargo check -p wyrd-server --features fdb,etcd --tests` (the latter covers the
  `(Fdb, Etcd)` tuple arm mirroring `crates/server/src/cli.rs:1416`); with the feature on,
  `from_config(Some("fdb"))` returns `Fdb`.
- **Test:** `crates/server/tests/fdb_backend_selection.rs` — three assertions, all binding
  on message/stderr content rather than `is_err()` (pre-fix `from_config(Some("fdb"))` is
  *already* an `Err`, just the wrong one). Fails pre-fix (`0 passed; 3 failed`, each on the
  old text), passes post-fix. Run with `cargo test -p wyrd-server --test fdb_backend_selection`.
- **Live round-trip (not a gate, but run):** against a single-node FoundationDB from
  `deploy/fdb-single-node/docker-compose.yml`, `wyrd put` → `wyrd get` with
  `--metadata-backend fdb` returned byte-identical data; a repeated `put` of the same key
  was rejected by the CAS path; a missing key reported not-found. The FoundationDB keyspace
  held the `dirent:`/`inode:`/`meta:next_inode` keys while `--data-dir` held only chunk
  fragments — the metadata plane really is FoundationDB.

Fixes #440
