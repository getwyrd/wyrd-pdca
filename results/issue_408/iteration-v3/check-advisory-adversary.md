# check-advisory-adversary.md — issue 408 (m4-checked-consistency-run-elle-report, v3)

Skeptic's pass. Evidence attacks were re-run for real in this sandbox (cargo + java + the
pinned elle-cli 0.1.9 jar are all present on this host); findings below are grounded on the
target source at `/home/eddie/development/wyrd/wyrd.pdca-wt`.

## Refutation attempts that FAILED (the evidence held)

- **Fixture authenticity** — fed all five committed EDN fixtures through the REAL
  `elle-cli-0.1.9-standalone.jar` on this host: `register-history-known-good.edn` → `true`
  (exit 0), `known-bad` → `false` (exit 1), `directory-history-known-good.edn` → `true`,
  `known-bad` → `false`, `rejected-vocabulary.edn` → `:unknown` (exit 0). Every claim in the
  brief's Design §3/§5 reproduced byte-for-byte, including the captured checker-output
  format (`<file> \t <token>`, Clojure `println`'s space-tab-space) in
  `xtask/tests/fixtures/consistency-run/checker-output-*.txt`. The "REAL samples" claim is
  genuine, not hand-written.
- **Green leg** — `cargo test -p xtask --test consistency_run_orchestration`: 25/25 pass.
- **Red leg** — `origin/main` (`e0e39c1`) lacks `xtask/src/consistency_run.rs` and the test
  file entirely (both added by this patch), so the kept test cannot compile without the fix;
  the C4-verify red is real, not a vacuum.
- **Parser attacks** — `:unknown`-with-exit-0, `true`-with-nonzero-exit, empty/garbage
  output (`xtask/src/consistency_run.rs:1704-1730` region) all resolve
  inconclusive/never-pass; `self_check_matches` demands a genuine `false` for known-bad, so
  an `:unknown` cannot masquerade as a caught violation. Could not refute.
- **Leg-name mismatch** — `xtask::nemesis::NemesisLegKind::as_str` (`xtask/src/nemesis.rs:67`)
  emits `network-partition`/`clock-skew`/`process-pause`, all accepted by the scenario's
  match arms (`crates/server/tests/consistency_run_fdb.rs:156-186`). No mismatch.

## Findings

- NEEDS-HUMAN [impl] — **The Wyrd-checked register delete pool is missing.** Design §2 says
  "the scenario runs three pools" and scope item (3) names the "Wyrd-checked delete pool"
  (PUT/GET/DELETE on a disjoint key set, judged by the landed session/resurrection checks,
  counted in the summary). `drive_pools` (`crates/server/tests/consistency_run_fdb.rs:326`)
  drives only two: the overwrite pool and the directory create pool; no DELETE traffic runs
  anywhere, none of the #406 Wyrd-side checks are invoked in the live run, and the summary
  (`:225`) carries no delete-pool counts. Tellingly, the orchestration test's own fixture
  claims the missing pool exists: `xtask/tests/consistency_run_orchestration.rs:145`
  describes "register delete pool (Wyrd)" in its attesting workload string — a pool the
  scenario never runs. The brief's DELETE-resurrection/lost-write coverage silently vanished.

- NEEDS-HUMAN [impl] — **Stale-status fabrication in the creator and the final-read sweep —
  a false `false` on a correct run.** `ObservableS3Client::put`/`get` return `Err` WITHOUT
  recording an op (`crates/server/src/consistency_observable.rs:161-170,179-186` — the `?`
  fires before `history.ops.push`). The scenario then reads
  `c.history().ops().last().map(|op| op.status).unwrap_or(0)`
  (`crates/server/tests/consistency_run_fdb.rs:437-439` and `:478-479`) — so a
  transport-errored create inherits the PREVIOUS create's status (e.g. 200) and is
  serialized as a determinate `:ok` `:add` of an element that may never have been created.
  Concrete failing case: mid-window connection error on create *i*>0 after a successful
  create → EDN says `:ok`, the post-heal sweep doesn't find the element → the Elle set
  checker returns `false` on a correct history — exactly the INV-1 "fabricated determinate
  completion" class this brief exists to bury (the `unwrap_or(0)` synthetic-indeterminate
  guard only covers the empty-history first op). The same pattern at `:478-479` marks a
  member Present off the previous member's 200.

- NEEDS-HUMAN [impl] — **Errored register ops are dropped, not `:info`.**
  `let _ = c.put(...)` / `let _ = c.get(...)`
  (`crates/server/tests/consistency_run_fdb.rs:360,371`) discard `io::Error`, and the op
  never enters the history at all. A PUT whose response was lost after the request was sent
  may have committed; omitting it (instead of recording an indeterminate `:info`, the
  synthetic-0 convention #406's own module doc reserves for this) means a later read can
  observe a version with **no corresponding write in the EDN** → rw-register `false` on a
  correct run. INV-1 says indeterminate → `:info`, never a definite outcome — silent
  omission fabricates the definite outcome "never happened".

- NEEDS-HUMAN [impl] — **Report/fixture conformance vs the brief.** (a) Design §6 requires
  the report carry "checker + **version** + jar SHA-256" and the "**member-id map**":
  `write_report` (`xtask/src/consistency_run_runner.rs:361-380`) records only the jar
  sha256 (no elle-cli version string), and the member-id map never crosses the seam —
  `RunSummary` (`xtask/src/consistency_run.rs`, struct at the "run summary" section) has no
  `member_id_map` field, so the scenario's emitted map (`consistency_run_fdb.rs:217-220`)
  is silently dropped from the report. (b) The Verification posture names a captured
  "stack-trace error" checker output among the golden fixtures; only pass/fail/unknown are
  committed under `xtask/tests/fixtures/consistency-run/` — the error shape is exercised
  only by an inline `"garbage"` literal, not a real capture.

- NEEDS-HUMAN — **The acceptance artifact is absent and the live half is unverifiable
  here.** No witnessed run's report exists under `docs/design/reviews/` in this diff (the
  brief's §6 ready-mark precondition), and the live leg (cluster bring-up, `drive_leg`, the
  two real `true` verdicts) cannot be exercised in this sandbox — that half of the verdict
  is provisional and is the human's at sign-off. Note the specific rationalization risk in
  the gate row set: C4-ci/C4-verify all-green covers **none** of
  `crates/server/tests/consistency_run_fdb.rs`'s runtime behavior — the two INV-1 defects
  above live exactly in that Check-blind file, and would first surface as a spurious
  `false`/lost-element verdict in the witnessed run itself.

## Verdict

The pure xtask core (parser, gate, invocation, report renderer) and the serializer
vocabulary survived every attack I could mount, including re-running the real checker over
the committed fixtures. The live scenario file did not: two concrete
correct-history-comes-back-`false` cases (stale-status inheritance; dropped errored ops) and
one whole missing pool — all builder-iterable before the witnessed run is attempted.
