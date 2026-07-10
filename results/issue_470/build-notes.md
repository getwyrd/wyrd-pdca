# Build notes — issue 470 / wyrd-fdb-oci-image (iteration 2)

## Context: what iteration 1 left open, and what changed

v1 built the binding gate (4 file-parse assertions) green and ran legs (a)/(b), but legs
(c) `wyrd --metadata-backend fdb` connects and (d) the #441 version-skew guided error were
**blocked**: #441's `preflight` module was absent from the wave fold, so the baked
external-client directory was inert. **#441 has since merged (PR #495)** and the worktree base
is now `f23848d` (`Merge pull request #495 …`), which carries `crates/metadata-fdb/src/lib.rs`'s
`preflight` module (`:832`), `WYRD_FDB_EXTERNAL_CLIENT_DIR` (`:398`), and the
`ExternalClientDirectory` boot wiring (`:1119-1128`). This iteration:

1. **Re-ran legs (c) and (d) live** — and in doing so found and fixed a **real image defect**
   the file-parse gate cannot see (see "The leg-(c) defect" below).
2. Applied the three carry-forward corrections (workflow backtick, the runtime `libfdb_c`
   handling, the vacuous `cargo xtask` half of the workflow↔dispatch check).

## Files (3 new, disjoint from #439's and #469's write-sets)

- `deploy/docker/wyrd/Dockerfile` — first-class, parameterized multi-stage OCI image.
- `.github/workflows/fdb-image.yml` — PR-triggered container build/smoke job.
- `xtask/tests/fdb_image.rs` — the binding, container-free consistency guard (4 tests).

No `xtask/src/` hunk, no `deploy/README.md` edit — the cross-wave `C4-verify` stale-patch
hazard the brief scopes around. Patch verified to apply cleanly on a fresh `origin/main`
worktree (`git apply --check`, all three files).

## The leg-(c) defect — root cause, and why the fix removes the cause not a symptom

v1's Dockerfile **populated** the baked external-client directory with a copy of the pinned
client (`cp …/libfdb_c.so …/libfdb_c_${FDB_VERSION}.so`) AND set
`ENV WYRD_FDB_EXTERNAL_CLIENT_DIR` at it. v1 never exercised this (no #441). With #441 present,
leg (c) reproduced a hard failure: `wyrd --metadata-backend fdb` reported the cluster
**unreachable in ~2 ms** (and intermittently segfaulted, exit 139) against a *healthy, matching*
7.3.77 cluster.

Root cause, confirmed from the FDB client trace (`FDB_NETWORK_OPTION_TRACE_ENABLE`), not
inferred:

```
Type="DuplicateClientVersion" Keeping="internal" KeptProtocolVersion="0x0FDB00B073000000"
  Disabling="/var/lib/wyrd/fdb/external-clients/libfdb_c_7.3.77.so"
  DisabledProtocolVersion="0x0FDB00B073000000"
```

FoundationDB's multi-version client rejects an external client whose protocol version equals
the linked ("internal") client's, and **disables it**. With the sole external client disabled
and the local client fronting an otherwise-empty multi-version set, `get_client_status()` fails
immediately, so #441's preflight (correctly) reports `Unreachable`. This is worse than the
"inert decoration" the brief warns about — it *actively breaks* the image's basic connect.

**The fix removes the cause:** the directory is baked as **layout only, empty in steady state**
(the deployment-view contract, `docs/design/architecture/07-deployment-view.md:117-125`: the
linked primary IS the single steady-state version; the external dir supplies a *different*
version only during a lockstep upgrade — "two client libraries" = the linked one plus one
dropped in the dir). #441 sets `ExternalClientDirectory` *without* `DisableLocalClient`
(`lib.rs:1119-1128`), so the only configuration that both connects today and bridges an upgrade
is: linked primary + external dir holding **non-primary** versions (empty when there is one
version). Verified: empty dir + env set → connects identically to env-unset.

Why not the carry-forward's narrower ask (harden the `cp` against a multiarch
`/usr/lib/<triple>/` path)? That hardens a step that should not exist — a green
`find`-resolved `cp` would still bake a same-version client and still break the connect. The
multiarch concern is deferred to the future upgrade `COPY` (a different version), which the
Dockerfile comment flags. Removing the populate step is −4 lines vs +2 lines to keep-and-harden
it, and it is the difference between an image that connects and one that does not.

