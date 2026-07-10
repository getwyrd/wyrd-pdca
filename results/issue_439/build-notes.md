# Build notes — issue 439 / fdb-dev-ci-harness (iteration 5)

## What this iteration changes vs iteration-v4

Iteration 4 was rejected on ONE seam (assertion 5, the impure client-library adapter). The
brief's design and the other four assertions survived every adversarial pass — the
carry-forward is explicit: *"do NOT re-plan … the harness is real and worth keeping — do not
churn it."* So the base of this patch is **iteration-v4/patch.diff applied verbatim**, with
exactly two focused deltas:

### Delta 1 — the assertion-5 guard is now BEHAVIOURAL, not substring (the rejection)

The adversary's landed refutation: the two structural body-assertions
(`the_client_library_adapter_supplies_the_real_environment_read`,
`the_conformance_preflight_is_handed_the_real_measured_probes`) pinned that the adapter body
*contains the text* `std::env::var` / `Path::new(` / `.exists()`. Two semantic-equivalent
mutations keep those tokens present yet reintroduce iteration-1's false-negative:

- `&|name| std::env::var(name).ok()` → `&|name| { let _ = std::env::var(name); None }`
  (env read discarded → working `FDB_CLIENT_LIB_PATH` build reported "missing" → false-skip
  locally / hard-fail in CI).
- `&|candidate| Path::new(candidate).exists()` → `&|candidate| { let _ = candidate; true }`
  (library always "present" → preflight proceeds into the container stack with no libfdb_c →
  linker error minutes in — the very thing the preflight exists to prevent).

The carry-forward's instruction: *"close the DEFECT CLASS, not the two literal mutation
spellings … drive the real adapter against a fake environment/filesystem."*

**Fix:**
1. Moved the client-library live wiring out of `main.rs` (a private binary fn the integration
   test cannot link) into the lib as `xtask::fdb_doctor::probe_client_library_live()`
   (`xtask/src/fdb_doctor.rs`, appended after `cluster_status_is_healthy`). It is a total,
   logic-free pass-through: the real `std::env::var` / `std::path::Path::exists` / `ldconfig -p`
   effects into the already-unit-tested pure `probe_client_library`. `main.rs`'s
   `run_fdb_conformance` (`:315`) and `run_fdb_doctor` (`:355`) now call it; the local
   `fn probe_client_library()` is deleted.
2. Replaced the substring test with `the_live_client_library_adapter_reads_the_real_env_and_filesystem`
   (`xtask/tests/fdb_harness.rs`), which drives the **production** `probe_client_library_live`
   end-to-end against a real `FDB_CLIENT_LIB_PATH` and a real temp file, binding on the
   **observed resolved path**:
   - **(A) env read is real:** `FDB_CLIENT_LIB_PATH` → a tempdir holding `libfdb_c.so`; the
     outcome must resolve THAT copy (`detail.contains(planted_path)`). Discarding the env read
     resolves a standard path or reports missing → RED.
   - **(B) existence check is real:** `FDB_CLIENT_LIB_PATH` → an EMPTY tempdir; the outcome must
     NOT be an `Ok` naming the (absent) configured file. Hard-coding existence to `true`
     resolves it → RED.

   **Host-independence** was the design constraint: the configured dir is searched FIRST and
   short-circuits, and the test controls whether the file under it exists, so neither assertion
   depends on whether the host has a system `libfdb_c`. This host does
   (`/usr/lib/libfdb_c.so`); the plain verify worktree does not — the test is green on both and
   red under each mutation on both. Assertion (B) binds on `!(passed && detail.contains(absent))`
   rather than pass/fail precisely because the *failure* message enumerates candidate paths
   (which include the configured one) — a naive `!detail.contains(absent)` would wrongly go red
   on the verify host. Verified by reading the `Outcome::failed` format in `fdb_doctor.rs`.

The third mutation the brief names (hard-coding `run_fdb_conformance`'s preflight args to a
passing `Outcome`) is still guarded structurally by
`the_conformance_preflight_is_handed_the_real_measured_probes` — updated to look for
`probe_client_library_live()`. That mutation is on a *call-site*, not effect-wiring; a
behavioural test of `run_fdb_conformance` would need Docker + a real container, so structural is
the honest headless option there and it is genuinely load-bearing (it asserts the three real
measurements are passed and no `Outcome::{ok,Ok,failed,Failed}` literal stands in).

### Delta 2 — HEALTH_COMMAND advisory text aligned (the minor fold-in)

`fdb_doctor.rs` `HEALTH_COMMAND` was `fdbcli --exec "status minimal"`, but the real probe
(`probe_cluster_health`, `main.rs`) runs `fdbcli -C <cluster file> --exec "status minimal"`.
Aligned the constant to `fdbcli -C <cluster file> --exec "status minimal"` so the command an
operator copies from the remediation matches the one the doctor actually ran. Cosmetic
(advisory string only); no test asserted the old literal.

## Refute-my-own-test (forced, recorded)

