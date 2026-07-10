# CI conformance signal and preflight doctor for the FoundationDB backend

## Summary
**User impact:** FoundationDB is the metadata backend we intend to run in
production, yet no automated check ever exercised it — a change could quietly
break it and nothing would flag it until someone hit the failure by hand. And a
developer building the backend without the FoundationDB client library installed,
or without a cluster running, got a raw linker error or a hung, timing-out
command instead of any hint about what was missing.

This adds a scheduled-and-on-PR check that actually runs the backend against a
throwaway single-node cluster, and a `fdb-doctor` command that inspects the local
setup and tells you exactly what to install or start when something is missing —
all without making the everyday build-and-test gate need Docker or the client
library.

Reported in #439.

## What to look at
- **The new check:** `.github/workflows/fdb-conformance.yml` — it installs the
  FoundationDB client, type-checks the backend, and runs `cargo xtask
  fdb-conformance` on pull requests that touch the backend and every night. It is
  deliberately *not* a required merge gate; the normal gate stays fast and
  container-free.
- **The doctor:** run `cargo xtask fdb-doctor`. On a machine with nothing set up
  it prints one line per prerequisite (client library, cluster file, cluster
  health) and, for each failure, the concrete package, environment variable, or
  command that fixes it. The conformance job runs this same preflight first, so a
  missing client library is a clear message rather than a link error minutes into
  a container run.
- To exercise it end-to-end with Docker present: `cargo xtask fdb-conformance`
  brings the cluster up, runs the suite, and tears it down.

## Root cause
The conformance driver landed with the backend itself
(`22d39b6035573d9b999a95430e62ae68a859bd29`) but was never invoked by any job, so
the backend had zero standing coverage. Separately, the `#[cfg(feature = "fdb")]`
code is compiled by no default build, and the backend links a system library
(`libfdb_c`) at build time — so a missing prerequisite surfaced as a linker error
or a transaction timeout rather than an actionable message.

## Fix
- A new workflow runs the conformance driver against the single-node cluster on
  the backend's paths and nightly, and type-checks the feature arms that no
  default build compiles.
- A pure `fdb_doctor` module maps each probe result to a verdict plus a
  remediation string; `cargo xtask fdb-doctor` renders it, and
  `run_fdb_conformance` uses the same client-library probe as its preflight so
  the job fails fast with guidance.
- The whole-tree gate gains opt-in feature type-checks, each behind its own
  toolchain flag so the FoundationDB check never couples to the unrelated TiKV
  one, and the merge gate stays green offline and container-free.
- `deny.toml`'s header records what dependency scanning can and cannot see for a
  crate that binds a system library.

## Verification
- **Claim:** the FoundationDB backend has a standing automated signal that runs
  the conformance suite.
  - **Checked:** `.github/workflows/fdb-conformance.yml:134` — the job runs
    `cargo xtask fdb-conformance` on the backend's PR paths and on a nightly cron,
    and its command heads are proven against the real subcommand table by
    `xtask/tests/fdb_harness.rs:873`, so a typo'd or merely-mentioned command
    fails the test rather than shipping.
- **Claim:** a missing prerequisite yields an actionable message, decided by
  logic that is testable with no FoundationDB present.
  - **Checked:** `xtask/src/main.rs:309` — `run_fdb_conformance` delegates to the
    injected preflight before any container starts; the pure verdict logic lives
    in `xtask/src/fdb_doctor.rs` and is driven directly by the harness, including
    a planted-failure case at `xtask/tests/fdb_harness.rs:305` that proves the
    row logic reports red on a real fault rather than resting on absence.
  - **Checked:** `xtask/tests/fdb_harness.rs:533` — drives the real client-library
    adapter against a controlled environment and filesystem, so a discarded
    environment read or a hard-coded "present" result flips it red (it cannot
    silently report a working custom-prefix build as missing).
- **Claim:** the feature arms no default build compiles are type-checked, without
  the merge gate needing the system library.
  - **Checked:** `xtask/src/lib.rs:84` — the gate emits both feature type-checks
    only when the FoundationDB toolchain flag is set, independently of the TiKV
    flag; the workflow sets that flag on the one runner that installs the client.
- **Test:** `xtask/tests/fdb_harness.rs` — fails pre-fix (the module, workflow,
  and extended gate do not exist) and passes post-fix; `cargo xtask ci` stays
  green. The workflow's execution on a hosted runner and the Docker-backed live
  cluster run are confirmed by a maintainer on the first nightly / landing run,
  as they cannot be observed from the offline gate.

Fixes #439