## Carry-forward corrections

1. **Workflow backtick bug** (`fdb-image.yml`, v1 `:78`). The failure-branch diagnostic used
   ``echo "expected \`wyrd\` …"`` — back-ticks inside a double-quoted string run command
   substitution on `wyrd`. Now `echo 'expected wyrd with no subcommand to exit non-zero'`
   (single-quoted; prints the literal word). Guard outcome was always intact; the message is now
   correct.
2. **Runtime `libfdb_c` handling.** Superseded by the root-cause fix above: there is no runtime
   `cp` to harden — the populate step is gone. The build no longer depends on the deb's exact
   `libfdb_c.so` leaf path at all in the runtime stage.
3. **Vacuous `cargo xtask` half of the workflow↔dispatch check** (`fdb_image.rs`). This workflow
   is a container job driving `docker` directly (no `cargo xtask fdb-image` subcommand exists,
   and this bundle adds none — `xtask/src/` is 439's write-set), so the `cargo_xtask_subs` loop
   iterated an empty set. Replaced with two **binding** `docker`-resolution checks (brief item 3
   reads "every `cargo xtask <sub>` **or** `docker` invocation … resolves"):
   - every `docker build -f <path>` names a Dockerfile that **exists**, and one is the new
     production image; and
   - every `docker run … <image>` runs a tag the workflow actually **built** (`-t`), so the job
     cannot smoke-test an image it never produced. This half is non-vacuous: I confirmed a red
     when a stray "docker build" token (the step *name*) produced a `-f`-less build — I renamed
     the step to `Build the wyrd:fdb image` so the parser scans only real invocations.

## Version pins (the load-bearing coupling — unchanged, still asserted)

- `FDB_VERSION=7.3.77` (Dockerfile) — single source of truth.
- exact patch `7.3.77` == `foundationdb/foundationdb:7.3.77` (`deploy/fdb-single-node/docker-compose.yml:22`).
- major.minor `7.3` == crate pin `features = ["fdb-7_3"]` (`Cargo.toml:108`).

`check_fdb_version_consistency()` drives both the real-tree green and the planted-red, so the
check is load-bearing, not resting on file non-existence.

## Test binding — the three forced refutation questions

- **(a) Genuine red?** Yes. Reverted (moved `Dockerfile` + workflow aside) and re-ran:
  `1 passed; 3 failed` — the three file-reading tests panic on the absent files; restored after.
  Assertion 4 (`consistency_check_is_red_on_a_mismatched_fdb_version`) is a self-contained
  planted-red feeding `FDB_VERSION=7.1.99` to the SAME `check_fdb_version_consistency` the green
  test uses and asserting `Err` names `7.1.99` — proving the check is load-bearing. I also
  observed the new item-3 docker-resolution check go red on the stray step-name token before
  renaming the step.
- **(b) Production path?** Yes. The tests read the REAL `deploy/docker/wyrd/Dockerfile`,
  `deploy/fdb-single-node/docker-compose.yml`, `Cargo.toml`, `.github/workflows/fdb-image.yml`
  from the workspace — the actual artifacts, not copies. The consistency check runs over the
  real Dockerfile; the planted-red swaps only the Dockerfile version, keeping real compose +
  Cargo.toml.
- **(c) Fixture includes the fault?** Yes. The planted-red fixture Dockerfile *contains* the
  mismatched `FDB_VERSION=7.1.99` and the assertion checks the error names it — the mismatch is
  not curated out. And leg (d)'s live fixture is a *real* 7.1.61 cluster the 7.3.77 client cannot
  speak to — the fault is injected, not excluded.

## Gates

- Binding: `cargo test -p xtask --test fdb_image` → **4 passed**.
- `cargo xtask ci` (C4-ci) via `./engine/xtask.sh ci` → **all checks passed** (with the new test
  compiled and run).
- `cargo fmt -p xtask -- --check` clean; `cargo clippy -p xtask --tests` clean. Commit-ready for
  the target's rustfmt/clippy hooks.

## Supplementary live legs (a)–(d) — ALL PASS on this host (#441 now present)

Host: `docker info` OK, `fdbcli`/`libfdb_c` 7.3.77, network egress OK.

- **(a) `docker build --build-arg FEATURES=fdb,etcd -f deploy/docker/wyrd/Dockerfile -t
  wyrd:fdb .` — PASS (exit 0).** Build stage installed `foundationdb-clients` 7.3.77 (.deb from
  GitHub releases) + `clang`/`libclang-dev`, then `cargo build --release --locked --features
  fdb,etcd` compiled `bindgen` against the FDB C headers and the etcd/tonic tree — the build the
  old test Dockerfile could not do (`release … in 2m 05s`).
  Runtime-stage evidence: `fdbcli --version` → `FoundationDB CLI 7.3 (v7.3.77)`; `ldd
  /usr/local/bin/wyrd` → `libfdb_c.so => /lib/libfdb_c.so`; `/var/lib/wyrd/fdb/external-clients`
  present and **empty** (the fixed layout); OCI labels `dev.wyrd.fdb.libfdb_c.version=7.3.77`,
  `dev.wyrd.fdb.fdbcli.version=7.3.77`.
- **(b) `docker run --rm wyrd:fdb` (no args) — PASS.** Exit **2** (non-zero, expected). Captured
  **stderr** contains the usage banner with `--metadata-backend redb|tikv|fdb`
  (`crates/server/src/cli.rs:277-278`). Asserted on stderr, not exit status.
- **(c) `wyrd --metadata-backend fdb` connects against `deploy/fdb-single-node/` (7.3.77) — PASS.**
  Brought the compose cluster up, `configure new single memory`, mounted the host cluster file,
  ran the image with `--network host`: `wyrd get nonexistent-key --metadata-backend fdb` →
  `wyrd: key \`nonexistent-key\` not found`, exit 1 (**the store opened and answered**;
  "connected, key absent"). Deterministic exit 1 over repeated trials once the transient
  first-run FDB teardown race passed. This is the fix's payoff: with v1's populated dir it was an
  immediate `Unreachable`/segfault. (Per the brief/#454/#455, "connects" = the store opens and
  answers, not a full object round-trip.)
- **(d) mismatched-version guided error (7.1.61 cluster) — PASS, and discharges #441 acceptance
  criterion 3.** Stood up `foundationdb/foundationdb:7.1.61` (host net, `configure new`), ran the
  7.3.77 `wyrd:fdb` image against it:
  - **exit 1**, **elapsed 0.30 s** — bounded, NOT an anonymous timeout, NOT an indefinite hang.
  - message: `FoundationDB metadata store: client/cluster protocol version mismatch — this
    client is api 730 (fdb-7_3 pin), the cluster reports protocol version fdb00b071010000. …
    load the cluster's libfdb_c into a multi-version external-client directory and point
    WYRD_FDB_EXTERNAL_CLIENT_DIR at it … see the multi-version client upgrade procedure in
    docs/design/architecture/07-deployment-view.md.` — #441's guided error, naming the client
    line and the cluster's protocol version, exactly the deployment-view §7.6 observation
    (~200 ms, exit 1, `fdb00b071…`).

All external test clusters torn down after the run.

## Manual re-validation steps for the human (if desired at sign-off)

1. `docker build --build-arg FEATURES=fdb,etcd -f deploy/docker/wyrd/Dockerfile -t wyrd:fdb .`
2. `docker compose -f deploy/fdb-single-node/docker-compose.yml up -d`; `docker compose … exec
   fdb fdbcli --exec "configure new single memory"`; write `docker:docker@127.0.0.1:4500` to a
   host cluster file.
3. `docker run --rm --network host -e WYRD_FDB_CLUSTER_FILE=/f -v <file>:/f:ro wyrd:fdb get k
   --metadata-backend fdb` → "key not found" (connected).
4. Repeat step 3 against a `foundationdb/foundationdb:7.1.61` cluster → the guided
   `client/cluster protocol version mismatch` error in well under a second.
