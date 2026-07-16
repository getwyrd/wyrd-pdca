# Design proposal — issue 408 / m4-checked-consistency-run-elle-report

> The Plan artifact for getwyrd/wyrd#408 — "M329.5 — the checked run + the public
> credibility artifact (non-vacuous Elle verdict)". Slice 5 of #329, the artifact ADR-0041
> exists to produce. Milestone note: the 2026-07-07 comment re-milestoned this to M14/FDB,
> and on 2026-07-08 it was milestoned **back to M4** (tracker timeline verified) — it is
> live now. The comment's *backend* point stands and is honoured below: the run targets the
> **FoundationDB** cluster (ADR-0042; GO verdict in `docs/design/reviews/m4-fdb-go-no-go.md`),
> through the backend-agnostic harness pieces (#406, #407).

- **Slug:** m4-checked-consistency-run-elle-report
- **Kind:** enhancement (design proposal)
- **Goal:** one opt-in command runs the #406 checked register + directory workload against
  the live multi-node FDB metadata cluster **under a #407 nemesis leg**, exports the
  Elle-EDN history, obtains a **non-vacuous verdict from the recognized checker (Elle,
  via elle-cli, off-Check)**, and emits the published run report — workload, nemesis,
  history size, model, verdict — the externally-recognizable credibility artifact (#329
  DoD item 2).
- **Success criterion:** the run pipeline exists end-to-end and refuses to overstate
  itself: it (a) drives the #406 multi-process workload and serializes its history via the
  landed Elle-EDN serializers, (b) obtains the verdict from the **pinned checker contract**
  (Design §3: elle-cli standalone jar, `--model rw-register` for the register history,
  `--model set-full` for the directory history, EDN input, per-file `true`/`false` verdict
  line) — honouring the landed routing (`consistency_verdict_dispatch` only **chooses the
  off-Check seat**; it invokes nothing — the invocation lives in the runner and never
  inside `cargo xtask ci`), (c) FAILS as **inconclusive** unless the **run summary** the
  scenario emits (Design §2 — the JSON carrying the #406 INV-2 witness result, the #407
  materialization evidence, and the history sizes) attests both a genuinely concurrent
  history AND a materialized fault — the gate decision is xtask-side arithmetic over that
  summary, (d) renders the report with the five fields the issue names (workload, nemesis,
  history size, model, verdict), and (e) ships **committed fixture files** — known-good /
  known-bad EDN histories and captured checker outputs — that at Check pin the verdict
  parser (pass / fail / error extraction) and the expected EDN vocabulary as golden files;
  the pinned models' actual *acceptance* of those fixtures is confirmed off-Check by the
  runner's fixtures self-check (part of the witnessed run, recorded in the report), since
  running the JVM checker at Check is banned (ADR-0041). The host-independent core — run
  orchestration plan, the summary-based inconclusive gate, elle-cli invocation building,
  verdict parsing, report rendering, missing-environment error paths — is exercised
  **red→green at Check** by `cargo test -p xtask --test consistency_run_orchestration`.
  The live checked run itself is opt-in (`WYRD_TIER1=1`), off-Check.
- **Falsifiability:** RED is produced in the C4-verify worktree: the gate reverts the
  production change, keeps the added `xtask/tests/consistency_run_orchestration.rs`, and
  the test fails against the missing orchestration/report/vacuity-gate code — the
  added-test classification was dry-run confirmed via `run-verify.sh --classify` (emits
  `ADDED_TEST xtask/tests/consistency_run_orchestration.rs`; crate `xtask` pre-exists, so
  the red leg is real). The *live* forbidden failure (Elle finding an ADR-0015 violation,
  or a vacuous history slipping through) is exhibitable only on the Docker FDB cluster +
  JVM — off-Check by ADR-0041's own MUST; the binding criterion is therefore the
  Check-testable core, and the inconclusive gate is itself unit-testable at Check (feed it
  a run summary attesting no INV-2 witness, or no materialized fault → it must refuse a
  verdict).
- **Repo + branch target:** getwyrd/wyrd @ main
- **Depends on:** 407
- **Ordering note:** depends on 407 because the run composes the checked workload WITH the
  nemesis (ADR-0041 sequences "nemesis first, then the checked artifact"), and both bundles
  edit `xtask/src/lib.rs` / `xtask/src/main.rs` (and possibly `xtask/Cargo.toml` / command
  help) — this bundle must build on 407's folded wave result and **consume 407's public
  nemesis seam as-is, never reopening its lifecycle logic**. Verified harness-side:
  `engine/scripts/run-verify.sh:186-192` honours `PDCA_BASE` → `PDCA_VERIFY_BASE` →
  `WYRD_VERIFY_BASE` before the brief base, so this wave-1 bundle's C4-verify runs against
  the wave's folded base, not a stale `origin/main`.
- **Scope:** the checked-run pipeline — one opt-in xtask subcmd, the live scenario test in
  `crates/server/tests/` (feature-gated), the EDN export call + **run-summary emission**
  (Design §2), the pinned elle-cli invocation + verdict parser, the summary-based
  inconclusive gate, the report renderer, the golden fixtures + off-Check fixtures
  self-check, the Check-time orchestration test, and the first witnessed run's committed
  report (Design §5). / out of scope: the scheduled privileged CI job and
  the bug→DST-regression loop (#409); any change to 407's nemesis lifecycle or the #406
  workload/serializer semantics (consume, don't rework); a TiKV-cluster leg (go/no-go
  carve-out 1); a full Clojure/Jepsen driver; any new ADR/spec (the report is a
  `docs/design/reviews/` document, precedent `m4-fdb-go-no-go.md`).
- **Difficulty:** medium
- **External dependencies:** Check-core red→green needs only the base Rust toolchain. The
  opt-in live run additionally needs `docker`, `libfdb_c loadable`, `fdb headers (bindgen)`,
  `java`, `elle-cli` — all five registered as `[[doctor.checks]]` rows in **this harness's
  `pdca.toml`** (the first three pre-existing; `java` and `elle-cli` added at Plan this
  cycle; the jar is located via `$WYRD_ELLE_CLI_JAR`) — and, plain prose, the ≥3-process
  `deploy/fdb-multi-replica` cluster topology with the #407 nemesis privileges (no-check:
  an environment shape, not an installable tool). Independently of the harness preflight,
  the **wyrd-side runner itself must hard-error** when opted in without Docker / Java /
  the jar (the `run_fdb_metadata_tier1` opted-in-but-missing rule), with the error paths
  pure-tested at Check.
- **Test file:** xtask/tests/consistency_run_orchestration.rs
- **Verification posture:** net-new coverage + deferred live green (postures (a)+(b),
  pre-declared). Built AND exercised at Check **by the named xtask test** (which, per
  Design §1, must not import `wyrd-server` — hence the run-summary seam): the
  run-orchestration plan (bring-up → workload → nemesis window → heal → export → check →
  report), the **summary-based** inconclusive gate, the elle-cli invocation building, the
  **verdict parser against fixture checker outputs**, the **golden fixture files**, the
  runner's opted-in-but-missing-environment error paths, and the report renderer. The
  EDN-export itself stays server-side where its types live — already unit-tested by the
  landed #406 suite (`to_elle_edn`, `is_genuinely_concurrent`); this slice only *calls* it
  in the live scenario and covers the scenario's summary-emission decision logic where it
  is testable. **Test-graph
  constraint, pinned:** everything the named test imports must be default-compiled — no
  feature/cfg gate, no FDB linkage, no Docker/Java/Elle dependency in the test's build
  graph, unconditional `#[test]` functions, the module `pub` and wired from
  `xtask/src/lib.rs` (otherwise the C4-verify red degrades to a vacuum). Deferred: the
  live run + real Elle verdict — opt-in `WYRD_TIER1=1`, confirmed by the maintainer's
  witnessed run whose report is committed before ready-mark (Design §5); the *scheduled*
  privileged CI job is explicitly #409, a separate work item this brief does not wave
  through. Deferred ≠ unbuilt: the live-scenario code must exist (feature-gated in
  `crates/server/tests/`, compiled under the fdb-toolchain opt-in like its siblings), with
  the named test exercising its actual decision logic, never inert scaffolding.
- **Citations expected:** Do must cite path:line on the target branch for every change.
  Peer callsites to mirror (composition slice — Do MAY open exactly these):
  - the verdict routing this run MUST honour: `consistency_verdict_dispatch`,
    `crates/server/src/consistency_workload.rs:887`, and its constants
    `ELLE_OFF_CHECK_VERDICT_JOB`/`ELLE_IN_GATE_CMD_VAR`
    (`crates/server/src/consistency_workload.rs:851,857`);
  - the histories + serializers to consume, not re-derive:
    `MultiProcessHistory` (`crates/server/src/consistency_workload.rs:79`), its INV-2
    witness `is_genuinely_concurrent` (`:165` — the vacuity gate binds to it) and
    `to_elle_edn` (`:324`); `DirectoryHistory` (`:703`) and its `to_elle_edn` (`:725`);
    the module doc (lines 1–47) states INV-1/INV-2;
  - the server-side FDB composition the live scenario builds on: the `fdb` feature
    (`crates/server/Cargo.toml:31`) and its CLI arm (`crates/server/src/cli.rs:122,141`),
    plus the loopback wire composition the observable already mirrors
    (`crates/server/src/consistency_observable.rs:1-10`, citing
    `crates/server/tests/s3_http_wire.rs`);
  - the opt-in-but-never-silently-skipped runner shape to mirror:
    `run_fdb_metadata_tier1`, `xtask/src/fdb_faults.rs:286` (brings up
    `deploy/fdb-multi-replica`, hard-errors when opted in without Docker, tears down
    unconditionally) and the inconclusive-not-pass rule it enforces
    (`docs/design/reviews/m4-fdb-go-no-go.md`, "a note is not a gate");
  - the dispatch/orchestration-test pattern: `xtask/src/metadata_faults.rs:53` and
    `xtask/tests/metadata_faults_orchestration.rs:1-25`;
  - the report's recognizability precedent: `docs/design/reviews/m4-fdb-go-no-go.md`
    (structure: what ran, what was found, what it does NOT license).
- **Prior-art check (by affected file path):** merged history — #406 (PR #479, commit
  3859d41) landed the workload/history/EDN substrate this consumes; #442 (commit 60469a4)
  landed the live-cluster runner pattern and the go/no-go report precedent; no open or
  closed PR titled for #408; no workflow named `elle-register-verdict` exists yet (that
  name is the landed dispatch's *seat*, filled by #409). Nothing supersedes this; genuinely
  additive.
