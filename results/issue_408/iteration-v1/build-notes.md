# Build notes — issue 408 / m4-checked-consistency-run-elle-report

## Base

Built on `origin/enhancement/407-m4-metadata-nemesis-partition-skew-pause` @ `003fa3a`
("docs(407): spell the nemesis lifecycle words whole") — the folded tip of #407, which
this bundle depends on per the brief's Ordering note. The `$PDCA_WORKTREE` had been reset
to plain `origin/main` (`8bc86ee`, one commit BEHIND 407's merge-base) when I started;
`origin/main` does not contain #407 at all. I checked the worktree out onto the 407 branch
tip before making any change (`git checkout origin/enhancement/407-…`), so `patch.diff` is a
clean diff against that tip, not against a stale `main` that would silently drop 407's
`nemesis.rs` seam this bundle depends on. `patch.diff` was generated with `git diff --cached`
after `git add -A` from that base.

## What I built (mapped to the brief's Success criterion)

- **(a) drives #406 + serializes via landed serializers**: `crates/server/tests/
  consistency_run_fdb.rs`, `#[ignore]`d + `fdb`-feature-gated + env-gated (clean-skip absent
  `WYRD_FDB_CLUSTER_FILE`, mirrors `crates/metadata-fdb/tests/tier1_metadata_nemesis.rs`'s
  shape). Hosts the production server+gateway on loopback (the `s3_http_wire.rs` composition),
  backed by the real `FdbMetadataStore::open(...).with_prefix(...)`, drives
  `ObservableS3Client` register PUT/GET and directory create/delete/probe, merges via
  `MultiProcessHistory::merge` and serializes via `to_elle_edn`/`DirectoryHistory::to_elle_edn`
  — the landed #406 code, called, never re-derived.
- **(b) pinned checker contract, invocation lives in the runner, never in `cargo xtask ci`**:
  `xtask::consistency_run::elle_invocation` (pure, builds the `-jar $JAR --model <m> <edn>`
  argv) + `xtask/src/consistency_run_runner.rs::run_checker` (impure, actually shells `java`).
  `consistency_run_runner` is a **private** `mod` of the `xtask` **binary** crate (wired only
  from `main.rs`), never `pub` from `xtask/src/lib.rs`, and `xtask::consistency_run` itself
  never shells anything — so `cargo xtask ci` (which only runs `cargo test --workspace` on the
  default feature set) cannot reach the JVM invocation.
