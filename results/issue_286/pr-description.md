# PR description

## Summary
The d-server test container image ran its process as **root** (uid 0): running
`docker run --rm --entrypoint id wyrd-dserver:test` printed `uid=0(root)`, so the
service held full root privileges it never needs — unnecessary blast radius if the
process or image is compromised. This makes the image run the d-server role as a
dedicated unprivileged user with a writable data directory.

## What to look at
- `crates/chunkstore-grpc/tests/dserver/Dockerfile` — the runtime stage (the final
  `FROM debian:bookworm-slim`). The change adds an unprivileged account, prepares
  `/data`, and switches to that user before `ENTRYPOINT`.
- To exercise: `docker build -f crates/chunkstore-grpc/tests/dserver/Dockerfile -t
  wyrd-dserver:test .` then `docker run --rm --entrypoint id wyrd-dserver:test` — the
  printed `uid=` should now be non-zero. Bring the compose cluster up (or run `cargo
  xtask integration`) to confirm the role still writes fragments under `/data` as that
  user.

## Root cause
The runtime stage copied the `wyrd` binary and set `ENTRYPOINT ["wyrd"]` with no user
created and no `USER` directive, so Docker defaults the process to uid 0. The compose
service runs `d-server … --data-dir /data`, so whatever user the process runs as must
own a writable `/data`.

## Fix
Before the entrypoint, the runtime stage now: creates a dedicated `--system` group and
user `dserver` (fixed `10001:10001`, so the identity stays stable across rebuilds);
`mkdir -p /data` and `chown dserver:dserver /data` while still root, so the directory
is owned by the new user; and sets `USER dserver:dserver`. The switch is placed after
the ownership setup and before `ENTRYPOINT`, so it actually applies to the started
process while the process retains write access to its data directory. The build stage
is untouched.

## Verification
- **Claim:** The runtime image declares a dedicated unprivileged user and sets
  `USER <uid>:<gid>` before `ENTRYPOINT`, and the d-server can still write to `/data`
  as that non-root user.
- **Checked:** on `main`, `crates/chunkstore-grpc/tests/dserver/Dockerfile:21-26` is
  the runtime stage — no user, no `USER`, `ENTRYPOINT ["wyrd"]` at `:26` — so the role
  defaults to root; the service command `--data-dir /data` is at
  `crates/chunkstore-grpc/tests/docker-compose.yml:17`. After the fix the runtime stage
  creates `dserver` (10001:10001), owns `/data` to it, and sets `USER dserver:dserver`
  before the entrypoint.
- **Test:** `crates/chunkstore-grpc/tests/dserver_image.rs` parses the Dockerfile's
  final (runtime) stage and asserts a non-root `USER` precedes `ENTRYPOINT`, rejecting
  every root form (`0`, `0:0`, `root`, `root:root`). It fails on the pre-fix root image
  and passes after the fix; it runs under `cargo xtask ci`.
- **Manual (Docker host, runtime behaviour):** build the image; confirm `docker run
  --rm --entrypoint id wyrd-dserver:test` prints a non-zero uid; start `d-server
  --data-dir /data` and confirm a fragment lands under `/data` owned by uid 10001 — or
  a green `cargo xtask integration` run, which builds this image and drives real writes
  through the compose cluster and subsumes the above. This runtime check needs a Docker
  host, which the headless gate does not provide, so it is left for a maintainer/CI run.

Fixes [#286](https://github.com/getwyrd/wyrd/issues/286)
