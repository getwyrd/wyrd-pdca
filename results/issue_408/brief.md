# Design proposal — issue 408 / m4-checked-consistency-run-elle-report (replan v3)

> The Plan artifact for getwyrd/wyrd#408 — "M329.5 — the checked run + the public
> credibility artifact (non-vacuous Elle verdict)". Slice 5 of #329, the artifact ADR-0041
> exists to produce. **Third-iteration replan**: iteration v2's pipeline ran end-to-end on
> the live 3-node FDB cluster (partition materialized with typed evidence, genuinely
> concurrent history, non-vacuity gate correct) but the REAL Elle checker rejected the EDN
> histories for BOTH models — the #406-landed serializers emit a vocabulary elle-cli does
> not accept, and v2's brief marked those serializers out of scope, making the fix
> unreachable. This brief redraws that boundary and pins the format contract to what
> elle-cli **actually accepts — every format claim below was verified at Plan by running
> the real elle-cli 0.1.9 jar on this host (2026-07-16)**, not asserted from a README.
> The 2026-07-07 tracker comment's backend point stands: the run targets the
> **FoundationDB** cluster (ADR-0042; GO verdict in `docs/design/reviews/m4-fdb-go-no-go.md`).

- **Slug:** m4-checked-consistency-run-elle-report
- **Kind:** enhancement (design proposal)
- **Goal:** one opt-in command runs the checked register + directory workload against the
  live multi-node FDB metadata cluster **under a #407 nemesis leg**, exports Elle-EDN
  histories in the **checker-verified vocabulary** (Design §3), obtains a **non-vacuous
  verdict from the recognized checker (Elle, via elle-cli, off-Check)**, and emits the
  published run report — workload, nemesis, history size, model, verdict — the
  externally-recognizable credibility artifact (#329 DoD item 2).
- **Success criterion:** the run pipeline exists end-to-end and refuses to overstate
  itself: it (a) **amends the #406 Elle-EDN serializers in place**
  (`crates/server/src/consistency_workload.rs`) so the register history is emitted in the
  transaction-shaped micro-op form and the directory history in the integer set form that
  elle-cli 0.1.9 accepts (Design §3 — the exact shapes verified at Plan), with INV-1
  preserved (indeterminate → `:info`, never a fabricated determinate completion) and with
  the unrepresentable ops (register DELETE, directory `:remove`/`:contains`) **excluded by
  workload-pool construction, never by per-op filtering** (Design §2); (b) obtains the
  verdict from the pinned checker contract — `java -jar $WYRD_ELLE_CLI_JAR --model
  rw-register <register.edn>` and `--model set <directory.edn>` — parsing the per-file
  verdict token: **only the literal `true` is a pass; `false` is a violation (run
  FAILURE); `:unknown` or anything unparseable is INCONCLUSIVE — never keyed on exit code
  alone (verified: `:unknown` exits 0)**; the landed routing is honoured
  (`consistency_verdict_dispatch` only chooses the off-Check seat; the invocation lives in
  the runner and never inside `cargo xtask ci`); (c) FAILS as **inconclusive** unless the
  run summary the scenario emits (the JSON carrying the #406 INV-2 witness result, the
  #407 **typed materialization evidence**, per-model history sizes, and per-op-kind
  outcome counts) attests both a genuinely concurrent history AND a materialized fault —
  the gate decision is xtask-side arithmetic over that summary; (d) renders the report
  with the five fields the issue names (workload, nemesis, history size, model, verdict);
  and (e) ships **committed golden fixtures that are REAL elle-cli-accepted samples** —
  known-good and known-bad EDN histories whose acceptance/verdicts were produced by the
  real checker (known-good → `true`, known-bad → `false`, plus a captured `:unknown`/error
  output) — pinning the verdict parser and the EDN vocabulary at Check, and re-confirmed
  off-Check by the runner's fixtures self-check on every live run. The host-independent
  core — run orchestration plan, the summary-based inconclusive gate, elle-cli invocation
  building, verdict-token parsing, report rendering, missing-environment error paths, and
  the new serializer vocabulary — is exercised **red→green at Check** by
  `cargo test -p xtask --test consistency_run_orchestration` (serializer unit tests stay
  in `crates/server` where the types live, run by `C4-ci`). The live checked run is opt-in
  (`WYRD_TIER1=1`), off-Check; **the PR is not marked ready until the first witnessed
  run's report — with real `true` verdicts from both models — is committed under
  `docs/design/reviews/`** (Design §6).
- **Falsifiability:** RED is produced in the C4-verify worktree: the gate reverts the
  production change, keeps the added `xtask/tests/consistency_run_orchestration.rs`, and
  the test fails against the missing orchestration/parser/vacuity-gate code (the
  `run-verify.sh --classify` ADDED_TEST classification was dry-run confirmed at the v2
  Plan for this same path; the v2 patch never merged, so the file is still absent on
  `origin/main` and the red leg stays real). The serializer amendment's red lives in the
  updated `crates/server` unit tests under `C4-ci`. The *live* forbidden failure (Elle
  returning `false`, or a vacuous history slipping through) is exhibitable only on the
  Docker FDB cluster + JVM — off-Check by ADR-0041's own MUST; checker *acceptance* is no
  longer a deferred assumption: the exact EDN shapes were fed to the real elle-cli 0.1.9
  at Plan (good → `true`, bad → `false`, `:remove`/string-set/nil-write traps → verified
  rejections, recorded in Design §3), and the committed fixtures pin those same shapes.
  The witnessed run's environment is provisioned on this host (java 25;
  `WYRD_ELLE_CLI_JAR=/home/eddie/Downloads/elle-cli-bin-0.1.9/target/elle-cli-0.1.9-standalone.jar`
  — must be exported for the run; the doctor row detects it).
