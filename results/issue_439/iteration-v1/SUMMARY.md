# Result — issue 439 / fdb-dev-ci-harness

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect / goal: `cargo xtask fdb-conformance` (`xtask/src/main.rs:292`) exists and works —
- Success criterion: `cargo test -p xtask --test fdb_harness` passes on the plain
- Repo + branch target: getwyrd/wyrd @ main
- Scope (one logical fix) / out of scope: Give the shipped-but-ungated `fdb` backend a standing automated signal and an

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: new-feature
- Confidence: medium
- Recommendation: (set by Do)

## 3. Correctness (Check — chain)
- C1 Spec: none — brief.md
- C2 Reproduction (red pre-fix): none — (no gate configured)
- C3 Change: none — patch.diff
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): pass — xtask ci: all checks passed
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass — run-verify.sh: PASS — red without the fix, green with it.
- C5 Causal adequacy: none — reviewer + human sign-off

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 Contribution: none — (no gate configured)
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Review of issue 439: add standing CI coverage, feature-gated typechecks, and an actionable preflight for the FoundationDB metadata backend.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The owed decision is whether the patch covers the three stated gaps; the target now has a PR/nightly FDB workflow, a doctor module, and independent FDB typecheck rows at `.github/workflows/fdb-conformance.yml:37`, `xtask/src/fdb_doctor.rs:1`, and `xtask/src/lib.rs:61`. |
| C2 Reproduction (red pre-fix) | PASS | Stashing the patch made `cargo test -p xtask --test fdb_harness` fail with no such test target, and `origin/main` still shows only the old TiKV row while the planted-red coverage now exists at `xtask/tests/fdb_harness.rs:195`. |
| C3 Change | PASS | The owed decision is whether the implementation reaches the intended surfaces without scope creep; the changed paths are limited to the workflow, xtask, the new harness test, and the deny policy note, with no docs/design or Cargo.toml edits, and the dispatch is grounded at `xtask/src/main.rs:79`. |
| C4 Verification (red→green) | PASS | The binding gate is independently green after restore: `cargo test -p xtask --test fdb_harness` passed 16 tests, `cargo xtask ci` passed, and the two FDB feature checks compile from `xtask/src/lib.rs:73`. |
| C5 Causal adequacy | PASS | The owed decision is whether this fixes the missing signal rather than masking it; the workflow actually invokes `cargo xtask fdb-conformance`, the preflight feeds the same pure doctor verdict, and CI wires independent feature checks at `.github/workflows/fdb-conformance.yml:116`, `xtask/src/main.rs:327`, and `xtask/src/main.rs:1424`. |
| T1 Structure | PASS | The host-independent decision logic is in the lib target and the impure probes stay in the binary, preserving a testable seam at `xtask/src/lib.rs:18` and `xtask/src/main.rs:357`. |
| T2 Shape | PASS | The workflow follows the existing container-tier shape with PR paths, nightly/dispatch triggers, Docker check, timeout, cache, and failure artifact at `.github/workflows/fdb-conformance.yml:37`, `.github/workflows/fdb-conformance.yml:96`, and `.github/workflows/fdb-conformance.yml:134`. |
| T3 Runtime | NEEDS-HUMAN | Docker daemon access was not exercised in this sandbox, so the live `cargo xtask fdb-conformance` and hosted workflow runtime remain unverified; `CI=true cargo xtask fdb-conformance` stopped at Docker unavailable while the workflow depends on Docker at `.github/workflows/fdb-conformance.yml:96`. |
| T4 Contribution | PASS | The owed decision is whether the change adds durable review value; the harness pins workflow-dispatch/typecheck/drift behavior and the audit note records the `libfdb_c` blind spot at `xtask/tests/fdb_harness.rs:297`, `xtask/tests/fdb_harness.rs:397`, and `deny.toml:7`. |
| T5 Judgment | NEEDS-HUMAN | Local merged history shows no prior committed FDB workflow/doctor/harness paths, but closed/rejected PR prior art is not mechanically available in this checkout; a human must clear that external prior-art state before relying on the novelty judgment. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Human sign-off is owed on whether the GitHub-hosted FDB conformance signal is fit for purpose, because the reviewer could verify Rust gates and FDB feature compilation but not the Docker-backed live run required by `.github/workflows/fdb-conformance.yml:116`. |

