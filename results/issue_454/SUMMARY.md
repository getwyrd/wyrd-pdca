# Result — issue 454 / gateway-composes-over-cluster-backends

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect / goal: The `wyrd s3` gateway role hardcodes its backends instead of selecting
- Success criterion: Over an **in-process loopback cluster** of real gRPC D-servers
- Repo + branch target: getwyrd/wyrd @ feat/m4-production-metadata-backend   (M4 integration branch per INTEGRATION §2 — the M4 slices stack here, not on `main`)
- Scope (one logical fix) / out of scope: Wire the `wyrd s3` gateway role to select its backends by configuration —

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: likely-fix
- Confidence: medium
- Recommendation: (set by Do)

## 3. Correctness (Check — chain)
- C1 Spec: none — brief.md
- C2 Reproduction (red pre-fix): none — (no gate configured)
- C3 Change: none — patch.diff
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): pass — xtask ci: all checks passed
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: fail — run-verify.sh: patch.diff does not apply on origin/main — the bundle is stale; rebase Do.
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

Reviewing issue 454: wire `wyrd s3` to compose configured metadata, coordination, and chunk backends so gateway PUT/GET can use cluster D-server fanout instead of a private local chunk store.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The brief defines a falsifiable cluster-composition slice: configured redb/mem/fanout S3 PUT/GET must keep local chunks empty and land fragments on loopback D-servers (`brief.md:11`). |
| C2 Reproduction (red pre-fix) | PASS | In an archived pre-patch HEAD with only the new test added, `cargo test -p wyrd-server --test s3_gateway_cluster --no-run` fails on missing `serve_s3_role`, so the old surface cannot satisfy the criterion (`crates/server/tests/s3_gateway_cluster.rs:39`). |
| C3 Change | PASS | The patch routes `cmd_s3` through parsed backend selection and endpoint fanout, so the decision is whether this composition is the intended product behavior rather than a local-only gateway (`crates/server/src/cli.rs:1224`). |
| C4 Verification (red→green) | NEEDS-HUMAN | The red compile failure and post-fix no-run build passed, but this host denies loopback binds, so the human must decide whether CI/runtime evidence discharges the real gRPC S3 red→green (`crates/server/tests/s3_gateway_cluster.rs:54`). |
| C5 Causal adequacy | PASS | The fix removes the hardcoded gateway backend cause by composing the selected stores directly, with no symptom-guard capability probe added (`crates/server/src/cli.rs:1298`). |
| T1 Structure | PASS | The implementation keeps the composition boundary in `cli.rs` and exposes a testable role helper without changing the backend traits, limiting the architectural impact (`crates/server/src/cli.rs:1289`). |
| T2 Shape | PASS | The two-axis dispatch covers redb/mem by default and feature-gated tikv/etcd arms; `cargo check -p wyrd-server --features tikv,etcd` passed, so the required combinations compile (`crates/server/src/cli.rs:1352`). |
| T3 Runtime | NEEDS-HUMAN | The loopback runtime path could not be exercised in this sandbox because binding `127.0.0.1:0` returns `PermissionDenied`; human must rely on a runner that permits sockets (`crates/server/tests/s3_gateway_cluster.rs:115`). |
| T4 Contribution | NEEDS-HUMAN | Local merged history by affected paths shows prior S3/deploy work but no duplicate gateway-backend wiring; closed/rejected remote work was not mechanically available here, so human prior-art sign-off remains owed (`deploy/small-multi-node/docker-compose.yml:34`). |
| T5 Judgment | PASS | The patch stays within the briefed scope: code wiring, loopback test, and small-multi-node gateway config, while explicitly leaving the live 9-D-server TiKV/etcd demonstration to #455 (`deploy/small-multi-node/docker-compose.yml:40`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Human must decide whether compile-tested tikv/etcd arms plus an unrun local loopback test are sufficient for this advisory slice, because the live stack remains deferred and this host could not run socket tests (`deploy/small-multi-node/docker-compose.yml:393`). |

### Advisory — adversary

# Adversarial review — issue #454 (gateway-composes-over-cluster-backends)

Advisory only; I never gate. I reproduced the evidence with the toolchain present
(`cargo`/`protoc`/`rustc` all available under `$PDCA_TARGET`):

- `cargo test -p wyrd-server --test s3_gateway_cluster` → **GREEN** (1 passed).
- `cargo check -p wyrd-server --features etcd --tests` → **compiles** (EXIT 0).
- `cargo check -p wyrd-server --features "tikv etcd" --tests` → **compiles** (EXIT 0).

