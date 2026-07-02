# Brief — issue 286 / dserver-container-non-root-user

> The Plan artifact (docs 02 §PLAN). Human-authored. Do reads ONLY this file.

- **Slug:** dserver-container-non-root-user
- **Defect:** The D-server runtime container image runs as root. The runtime stage of
  `crates/chunkstore-grpc/tests/dserver/Dockerfile` starts from `debian:bookworm-slim`, copies
  `/usr/local/bin/wyrd`, exposes 50051, and sets `ENTRYPOINT ["wyrd"]` (`Dockerfile:22-26`) with
  no unprivileged user created and no `USER` directive — so the process runs as uid 0 by
  default. The compose service runs `d-server … --data-dir /data`
  (`crates/chunkstore-grpc/tests/docker-compose.yml:17`), so the container writes fragments
  under `/data`, which must be writable by the non-root user. Running as root is unnecessary for
  this role and enlarges blast radius if the process/image is compromised.
- **Success criterion:** The runtime image declares a dedicated unprivileged user and sets
  `USER <uid>:<gid>` before `ENTRYPOINT`, and the D-server can still start and write fragments to
  its `--data-dir` (`/data`) as that non-root user. BINDING observable: the built image runs the
  d-server role as a non-root uid and successfully writes a fragment. The specific uid/gid values
  and whether a named user or numeric uid is used are ILLUSTRATIVE (a Do call).
- **Repo + branch target:** getwyrd/wyrd @ main
- **Surfaces:** data
- **Difficulty:** low
- **Scope:** The D-server runtime image runs privileged because it never drops to a non-root
  user, and `/data` ownership is not arranged for a non-root writer. Make the image run the
  d-server role unprivileged with a writable data directory. / out of scope: the build stage
  (compilation stays as-is); the M4.5 deploy tree / production compose (#256, #367) beyond this
  test-fixture image; any change to the `wyrd` binary itself or its CLI.
- **Repro instruction:** On `main`, `docker build -f crates/chunkstore-grpc/tests/dserver/Dockerfile .`
  then `docker run --rm --entrypoint id wyrd-dserver:test` prints `uid=0(root)` — the container
  runs as root.
- **Test file:** crates/chunkstore-grpc/tests/dserver_image.rs (a Rust test asserting the
  Dockerfile encodes a non-root `USER` before `ENTRYPOINT` — flippable at Check; red pre-fix,
  green post-fix)
- **Verification posture:** DEFERRED / off-Check for the runtime behaviour. What IS built and
  exercised at Check: the Dockerfile change plus the named Rust test asserting a non-root `USER`
  directive is present (runs inside `cargo xtask ci`, red pre-fix / green post-fix — the
  load-bearing seam that the invariant is encoded in the image). What is DEFERRED: actually
  building the image and running the container to confirm the d-server starts and writes
  fragments as the non-root user — this needs a Docker host, which the `cargo xtask ci` gate does
  not provide (containers are deliberately outside the DST/gate substrate, INTEGRATION §3). WHO
  confirms the deferred green: a maintainer / CI job with Docker running the compose smoke path
  (`docker build` + `docker run … id` non-zero uid + a fragment write). Do should also capture a
  demonstrated red for the text-assertion test (it fails on the current root Dockerfile) so the
  encoded seam is shown load-bearing.
- **Off-Check verification instructions (HUMAN, at sign-off — Docker host required):** run these
  from the `../wyrd` checkout root and confirm all three before accepting §6:
  1. Build the image:
     `docker build -f crates/chunkstore-grpc/tests/dserver/Dockerfile -t wyrd-dserver:test .`
  2. Confirm the process is non-root:
     `docker run --rm --entrypoint id wyrd-dserver:test` → the printed `uid=` MUST be non-zero
     (NOT `uid=0(root)`).
  3. Confirm the non-root user can start the role and WRITE to its data dir — start the service
     as compose does and verify a fragment lands under `/data` owned by the non-root uid:
     `docker run --rm -d --name wyrd-nonroot-smoke wyrd-dserver:test d-server --bind 0.0.0.0:50051 --data-dir /data`,
     then exercise a put against the mapped port (or the existing `cargo xtask integration` path,
     which brings the compose cluster up and drives real writes), and finally
     `docker exec wyrd-nonroot-smoke sh -c 'ls -ln /data && id'` → the data files exist and are
     owned by the non-root uid, and `id` is non-zero. Tear down with
     `docker rm -f wyrd-nonroot-smoke`.
  If step 2 or 3 fails (e.g. `/data` is not writable by the dropped user), the fix is NOT
  complete — record it as a §6 open item and iterate; do not accept on the in-gate text
  assertion alone. The simplest full-path confirmation is `cargo xtask integration` on a Docker
  host: it builds this image and drives real fragment writes through the compose cluster, so a
  green integration run subsumes steps 1–3.
- **Citations expected:** Do must cite path:line on the target branch for every change.
- **Prior-art check (triage cycles):** searched by file path —
  `crates/chunkstore-grpc/tests/dserver/Dockerfile` last touched 8f44f88 / 5440282 (build
  hygiene, original add), neither adds a `USER`; no open PR referencing 286 or a non-root
  d-server image. No prior or in-flight fix.
- **Disposition hint:** likely-fix

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a
draft PR MAY happen during the cycle. The PR MUST NOT be marked ready before sign-off.

## Iteration 1 — carry-forward (from the previous attempt)
- Sign-off rationale: Primary reason: the bundle has NO advisory review — both Check reviewer leaves crashed on this run (claude `reviewer` exit 1 / no output; codex exit 1 / no output), so no independent verdict exists and the bundle cannot be accepted. The rebuild's Check must yield a working advisory review before sign-off. The patch itself looked sound on inspection and is not the reason for iterating: Dockerfile adds a --system dserver:dserver user (10001:10001), pre-creates and chowns /data, and sets USER before ENTRYPOINT (correct ordering); the dserver_image.rs test enforces the non-root-USER-before-ENTRYPOINT seam and C4-verify passed red->green. Keep this approach unless the (now-running) reviewer surfaces a real defect. Note: the gating C4-ci failure (cargo test --workspace exit 101) was base-drift / transient, NOT this patch — re-running the identical command on the applied patch (base now at #402 merge) is green, exit 0, zero failures, and the new dserver_image.rs test runs. Not a reason to iterate.
- Failing gate: C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance) — xtask: `cargo test --workspace --exclude wyrd-dst` failed with exit status: 101
- Full previous attempt preserved in `iteration-v1/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).