### Advisory — adversary

# Adversarial review — issue 439 / fdb-dev-ci-harness (advisory, non-gating)

Method: re-ran `cargo test -p xtask --test fdb_harness` on `$PDCA_TARGET` (16/16 green),
then took a scratch copy of the patched tree and **mutated production code** to see what
the harness actually catches. Also drove the real `fdbcli` 7.3.77 present on this host
against a dead coordinator. Toolchain was fully available; no `NEEDS-HUMAN` below is a
sandbox artefact.

Two mutations that reinstate the exact defects the brief says the slice closes leave the
suite **green**. Both are reproduced below.

---

## Refuted — the evidence does not pin what it claims

- **NEEDS-HUMAN — `xtask/tests/fdb_harness.rs:298-312`: the workflow↔dispatch test passes on a workflow that runs no `xtask` command at all.**
  `xtask_subcommands` (`xtask/tests/fdb_harness.rs:281`) scrapes every `cargo xtask <sub>`
  token in the file, **including the header comment prose**. The workflow mentions
  `cargo xtask fdb-conformance` at `.github/workflows/fdb-conformance.yml:6` and `:33`, and
  `cargo xtask ci` at `:23` and `:67` — before any `run:` key is reached.
  *Concrete failing case, executed:* I deleted the only two steps that invoke xtask
  (`.github/workflows/fdb-conformance.yml:116-117` and `:128-130`), leaving the header,
  `docker info`, the client-package install and the artifact upload. All 16 tests still
  passed — including the one whose message reads *"the workflow must actually run
  `cargo xtask fdb-conformance` — that is the whole job."* The `subs.len() >= 2`
  non-vacuity guard at `:311` is satisfied by prose alone.
  This is gap 1 of the brief ("**no CI job runs it**"). The test asserts the workflow file
  *mentions* the subcommand, never that it *executes* it. Assertion 3 of the success
  criterion is satisfied for the wrong reason. A `run:`-scoped scrape (or asserting the
  token appears after a `run:` line) would restore the intended bind.

- **NEEDS-HUMAN — `xtask/src/main.rs:1433-1436`: the coupling hazard the brief calls "the reason for two parameters" is still reachable, and the patch's own comment says it is not.**
  Nothing in the test suite invokes `run_ci()`; `fdb_toolchain_available()`
  (`xtask/src/main.rs:1378`) has exactly one caller, `run_ci` at `:1435`, and no test
  exercises that argument wiring — `recorded_invocations` (`:1660-1668`) passes booleans
  directly to `run_ci_steps`.
  *Concrete failing case, executed:* changing `:1435` from `fdb_toolchain_available()` to
  `tikv_toolchain_available() && fdb_toolchain_available()` — i.e. brief hazard (ii)
  verbatim, the FDB typecheck firing only when `WYRD_TIKV_TOOLCHAIN` is also set — leaves
  `cargo test -p xtask` **entirely green** (19+6+12+16+3+1+2 passed, 0 failed).
  The comment at `xtask/src/main.rs:1713-1714` states: *"Setting `fdb_toolchain` to
  `tikv_toolchain` in `run_ci` … flips this red."* **It does not.** (Only the naked
  `tikv_toolchain_available(), tikv_toolchain_available()` form goes red, and by accident —
  a `dead_code` lint on the now-unused fn, not by any assertion.) `xtask/src/lib.rs:54-55`
  makes the same unwarranted claim: the fdb_harness assertion covers the *pure function's*
  signature, never the call site that chooses its arguments.