- **Repo + branch target:** getwyrd/wyrd @ main
- **Depends on:** none
- **Ordering note:** the v2 dependency on #407 is discharged — #407 merged to `main` as
  PR #569 (`e0e39c1`); its nemesis seam is consumed as-is from
  `crates/metadata-fault-conformance/src/nemesis.rs`, never reopened. Single bundle, no
  wave; C4-verify resolves its base from this brief's target (`origin/main`).
- **Scope:** (1) the Elle-EDN **format-contract fix at its source**: amend
  `MultiProcessHistory::to_elle_edn` and `DirectoryHistory::to_elle_edn` (+ their render
  helpers and the landed #406 serializer unit tests) to the checker-verified vocabulary of
  Design §3; (2) the checked-run pipeline — one opt-in xtask subcmd, the live scenario
  test in `crates/server/tests/` (feature-gated `fdb`), the run-summary emission, the
  pinned elle-cli invocation + verdict-token parser, the summary-based inconclusive gate,
  the report renderer, the real-sample golden fixtures + off-Check fixtures self-check,
  the Check-time orchestration test; (3) the workload reshaping of Design §2 (Elle-fed
  overwrite pool / Wyrd-checked delete pool; directory create-only universe + post-heal
  composed full-set read); (4) the first witnessed run's committed report (Design §6).
  / out of scope: the scheduled privileged CI job and the bug→DST-regression loop (#409);
  any change to #407's nemesis lifecycle (consume `drive_leg` and the leg impls as-is);
  any change to the #406 *checks* (`is_genuinely_concurrent`, session/monotonicity checks
  stay untouched — only the EDN serialization layer is amended); a TiKV-cluster leg
  (go/no-go carve-out 1); a full Clojure/Jepsen driver; any new ADR/spec (the report is a
  `docs/design/reviews/` document, precedent `m4-fdb-go-no-go.md`).
- **Difficulty:** high
- **External dependencies:** Check-core red→green needs only the base Rust toolchain. The
  opt-in live run additionally needs `docker`, `libfdb_c loadable`, `fdb headers
  (bindgen)`, `java`, `elle-cli` — all five registered as `[[doctor.checks]]` rows in this
  harness's `pdca.toml` (the jar located via `$WYRD_ELLE_CLI_JAR`) — and, plain prose, the
  ≥3-process `deploy/fdb-multi-replica` cluster topology with the #407 nemesis privileges
  (no-check: an environment shape, not an installable tool). Independently of the harness
  preflight, the wyrd-side runner itself must hard-error when opted in without Docker /
  Java / the jar (the `run_fdb_metadata_tier1` opted-in-but-missing rule), with the error
  paths pure-tested at Check.
- **Test file:** xtask/tests/consistency_run_orchestration.rs
- **Verification posture:** net-new coverage + deferred live green (postures (a)+(b),
  pre-declared). Built AND exercised at Check by the named xtask test (which must not
  import `wyrd-server` — the run-summary JSON is the seam): the run-orchestration plan
  (bring-up → workload → nemesis window → heal → quiesce → composed final read → export →
  check → report), the summary-based inconclusive gate, the elle-cli invocation building,
  the **verdict-token parser against the real captured checker outputs** (`true`, `false`,
  `:unknown`, stack-trace error — `:unknown`-with-exit-0 MUST parse as inconclusive), the
  golden fixture files, the opted-in-but-missing-environment error paths, and the report
  renderer. The amended serializers stay server-side where their types live, unit-tested
  in `crates/server` (red→green under `C4-ci`: the existing #406 serializer tests are
  updated to pin the NEW vocabulary — including "a register delete/nil-write can never be
  emitted" and "directory EDN contains only `:add`/`:read` with integer elements").
  **Test-graph constraint, pinned:** everything the named test imports must be
  default-compiled — no feature/cfg gate, no FDB linkage, no Docker/Java/Elle dependency
  in the test's build graph, unconditional `#[test]` functions, the modules `pub` and
  wired from `xtask/src/lib.rs` (otherwise the C4-verify red degrades to a vacuum).
  Deferred: the live run + real Elle verdicts — opt-in `WYRD_TIER1=1`, confirmed by the
  witnessed run whose report is committed before ready-mark (Design §6). Deferred ≠
  unbuilt: the live-scenario code must exist (feature-gated in `crates/server/tests/`,
  compiled under the fdb opt-in like its siblings), with the named test exercising its
  actual decision logic, never inert scaffolding. **Prior attempt reusable:** the v2
  patch (`iteration-v2/patch.diff` in this bundle) implements most of the pipeline and
  was reviewed sound in shape; Do MAY start from it, rebasing onto `origin/main` (its
  base has since merged) — the delta is the serializer/format contract, the real-sample
  fixtures, the workload pools, the richer evidence schema, and the witnessed report.
- **Citations expected:** Do must cite path:line on the target branch (`origin/main`) for
  every change. Peer callsites to mirror (composition slice — Do MAY open exactly these):
  - the serializers to amend — `MultiProcessHistory::to_elle_edn`
    (`crates/server/src/consistency_workload.rs:324`), `render_entry` (`:555`),
    `register_invoke_value` (`:617`), `DirectoryHistory::to_elle_edn` (`:725`), `dir_f`
    (`:794`), `dir_invoke_value` (`:804`); the module doc (lines 1–47) states INV-1/INV-2;
    the checks that stay untouched: `is_genuinely_concurrent` (`:165`), the session checks
    (`:187`, `:291`, `:307`);
  - the verdict routing this run MUST honour: `consistency_verdict_dispatch`
    (`crates/server/src/consistency_workload.rs:887`) and its constants
    `ELLE_OFF_CHECK_VERDICT_JOB` / `ELLE_IN_GATE_CMD_VAR` (`:851`, `:857`);
  - the #407 nemesis seam to consume as-is:
    `crates/metadata-fault-conformance/src/nemesis.rs` — `NemesisLegKind` (`:53`),
    `MaterializationEvidence` (`:94`, typed evidence: `PartitionEvidence:109`,
    `PauseEvidence:156`, `SkewEvidence:196`), `NemesisLeg` (`:240`), `drive_leg` (`:295`),
    the live legs (`PartitionLeg:546`, `ProcessPauseLeg:717`, `ClockSkewLeg:850`); wired
    via `crates/metadata-fault-conformance/src/lib.rs:65`;
  - the server-side FDB composition the live scenario builds on: the `fdb` feature
    (`crates/server/Cargo.toml:31`) and its CLI arms (`crates/server/src/cli.rs:138,156`
    on origin/main), plus the loopback wire composition the observable already mirrors
    (`crates/server/src/consistency_observable.rs:1-10`, citing
    `crates/server/tests/s3_http_wire.rs`);
  - the opt-in-but-never-silently-skipped runner shape: `run_fdb_metadata_tier1`
    (`xtask/src/fdb_faults.rs:286`) and the inconclusive-not-pass rule
    (`docs/design/reviews/m4-fdb-go-no-go.md`, "a note is not a gate");
  - the dispatch/orchestration-test pattern — #407's own is the closest peer:
    `xtask/src/nemesis.rs` + `xtask/tests/nemesis_orchestration.rs`; also
    `xtask/src/metadata_faults.rs:53` + `xtask/tests/metadata_faults_orchestration.rs:1-25`;
  - the report's recognizability precedent: `docs/design/reviews/m4-fdb-go-no-go.md`.
- **Prior-art check (by affected file path):** merged history — #406 (PR #479) landed the
  workload/history/EDN substrate this amends; #407 (PR #569, merge `e0e39c1`) landed the
  nemesis seam this consumes; #442 (`60469a4`) landed the live-cluster runner pattern and
  the go/no-go report precedent. No open or closed PR titled for #408 and no
  `enhancement/408-*` branch on origin (v1/v2 were iterated-to-Plan before publish; their
  patches live only in this bundle's `iteration-v*/`). No workflow named
  `elle-register-verdict` exists (that seat is #409's). Nothing supersedes this;
  genuinely additive plus an in-place amendment of #406's serialization layer.
- **Disposition hint:** new-feature

## Motivation

This is #329's whole point: turn "trust us, we don't lose data" into an
externally-recognizable artifact — a genuine, non-trivial checked run under failure, with
a recognized checker's verdict an outsider can inspect (#329 DoD item 2). Iterations v1/v2
proved the pipeline shape and the live environment; what failed was the one premise nobody
had verified against the real checker: the EDN vocabulary. The v2 witnessed run
(2026-07-16) produced real rejections — rw-register: "Don't know how to create ISeq from:
java.lang.Long" (scalar `:value`; Elle wants transaction micro-ops); set-full: "No matching
clause: :contains" (the set checkers know only `:add`/`:read`). This replan fixes the
format contract at its source and pins every shape to a Plan-time run of the actual jar.

## Design

1. **Runner architecture — unchanged from v2 (reviewed sound).** Opt-in entry
   (`WYRD_TIER1=1 cargo xtask consistency-run`) mirrors `run_fdb_metadata_tier1`'s
   discipline (`xtask/src/fdb_faults.rs:286`): self-contained bring-up of
   `deploy/fdb-multi-replica`, unconditional teardown, hard error when opted in without
   Docker / Java / `$WYRD_ELLE_CLI_JAR` — never a silent skip. The live scenario lives in
   `crates/server/tests/` (env- and `fdb`-feature-gated), launched by xtask shelling out
   to `cargo test -p wyrd-server --features fdb --test <scenario>`; `xtask` gains no
   `wyrd-server`/FDB/JVM dependency, and the runner derives inconclusive/verdict/report
   from run-summary JSON + checker output alone — which keeps the gate arithmetic testable
   at Check.

2. **Workload reshaping — exclusion by pool construction, never per-op filtering.** The
   checker-verified models cannot represent every op the wire supports, and filtering
   individual ops out of a history fabricates order; whole-key exclusion is sound (Elle
   partitions per key). So the scenario runs three pools over the loopback
   server+gateway backed by `FdbMetadataStore`:
   - **Register overwrite pool (Elle-fed):** N processes drive overwriting PUT (unique
     version per write) and GET on a small shared key set. No DELETE — verified: a delete
     has no faithful rw-register encoding (a nil-write makes a *correct* history come back
     `false`; a 404-after-delete read maps to `nil`, indistinguishable from unwritten).
   - **Register delete pool (Wyrd-checked):** PUT/GET/DELETE traffic on a disjoint key
     set, judged by the landed INV-1-sound checks (`session_read_your_writes`,
     `reads_monotone_per_key`, the resurrection/lost-write logic) — in the run, counted in
     the summary, never serialized into the Elle register EDN.
   - **Directory pool (Elle-fed, `set` model):** create-only unique members during the
     fault window (each member assigned a unique **integer id**; the name↔id map goes in
     the summary and report — verified: jepsen's set checker requires integer elements,
     string elements crash the valid case to `:unknown`); after heal + quiesce, the
     scenario probes every member of the known universe sequentially and emits ONE
     composed full-set `:read` (Jepsen's own final-read pattern; sound because the set is
     no longer mutating). Mid-run probes and directory deletes may run for Wyrd-side
     checks and the concurrency witness but never enter the set EDN.
   The **INV-2 witness (`is_genuinely_concurrent`) binds to the Elle-fed register pool** —
   the non-vacuity gate attests concurrency where the verdict is claimed.

3. **The checker contract — every line verified against elle-cli 0.1.9 at Plan
   (2026-07-16, this host).** Checker: elle-cli (github.com/ligurio/elle-cli), the
   standalone-jar packaging of Elle; pin the release current at implementation (0.1.9 at
   Plan) and record version + jar SHA-256 in the report.
   - **Register:** `java -jar $WYRD_ELLE_CLI_JAR --model rw-register <register.edn>`.
     Entries `{:process P, :type :invoke|:ok|:fail|:info, :f :txn, :value [[:w "key" v]],
     :time N}` — `:value` is a **vector of micro-op vectors**; write `[[:w <key> <int>]]`,
     read invoke `[[:r <key> nil]]`, read completion `[[:r <key> <int-or-nil>]]` (nil =
     absent/unwritten). String keys verified OK; `:index` optional; `:info` completions
     verified fine in realistic histories. **Never emit `[[:w k nil]]`** (verified
     rejection).
   - **Directory:** `--model set <directory.edn>`. `{:f :add, :value <int>}` per create;
     one `{:f :read, :value #{<ints>}}` composed final read. **Integer elements only**;
     `:remove`/`:contains` verified rejected ("No matching clause" → `:unknown`).
   - **Verdict parsing:** per-file output line `<file> \t <token>`. `true` → pass;
     `false` → violation (run FAILURE); `:unknown` or anything else → INCONCLUSIVE.
     Exit codes verified: `true`→0, `false`→1, **`:unknown`→0** — so the parser keys on
     the token, and exit-status is only a cross-check. A checker stack trace on stderr
     with `:unknown` on stdout is the observed error shape (capture it as a fixture).
   `consistency_verdict_dispatch` stays the seat-chooser; `cargo xtask ci` stays JVM-free.

4. **Non-vacuity is a gate, not a note (unchanged, plus richer evidence).** Before any
   verdict is reported the runner asserts, over the run summary: (a) the INV-2 witness
   held on the Elle-fed register pool, and (b) the #407 fault materialized — carrying the
   leg's **typed materialization evidence** (which fault, which target, how it provably
   bit — `PartitionEvidence`/`PauseEvidence`/`SkewEvidence` serialized into the summary,
   the v2 T2 finding), never a bare hard-coded boolean. The summary also carries
   **per-op-kind outcome counts** (per pool: invoked / ok / fail / info) so a degenerate
   workload is visible. Failing either assertion makes the run inconclusive (non-zero).

5. **Fixtures, two-tier — now real samples.** Committed under
   `xtask/tests/fixtures/consistency-run/`: known-good and known-bad register and
   directory EDN histories **in the §3 vocabulary** (the Plan-time verified shapes), plus
   captured real checker outputs (`true`, `false`, `:unknown`+error). At Check they pin
   the verdict parser and the vocabulary as golden files; off-Check the runner's fixtures
   self-check feeds them through the real elle-cli on every live run (known-bad must come
   back `false` for the SAME model the live history uses, known-good `true`) — recorded in
   the report. The v2 gap — self-check covering only register fixtures — is closed: both
   models, both polarities.

6. **The report is the artifact — one deliverable.** Markdown under
   `target/consistency-run/`: workload parameters (pools, processes, ops), nemesis leg +
   typed materialization evidence, history sizes (per model), member-id map, models
   checked, checker + version + jar SHA-256, fixtures-self-check result, verdicts. **The
   PR is not marked ready until the first witnessed run's report — real `true` verdicts
   from both models under a materialized partition — is committed under
   `docs/design/reviews/`** (precedent: `m4-fdb-go-no-go.md`), produced by Do if its
   environment preflights green, otherwise by the maintainer's witnessed run at sign-off
   (the ready-mark is the human's step regardless). Acceptance = Check-core green at
   Check + the witnessed report present at sign-off.

## Alternatives considered

- **Export-time translation layer owned by #408 (leave #406 serializers untouched):**
  rejected — the landed vocabulary has zero consumers besides this runner and is
  checker-rejected; keeping it alive behind a translator preserves dead-wrong code and
  splits the format truth across two layers (a symptom-guard; principles 1.2 — the
  invariant is "the serialized history is checker-consumable", restored at its source).
- **set-full over probe-derived reads (the v2 pin):** falsified by the real checker —
  no `:contains`/`:remove` clauses, string elements crash, and mid-run single-member
  probes cannot compose an atomic set read. `set` with a post-heal composed final read is
  the model that states what we actually observe.
- **Encoding register deletes as nil-writes or tombstone values:** falsified — verified
  `false` on a correct history (nil-write), and a 404 observation cannot name which
  tombstone it saw (any definite mapping fabricates, breaking INV-1).
- **Full Clojure/Jepsen driver / homegrown in-gate checker / TiKV leg:** rejected as in
  v2 (ADR-0041 allows the Rust driver with the recognized checker; the artifact's value is
  outsider recognition; ADR-0042 pins FDB).

## Impact & compatibility

Additive harness/report code plus an in-place amendment of the #406 EDN serialization
layer (its only consumer is this pipeline; the consistency *checks* and the workload
substrate are untouched). No production crate's runtime behavior changes. `cargo xtask ci`
remains unprivileged, container-free, JVM-free. The committed report is a
`docs/design/reviews/` document — maintainer-authority territory, explicitly the human's
to accept at sign-off; no ADR/spec is edited (no immutability concern).

## Open questions

- None blocking. The v2 open questions are resolved: model = `set` (not set-full), pinned
  by Plan-time verification; elle-cli release = 0.1.9 (record version + SHA-256 in the
  report; take a newer release only if the fixtures self-check passes against it).

## Iteration carry-forward

- **v1 (rejected at sign-off):** hard-coded `nemesis_materialized = true`; no witnessed
  run. → §4 requires the typed evidence end-to-end; §6 requires the committed report.
- **v2 (iterated-to-Plan at sign-off, witnessed run 2026-07-16):** pipeline ran
  end-to-end on the live cluster; real Elle rejected both histories (scalar `:value`;
  `:contains`). → this brief's whole delta: §2 pools, §3 verified vocabulary, §5 real
  fixtures. Full attempts preserved in `iteration-v1/` and `iteration-v2/`;
  `iteration-v2/patch.diff` is a legitimate starting point (rebase onto `origin/main`).
- Do NOT re-attempt the rejected approaches unchanged; satisfy this brief's Success
  criterion (the end result).

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a draft PR
MAY happen during the cycle (useful for CI feedback). The PR MUST NOT be marked ready
before sign-off accepts — and per §6 above, not before the witnessed report is committed.

## Iteration 3 — carry-forward (from the previous attempt)
- Sign-off rationale: Human decision: the pure xtask core (parser, gate, report renderer, fixtures) held under adversarial re-verification and stays; the live scenario file must be fixed before the witnessed run is attempted. Builder must address, within the existing brief: 1. Missing Wyrd-checked DELETE pool — `drive_pools` (crates/server/tests/consistency_run_fdb.rs:326,351) drives only the overwrite and directory-create pools; add the disjoint PUT/GET/DELETE pool judged by the #406 Wyrd-side checks, with its counts in the run summary. 2. Stale-status fabrication — errored `put`/`get` inherit the PREVIOUS op's status via `history().ops().last() ... unwrap_or(0)` (consistency_run_fdb.rs:437-439, 478-479); a transport error must never serialize as a definite `:ok` (INV-1). 3. Dropped errored register ops — `let _ = c.put(...)` / `let _ = c.get(...)` (consistency_run_fdb.rs:360,371) silently omit indeterminate ops from the history; record them as `:info` per the synthetic-0 convention, never omit. 4. Report conformance — report must carry the elle-cli version string (not just jar sha256) and the member-id map must cross the RunSummary seam into the report (xtask/src/consistency_run_runner.rs:361-380); the map is currently emitted by the scenario but dropped. The privileged witnessed run (WYRD_TIER1=1 cargo xtask consistency-run on the live FDB topology) and the committed report under docs/design/reviews/ — the brief's acceptance artifact — will be produced in the NEXT iteration, after these fixes land; do not attempt the live leg before they do.
- Full previous attempt preserved in `iteration-v3/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 4 — carry-forward (from the previous attempt)
- Sign-off rationale: Rejected on the adversary review's two execution-verified defects in the live scenario file (crates/server/tests/consistency_run_fdb.rs — the one file no Check gate exercises); the pure core, serializer vocabulary, and golden fixtures are confirmed against the real elle-cli and should be kept as-is. Do NOT attempt the witnessed live run until these are fixed: 1. Delete pool fabricates violations on a correct system: per-process disjoint version BANDS on the shared DELETE_POOL_KEY break the commit-order-monotone assumption of all three #406 checks (verified by running the production checks). Fix by construction: per-process disjoint KEYS in the delete pool (single writer per key), not shared-key bands. 2. Composed final read silently omits Unknown-probed members, fabricating a "lost element" false from Elle (verified against the real jar). An Unknown probe must re-probe, abort, or degrade the composed read to :info — never silent omission. Also add the Design §2 quiesce before compose_final_read (currently runs immediately after drive_leg). Secondary, fix alongside: 3. Add #[serde(deny_unknown_fields)] to the nested seam objects (NemesisEvidence, OutcomeCounts) — serde does not propagate it from RunSummary; pin with a nested-unknown-field test, not only the top-level one. 4. Register unzip in the external-dependencies list (runner preflight reads the elle-cli version via `unzip -p`; elle-cli 0.1.9 has no --version flag). 5. Add a Check-time test covering the two-writer banded/delete-pool history shape so defect (1) cannot regress silently, and fix the ~2x directory_ops overstatement (sweep probes counted as history ops though they enter the EDN as one composed read).
- Full previous attempt preserved in `iteration-v4/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).
