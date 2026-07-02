# Build notes — issue 286 / dserver-container-non-root-user

## What I changed and why

`crates/chunkstore-grpc/tests/dserver/Dockerfile:22-26` (base commit `b91401a`,
`getwyrd/wyrd@main`) is the runtime stage: `FROM debian:bookworm-slim`, copy the
`wyrd` binary, `EXPOSE 50051`, `ENTRYPOINT ["wyrd"]` — no `USER`, so the process
Docker starts defaults to uid 0. The compose service runs `d-server … --data-dir
/data` (`docker-compose.yml:17`), so `/data` needs to exist and be writable by
whatever user the process runs as.

The fix (`Dockerfile:24-38` post-patch) adds, before `ENTRYPOINT`:

1. `groupadd --system --gid 10001 dserver && useradd --system --uid 10001 --gid
   dserver --no-create-home --shell /usr/sbin/nologin dserver` — a dedicated
   unprivileged system account. `debian:bookworm-slim` ships `passwd`
   (`groupadd`/`useradd`) as part of the Debian `essential` set, so no extra
   package install is needed.
2. `mkdir -p /data && chown dserver:dserver /data` — the compose `--data-dir
   /data` target, created and owned by the new user *before* the switch, since
   `USER` only changes the *process* uid, not retroactively the ownership of
   files already created as root during the build.
3. `USER dserver:dserver` — placed after the `chown` (so the chown itself still
   runs as root, which is required) and before `ENTRYPOINT` (Docker only applies
   `USER` to instructions/processes that come after it — a `USER` after
   `ENTRYPOINT` would be inert).

Fixed numeric uid/gid (10001) rather than an unnumbered `useradd` allocation:
the brief marks the specific uid ILLUSTRATIVE, but a fixed value keeps `id`
output deterministic for the human's manual smoke check (§ Off-Check
verification instructions) and avoids the image's uid drifting across rebuilds
if the base image's next free system uid changes.

## Alternatives considered

- **`USER 10001` (numeric only, no named account).** Slightly shorter (drops
  the `groupadd`/`useradd` line, ~2 lines saved) but `docker run --rm
  --entrypoint id` then prints `uid=10001 gid=0(root)` unless a matching group
  is also created — the default GID for an unmapped UID is 0, which reintroduces
  a root *group* membership even though the *user* is non-root. Rejected: it
  would make the human's step-2 manual check (`id` must show non-zero uid) pass
  while still leaving root-group write access on anything group-writable by
  root, which is exactly the kind of half-fix the brief's "if step 2 or 3 fails
  … the fix is NOT complete" language is guarding against.
- **`USER nobody:nogroup`** (the pre-existing `debian:bookworm-slim` account).
  Costs 0 extra lines (`RUN groupadd …` / `useradd …` fully dropped, just `RUN
  mkdir -p /data && chown nobody:nogroup /data` + `USER nobody:nogroup`) —
  genuinely cheaper. Ruled out because `nobody` is a shared, well-known
  low-privilege identity conventionally used as a *catch-all* de-privileged
  user across unrelated processes/containers on a host; a dedicated `dserver`
  account is the more defensible choice for a service that owns its own
  writable data directory (least-surprise: `docker exec … id` genuinely
  identifies "the d-server process," not "some anonymous nobody process"), and
  costs only 2 extra Dockerfile lines. Not a hard requirement of the brief
  (uid/gid choice is explicitly ILLUSTRATIVE) — noted here so a reviewer who'd
  prefer `nobody` can see the exact 2-line delta to switch, not just an
  adjective.
- **Chown the data dir at container *start* via an entrypoint wrapper script**
  instead of at build time. Rejected as unnecessary complexity for a
  test-fixture image with no host-mounted volume (no bind-mount in
  `docker-compose.yml` — `/data` is purely inside the image), so a build-time
  `chown` is sufficient and avoids adding a wrapper script + `ENTRYPOINT`
  change (out of scope: "any change to the `wyrd` binary itself or its CLI").

## Test

`crates/chunkstore-grpc/tests/dserver_image.rs` (new, per the brief's named test
file) parses the Dockerfile's *runtime* (last) stage only — it explicitly
segments stages on `FROM`, so a `USER`/`ENTRYPOINT` in the `build` stage
wouldn't false-pass — and asserts:

1. the runtime stage has an `ENTRYPOINT`;
2. the nearest preceding `USER` directive is non-root, rejecting all four forms
   `USER` accepts for root: `0`, `0:0`, `root`, `root:root` (case-insensitive).

This directly encodes the brief's BINDING observable's *build-time-checkable*
half ("the runtime image declares a dedicated unprivileged user and sets `USER
<uid>:<gid>` before `ENTRYPOINT`") using only `std::fs`/`std::path` — no Docker,
no heavy deps, so it runs happily under a headless `cargo test`.

Red→green, via the project's own runner (`cargo test`, the same command
`cargo xtask ci`'s `run_ci` invokes at `xtask/src/main.rs:550`
`cargo(&["test", "--workspace", "--exclude", "wyrd-dst"])`):

- **Pre-fix** (Dockerfile at `main`/`b91401a`, no `USER`):
  `cargo test -p wyrd-chunkstore-grpc --test dserver_image` →
  `runtime_stage_sets_non_root_user_before_entrypoint ... FAILED` — panics with
  "runtime stage of tests/dserver/Dockerfile must set a non-root USER before
  ENTRYPOINT (the d-server role currently defaults to uid 0 / root)".
- **Post-fix** (this patch applied): same command →
  `runtime_stage_sets_non_root_user_before_entrypoint ... ok`,
  `test result: ok. 1 passed; 0 failed`.
- Also ran the whole crate's test suite (`cargo test -p wyrd-chunkstore-grpc`)
  post-fix: all non-ignored tests pass (14 in `list_delete`/`read_fault_seam`/
  `round_trip`/etc., 9 in `tier2_kill_reconstruct`, the Tier-2 container tests
  stay `ignored` as designed — they need Docker, unrelated to this change).
- `cargo fmt --all -- --check`: clean (no reformatting needed — the Dockerfile
  isn't Rust-formatted, and the new Rust test file was written already
  `rustfmt`-clean; verified).
- `cargo clippy -p wyrd-chunkstore-grpc --all-targets`: clean, no warnings
  (workspace lints, incl. warnings-as-errors, come from root `Cargo.toml`
  `[workspace.lints]` per `xtask/src/main.rs:534-536`).

## What's deferred (per the brief's Verification posture)

Building the image and actually running the container (`docker run --rm
--entrypoint id wyrd-dserver:test`, then a real fragment write under `/data`)
needs a Docker host, which this environment/the `cargo xtask ci` gate does not
provide. Per the brief this is explicitly DEFERRED / off-Check, to be confirmed
by a maintainer per the brief's "Off-Check verification instructions" section
(build the image, confirm `id` prints non-zero uid, confirm a write lands under
`/data` owned by that uid) — or subsumed by a green `cargo xtask integration`
run on a Docker host, which builds this same image and drives real writes
through the compose cluster. I did not fabricate a Docker-based test to
sidestep this; the shipped test only proves the *encoded* seam (the Dockerfile
declares the non-root user correctly), which is the load-bearing regression
guard `cargo xtask ci` can actually run.

## Files

- `patch.diff` — `crates/chunkstore-grpc/tests/dserver/Dockerfile` (13 lines
  added) + new `crates/chunkstore-grpc/tests/dserver_image.rs` (106 lines).
- Test file lives at the brief's named path:
  `crates/chunkstore-grpc/tests/dserver_image.rs`.
