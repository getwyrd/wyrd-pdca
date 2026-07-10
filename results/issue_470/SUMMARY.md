# Result — issue 470 / wyrd-fdb-oci-image

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect / goal: Wyrd has **no first-class container image**. Verified against `origin/main`
- Success criterion: `cargo test -p xtask --test fdb_image` passes on the plain
- Repo + branch target: getwyrd/wyrd @ main
- Scope (one logical fix) / out of scope: Promote the ad-hoc test-Dockerfile build to a first-class, version-pinned OCI

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

Review target: promote Wyrd's FoundationDB backend to a first-class, version-pinned OCI image with a container-free consistency gate.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The decision owed is whether the scoped deliverable is the first-class FDB image plus file-based binding gate, while live Docker execution is explicitly deferred/off-Check; the brief states that split at `brief.md:24` and `brief.md:166`. |
| C2 Reproduction (red pre-fix) | PASS | The red condition is real enough for this slice: hiding the three patch files made `cargo test -p xtask --test fdb_image` fail with no such test target, and the added planted-red mismatch guard exercises the load-bearing version check at `xtask/tests/fdb_image.rs:328`. |
| C3 Change | PASS | The change addresses the intended surface without crossing the excluded write-sets: new Dockerfile, workflow, and local test helpers only, with version pins rooted at `deploy/docker/wyrd/Dockerfile:28`, `.github/workflows/fdb-image.yml:57`, and `xtask/tests/fdb_image.rs:35`. |
| C4 Verification (red→green) | PASS | The direct reruns are green: `cargo test -p xtask --test fdb_image` passed 4/4 after the red hide, and `cargo xtask ci` ended with `xtask ci: all checks passed`; the assertions cover shape, version coupling, workflow resolution, and planted red at `xtask/tests/fdb_image.rs:191`, `xtask/tests/fdb_image.rs:240`, `xtask/tests/fdb_image.rs:260`, and `xtask/tests/fdb_image.rs:328`. |
| C5 Causal adequacy | PASS | The root cause is missing production FDB image/version coupling, not a guarded runtime symptom: the Dockerfile installs pinned FDB clients in build and runtime and wires the accepted #441 external-client option at `deploy/docker/wyrd/Dockerfile:45`, `deploy/docker/wyrd/Dockerfile:76`, `deploy/docker/wyrd/Dockerfile:114`, and `crates/metadata-fdb/src/lib.rs:1119`. |
| T1 Structure | PASS | The scheduling decision is low-conflict: helpers stay test-local and no `xtask/src/` or `deploy/README.md` hunk is introduced, matching the local-helper pattern at `xtask/tests/fdb_image.rs:22`. |
| T2 Shape | PASS | The artifact shape meets the binding criterion: multi-stage build, `ARG FEATURES`, `ARG FDB_VERSION`, and non-root `USER` before `ENTRYPOINT` are grounded at `deploy/docker/wyrd/Dockerfile:28`, `deploy/docker/wyrd/Dockerfile:31`, `deploy/docker/wyrd/Dockerfile:36`, `deploy/docker/wyrd/Dockerfile:69`, `deploy/docker/wyrd/Dockerfile:134`, and `deploy/docker/wyrd/Dockerfile:135`. |
| T3 Runtime | NEEDS-HUMAN | The human must decide whether to accept the unexercised live image runtime: Docker is installed here but `/var/run/docker.sock` is not accessible, so `docker build`, usage smoke, FDB connect, and 7.1.x skew-error legs from `brief.md:57` through `brief.md:66` were not reproduced. |
| T4 Contribution | PASS | The contribution is intentionally a reusable FDB image skeleton, not GHCR publishing or deploy-profile consumption, so follow-on #469/#471 can wire it without this patch editing their files; the workflow/input surface is limited at `.github/workflows/fdb-image.yml:22` and `deploy/docker/wyrd/Dockerfile:11`. |
| T5 Judgment | NEEDS-HUMAN | Open/closed PR prior-art could not be mechanically settled because `gh` could not reach `api.github.com`; local merged-history checks for `.github/workflows/fdb-image.yml`, `deploy/docker/wyrd/Dockerfile`, and `xtask/tests/fdb_image.rs` found no prior commits, but the human must clear remote prior art. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Fitness remains a sign-off decision: the Rust gate proves pinned inputs, but the production artifact's actual build/connect/skew behavior is an external dependency not exercised here, exactly the deferred posture called out at `brief.md:170` and `brief.md:181`. |

### Advisory — adversary

# Adversarial review — issue #470 (wyrd-fdb-oci-image)

Advisory only; nothing here gates. Grounded on `$PDCA_TARGET` @ `f23848d`.

## Refutations of the evidence / verdict

