# PR description

## Summary
**User impact:** An operator who wants to run Wyrd on its production
FoundationDB backend had no container image to deploy. The only image that
built the `wyrd` binary was a test fixture buried inside a crate's test
directory — it could not even build the FoundationDB support, and a running
container would have been missing the FoundationDB client library and CLI
the backend needs. In practice there was no supported way to run the
production metadata backend in a container.

This adds a first-class, version-pinned FoundationDB image for `wyrd`,
plus a CI job that builds and smoke-tests it. Reported in #470.

## What to look at
The new image definition at `deploy/docker/wyrd/Dockerfile` and the CI job
at `.github/workflows/fdb-image.yml`. The single most important property is
that the FoundationDB version baked into the image cannot silently disagree
with the cluster the repo deploys or the client the binary is compiled
against — that coupling is enforced by a plain Rust test, no Docker needed.

To try it end to end:

```
docker build --build-arg FEATURES=fdb,etcd -f deploy/docker/wyrd/Dockerfile -t wyrd:fdb .
docker run --rm wyrd:fdb            # prints usage naming the redb|tikv|fdb backends
```

Point it at a matching FoundationDB cluster and `wyrd --metadata-backend fdb`
connects; point it at a mismatched cluster line and it fails fast with a
guided version-mismatch message instead of hanging.

## Root cause
Wyrd chose FoundationDB as its production metadata backend, but the only
Dockerfile building the binary was a test fixture that installed none of the
FoundationDB build or runtime dependencies — so the production artifact an
operator would run simply did not exist. There was also nothing tying the
client version an image would bake to the cluster version the repo deploys,
which is the exact drift the backend's version-skew guard exists to catch.

## Fix
A new parameterized multi-stage `deploy/docker/wyrd/Dockerfile`:

- installs the pinned `foundationdb-clients` package (C headers + `libfdb_c`
  for the FFI build, plus `libclang` for the bindings generator) in the
  build stage, and `libfdb_c` + `fdbcli` in the runtime stage;
- pins `FDB_VERSION` as a single source of truth and records it in OCI
  labels;
- runs as a dedicated non-root user, with `USER` before `ENTRYPOINT`;
- bakes the multi-version external-client directory as *layout only* —
  empty in steady state — and points the backend at it, so an ordinary
  single-version deployment connects on the linked client, and a lockstep
  cluster upgrade becomes an image rebuild that drops in the new client
  library rather than a code change. (Pre-populating it with the image's own
  version made FoundationDB's multi-version client disable the sole external
  client and misreport the cluster as unreachable; leaving it empty avoids
  that.)

`.github/workflows/fdb-image.yml` builds the image and smoke-tests it on
pull requests touching the FoundationDB surface. The Dockerfile is left
parameterized (`ARG FEATURES`) so the TiKV flavor can adopt the same
skeleton later without a rewrite.

## Verification
- **Claim:** the FoundationDB client version baked into the image agrees,
  down to the exact patch, with the cluster the repo deploys, and on
  major.minor with the crate the binary links — so client/cluster drift is
  impossible to merge silently.
  **Checked:** `deploy/docker/wyrd/Dockerfile` pins `FDB_VERSION=7.3.77`,
  cross-checked against `deploy/fdb-single-node/docker-compose.yml:22`
  (`foundationdb/foundationdb:7.3.77`) and `Cargo.toml:108`
  (`features = ["fdb-7_3"]`) by `xtask/tests/fdb_image.rs`.
- **Claim:** the image shape is production-fit — multi-stage, non-root
  before entrypoint, and parameterized.
  **Checked:** `xtask/tests/fdb_image.rs` asserts the `FROM` count, the
  `COPY --from=build`, `USER` ordered before `ENTRYPOINT`, and the
  `ARG FEATURES` / `ARG FDB_VERSION` inputs.
- **Claim:** the CI workflow actually builds and exercises the production
  image, and only fires on relevant changes.
  **Checked:** `xtask/tests/fdb_image.rs` resolves every `docker build -f`
  to an existing Dockerfile (including the new production image), confirms
  each `docker run` uses a tag the workflow built, and checks the pull-
  request path filter covers `crates/metadata-fdb/**` and
  `deploy/docker/wyrd/**`.
- **Claim:** the external-client directory support the empty-directory
  design relies on is present.
  **Checked:** `crates/metadata-fdb/src/lib.rs:1123` sets
  `NetworkOption::ExternalClientDirectory` from
  `WYRD_FDB_EXTERNAL_CLIENT_DIR` (`crates/metadata-fdb/src/lib.rs:398`)
  without disabling the local client.
- **Test:** `xtask/tests/fdb_image.rs` (new) — fails pre-fix (the image,
  workflow, and consistency check do not exist) and passes post-fix
  (4 tests), including a planted-red case that drives the same consistency
  check with a mismatched `FDB_VERSION=7.1.99` and asserts it is rejected,
  proving the coupling is load-bearing rather than resting on file
  existence. It is container-free and needs no network.
- **Manual, off-gate:** the image was built (`FEATURES=fdb,etcd`), its
  no-argument usage banner naming `redb|tikv|fdb` was confirmed on stderr,
  `wyrd --metadata-backend fdb` connected against a matching 7.3.77 cluster,
  and against a 7.1.61 cluster it returned the guided protocol-mismatch
  error in under a second (exit 1) rather than hanging. The CI workflow
  reproduces the build-and-smoke path on every qualifying pull request.

Fixes #470