- **`xtask/src/fdb_doctor.rs:14` — "one doctor, two call sites" is asserted in prose, pinned by nothing.**
  *Concrete failing case, executed:* deleting the whole preflight block
  (`xtask/src/main.rs:327-340`) leaves `cargo test -p xtask` green. Likewise no test reaches
  `run_fdb_doctor` (`:369`) or any `probe_*` fn (`:396`, `:417`, `:433`). The load-bearing
  red (`fdb_harness.rs:290`, the planted outcome) exercises `diagnose` → `is_ok` →
  `into_result`, a pure mapping; it does not witness that production ever calls it. That is
  the honest scope of the red→green in `check-gates.json` (`C4-verify`), and it is narrower
  than the brief's "the row logic is load-bearing" reads.

## Refuted — inputs that break the fix

- **`xtask/src/main.rs:396-413` (`probe_client_library`): a supported, working FDB build is reported as a missing client library.**
  `foundationdb-sys` 0.10's `build.rs:61-64` emits `cargo:rustc-link-lib=fdb_c` plus a
  `rustc-link-search` derived from **`FDB_CLIENT_LIB_PATH`**. The probe checks five
  hard-coded paths (`xtask/src/fdb_doctor.rs:50-56`) and `ldconfig -p`.
  *Concrete failing case:* `libfdb_c.so` under `/opt/foundationdb/lib` with
  `FDB_CLIENT_LIB_PATH=/opt/foundationdb/lib` and no `ld.so.conf` entry — the build links
  fine, the probe reports `Failed`. Consequences, both new with this patch: locally
  `cargo xtask fdb-conformance` now prints a warning and **`return Ok(())`**
  (`xtask/src/main.rs:339`) — exit 0, five test legs silently not run, a false green where
  the suite previously ran and passed; in CI (`CI=true`) it **hard-fails**
  (`:333`) a job whose build would have succeeded.
  The doc comment at `xtask/src/main.rs:393-395` — "presence on the linker's search path is
  exactly the property that decides whether the build can succeed" — is therefore false in
  both directions. Reading `FDB_CLIENT_LIB_PATH` as a fourth candidate source would close it.

- **`.github/workflows/fdb-conformance.yml:129` vs `:42`: the PR leg never type-checks the server's `fdb` arms, which is half of gap 2.**
  The path filter includes `crates/server/**` (`:42`) precisely because, per the brief, the
  `#[cfg(feature = "fdb")]` arms in `crates/server/src/cli.rs` are never compiled. But the
  only step that runs `cargo check -p wyrd-server --features fdb` is `cargo xtask ci`, and
  it is skipped on `pull_request` (`:129`). The PR leg runs only `cargo xtask
  fdb-conformance`, whose legs are `cargo test -p wyrd-metadata-fdb --features fdb`
  (`xtask/src/main.rs:566`) — the server crate is never built with the feature on.
  *Concrete failing case:* introduce a type error inside a `#[cfg(feature = "fdb")]` arm of
  `crates/server/src/cli.rs`. `ci.yml` is green (default features); `fdb-conformance.yml`'s
  PR leg is green; the error surfaces only on the 06:00 UTC cron, up to 24h after merge.
  For a `crates/server/**` PR the filter buys nothing it claims to buy.

## Attempted and could not refute

