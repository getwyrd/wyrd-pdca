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

Reviewing issue 470: add a first-class FoundationDB-capable `wyrd` OCI image with pinned client/cluster version checks and CI coverage.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The owed behavior is clear: a production-shaped FDB image plus mechanical version coupling, with Docker execution treated as supplementary sign-off rather than the container-free binding gate. |
| C2 Reproduction (red pre-fix) | PASS | Red was reproduced after stashing the patch: `cargo test -p xtask --test fdb_image` exited 101 because the `xtask` package had no `fdb_image` test target. |
| C3 Change | PASS | The patch creates the production image surface and its checks: pinned `FDB_VERSION` at `deploy/docker/wyrd/Dockerfile:28`, non-root runtime at `deploy/docker/wyrd/Dockerfile:122`, PR image workflow at `.github/workflows/fdb-image.yml:57`, and the consistency guard at `xtask/tests/fdb_image.rs:93`. |
| C4 Verification (red→green) | NEEDS-HUMAN | Human must accept or rerun the exact harness gate because targeted red→green passed and `cargo xtask ci` ended green, but the configured wrapper `./engine/xtask.sh ci` was absent here and Docker daemon access was denied. |
| C5 Causal adequacy | PASS | No symptom-guard smell found: the change adds the missing FDB-capable image and load-bearing version drift check rather than probing around an eager/runtime failure (`xtask/tests/fdb_image.rs:106`, `xtask/tests/fdb_image.rs:285`). |
| T1 Structure | PASS | The work stays in the intended slice: Dockerfile, workflow, and test-local helpers only, with no `xtask/src/` or deploy-profile migration (`xtask/tests/fdb_image.rs:20`, `xtask/tests/fdb_image.rs:35`). |
| T2 Shape | PASS | The file-shape obligations are covered: multi-stage build, parameterized features/version, pinned FDB client install, workflow path filters, and exact crate/compose/image line coupling (`deploy/docker/wyrd/Dockerfile:36`, `.github/workflows/fdb-image.yml:22`, `Cargo.toml:108`, `deploy/fdb-single-node/docker-compose.yml:22`). |
| T3 Runtime | NEEDS-HUMAN | Human must run the real image legs because this sandbox has Docker CLI/compose but cannot connect to `/var/run/docker.sock`; the current evidence proves files and tests, not `docker build`, `docker run`, FDB connect, or version-skew behavior (`.github/workflows/fdb-image.yml:59`). |
| T4 Contribution | NEEDS-HUMAN | Human must confirm non-local open/closed/rejected prior art because local affected-path history for the three new paths was empty and `HEAD` still lists only the old Dockerfiles, but PR-state history was not mechanically available here. |
| T5 Judgment | PASS | The patch respects the declared boundaries: it does not migrate the TiKV stack or edit `deploy/README.md`, and the workflow only builds `wyrd:fdb` from the new image definition (`.github/workflows/fdb-image.yml:57`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Human must decide production fitness after exercising the Docker-dependent path, because the operator artifact is the image itself and reviewer verification stopped at red→green tests plus `cargo xtask ci` due Docker socket denial. |

### Advisory — adversary

# Adversarial review — issue 470 (wyrd-fdb-oci-image)

Advisory only; never gates. Target: `/home/eddie/wyrd/wyrd.pdca-wt` @ `b1ccca3`
(origin/main, the `C4-verify` base). Binding gate is
`cargo test -p xtask --test fdb_image` (container-free).

## What I could NOT refute (attempted, failed)

- **Ran the binding gate green.** All 4 tests in `xtask/tests/fdb_image.rs` pass on
  target. The red is legitimate for a new-file bundle: pre-fix `read(DOCKERFILE)` panics
  because the files do not exist.
- **The consistency check is genuinely load-bearing, not vacuous.**
  `consistency_check_is_red_on_a_mismatched_fdb_version`
  (`xtask/tests/fdb_image.rs:491`) drives the *same* `check_fdb_version_consistency`
  (`:314`) the green test uses, against the *real* compose + `Cargo.toml`, with only a
  planted `FDB_VERSION=7.1.99`, and asserts the `Err` names `7.1.99`. Confirmed passing.
  I tried to find an equal-but-wrong drift that slips through: the `major_minor(v)=="7.3"`
  sanity assert (`:434`) and the three-way `df==tag`/`df_mm==tag_mm==crate_mm` checks
  (`:327`,`:336`) close that hole.
- **The `cargo build --bin wyrd --features "$FEATURES"` form is accepted at the virtual-
  workspace root** (`deploy/docker/wyrd/Dockerfile:158`). I feared a "--features not allowed
  in root of a virtual workspace" break; probing `cargo build --bin wyrd --features <x>`
  returned a feature-name error, not a workspace error, and `fdb`/`etcd` are real features on
  `crates/server/Cargo.toml:31,` so resolution holds.
- **The workflow smoke grep is correct, not the bug it looks like.**
  `.github/workflows/fdb-image.yml:81` `grep -Eq 'redb\|tikv\|fdb'`: under `-E`, `\|` is a
  *literal* pipe, and `crates/server/src/cli.rs:268` prints the literal
  `redb|tikv|fdb`, so it matches. Not a defect.

## Findings a human must adjudicate

- **NEEDS-HUMAN — the image's headline capability (multi-version `libfdb_c` loading via
  #441's `ExternalClientDirectory`) is inert on the tested base, and the binding gate never
  touches it.** `deploy/docker/wyrd/Dockerfile:186-194` bakes
  `/var/lib/wyrd/fdb/external-clients` and sets `ENV WYRD_FDB_EXTERNAL_CLIENT_DIR=...`, but
  on target `b1ccca3` **no wyrd source reads that env var or sets any
  `NetworkOption::ExternalClientDirectory`** — `crates/metadata-fdb/src/lib.rs:867`
  calls bare `foundationdb::boot()` with no options. The brief concedes this ("without 441
  the directory would be inert decoration") and defers the coupling to the #441 wave fold.
  Since the fold is not present in this checkout, confirm at sign-off that #441 actually
  landed in the folded base; if it did not, the image ships a labelled, ENV-advertised
  external-client directory that nothing loads — the exact silent-decoration failure the
  brief warns about. `xtask/tests/fdb_image.rs` asserts none of this.

- **NEEDS-HUMAN (toolchain unavailable; verdict provisional) — the runtime `cp
  /usr/lib/libfdb_c.so` (`deploy/docker/wyrd/Dockerfile:191`) assumes the
  `foundationdb-clients` `.deb` installs `libfdb_c.so` at exactly that path.** If the deb
  places it under a multiarch dir (`/usr/lib/x86_64-linux-gnu/`), the `cp` fails and the
  whole `docker build` errors — but the binding gate is a file-parse and never builds, so it
  would stay green. I cannot run `docker build` here to settle it; this must be confirmed by
  the deferred `.github/workflows/fdb-image.yml` run / builder leg (a), not scored as a
  refutation.

- **NEEDS-HUMAN — latent bash bug in the workflow's failure branch.**
  `.github/workflows/fdb-image.yml:78` `echo "expected \`wyrd\` with no subcommand..."`
  uses back-ticked ``\`wyrd\``` inside a double-quoted string, so bash runs command
  substitution on `wyrd` (not on PATH on the runner → "command not found" on stderr, empty
  substitution) instead of printing the literal word. The step still `exit 1`s correctly, so
  the guard's *outcome* is intact; only the diagnostic message is corrupted. Low impact,
  concrete, and in the deferred (ungated) workflow.

## Weak spots (not defects, noted for completeness)

- `workflow_exists_resolves_and_filters_the_fdb_surface` (`xtask/tests/fdb_image.rs:452`)
  iterates `cargo_xtask_subs`, but `fdb-image.yml` names **no** `cargo xtask` command, so the
  "every `cargo xtask <sub>` resolves" half of brief item 3 is satisfied *vacuously*. The
  `docker build -f <path>` half is exercised and does bind to `DOCKERFILE`.

## Bottom line

I attempted to refute (a) the red→green evidence, (b) the vacuity of the version-consistency
check, (c) the cargo build invocation, and (d) the smoke-grep — and could not break the
**binding gate**. Every substantive residual risk lives in the **deferred** half (the actual
`docker build`, the runtime `libfdb_c` path, and the #441-dependent external-client loading),
which the brief pre-declares as off-Check sign-off items. These are for the human to
adjudicate, not deterministic refutations of the patch.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C4 Verification (red→green) — Human must accept or rerun the exact harness gate because targeted red→green passed and `cargo xtask ci` ended green, but the configured wrapper `./engine/xtask.sh ci` was absent here and Docker daemon access was denied.
- [ ] T3 Runtime — Human must run the real image legs because this sandbox has Docker CLI/compose but cannot connect to `/var/run/docker.sock`; the current evidence proves files and tests, not `docker build`, `docker run`, FDB connect, or version-skew behavior (`.github/workflows/fdb-image.yml:59`).
- [ ] T4 Contribution — Human must confirm non-local open/closed/rejected prior art because local affected-path history for the three new paths was empty and `HEAD` still lists only the old Dockerfiles, but PR-state history was not mechanically available here.
- [ ] Validation — fitness-to-purpose — Human must decide production fitness after exercising the Docker-dependent path, because the operator artifact is the image itself and reviewer verification stopped at red→green tests plus `cargo xtask ci` due Docker socket denial.
- [ ] external dependency: #441 preflight module (wave fold) — absent from

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
- Iteration delta (if iterating): Rejected because the bundle was built and verified against a base WITHOUT #441: its preflight module was absent from the wave fold (crates/metadata-fdb/src had only lib.rs; lib.rs:868 still called bare foundationdb::boot()). #441 has since merged as PR #495. What to change next: 1. Re-fold onto the base that now contains #441 (PR #495) and exercise the two legs that were blocked and left honestly unverified: (c) `wyrd --metadata-backend fdb` connects, and (d) the #441 version-skew GUIDED error against a 7.1.x cluster within a bounded deadline (not an anonymous hang). These prove the baked external-client directory / WYRD_FDB_EXTERNAL_CLIENT_DIR is actually LOADED, not the inert decoration the brief warns about — the headline capability of the image and #441 acceptance criterion 3 "discharged here." The binding container-free gate and legs (a) build / (b) usage smoke already pass and need no rework. 2. Correct the adversarial reviewer feedback: - .github/workflows/fdb-image.yml:78 — the failure-branch echo uses back-ticked `wyrd` inside a double-quoted string, so bash runs command substitution instead of printing the literal word (guard outcome intact, diagnostic corrupted). Quote it so the literal `wyrd` prints. - Harden the runtime `cp /usr/lib/libfdb_c.so` (Dockerfile:191) against a multiarch install path (/usr/lib/x86_64-linux-gnu/) so a deb that lands the lib elsewhere does not silently break `docker build` while the file-parse gate stays green. - The `cargo xtask <sub>` half of the workflow↔dispatch check (xtask/tests/fdb_image.rs) is vacuous — fdb-image.yml names no `cargo xtask` command, so that assertion passes on an empty set. Either drop it or make the check meaningful for this workflow's real command surface.
- By / date: Eduard Ralph / 2026-07-10

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