- **(a) Genuine red?** YES, two ways. (i) Reverting the WHOLE patch removes
  `xtask/src/fdb_doctor.rs` and the workflow, so `xtask/tests/fdb_harness.rs` fails to compile
  (module + `probe_client_library_live` unresolved) → red. (ii) The specific new behavioural
  test: I reverted each named mutation in `probe_client_library_live` and re-ran —
  `env → None` failed with `Ok("found at /usr/lib/libfdb_c.so")` (scenario A), and
  `exists → true` failed with `Ok("found at /tmp/…-absent/libfdb_c.so")` (scenario B). Both
  reverted; suite green again.
- **(b) Production path?** YES. The test imports and calls
  `xtask::fdb_doctor::probe_client_library_live` — the exact function `main.rs`'s
  `run_fdb_conformance` / `run_fdb_doctor` invoke. No copy, no mock; the fake is only the
  *environment* (a real env var + a real temp file), not the code under test.
- **(c) Fixture includes the fault?** YES. Scenario A plants a real `libfdb_c.so` under the
  configured dir and asserts it is resolved (the env read is the fault-carrying element);
  scenario B leaves the configured dir empty and asserts it is NOT resolved (the existence
  check is the fault-carrying element). The killed elements (a discarded read, a hard-coded
  `true`) are exactly what each scenario exercises.

## Live evidence (supplementary leg — the brief requires it, deps all present here)

`docker info` OK; `/usr/lib/libfdb_c.so` and `/lib/libfdb_c.so` = 7.3.77; `fdbcli` v7.3.77.

- `cargo run -p xtask -- fdb-doctor` → client library `ok` (found at `/usr/lib/libfdb_c.so`),
  cluster file + health `FAIL` (no cluster up), exit 1. The health remediation now prints the
  aligned `fdbcli -C <cluster file> --exec "status minimal"`.
- `cargo run -p xtask -- fdb-conformance` → **EXIT 0**. Built `wyrd-metadata-fdb --features fdb`
  (linked the real `libfdb_c`), brought up `deploy/fdb-single-node/` compose, configured the
  db, wrote the host cluster file, ran the conformance + contention + scan + timeout legs (all
  green), tore the stack down. This exercises the live FDB topology the earlier iterations'
  NEEDS-HUMAN T3 could not (Docker was denied then; it is available in this sandbox).

## Verification run (red→green)

`cargo test -p xtask` (via cargo in `$PDCA_WORKTREE`, bounded by `timeout`): lib 0, main 19,
`fdb_harness` **28 passed**, plus the other integration suites — all green. `cargo fmt --check
-p xtask` clean; `cargo clippy -p xtask --all-targets` clean (workspace `warnings = "deny"`).
Patch re-verified to `git apply --check` cleanly against the base `b1ccca3`.

## Alternatives considered / ruled out

- **Keep the structural body-assertion, add more forbidden substrings** (e.g. reject
  `let _ =`). Rejected: it is a spelling blacklist, not a defect-class guard — the adversary's
  whole point was that a *semantic-equivalent* rewrite evades any finite token check. Cost of
  the behavioural test is ~55 lines in the test file + a ~15-line lib fn (moved, not net-new
  logic); it closes the class rather than one more spelling.
- **Test the live adapter for the "missing" case via pass/fail** (`!outcome.passed()` on an
  empty dir). Rejected: not host-independent — this host resolves `/usr/lib/libfdb_c.so` and
  passes even with an empty configured dir. Binding on the resolved *path* instead makes both
  scenarios deterministic on a machine with OR without a system client.
- **Put the live wiring in `lib.rs` top-level instead of `fdb_doctor.rs`.** Either is in the
  brief's write-set; chose `fdb_doctor.rs` so all FDB-doctor wiring is one module and the test's
  existing `use xtask::fdb_doctor::…` import extends naturally. Updated the module/ test docs to
  note the one impure fn now lives there (the pure decision core is unchanged).

## NEEDS-HUMAN (carried forward, for §6 at sign-off)

The live legs above were runnable in THIS sandbox, but the brief's own posture keeps these
off-Check for the maintainer to confirm on a hosted runner:

- **Hosted workflow green (deferred, brief "Verification posture").** `.github/workflows/fdb-conformance.yml`
  cannot be observed green from `cargo xtask ci`; the maintainer confirms it on the first
  nightly / on the landing PR. Every command it references is proven to exist and to be a real
  dispatched subcommand by `the_fdb_conformance_workflow_runs_only_real_dispatched_subcommands`.
- **T4 prior-art** (closed/rejected PR state not mechanically settled) and **fitness-to-purpose**
  (container-free Check evidence before the first hosted FDB workflow green) — unchanged from
  prior iterations; human adjudicates at sign-off.

No missing external dependency blocked this build — Docker, libfdb_c 7.3.77 and fdbcli 7.3.77
were all present, so the supplementary live leg ran for real rather than being substituted with
a code-read.
</content>
</invoke>