- **The `HEALTHY_STATUS_NEEDLE` (`xtask/src/fdb_doctor.rs:84`).** I ran the real `fdbcli`
  7.3.77 on this host against an unreachable coordinator: stdout is ``The database is
  unavailable; type `status' for more information.`` and **exit code 0**.
  `cluster_status_is_healthy` correctly returns `false`, and matching on text rather than
  exit status is demonstrably the *right* predicate here. No false positive found.
- **`probe_cluster_health` (`xtask/src/main.rs:433-436`) has no timeout on `Command::output()`.**
  I expected a hang on a dead coordinator; measured, `fdbcli` self-bounds and returns in 5s.
  Not a hang. (Adjacent, and *pre-existing*, so out of this diff's scope but worth the
  human's eye: `configure_fdb_database`'s readiness poll at `xtask/src/main.rs:483-508`
  gates on `status.success()` with stdout/stderr nulled — given the exit-0 result above it
  returns `Ok` on attempt 1 whether or not the database is available. This patch introduces
  the tested predicate that would fix it and does not use it there.)
- **`cluster_file_path` (`xtask/src/fdb_doctor.rs:113`)** genuinely mirrors the driver's
  `config::cluster_file` unset/blank/trim semantics (`crates/metadata-fdb/src/lib.rs:424-431`).
- **The drift guards** (`fdb_harness.rs:449`, `:475`) really do read the driver, the compose
  file and the workflow; a partial version bump does go red. `deny.toml`'s header is a
  comment block and `cargo deny` is unaffected (`C4-ci` green, re-run).
- **`actions/checkout@v7` / `actions/upload-artifact@v7`** are consistent with every other
  workflow in this repo (`ci.yml:92`, `integration-nightly.yml:38,59`), not a fabrication.

## On the verdict

`check-gates.json`'s `C4-verify` — *"red without the fix, green with it"* — is literally
true but weak: 15 of the 16 tests are red pre-fix by **non-existence** (`xtask::fdb_doctor`
does not compile), and the sole planted-red exercises a pure two-line mapping. The three
production seams the slice exists to create — the workflow's `run:` steps, `run_ci`'s
choice of `fdb_toolchain_available()`, and `run_fdb_conformance`'s preflight — are each
deletable with the suite still green. A reviewer reading the module docs
(`fdb_doctor.rs:14`, `lib.rs:54-55`, `main.rs:1713`) would take those seams as covered;
they are asserted in prose, not in tests.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] T3 Runtime — Docker daemon access was not exercised in this sandbox, so the live `cargo xtask fdb-conformance` and hosted workflow runtime remain unverified; `CI=true cargo xtask fdb-conformance` stopped at Docker unavailable while the workflow depends on Docker at `.github/workflows/fdb-conformance.yml:96`.
- [ ] T5 Judgment — Local merged history shows no prior committed FDB workflow/doctor/harness paths, but closed/rejected PR prior art is not mechanically available in this checkout; a human must clear that external prior-art state before relying on the novelty judgment.
- [ ] Validation — fitness-to-purpose — Human sign-off is owed on whether the GitHub-hosted FDB conformance signal is fit for purpose, because the reviewer could verify Rust gates and FDB feature compilation but not the Docker-backed live run required by `.github/workflows/fdb-conformance.yml:116`.

## 7. Proven / not proven
- Proven by which oracle: gates overall = pass (stub oracles).
- Unproven / needs manual run: anything flagged in §6.

## 8. Ready-to-ship attachments
- patch.diff
- tracker-comment.md     (ALWAYS, every tracker item)
- build-notes.md         (builder rationale — for the human, not the reviewer)

## 9. Check sign-off                     ← human completes Check here
- Disposition confirmed / overridden:
- Outcome: iterated-to-Do
- Iteration delta (if iterating): The brief and the design stand; the workflow itself is largely right. What fails is that the tests meant to pin the three seams this slice exists to create are vacuous, and shipped comments assert coverage that does not exist. Rebuild against the same brief — do NOT re-plan. Note for the rebuild: the primary reviewer returned PASS on all of C1-C5/T1-T4. It was the adversarial pass, which mutated production code and watched the suite stay green, that found these. Trust the mutations, not the PASS row. WHAT TO FIX (four items, all local) 1. The workflow<->dispatch test passes on a workflow that runs no xtask command. `xtask_subcommands` (xtask/tests/fdb_harness.rs:281) scrapes `cargo xtask <sub>` from the WHOLE workflow file text, including the `#` header comment prose, which mentions fdb-conformance, ci and fdb-doctor before any `run:` key is reached. Deleting both steps that actually invoke xtask leaves all 16 tests green — the `subs.len() >= 2` non-vacuity guard at :311 is satisfied by comments alone. So success-criterion assertion 3 is met for the wrong reason. FIX: scope the scrape to `run:` lines (or assert the token appears after a `run:` key), so the test binds execution rather than mention. 2. False coverage claims in shipped comments. xtask/src/main.rs:1713-1714 states "Setting `fdb_toolchain` to `tikv_toolchain` in `run_ci` ... flips this red." It does not — that exact mutation leaves `cargo test -p xtask` entirely green. xtask/src/lib.rs:54-55 repeats the claim. Nothing invokes `run_ci()`; `recorded_invocations` passes booleans straight to `run_ci_steps`, never exercising the call site that chooses the arguments. Likewise "one doctor, two call sites" (xtask/src/fdb_doctor.rs:14) is pinned by nothing: deleting the whole preflight block (main.rs:327-340) leaves the suite green. FIX: cover the `run_ci` call site and the preflight call site, OR delete the comments. A comment claiming a red that does not exist is worse than no test — a future reviewer trusts a guard that is not there. 3. `probe_client_library` reports a working FDB build as a missing client library. CLIENT_LIBRARY_CANDIDATES (xtask/src/fdb_doctor.rs:50-56) checks five hard-coded paths plus `ldconfig -p`, but never reads FDB_CLIENT_LIB_PATH — the variable foundationdb-sys 0.10's own build.rs:61-64 uses to locate the library. A lib under /opt/foundationdb/lib with FDB_CLIENT_LIB_PATH set and no ld.so.conf entry links fine and probes Failed. Consequences, both NEW with this patch: locally `cargo xtask fdb-conformance` warns and `return Ok(())` (main.rs:339) — exit 0 with five test legs silently skipped, a FALSE GREEN where the suite previously ran and passed; in CI it hard-fails a job whose build would have succeeded. The doc comment at main.rs:393-395 is false in both directions. FIX: read FDB_CLIENT_LIB_PATH as a fourth candidate source. 4. The PR leg never type-checks the server's `fdb` arms — half of gap 2. `cargo xtask ci` carries `if: github.event_name != 'pull_request'` (.github/workflows/fdb-conformance.yml:129/135), so on a PR the only xtask step is `cargo xtask fdb-conformance`, whose legs build `-p wyrd-metadata-fdb` only. The server crate is never compiled with the feature on. Yet the path filter includes `crates/server/**` (:42) precisely because those `#[cfg(feature = "fdb")]` arms are never compiled. A type error in one is green on ci.yml and green on the PR leg, and surfaces only on the 06:00 UTC cron, up to 24h after merge. FIX: either run `cargo check -p wyrd-server --features fdb` on the PR leg, or drop `crates/server/**` from the path filter. Do not leave the filter promising a check it does not perform. WHAT SURVIVED ADVERSARIAL ATTACK — keep as-is, do not churn - HEALTHY_STATUS_NEEDLE (fdb_doctor.rs:84): matching on text rather than exit status is demonstrably the right predicate; real fdbcli 7.3.77 exits 0 against a dead coordinator. No false positive found. - The drift guards (fdb_harness.rs:449, :475) genuinely read driver, compose file and workflow; a partial version bump does go red. - cluster_file_path (fdb_doctor.rs:113) faithfully mirrors config::cluster_file. - probe_cluster_health has no timeout but fdbcli self-bounds at 5s. Not a hang. - actions/checkout@v7, actions/upload-artifact@v7 match every other workflow here. OUT OF SCOPE — recorded as a §10 Act candidate, do NOT fix in this rebuild `configure_fdb_database`'s readiness poll (main.rs:483-508) gates on `status.success()` with stdout/stderr nulled; given fdbcli's exit-0-when-unavailable behaviour it returns Ok on attempt 1 regardless. Pre-existing, not introduced by this diff. This patch adds the tested predicate that would fix it, but wiring it there is a separate slice.
- By / date: Eduard Ralph / 2026-07-10

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
- File a bug: `configure_fdb_database`'s readiness poll (`xtask/src/main.rs:483-508`) gates on `status.success()` with stdout nulled, but `fdbcli` exits 0 even when the database is unavailable — so it returns `Ok` on attempt 1 regardless. Pre-existing, out of scope for this diff; this patch adds the tested predicate that would fix it.