- **NEEDS-HUMAN — the red→green gate does not exercise the defect it claims to fix.**
  The brief's Defect (§1/§2) is that the old Dockerfile installed only
  `cmake protobuf-compiler libssl-dev pkg-config` and **no** FDB client headers /
  `libfdb_c` / `libclang`, so the `fdb` feature cannot build and the runtime cannot
  link. The fix *does* install them (`deploy/docker/wyrd/Dockerfile:52-56` build stage,
  `:80-82` runtime stage). **But the binding gate asserts none of it:**
  `dockerfile_is_multistage_nonroot_and_parameterized`
  (`xtask/tests/fdb_image.rs:424-468`) checks only FROM-count, `COPY --from=build`,
  `USER`-before-`ENTRYPOINT`, `ARG FEATURES`, `ARG FDB_VERSION` — the words
  `foundationdb-clients`, `libclang`, `clang`, `fdbcli`, `libfdb_c` appear **nowhere**
  in the test (verified: zero matches). Concrete regression that keeps the gate green:
  delete `Dockerfile:52-56` (the `*,fdb,*` build-stage install). `cargo test -p xtask
  --test fdb_image` still passes — the image is back to the exact pre-fix broken state
  the brief describes, and the gate cannot tell. The C4-verify red is therefore red only
  because the new files are *absent* pre-fix (`read()` panics), not because the fix's
  substance is tested. The substance is entirely deferred to the off-gate `docker build`
  in `fdb-image.yml`. A human must decide whether a shape-only gate is adequate sign-off
  for a defect that is about *package contents*.

- **NEEDS-HUMAN — the headline capability (empty external-client dir is load-bearing and
  connects) is unverified by any gate; this is exactly what sank iteration v1.** The
  Dockerfile bakes an *empty* `WYRD_FDB_EXTERNAL_CLIENT_DIR`
  (`deploy/docker/wyrd/Dockerfile:114`, `:189-206` in the diff comment) and asserts, in
  prose only, that empty-dir + env-set is "byte-safe" and the linked primary connects
  normally, while a populated-with-own-version dir would misreport `Unreachable`. The
  #441 path that consumes it is present in the target
  (`crates/metadata-fdb/src/lib.rs:1114-1123`, `ensure_network` →
  `NetworkOption::ExternalClientDirectory`), so it is no longer inert decoration — but
  whether an *empty* dir actually connects (leg c) and whether a 7.1.x cluster yields the
  bounded #441 guided error rather than an anonymous hang (leg d) is pure runtime
  behaviour that **no row in `check-gates.json` exercises** (both are "human at
  sign-off"). v1 was rejected for leaving precisely these legs unverified. I cannot
  reproduce them here (no live cluster; `build-notes.md` withheld from this lens), so
  this is **provisional** — the human must confirm legs (c)/(d) were actually run this
  iteration and passed, not merely re-declared deferred.

## Attempted and could not refute

- **Smoke grep `fdb-image.yml:81` `grep -Eq 'redb\|tikv\|fdb'`.** Suspected a bug: in ERE
  (`grep -E`) `\|` is a *literal* pipe, not alternation, so this matches the literal
  string `redb|tikv|fdb`. But the usage output at `crates/server/src/cli.rs:277-281`
  literally prints `--metadata-backend redb|tikv|fdb` with pipes, so the match succeeds.
  Correct, if coincidentally. Could not refute. (Off-gate CI leg regardless.)

- **Version-consistency parsing.** `cargo_fdb_major_minor`
  (`xtask/tests/fdb_image.rs:305-316`) uses the *first* `fdb-` substring in `Cargo.toml`;
  I checked the target and the only `fdb-` is line 108 (`fdb-7_3`) — lines 18/51
  (`metadata-fdb`) contain `fdb` but not `fdb-`, so no earlier false match. Dockerfile
  `ARG FDB_VERSION=7.3.77` == compose `foundationdb/foundationdb:7.3.77` == crate `7.3`;
  the exact/major-minor checks all hold. The planted-red test
  (`:561-600`) drives the *same* `check_fdb_version_consistency` with `7.1.99` and
  genuinely errors naming the version — not vacuous. Could not refute the consistency
  logic on this tree. (Note for the human, not a break today: the first-match parse is
  fragile — a future `Cargo.toml` line introducing an earlier `fdb-` token would silently
  misread the version while the gate stays green.)

- **Workflow build↔run coupling** (`xtask/tests/fdb_image.rs:493-557`): `docker_builds`
  and `docker_run_images` correctly resolve `wyrd:fdb` as both built (`-t wyrd:fdb`) and
  run; the `--rm` / `--entrypoint fdbcli` flag skipping matches this workflow's exact
  token surface. Could not refute for this diff.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [x] T3 Runtime — The human must decide whether to accept the unexercised live image runtime: Docker is installed here but `/var/run/docker.sock` is not accessible, so `docker build`, usage smoke, FDB connect, and 7.1.x skew-error legs from `brief.md:57` through `brief.md:66` were not reproduced.
- [x] T5 Judgment — Open/closed PR prior-art could not be mechanically settled because `gh` could not reach `api.github.com`; local merged-history checks for `.github/workflows/fdb-image.yml`, `deploy/docker/wyrd/Dockerfile`, and `xtask/tests/fdb_image.rs` found no prior commits, but the human must clear remote prior art.
- [x] Validation — fitness-to-purpose — Fitness remains a sign-off decision: the Rust gate proves pinned inputs, but the production artifact's actual build/connect/skew behavior is an external dependency not exercised here, exactly the deferred posture called out at `brief.md:170` and `brief.md:181`.

## 7. Proven / not proven
- Proven by which oracle: gates overall = pass (stub oracles).
- Unproven / needs manual run: anything flagged in §6.

## 8. Ready-to-ship attachments
- patch.diff
- tracker-comment.md     (ALWAYS, every tracker item)
- build-notes.md         (builder rationale — for the human, not the reviewer)

## 9. Check sign-off                     ← human completes Check here
- Disposition confirmed / overridden:
- Outcome: merged-wider
- Iteration delta (if iterating):
- By / date: Eduard Ralph / 2026-07-10

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
