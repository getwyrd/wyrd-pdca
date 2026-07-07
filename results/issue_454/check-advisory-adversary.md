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