- **(c) summary-based inconclusive gate**: `xtask::consistency_run::{RunSummary,
  evaluate_summary, InconclusiveReason}` — refuses a verdict unless the summary attests BOTH
  `genuinely_concurrent` (#406 INV-2) and `nemesis_materialized` (#407). Wired into the runner
  (`run_consistency_check`) BEFORE any elle-cli invocation.
- **(d) report renderer, five fields**: `xtask::consistency_run::render_report` (pure) +
  `consistency_run_runner::write_report` (writes `target/consistency-run/report.md`).
- **(e) committed golden fixtures + parser/vocabulary pin + off-Check self-check**:
  `xtask/tests/fixtures/consistency-run/{register-history-known-{good,bad}.edn,
  checker-output-{pass,fail,error}.txt}`, pinned by `edn_history_has_expected_vocabulary` and
  `parse_checker_output` in the named test. `self_check_matches` (pure) is reused UNCHANGED by
  `consistency_run_runner::fixtures_self_check` (impure, off-Check) to feed the same fixtures
  through the real `elle-cli` and compare — the two-tier design Design §4 asks for.

The named Check-time test, `xtask/tests/consistency_run_orchestration.rs`, exercises all of
the above through `xtask::consistency_run`, which is `pub mod`-wired from `xtask/src/lib.rs`
(mirrors the `metadata_faults` / `nemesis` precedent exactly) and pulls in nothing beyond
`std` + `serde`/`serde_json` (already `xtask`'s only deps, `xtask/Cargo.toml:11-14`
unchanged) — no `wyrd-server`, no FDB, no Docker, no Java in its build graph.

## Design choices and what I ruled out

**Reused `xtask::nemesis::NemesisLegKind` for leg selection rather than a second enum.** The
brief's Ordering note forbids reopening #407's nemesis *lifecycle* logic, but leg
*enumeration* is a value type, not lifecycle logic, and #407's own `xtask/src/nemesis.rs`
already established the "xtask owns a mirrored, pure leg-kind enum; the fault-conformance
crate owns the lifecycle trait + impls" split for its own dispatch. Importing the existing
enum (`xtask::consistency_run::selected_leg -> NemesisLegKind`) is strictly less code than
a parallel `ConsistencyNemesisLeg` enum with the same three variants, and it cannot drift
from #407's own set of legs. Cost of the rejected alternative (a second enum): ~40 lines
(3 variants + `as_str` + a second `metadata_nemesis_legs()`-shaped constant) that duplicate,
not extend, #407's own type.

**Directory-as-set live driving reuses `ObservableS3Client::{put,get,delete}` reinterpreted,
rather than adding a new directory-specific wire client.** Over the S3 floor this repo
exposes (PUT/GET/DELETE only, no wire `LIST`), `consistency_workload.rs`'s own module doc
(`crates/server/src/consistency_workload.rs:18-20`) states create=PUT / delete=DELETE /
membership=GET-probe — i.e. the directory ops ARE the same three wire calls the register
client already drives and already has a real, tested round trip for. So
`drive_checked_workload` (in the new scenario file) runs a second `ObservableS3Client` under
a `dir/` key prefix and maps its recorded `OpKind::{Put,Delete,Get}` history entries onto
`DirOpKind::{Create,Delete,Probe}` `DirRecord`s (`DirRecord`'s fields are `pub`, meant to be
built directly — see its existing hand-built use in `consistency_workload.rs`'s own crafted
tests). The alternative — writing a *new* directory-specific wire client that duplicates
`ObservableS3Client::send`/`object_path`/SigV4 signing (~90 lines,
`consistency_observable.rs:230-266`) for a floor that is byte-for-byte the same requests —
was rejected: it would be new, untested wire-calling code duplicating exactly what
`ObservableS3Client` already does and #405 already tests, for zero behavioural difference.

**The run summary is hand-assembled JSON in the scenario, not a shared `serde::Serialize`
struct with `xtask::consistency_run::RunSummary`.** The Design §1/Verification-posture
constraint is explicit: the module the named xtask test imports "must not import
`wyrd-server`". Since `RunSummary` lives in `xtask::consistency_run` (so the xtask-side
`parse_run_summary`/`evaluate_summary` can be Check-tested without pulling in `wyrd-server`),
and the scenario lives in `crates/server/tests/` (so IT must not gain an `xtask` dependency
either — that direction would be backwards, `xtask` is the orchestrator), the two sides
cannot share one Rust type without violating one direction or the other of that split. A
hand-formatted JSON literal with the exact field names `RunSummary` deserializes
(`workload`, `nemesis_leg`, `genuinely_concurrent`, `nemesis_materialized`, `register_ops`,
`directory_ops`) is the seam Design §2 names ("the summary is the seam... which is exactly
what makes the gate arithmetic testable at Check without xtask importing wyrd-server") —
this is not an oversight, it is the design's own consequence, made explicit in both files'
doc comments so a field-name drift is diagnosable (a malformed/missing-field summary is a
hard `parse_run_summary` error, tested in the named suite, never a silently-defaulted
"conclusive" summary).

**`consistency_run_runner.rs` duplicates ~50 lines of `docker compose`/`configure_database`
glue from `fdb_faults.rs` rather than sharing it.** Every existing Tier-1 runner in this repo
(`faults.rs`, `fdb_faults.rs`) owns its own private bring-up helpers — there is no
established shared-bring-up module to extend, and the alternative (exposing
`fdb_faults::{compose, configure_database, write_cluster_file, docker_available,
container_name_of}` as `pub(crate)` so `consistency_run_runner` could call them) would couple
two independently-evolving privileged runners through shared mutable-looking free functions
closed over `fdb_faults`'s OWN project/compose consts — a `consistency-run` compose-project
bug could then silently corrupt `fdb-metadata-tier1`'s behaviour via a shared helper. A
distinct compose project (`wyrd-consistency-run`, vs `wyrd-fdb-tier1-metadata` /
`wyrd-metadata-nemesis`) and a private, self-contained bring-up mirrors the existing
per-runner-file convention and keeps the three privileged runners independently reasoned
about, at the cost of ~50 duplicated lines (the same order of magnitude
`fdb_faults.rs`'s own `run_fdb_metadata_tier1` vs `run_metadata_nemesis` ALREADY duplicate
inside that one file — `compose`/`configure_database`/`build_fault_agent` are shared
functions there because they are the SAME runner's two entry points, not two different
privileged campaigns).

## What I verified, and how (the project's own runner)

- `cargo test -p xtask --test consistency_run_orchestration` — 23/23 green.
- **Genuine RED, checked by hand**: moved `xtask/src/consistency_run.rs` aside and reverted
  the `pub mod consistency_run;` line in `xtask/src/lib.rs`, keeping the test file — compile
  error `E0432: unresolved import xtask::consistency_run`. Restored both; re-ran green
  (23/23). Recorded verbatim in the tool transcript this cycle.
- `cargo test -p xtask` (whole crate, lib + every `xtask/tests/*` file) — all green, nothing
  regressed (`fdb_faults`/`metadata_faults`/`nemesis_orchestration`/`fdb_harness`/`fdb_image`
  suites unaffected).
- `cargo clippy -p xtask --all-targets` — clean.
- `cargo fmt --all -- --check` — clean (ran `cargo fmt --all` to apply, then re-checked).
- **The live scenario ACTUALLY type-checks and clippy-checks under the real `fdb` feature** —
  this Do environment happens to have `libfdb_c` loadable + the FDB headers (confirmed via the
  harness's own doctor probes) and Docker, though not Java: `WYRD_FDB_TOOLCHAIN=1 cargo check
  -p wyrd-server --features fdb --tests` and the `clippy` equivalent both ran clean against
  `crates/server/tests/consistency_run_fdb.rs` — this is real evidence the scenario compiles
  against the real `FdbMetadataStore` / `wyrd_metadata_fault_conformance::nemesis` /
  `consistency_workload` types, not merely against a stub.
- `cargo test -p wyrd-server --test consistency_run_fdb` (default features, no
  `WYRD_FDB_CLUSTER_FILE`) — reports the single test `ignored` (the outer `#[test]` attribute
  is unconditional, per the test-graph convention this file's siblings use).
  `WYRD_FDB_TOOLCHAIN=1 cargo test -p wyrd-server --features fdb --test consistency_run_fdb --
  --ignored --nocapture` with `WYRD_FDB_CLUSTER_FILE` unset — clean-skip path confirmed
  (`WYRD_FDB_CLUSTER_FILE not set — skipping`, exit 0), never a hang or a false pass.
- `cargo test --workspace` (default features, whole tree) — all green, nothing else broke
  from the `crates/server/Cargo.toml` dev-dependency addition or the `xtask/src/lib.rs`/
  `main.rs` edits.
- `cargo clippy --workspace --all-targets` (default features) — clean.
- **The project's actual gate, run end-to-end**: `cargo xtask ci` (fmt + clippy -D warnings +
  build + `cargo test --workspace` + cargo-deny + conformance + statics + deploy-guard + DST)
  — `xtask ci: all checks passed`, and the named test's 23/23 green run is inside that
  transcript (`consistency_run_orchestration.rs ... test result: ok. 23 passed`).
- `xtask consistency-run` (not opted in) — prints the DEFERRED message, exits 0.
- `WYRD_TIER1=1 xtask consistency-run` (Java + `$WYRD_ELLE_CLI_JAR` absent) — hard-errors
  BEFORE touching Docker (`missing: java, the elle-cli standalone jar ($WYRD_ELLE_CLI_JAR)`),
  exit 1 — confirms the opted-in-but-missing-environment rule fires for real, not merely in
  the unit test.

## The three refutation questions

**(a) Genuine red?** Yes — see "checked by hand" above: reverting the production module
(`consistency_run.rs` + its `lib.rs` wiring) while keeping the test produces a **compile-time**
red (`E0432`), which is the strongest form of red a Rust test can produce; restoring the
module goes green (23/23).

**(b) Production path?** Yes. The named test imports `xtask::consistency_run::*` directly —
the real, `pub`-wired module, not a copy — and that module is the SAME one
`xtask/src/consistency_run_runner.rs` (the actual `cargo xtask consistency-run` subcommand)
calls for every decision (leg selection, the vacuity gate, invocation building, verdict
parsing, report rendering, preflight). The live scenario
(`crates/server/tests/consistency_run_fdb.rs`) is likewise real, non-stub code: it drives the
REAL `ObservableS3Client`/`MultiProcessHistory`/`DirectoryHistory` (#406, unmodified) and the
REAL `wyrd_metadata_fault_conformance::nemesis::drive_leg`/`PartitionLeg`/`ClockSkewLeg`/
`ProcessPauseLeg` (#407, unmodified — consumed, never reopened), and it type-checks +
clippy-checks cleanly under the genuine `--features fdb` build in this environment.

**(c) Fixture includes the fault?** Partially, honestly caveated. The committed EDN histories
and checker-output fixtures are constructed to match Design §3's **pinned** contract (the
per-file `true`/`false` trailing line + exit status) — they are NOT literally captured from a
live `elle-cli` run, because this Do environment has no `java`/`elle-cli` jar (confirmed:
`which java` exits 1; no `$WYRD_ELLE_CLI_JAR`). This is exactly the two-tier fixtures split
Design §4 specifies: the Check-time tier pins the **parser** and the **EDN vocabulary**
against golden files (which is what `consistency_run_orchestration.rs` does, and does
correctly — the fixture set is NOT curated to omit the failing/erroring cases:
`checker-output-fail.txt` parses `Fail`, `checker-output-error.txt` parses `Fail`, both
correctly rejected, not just the pass case). The SECOND tier — feeding those same fixtures
through the REAL `elle-cli` (the runner's `fixtures_self_check`, implemented and wired into
`run_consistency_check`) — is what the design itself defers off-Check, and I could not
exercise it here for the reason above. This is the one honest gap; flagged below.

## NEEDS-HUMAN (pre-declared by the brief, not an undeclared Plan gap)

The brief's own Design §5 and External-dependencies section anticipate this exactly: *"the
report… produced by Do if its environment preflights green, otherwise by the maintainer's
witnessed run at sign-off"*. This Do environment's preflight is **not** green — `java` is
absent (`which java` exits 1) and no `$WYRD_ELLE_CLI_JAR` is set — so per the brief's own
words, the following are the maintainer's to produce at sign-off, not mine to fabricate:

- The **first witnessed `WYRD_TIER1=1 cargo xtask consistency-run`** run (needs Docker — 
  present here — Java and the elle-cli jar — absent here — plus the real
  `deploy/fdb-multi-replica` topology brought up and a real fault injected).
- The **fixtures self-check's actual pass** (`fixtures_self_check` feeding the committed
  golden EDN through the real `elle-cli`) — implemented and wired, never run against a real
  JVM here.
- The **committed `docs/design/reviews/` report** (precedent `m4-fdb-go-no-go.md`) — Design §5
  requires this before the PR is marked ready; it does not exist in this patch because there
  is no witnessed run to report on yet.

Concrete steps for the human to validate/produce these (on a machine with Docker, Java, and
an `elle-cli` standalone jar — `https://github.com/ligurio/elle-cli/releases`, 0.1.9 or the
then-current release):

```
export WYRD_ELLE_CLI_JAR=/path/to/elle-cli-standalone.jar
WYRD_TIER1=1 cargo xtask consistency-run
```

This brings up `deploy/fdb-multi-replica`, drives the checked workload under the default
partition leg, runs the fixtures self-check, obtains the elle-cli verdict, and writes
`target/consistency-run/report.md`. Copy that report to
`docs/design/reviews/m4-checked-consistency-run-elle-report.md` (naming precedent
`m4-fdb-go-no-go.md`) and commit it before the PR is marked ready (STOP discipline — the
ready-mark stays the human's regardless).

I did **not** fabricate a stand-in for any of the above (no mocked elle-cli, no
hand-written "witnessed" report, no invented docker/java presence) — the honest gap is
recorded here for §6 rather than routed around.

## Formatting / commit-readiness

`cargo fmt --all` was run over every touched file and `cargo fmt --all -- --check` is clean.
`cargo clippy --workspace --all-targets` (default features) and `cargo clippy -p wyrd-server
--features fdb --tests` are both clean. `cargo xtask ci` (the project's own gate, which is
also what its commit hooks amount to per `docs/INTEGRATION.md`'s T2 row — "rustfmt +
`clippy -D warnings` … inside `cargo xtask ci`") passes end to end.