- **Disposition hint:** new-feature

## Motivation

This is #329's whole point: turn "trust us, we don't lose data" into an
externally-recognizable artifact — a genuine, non-trivial checked run under failure, with a
recognized checker's verdict an outsider can inspect (#329 DoD item 2;
`architecture/10-quality-risks-glossary.md` credibility-artifact framing). Every
prerequisite has now landed: the substrate decision (ADR-0041), the networked observable
(#405), the workload + Elle-EDN histories + verdict dispatch (#406), the live FDB cluster
and runner discipline (#442) — and the nemesis is wave 0 of this very batch (#407).

## Design

1. **Runner architecture, pinned (the repo's established split — xtask stays thin).** The
   opt-in entry (e.g. `WYRD_TIER1=1 cargo xtask consistency-run`) mirrors
   `run_fdb_metadata_tier1`'s discipline (`xtask/src/fdb_faults.rs:286`): self-contained
   bring-up of `deploy/fdb-multi-replica`, unconditional teardown, hard error when opted
   in without its environment (Docker, Java, `$WYRD_ELLE_CLI_JAR`) — never a silent skip.
   The **live scenario itself lives in `crates/server/tests/`** as an env-gated,
   feature-gated (`fdb`) integration test, launched by xtask **shelling out to `cargo
   test -p wyrd-server --features fdb --test <scenario>`** exactly as the FDB fault
   battery launches its scenarios — so `xtask` gains NO `wyrd-server`/FDB/JVM dependency
   (today it has none, `xtask/Cargo.toml:11-14`) and the default build graph stays clean.
2. **Compose, don't rebuild.** The scenario hosts the production server + gateway on a
   loopback listener (the `s3_http_wire.rs` composition the #405 observable already
   mirrors), backed by `FdbMetadataStore` (the server `fdb` feature + CLI arm,
   `crates/server/Cargo.toml:31`, `cli.rs:122,141`) pointed at the Docker
   `deploy/fdb-multi-replica` cluster. N concurrent client tasks drive overwriting
   PUT / GET / DELETE + directory create/delete on a small shared key set through
   `consistency_observable`; a #407 nemesis leg (partition first; skew/pause selectable)
   runs mid-window against the FDB containers, consumed via 407's public seam
   (dev-dependency on `wyrd-metadata-fault-conformance`, where 407 placed the importable
   lifecycle + leg impls); histories merge via `MultiProcessHistory` and serialize with
   the landed `to_elle_edn` (register model = the main leg; directory-as-set = the
   secondary, per ADR-0041 §Decision 1–2), written under `target/consistency-run/`
   **together with a machine-readable run summary** (JSON: the INV-2
   `is_genuinely_concurrent` result, the nemesis leg + its typed materialization
   evidence, per-model history sizes, op/outcome counts). The summary is the seam between
   the server-side scenario and the xtask runner: the runner derives
   inconclusive/verdict/report from summary + checker output alone, which is exactly what
   makes the gate arithmetic testable at Check without xtask importing `wyrd-server`.
3. **The checker contract, pinned (verified against elle-cli upstream at Plan).** The
   checker is **elle-cli** (github.com/ligurio/elle-cli; pin the release current at
   implementation — 0.1.9 at Plan time — recording the jar's SHA-256 in the report), the
   standalone-jar packaging of Elle itself, satisfying ADR-0041's "the checker itself is
   the recognized one" while the driver stays Rust. Invocation:
   `java -jar $WYRD_ELLE_CLI_JAR --model rw-register <register-history.edn>` and
   `java -jar $WYRD_ELLE_CLI_JAR --model set-full <directory-history.edn>` (elle-cli
   accepts EDN natively — its README's own history format; `set-full` is the rigorous
   reads-over-time set checker matching the GET-probe membership vocabulary #406 emits).
   Verdict = the per-file `true`/`false` line + exit status; `false`, a non-zero exit, or
   unparseable output is a run FAILURE, never a silent pass — the parser is unit-tested
   against committed fixture outputs. `consistency_verdict_dispatch` stays what it is —
   the seat-chooser (`elle-register-verdict`, off-Check); the invocation code lives in the
   runner, and `cargo xtask ci` stays JVM-free.
4. **Non-vacuity is a gate, not a note:** before any verdict is reported, the xtask runner
   asserts, over the run summary, (a) the INV-2 concurrency witness held and (b) the #407
   fault materialized; failing either makes the run **inconclusive** (non-zero), because a
   vacuous history is precisely the #250 failure mode this issue exists to bury. Fixtures,
   two-tier: at Check, committed known-good/known-bad EDN histories + captured checker
   outputs pin the expected vocabulary (golden files) and the parser (a `false` verdict and
   a checker error must both surface as failure); off-Check, the runner's **fixtures
   self-check** feeds those same fixtures through the real elle-cli (known-bad must come
   back `false`, known-good `true`) — run as part of every live run and the witnessed run,
   recorded in the report, so checker-acceptance is demonstrated where the JVM is allowed
   to exist.
5. **The report is the artifact — one deliverable, no either/or.** The runner renders a
   Markdown report — workload parameters, nemesis leg + materialization evidence, history
   size (ops, per model), model(s) checked, checker + version + jar SHA-256, verdict —
   under `target/consistency-run/`. **This PR is not marked ready until the first
   witnessed run's report is committed** to the same branch under `docs/design/reviews/`
   (precedent: `m4-fdb-go-no-go.md`) — produced by Do if its environment preflights green,
   otherwise by the maintainer's witnessed run at sign-off (the ready-mark is the human's
   step regardless, per STOP discipline). Acceptance = Check-core green at Check + the
   witnessed report present at sign-off; the issue's "published report" DoD is met by that
   commit, not deferred to a separate issue.

## Alternatives considered

- **Full Clojure/Jepsen driver:** rejected — ADR-0041 explicitly allows a Rust driver with
  the recognized checker, and five #250 iterations showed the literal-stack path produced
  vacuous histories; the substrate now emits checker-native EDN directly.
- **A homegrown Rust linearizability checker in-gate:** rejected — ADR-0041: the artifact's
  entire value is outsider recognition; #406's module doc likewise reserves the global
  verdict for Elle, off-Check.
- **Gating on the TiKV stack too:** rejected — ADR-0042 makes FDB the production backend;
  the TiKV tier1 leg is red on unmodified main (go/no-go carve-out 1, its own issue), and
  the 2026-07-07 tracker comment pins the checked run to the FDB cluster.

## Impact & compatibility

Additive harness/report code; no production crate's runtime behavior changes. `cargo xtask
ci` remains unprivileged, container-free, JVM-free (new code compile-checked only). New
external tooling (JVM + elle-cli) is confined to the opt-in leg and preflighted by the two
doctor rows registered this cycle. The committed report is a `docs/design/reviews/` document
— maintainer-authority territory, so its acceptance is explicitly the human's at sign-off
(same class as the go/no-go review; it is NOT an ADR/spec edit, so no immutability concern).

## Open questions

- `set-full` vs `set` on cost grounds (Design §3 pins `set-full`, the rigorous
  reads-over-time checker; if history sizes make it impractical, dropping to `set`
  requires a single final read in the workload and is the maintainer's call — record the
  choice in the report).
- Exact elle-cli release to pin at implementation time (0.1.9 at Plan; take the then-current
  release and record version + jar SHA-256 in the report).

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a
draft PR MAY happen during the cycle (useful for CI feedback). The PR MUST NOT be
marked ready before sign-off accepts.

## Iteration 1 — carry-forward (from the previous attempt)
- Sign-off rationale: Rejected at sign-off on the reviewer's findings; the pipeline shape is right but the credibility artifact cannot yet substantiate itself: - T2 FAIL (primary): the live scenario hard-codes `nemesis_materialized = true` and the run summary/report carry only that boolean. Propagate the #407 typed materialization evidence (what the leg observed: which fault, on which target, how it provably bit) through `run-summary.json` and into the report, instead of asserting the boolean from `drive_leg`'s contract (crates/server/tests/consistency_run_fdb.rs:194,215; xtask/src/consistency_run_runner.rs:352). - T3/T5/Validation: no real witnessed run exists — the privileged `WYRD_TIER1=1 ... cargo xtask consistency-run` was never executed on a Docker/JVM host, and the promised first witnessed report under docs/design/reviews/ is absent. The next attempt should include (or be verified against) an actual live run and its committed report, and confirm `set-full` is practical for the directory model. Keep the existing pure-core/runner split and the non-vacuity gate — those were reviewed as sound; the delta is evidence fidelity plus the witnessed run, not a redesign.
- Full previous attempt preserved in `iteration-v1/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 2 — carry-forward (from the previous attempt)
- Sign-off rationale: Witnessed run performed at sign-off (WYRD_TIER1=1 cargo xtask consistency-run, elle-cli 0.1.9, live 3-node FDB cluster): the pipeline ran end-to-end — partition materialized with real typed evidence, genuinely concurrent history (120 register + 180 directory ops), non-vacuity gate and fixtures self-check behaved correctly (exit 1, no fabricated verdict) — but the REAL Elle checker rejected the EDN histories for BOTH models, so no verdict/report is obtainable: - rw-register (real history AND both committed fixtures): "Don't know how to create ISeq from: java.lang.Long" — Elle expects :value to be a transaction (vector of micro-ops, e.g. [[:w :x 1]]); the #406-landed serializer emits single ops with scalar :value and :f :read/:write. - set-full (directory history): "No matching clause: :contains" → :unknown — the jepsen set-full checker expects :add/:read set semantics, not the create/delete/probe vocabulary emitted. The brief's premise "(a) serializes its history via the landed Elle-EDN serializers" is falsified: those serializers are not checker-compatible, and the brief marks "#406 workload/serializer semantics (consume, don't rework)" OUT of scope — so the needed fix is unreachable under the current plan. Replan must: (1) redraw the scope boundary so the EDN format contract can be fixed (amend #406's serializers or add an export-time translation owned by #408); (2) pin the format to what elle-cli actually accepts (transaction-shaped :value micro-ops for rw-register; the set model's :add/:read vocabulary for set-full) and make the committed golden fixtures REAL elle-cli-accepted samples, not Wyrd-vocabulary pins; (3) revisit whether set-full is the right model for the directory workload (the open T5 question). Confirms reviewer C3 FAIL (checker acceptance unproven). Environment is now fully provisioned for the next witnessed run (java + WYRD_ELLE_CLI_JAR=/home/eddie/Downloads/elle-cli-bin-0.1.9/target/elle-cli-0.1.9-standalone.jar).
- Full previous attempt preserved in `iteration-v2/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).
