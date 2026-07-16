# Build notes — issue 408 (iteration 2)

Slice 5 of #329: the checked consistency run + the public credibility artifact
(non-vacuous Elle verdict). Target: getwyrd/wyrd @ main, built in `$PDCA_WORKTREE`
(`/home/eddie/development/wyrd/wyrd.pdca-wt`, detached at the wave-folded base
`e0e39c1`, which already has #407 merged).

## What the carry-forward asked for (iteration 1 → 2)

The pipeline *shape* was reviewed sound (pure-core/runner split + the summary-based
non-vacuity gate — "keep those"). Two deltas:

- **T2 (primary):** the live scenario hard-coded `nemesis_materialized = true` and the
  summary/report carried only that boolean. Propagate the **#407 typed materialization
  evidence** (which fault, on which target, how it provably bit) through
  `run-summary.json` and into the report, instead of asserting the boolean from
  `drive_leg`'s contract.
- **T3/T5/Validation:** no real witnessed run existed; the first witnessed report under
  `docs/design/reviews/` is absent.

This iteration addresses T2 in full (built + bound red→green at Check) and declares
T3/T5 a NEEDS-HUMAN (the environment here cannot host it — see §"NEEDS-HUMAN").

## The T2 fix — typed evidence, carried not asserted

The root cause of iteration 1's boolean: `drive_leg<L,W,T>(leg, workload) -> Result<T>`
(`crates/metadata-fault-conformance/src/nemesis.rs:295`) computes the leg's typed
`MaterializationEvidence` internally (via `confirm_materialized`) and **gates** on it,
but **throws it away** — it returns only the workload's `T`. The battery consumers
(`crates/metadata-fdb/tests/tier1_metadata_nemesis.rs:92,113,160`) never get the
evidence either. So iteration 1 could not surface it and asserted `= true`.

**How I got the evidence out without reopening #407's lifecycle** (Ordering note /
out-of-scope: "consume 407's public nemesis seam as-is, never reopening its lifecycle
logic"): the scenario's `drive_with_evidence` helper
(`crates/server/tests/consistency_run_fdb.rs`) calls `leg.confirm_materialized()`
**inside the `drive_leg` workload closure** — where the fault is still active — to
capture the leg's OWN typed evidence, then returns it alongside the histories. This is
consuming the public trait method as-is, not editing `drive_leg`. It is sound because:

- `drive_leg` has *already* gated materialization before it enters the workload closure,
  so this second `confirm_materialized` sees the fault already in effect and returns fast
  (its partition/pause polls break on the first `!peers_see_target_live()` sample).
- `confirm_materialized` is **read-only** for all three legs (a survivor `status json`
  reachability probe / a `docker inspect` status / a container `date +%s` read —
  `nemesis.rs:653,782,969`), so re-sampling perturbs neither the fault nor the workload.
- It samples *within the fault window*, so the recorded evidence genuinely attests the
  fault was live while the checked workload ran — arguably stronger than the pre-workload
  gate sample.

The evidence flows: leg `MaterializationEvidence::{kind, materialized, diagnosis}` →
`NemesisEvidenceRecord` (+ the target descriptor) → the nested `nemesis` object in
`run-summary.json` (built with `serde_json` so field names match) →
`xtask::consistency_run::{RunSummary, NemesisEvidence}` (serde) → the non-vacuity gate
(`evaluate_summary` now reads `summary.nemesis.materialized`, the leg's real oracle
verdict) → the report (`ReportInputs::nemesis_field` renders `kind` + `target` +
`materialized` + the leg's own `diagnosis`, e.g. a partition's
`peers_saw_target before=true during=false … target_running_during=true`).

So the credibility artifact now attests *what the leg observed*, not a bare boolean.

### Rejected alternative for T2 (with its cost)

The most faithful way to carry the *exact* evidence `drive_leg` gated on would be to
change `drive_leg` to return `(Evidence, T)`. Rejected: that edits #407's lifecycle
(`nemesis.rs:295-375`, ~30 lines of the central runner) plus all three existing battery
callsites (`tier1_metadata_nemesis.rs:92,113,160`) — explicitly out of scope ("any
change to 407's nemesis lifecycle … consume, don't rework"). The re-sample costs one
extra read-only `confirm_materialized` call per run and zero lines in #407. It records
independent evidence within the same fault window, which meets the T2 requirement ("what
the leg observed, how it provably bit") without reopening the seam.

## Architecture (unchanged from iteration 1, reviewed sound)

- **Pure core** `xtask/src/consistency_run.rs` — run plan, `selected_leg`, the
  `RunSummary`/`NemesisEvidence` seam types, `evaluate_summary` (non-vacuity gate),
  `elle_invocation`, `parse_checker_output`, `self_check_matches`, the golden EDN
  vocabulary pin, `preflight`, `render_report`. No `wyrd-server`/FDB/Docker/Java/elle
  dependency, so it compiles in the default `cargo xtask ci` graph and is exercised by
  the named test.
- **Impure runner** `xtask/src/consistency_run_runner.rs` (wired from `main.rs`) — the
  `docker compose` bring-up, the `cargo test -p wyrd-server --features fdb` shell-out, the
  `java -jar` elle-cli invocation, the fixtures self-check, report write. Mirrors
  `fdb_faults::run_fdb_metadata_tier1` (`xtask/src/fdb_faults.rs:286`): opt-in
  `WYRD_TIER1=1`, hard error when opted in without Docker/Java/jar, unconditional
  teardown via `finalize_panic_safe`.
- **Live scenario** `crates/server/tests/consistency_run_fdb.rs` (`fdb`-feature-gated,
  `#[ignore]`d, env-gated) — hosts the production server + S3 gateway on loopback backed
  by the real `FdbMetadataStore`, drives the #406 register + directory workload under a
  #407 leg, writes the EDN histories + typed-evidence run summary. Consumes #406's
  `MultiProcessHistory`/`DirectoryHistory`/`to_elle_edn`/`is_genuinely_concurrent` and
  #407's `PartitionLeg`/`ClockSkewLeg`/`ProcessPauseLeg`/`drive_leg` as-is.
- `crates/server/Cargo.toml` gains a dev-dependency on
  `wyrd-metadata-fault-conformance` (trait-only + docker shell-outs, no `libfdb_c`, so it
  does not widen the default graph). `Cargo.lock` updated (one line).

## Test — `xtask/tests/consistency_run_orchestration.rs` (the named discriminator)

24 unconditional `#[test]`s, all default-compiled (no wyrd-server/FDB/JVM in the build
graph — the test-graph constraint). New/strengthened for T2:

- `a_summary_whose_typed_evidence_did_not_materialize_is_inconclusive` — the gate reads
  `nemesis.materialized` (the leg's oracle), refuses the verdict when it is false.
- `the_run_summary_is_parsed_from_json_carrying_the_typed_evidence` — the JSON seam
  round-trips the nested evidence + its `diagnosis`; a summary lacking the evidence is a
  hard parse error (never an accidentally-conclusive default).
- `the_report_nemesis_field_attests_how_the_fault_bit_not_a_bare_boolean` — the report
  renders the fault class, the target (service + address), and the leg's own diagnosis.

### Red→green (refutation, recorded)

Verified via the project's own `engine/scripts/run-verify.sh` (C4-verify), which applies
`patch.diff` in an isolated `../wyrd-verify` worktree off `origin/main` (which already has
#407):

```
GREEN with the fix:  running 24 tests … test result: ok. 24 passed
RED   production reverted, test kept:  E0432 unresolved import `xtask::consistency_run`
                                       (+ the golden fixtures gone)
run-verify.sh: PASS — red without the fix, green with it.
```

- **(a) Genuine red?** YES — with production reverted the named test fails to
  compile (the `consistency_run` module and its fixtures are gone). Confirmed by
  run-verify's own red leg above, not asserted.
- **(b) Production path?** YES — the test drives the real `xtask::consistency_run`
  functions (`evaluate_summary`, `parse_run_summary`, `render_report`,
  `ReportInputs::nemesis_field`, `preflight`, `parse_checker_output`) — the very
  functions `consistency_run_runner.rs` calls at runtime. No copy/mock/re-impl.
- **(c) Fixture includes the fault?** YES — the "fault" this gate exists to catch is a
  run overstating itself. `a_summary_whose_typed_evidence_did_not_materialize_is_inconclusive`
  feeds a summary whose typed evidence says the fault did NOT bite → the gate must refuse.
  The golden fixtures include the known-bad EDN history and the `false` / crashed checker
  outputs (the actually-failing elements), which the parser must surface as failure.

The live forbidden failure (Elle finding an ADR-0015 violation, or a vacuous history
slipping through) is exhibitable only on the Docker FDB cluster + JVM — off-Check by
ADR-0041's own MUST — so the binding Check criterion is this pure core, per the brief's
Falsifiability.

## Commit-readiness

- `cargo fmt --check` — clean over the whole tree.
- `cargo clippy -p xtask --all-targets -- -D warnings` — clean.
- `cargo clippy -p wyrd-server --features fdb --tests -- -D warnings` — clean (the
  `fdb`-gated scenario type-checks against the real `libfdb_c`/bindgen toolchain, which
  is present on this host).
- `cargo test -p xtask --test consistency_run_orchestration` — 24 passed.

## NEEDS-HUMAN — the witnessed live run (carry-forward T3/T5/Validation)

Design §5 says Do produces the first witnessed report **if its environment preflights
green**, otherwise the maintainer's witnessed run at sign-off does (the ready-mark is the
human's step regardless, per STOP discipline). This host does **not** preflight green:

- `docker` — OK
- `java` — **MISSING**
- `$WYRD_ELLE_CLI_JAR` (the elle-cli standalone jar) — **MISSING / unset**

Without java + the elle-cli jar the run cannot obtain a real Elle verdict, and the
runner's own `preflight` would (correctly) hard-error. So the first witnessed
`WYRD_TIER1=1 cargo xtask consistency-run` and its committed report under
`docs/design/reviews/` must be produced by the maintainer on a Docker+JVM host at
sign-off, before ready-mark. To validate manually:

```
# on a host with docker, java, and the elle-cli standalone jar:
export WYRD_ELLE_CLI_JAR=/path/to/elle-cli-standalone.jar
WYRD_TIER1=1 cargo xtask consistency-run          # partition leg (default)
# report + histories land under target/consistency-run/; commit report.md to
# docs/design/reviews/m4-checked-consistency-run.md (precedent: m4-fdb-go-no-go.md).
# Also confirm the Open Question: set-full practical for the directory model at the
# observed history size (else drop to `set` and record the choice in the report).
```

NEEDS-HUMAN external dependency: java + elle-cli (WYRD_ELLE_CLI_JAR) — blocks the
privileged `WYRD_TIER1=1 cargo xtask consistency-run` witnessed run and its committed
credibility report; the Check-core (this bundle) is fully built and green, but the live
Elle verdict cannot be produced here.

The `[[doctor.checks]]` rows for both are already registered in this harness's
`pdca.toml` (`id = "java"` at line 708, `id = "elle-cli"` at line 718 — added at Plan this
cycle), so no new registration is needed; they simply report MISSING on this host.