**Attempted to refute and could not:** the core composition is genuinely wired, not a
double or a parallel re-implementation. `cmd_s3` (`cli.rs:1260`) calls the same
`serve_s3_role` the test drives (`s3_gateway_cluster.rs:151`), over real tonic D-servers.
Match exhaustiveness across all four feature configs holds (the `#[cfg]` on each
`serve_s3_dispatch` arm mirrors the `#[cfg]` on the enum variants, `cli.rs:95/156`). The
by-value `Gateway::new(meta, chunks, coord)` in the etcd arms typechecks because
`Coordination` is implemented for `EtcdCoordination` by value (`coordination-etcd/src/store.rs:231`),
so the "peer wraps in `Arc`, this doesn't" angle is a non-issue. All four backend arms
actually compile. Assertions (1) round-trip and (2) fan-out-to-D-servers are real and
load-bearing. So the fix is not broken. My findings below are about the *reach of the
proof* and *overstated claims*, not a broken patch.

## Findings

- **NEEDS-HUMAN — The per-fix red→green (C4-verify) never ran; the RED is a compile-red by
  construction.** `check-gates.json` C4-verify = **fail**: "patch.diff does not apply on
  origin/main — the bundle is stale." So the deterministic red→green gate was blocked and
  the RED half was never mechanically demonstrated. The brief's own falsifiability is a
  *compile* red — the test imports `serve_s3_role`, a symbol this very diff introduces
  (`cli.rs:1289`) — so the pre-fix *behavioural* defect (fragments landing on the local
  `FsChunkStore`) is never observed failing; any new function name would satisfy that RED.
  I reproduced GREEN, but a human should note the red side rests on assertion, not a run.
  Also: the brief names branch `feat/m4-production-metadata-backend`, but the target
  worktree is on `feat/m4.5-deploy-tikv-pd-etcd` and verify tried `origin/main` — confirm
  this base/branch mismatch is benign, not a sign the bundle was cut against the wrong base.

- **NEEDS-HUMAN — The `tikv`/`etcd` `serve_s3_dispatch` arms are compiled by NO `cargo xtask ci`
  step, so the brief's "cfg-gated code that COMPILES at Check" (Verification posture b) is
  unwarranted.** The three feature-gated arms (`cli.rs:1358` `(Tikv,Mem)`, `cli.rs:1364`
  `(Redb,Etcd)`, `cli.rs:1371` `(Tikv,Etcd)`) are only built with `--features tikv`/`etcd`.
  `run_ci_steps` builds default features only; the sole feature-gated step,
  `feature_gated_checks()` (`xtask/src/main.rs:1044`), type-checks *`wyrd-metadata-tikv`* —
  never `wyrd-server` — and is itself gated on `WYRD_TIKV_TOOLCHAIN` (off by default,
  `xtask/src/main.rs:1062`). `etcd-conformance` builds `wyrd-coordination-etcd`, not
  `wyrd-server`. I confirmed the arms compile *by running the checks myself*, but the Check
  gate does not: a type error introduced into any of those three arms would ship GREEN. The
  Scope calls these "REQUIRED deliverables, genuinely wired — not stubbed", yet Check has no
  guard on them.

- **The test enters below the CLI flag-parse seam — the literal defect is not executably
  proven.** The defect is "`cmd_s3` honours neither `--metadata-backend`,
  `--coordination-backend`, nor `--endpoints`." The test calls `serve_s3_role` directly with
  hand-built `MetadataBackend::Redb` / `CoordinationBackend::Mem` / `Some(&endpoints)`
  (`s3_gateway_cluster.rs:151`), bypassing `cmd_s3`'s parsing
  (`resolve_backend`/`resolve_coordination_backend`/`parse_endpoints`/`parsed.flag("endpoints")`,
  `cli.rs:1224-1234`). Concrete failing case that stays GREEN: mistype `parsed.flag("endpoints")`
  as `parsed.flag("endpoint")` in `cmd_s3` — the gateway silently falls back to the
  single-node local-FS door, yet the cluster test still passes because it never exercises
  that line. The fix is correct in source, but the flag-wiring — the exact thing #454 is
  about — has no executable coverage.

