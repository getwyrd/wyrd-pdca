# Build notes — issue 470 / wyrd-fdb-oci-image

## What I built

Three new files, disjoint from 439's and 469's write-sets (no `xtask/src/` hunk, no
`deploy/README.md` edit — the cross-wave stale-patch hazard the brief scopes around):

1. `deploy/docker/wyrd/Dockerfile` — a first-class, parameterized multi-stage OCI image
   for the `wyrd` binary. `ARG FEATURES` + `ARG FDB_VERSION`; when `fdb` ∈ `FEATURES` the
   **build** stage installs `clang`/`libclang-dev` (for `foundationdb-sys`' `bindgen`) plus
   the pinned `foundationdb-clients` `.deb` (FDB C headers + `libfdb_c`), and the **runtime**
   stage installs the same `.deb` (`libfdb_c` + `fdbcli` for #439's doctor and operators). It
   bakes an external-client directory (`/var/lib/wyrd/fdb/external-clients`, exported via
   `WYRD_FDB_EXTERNAL_CLIENT_DIR`) so #441's multi-version client support turns a lockstep
   cluster upgrade into an image rebuild, drops root to a non-root `wyrd` user (uid/gid
   10001, mirroring the #286 precedent in the dserver fixture at `:56-64`), and records the
   client/cli version in OCI labels.

2. `.github/workflows/fdb-image.yml` — a PR-triggered container job that actually builds the
   image (`docker build --build-arg FEATURES=fdb,etcd`), smoke-runs it for the usage banner,
   and confirms `fdbcli` is present. Path filter includes `crates/metadata-fdb/**` and
   `deploy/docker/wyrd/**` (plus `Cargo.toml`/`Cargo.lock` and the compose file — the other
   inputs to the version coupling). Not part of `cargo xtask ci` (ADR-0016 keeps that
   container-free); modeled on `integration-nightly.yml` (`docker info`, `timeout-minutes`).

3. `xtask/tests/fdb_image.rs` — the binding, container-free consistency guard. Four `#[test]`s
   proving assertions 1–4. Parsing helpers are **local to the test file** (following
   `xtask/tests/readme_dev_section.rs`), importing nothing from `xtask`'s lib.

## Version pins (the load-bearing coupling)

- `FDB_VERSION=7.3.77` in the Dockerfile — single source of truth.
- Exact patch `7.3.77` == `foundationdb/foundationdb:7.3.77` cluster tag in
  `deploy/fdb-single-node/docker-compose.yml:22`.
- major.minor `7.3` == crate pin `features = ["fdb-7_3"]` at `Cargo.toml:108`.

`check_fdb_version_consistency()` asserts exact-patch Dockerfile↔compose and major.minor
across all three. A single function drives both the real-tree green and the planted-red, so
the check is proven load-bearing, not resting on file-non-existence.

## Why this shape, alternatives ruled out

- **Why not extend the existing `crates/chunkstore-grpc/tests/dserver/Dockerfile`?** It is a
  test fixture inside a crate's `tests/` dir, still built by `cargo xtask integration` and the
  Tier-1 Jepsen leg (its `iptables` install, `:39-48`). Migrating it would collide with #471's
  rename and #469's compose rewiring (both explicitly out of scope). A new file under
  `deploy/docker/wyrd/` is the first-class home #470 asks for and the skeleton #471 adopts.
- **Why parse with substring helpers, not a YAML/Dockerfile crate?** `xtask/Cargo.toml` has no
  such parser, and adding a crate is the ADR-0003 audit + `deny.toml` allowlist — a human-only
  decision (INTEGRATION §4). The assertions are file-reads and string compares, so local
  helpers suffice, exactly as `readme_dev_section.rs` does.
- **Why keep helpers test-local rather than promote to `xtask/src/lib.rs`?** The brief cites
  `xtask/src/lib.rs:1-19` (the `deploy_guard` "pure helper in the lib" precedent) to be
  *rejected* here: 439 (wave 1) writes `xtask/src/lib.rs`, and `C4-verify` resets to
  `origin/main` not the folded tip (`run-verify.sh:_resolve_base_ref`, `:109-113`), so a
  shared-file hunk would apply stale. Test-local keeps this bundle's file set disjoint.

## Test binding — the three forced refutation questions

- **(a) Genuine red?** Yes. With the Dockerfile and workflow removed (fix reverted), 3 of the
  4 tests fail (`dockerfile_is_multistage_nonroot_and_parameterized`,
  `fdb_version_is_consistent_across_dockerfile_compose_and_crate`,
  `workflow_exists_resolves_and_filters_the_fdb_surface`) on `read(...)` of the absent files.
  Verified by moving both files aside and re-running: `1 passed; 3 failed`. Restored after.
  Assertion 4 (`consistency_check_is_red_on_a_mismatched_fdb_version`) is a self-contained
  planted-red: it feeds a temp Dockerfile pinning `FDB_VERSION=7.1.99` to the SAME
  `check_fdb_version_consistency` the green test drives and asserts it returns `Err`. If the
  consistency check were vacuous (e.g. only checked file existence), this test would fail —
  proving the check is load-bearing.
- **(b) Production path?** Yes. The tests read the REAL `deploy/docker/wyrd/Dockerfile`,
  `deploy/fdb-single-node/docker-compose.yml`, `Cargo.toml`, `.github/workflows/fdb-image.yml`
  and `xtask/src/main.rs` from the workspace — the actual production artifacts, not copies.
  The consistency check runs over the real Dockerfile in the green case; the planted-red uses
  the real compose + Cargo.toml with only the Dockerfile version swapped, so only the injected
  drift differs.
- **(c) Fixture includes the fault?** Yes. The planted-red fixture Dockerfile *contains* the
  mismatched `FDB_VERSION=7.1.99` (the injected fault), and the assertion checks the error
  message names `7.1.99` — it does not curate the mismatch out.

## Gate

`cargo xtask ci` (the gating `C4-ci`) via `./engine/xtask.sh ci` → **all checks passed** with
the new test compiled and run in the suite. `cargo fmt -p xtask -- --check` clean; `cargo
clippy -p xtask --tests` clean. Commit-ready for the target's rustfmt/clippy hooks.

## Supplementary live legs (a)–(d) — Docker available here

Environment verified: `docker info` OK, host `fdbcli`/`libfdb_c` **7.3.77**, network egress to
`github.com` and `docker.io` OK.

- **(a) `docker build --build-arg FEATURES=fdb,etcd -f deploy/docker/wyrd/Dockerfile -t
  wyrd:fdb .`** — ATTEMPTED. The build stage installed the pinned `foundationdb-clients`
  7.3.77 `.deb` (headers + `libfdb_c`) and `clang`/`libclang-dev`, then reached
  `cargo build --release --locked --features fdb,etcd` (confirming the `fdb`-feature build the
  old test Dockerfile could not do). Result recorded below (see "Live-leg results").
  (A temporary, UNCOMMITTED `.dockerignore` was not needed — the repo already ships one that
  excludes `target/`; I only confirmed it.)
- **(b) `docker run --rm wyrd:fdb`** (no args) — usage banner assertion on captured **stderr**
  containing `redb|tikv|fdb` (`crates/server/src/cli.rs:268-269`), expecting a non-zero exit.
  Result below.
- **(c) connect against `deploy/fdb-single-node/`** — see caveat below.
- **(d) mismatched-version guided error** (7.1.x cluster) — see caveat below.

### Caveat on legs (c) and (d): #441 is NOT in this worktree

The wave fold was supposed to deliver #441's `preflight` module and
`WYRD_FDB_EXTERNAL_CLIENT_DIR` support. It is **absent** in `$PDCA_WORKTREE`:
`crates/metadata-fdb/src/` contains only `lib.rs`, no `preflight` module, and
`crates/metadata-fdb/src/lib.rs:868` still calls bare `foundationdb::boot()` (which accepts no
network options) — exactly the pre-#441 state the brief describes. The brief's out-of-scope
note is explicit: *"If `preflight` is absent when Do runs, the wave fold has failed — stop and
say so, do not reimplement it here."* I am NOT reimplementing #441.

Consequence: leg (d) (the #441 version-skew *guided* error, which is #441's acceptance
criterion 3) cannot be exercised — without `preflight` a 7.1.x mismatch produces an
anonymous timeout, not #441's guided `protocol`-naming error. Leg (c)'s connect exercises
`boot()` directly (which works for a single matching client), but the external-client
directory the image bakes is inert decoration without #441, so a connect here would not prove
the image's multi-version story. I decline to fabricate a passing result for (c)/(d) against a
tree that lacks the code they verify.

NEEDS-HUMAN external dependency: #441 preflight module (wave fold) — absent from
$PDCA_WORKTREE (crates/metadata-fdb/src has only lib.rs; lib.rs:868 still calls bare
foundationdb::boot()); blocks live legs (c) wyrd --metadata-backend fdb connect proving the
baked external-client dir and (d) the #441 version-skew guided error (441 acceptance
criterion 3 "discharged here"). The binding criterion (cargo test -p xtask --test fdb_image)
does not depend on #441 and is fully green.

### Live-leg results

- **(a) `docker build` — PASS.** Exit 0. Build stage installed `foundationdb-clients` 7.3.77
  (`.deb` from GitHub releases) + `clang`/`libclang-dev`, then `cargo build --release --locked
  --features fdb,etcd` compiled `bindgen` against the FDB C headers and the etcd/tonic tree —
  the exact build the old test Dockerfile could not do. Image `wyrd:fdb`, 309MB.
- **(b) `docker run --rm wyrd:fdb` (no args) — PASS.** Exit **2** (non-zero, as expected).
  Captured **stderr** contains the usage banner with `--metadata-backend redb|tikv|fdb`
  (`crates/server/src/cli.rs:268-269`). Asserted on stderr, not exit status.
- **Runtime-stage evidence (all PASS):**
  - `fdbcli --version` inside the image → `FoundationDB CLI 7.3 (v7.3.77)` — the pinned cli is
    baked for #439's doctor and operators.
  - `ldd /usr/local/bin/wyrd` → `libfdb_c.so => /lib/libfdb_c.so` — the binary dynamically
    links the FDB client at load time (the old runtime stage could not).
  - `/var/lib/wyrd/fdb/external-clients/libfdb_c_7.3.77.so` present — the external-client dir is
    baked and populated (world-readable, so the non-root `wyrd` user reads it).
  - OCI labels: `dev.wyrd.fdb.libfdb_c.version=7.3.77`, `dev.wyrd.fdb.fdbcli.version=7.3.77`,
    plus `org.opencontainers.image.{title,description,source}`.

Legs (a) and (b) do not depend on #441 and passed fully. Legs (c)/(d) are blocked on the
missing #441 fold (see caveat above) and left honestly unverified.

## Manual validation steps for the human (legs c/d, once #441 is folded)

1. Build: `docker build --build-arg FEATURES=fdb,etcd -f deploy/docker/wyrd/Dockerfile -t
   wyrd:fdb .`
2. `docker compose -f deploy/fdb-single-node/docker-compose.yml up -d`; `configure new` per
   that file's header; write the host cluster file.
3. `docker run --rm --network host -e WYRD_FDB_CLUSTER_FILE=... wyrd:fdb --metadata-backend
   fdb <op>` → store opens and answers (note: not an object round-trip; #454/#455 gaps).
4. Point the image at a `foundationdb/foundationdb:7.1.x` cluster and confirm #441's guided
   `protocol`-naming error within a bounded deadline (not an anonymous hang).