- **The single-node (`--endpoints` absent) front door has zero test coverage.** The new test
  drives only the cluster arm (`Some(&serve_endpoints)`); the pre-existing `s3_http_wire.rs`
  builds `Gateway::new`/`S3Gateway::new` directly (`s3_http_wire.rs:52,79`), never touching
  `serve_s3_role`/`serve_s3_dispatch`. So the `None` arm (`cli.rs:1317` →
  `open_local_chunks`), which the Scope explicitly requires to preserve the #367
  first-deployment path, is refactored but re-verified by nothing. A regression there passes
  the gate.

- **Assertion (3) "local `data-dir/chunks` stays empty" is vacuous in the cluster arm.** In
  the `Some(endpoints)` path `open_local_chunks` is never called, so `data_dir/chunks` is
  never created; `count_fragments` then returns 0 via its `let Ok(...) else { return 0 }`
  early-out (`s3_gateway_cluster.rs:307`) no matter what. The assert
  (`s3_gateway_cluster.rs:450-457`) would pass even if the gateway wrote nothing at all. The
  brief presents this as a co-equal load-bearing assertion; it proves nothing on its own
  (assertions 1 and 2 carry the proof).

- **The headline invariant ("a fleet of gateways shares one logical store") is not
  demonstrated at Check.** The exercised redb+mem arm holds metadata in a *local* redb
  (`cli.rs:1354` `open_local_meta_redb`) and coordination in process-local `MemCoordination`
  (`cli.rs:1355`). Two such gateways would each keep private metadata/coordination — an
  object PUT via gateway0 could not be GET via gateway1 — so a fleet of redb+mem+fanout
  gateways is NOT one pool; only the (unexercised-at-Check) tikv+etcd arm achieves that. The
  single-gateway test proves "chunks fan out to shared D-servers", not "fleet is one pool".
  The brief scopes fleet-pooling to #455, so this is likely acceptable-as-deferred — flagged
  so the human doesn't read the GREEN as establishing the stated invariant.

- **NEEDS-HUMAN (deploy, off-Check) — the compose change makes every gateway require the
  image to carry BOTH the `tikv` AND `etcd` features; `etcd` is a NEW image requirement.**
  `docker-compose.yml:393,408,422` add `--metadata-backend tikv --coordination-backend etcd`
  to `gateway0/1/2`. The pre-existing custodian needed only `tikv`; the gateway now also
  needs `etcd`. If `wyrd-single-zone:local` is not built `--features tikv,etcd`,
  `resolve_coordination_backend` returns "coordination backend `etcd` requires building
  `wyrd` with `--features etcd`" (`cli.rs:170`) at startup and all three gateways crash-loop
  under `restart: on-failure`. No Dockerfile in this checkout builds that image, so the
  precondition is unverifiable here — confirm before #455 stands the stack up. (Minor: the
  gateway `depends_on` lists only `dserver0,dserver8` of the 9 fan-out endpoints,
  `docker-compose.yml:392`; `connect_fanout` dials all 9 up front, so startup relies on
  `restart: on-failure` to ride out unready peers.)

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [x] C4 Verification (red→green) — The red compile failure and post-fix no-run build passed, but this host denies loopback binds, so the human must decide whether CI/runtime evidence discharges the real gRPC S3 red→green (`crates/server/tests/s3_gateway_cluster.rs:54`).
- [x] T3 Runtime — The loopback runtime path could not be exercised in this sandbox because binding `127.0.0.1:0` returns `PermissionDenied`; human must rely on a runner that permits sockets (`crates/server/tests/s3_gateway_cluster.rs:115`).
- [x] T4 Contribution — Local merged history by affected paths shows prior S3/deploy work but no duplicate gateway-backend wiring; closed/rejected remote work was not mechanically available here, so human prior-art sign-off remains owed (`deploy/small-multi-node/docker-compose.yml:34`).
- [x] Validation — fitness-to-purpose — Human must decide whether compile-tested tikv/etcd arms plus an unrun local loopback test are sufficient for this advisory slice, because the live stack remains deferred and this host could not run socket tests (`deploy/small-multi-node/docker-compose.yml:393`).

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
- By / date: Eduard Ralph / 2026-07-06

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
- Harness bug (issue_454): C4-verify/publish base parser (`_brief_base`/`_clean_ref`, "backtick span wins over first token") takes the FIRST backtick span anywhere after `@`, so a backticked branch name in a trailing prose aside ("not on `main`") hijacks the base — resolved `origin/main` instead of `feat/m4-production-metadata-backend`, false-failing C4-verify ("patch does not apply — stale") and would misdirect publish's PR base. Fix: anchor the parse to the token immediately after `@`; don't let a backtick span in trailing prose win.
