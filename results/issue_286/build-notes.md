# Build notes — issue 286 / dserver-container-non-root-user (iteration 2)

## Carry-forward disposition

Iteration 1's sign-off rationale (`brief.md:77-79`) rejected the bundle **only**
because both Check reviewer leaves crashed (no advisory review existed to weigh
at sign-off) — the reviewer subprocess crash is a Check-leaf/tooling problem,
not something Do produces or can fix by changing the patch. The rationale is
explicit: *"The patch itself looked sound on inspection and is not the reason
for iterating... Keep this approach unless the (now-running) reviewer surfaces
a real defect."* It also notes the one failing gate cited (C4-ci `cargo test
--workspace` exit 101) was **base-drift / transient**, not caused by this
patch: re-running the identical command against the applied patch, with the
base already at `#402` merge, was green (exit 0, zero failures).

**What I did differently this iteration:** nothing to the fix itself — per the
carry-forward instruction ("do NOT re-attempt the rejected approach unchanged"
only applies to a *rejected* approach; this approach was never rejected on its
merits, only orphaned by a reviewer-tooling crash). I rebuilt the identical
patch from scratch against the current worktree base (still `b91401a`, the
same `#402`-merge commit iteration 1 built against — confirmed via `git log -1
--oneline` in `$PDCA_WORKTREE`, matching `iteration-v1/build-notes.md`'s cited
base), re-derived it with `git diff` rather than copying the old `patch.diff`
byte-for-byte (so it's provably fresh against this run's tree, not a stale
artifact), and re-ran the full red→green cycle plus `fmt`/`clippy` myself
rather than trusting the prior run's notes. The resulting diff is
content-identical to iteration 1's, which is expected: the base didn't move
and the approach wasn't faulted.

## What I changed and why

`crates/chunkstore-grpc/tests/dserver/Dockerfile:21-26` (base `b91401a`,
`getwyrd/wyrd@main`, confirmed via `git log -1 --oneline` in
`$PDCA_WORKTREE`) is the runtime stage: `FROM debian:bookworm-slim`, copy the
`wyrd` binary, `EXPOSE 50051`, `ENTRYPOINT ["wyrd"]` — no `USER`, so the
process Docker starts defaults to uid 0 (confirmed via the brief's repro
instruction description; not re-run here since it needs a Docker host, see
"What's deferred" below). The compose service runs `d-server … --data-dir
/data` (`crates/chunkstore-grpc/tests/docker-compose.yml:17`), so `/data`
needs to exist and be writable by whatever user the process runs as.

The fix (`Dockerfile:24-38` post-patch, see `patch.diff`) adds, before
`ENTRYPOINT`:

1. `groupadd --system --gid 10001 dserver && useradd --system --uid 10001
   --gid dserver --no-create-home --shell /usr/sbin/nologin dserver` — a
   dedicated unprivileged system account. `debian:bookworm-slim` ships
   `passwd` (`groupadd`/`useradd`) as part of the Debian `essential` set, so no
   extra package install is needed.
2. `mkdir -p /data && chown dserver:dserver /data` — the compose `--data-dir
   /data` target, created and owned by the new user *before* the switch, since
   `USER` only changes the *process* uid, not retroactively the ownership of
   files already created as root during the build.
3. `USER dserver:dserver` — placed after the `chown` (so the chown itself
   still runs as root, which is required) and before `ENTRYPOINT` (Docker only
   applies `USER` to instructions/processes that come after it — a `USER`
   after `ENTRYPOINT` would be inert).

Fixed numeric uid/gid (10001) rather than an unnumbered `useradd` allocation:
the brief marks the specific uid ILLUSTRATIVE (`brief.md:18`), but a fixed
value keeps `id` output deterministic for the human's manual smoke check
(brief's "Off-Check verification instructions") and avoids the image's uid
drifting across rebuilds if the base image's next free system uid changes.

## Alternatives considered

- **`USER 10001` (numeric only, no named account).** Slightly shorter (drops
  the `groupadd`/`useradd` line, ~2 lines saved) but `docker run --rm
  --entrypoint id` then prints `uid=10001 gid=0(root)` unless a matching group
  is also created — the default GID for an unmapped UID is 0, which
  reintroduces a root *group* membership even though the *user* is non-root.
  Rejected: it would make the human's step-2 manual check (`id` must show
  non-zero uid) pass while still leaving root-group write access on anything
  group-writable by root, which is exactly the kind of half-fix the brief's
  "if step 2 or 3 fails … the fix is NOT complete" language (`brief.md:59-61`)
  is guarding against.
- **`USER nobody:nogroup`** (the pre-existing `debian:bookworm-slim` account).
  Costs 0 extra lines (`RUN groupadd …`/`useradd …` fully dropped, just `RUN
  mkdir -p /data && chown nobody:nogroup /data` + `USER nobody:nogroup`) —
  genuinely cheaper. Ruled out because `nobody` is a shared, well-known
  low-privilege identity conventionally used as a *catch-all* de-privileged
  user across unrelated processes/containers on a host; a dedicated `dserver`
  account is the more defensible choice for a service that owns its own
  writable data directory (least-surprise: `docker exec … id` genuinely
  identifies "the d-server process," not "some anonymous nobody process"), and
  costs only 2 extra Dockerfile lines. Not a hard requirement of the brief
  (uid/gid choice is explicitly ILLUSTRATIVE, `brief.md:18`) — noted here so a
  reviewer who'd prefer `nobody` can see the exact 2-line delta to switch, not
  just an adjective.
- **Chown the data dir at container *start* via an entrypoint wrapper script**
  instead of at build time. Rejected as unnecessary complexity for a
  test-fixture image with no host-mounted volume (no bind-mount in
  `docker-compose.yml` — `/data` is purely inside the image), so a build-time
  `chown` is sufficient and avoids adding a wrapper script + `ENTRYPOINT`
  change (out of scope per the brief: "any change to the `wyrd` binary itself
  or its CLI", `brief.md:26`).

## Test

`crates/chunkstore-grpc/tests/dserver_image.rs` (new, per the brief's named
test file, `brief.md:30`) parses the Dockerfile's *runtime* (last) stage
only — it explicitly segments stages on `FROM`, so a `USER`/`ENTRYPOINT` in
the `build` stage wouldn't false-pass — and asserts:

1. the runtime stage has an `ENTRYPOINT`;
2. the nearest preceding `USER` directive is non-root, rejecting all four
   forms `USER` accepts for root: `0`, `0:0`, `root`, `root:root`
   (case-insensitive).

This directly encodes the brief's BINDING observable's *build-time-checkable*
half ("the runtime image declares a dedicated unprivileged user and sets
`USER <uid>:<gid>` before `ENTRYPOINT`", `brief.md:14-15`) using only
`std::fs`/`std::path` — no Docker, no heavy deps, so it runs happily under a
headless `cargo test` (no GUI/display dependency at load, satisfying the
headless-runner constraint).

### Red→green, this run, via the project's own runner

Ran directly in `$PDCA_WORKTREE` (`/home/eddie/wyrd/wyrd.pdca-wt-l1`, the
isolated worktree off `main` the driver provides — the host's primary
checkout was never touched), using `cargo test`/`cargo fmt`/`cargo clippy`,
the same underlying tool `cargo xtask ci`'s `run_ci` invokes for the test step
(`xtask/src/main.rs:550`: `cargo(&["test", "--workspace", "--exclude",
"wyrd-dst"])`) — scoped to the touched crate for a fast sanity pass rather
than the full multi-minute workspace gate, per the "fast sanity pass" allowance
(Check's gates re-run the real suite):

1. **Pre-fix (red):** `git stash push -- crates/chunkstore-grpc/tests/dserver/Dockerfile`
   (reverting only the Dockerfile back to the unmodified `main` content, new
   test file left in place) then `cargo test -p wyrd-chunkstore-grpc --test
   dserver_image`:
   ```
   test runtime_stage_sets_non_root_user_before_entrypoint ... FAILED
   thread '...' panicked at crates/chunkstore-grpc/tests/dserver_image.rs:95:9:
   runtime stage of tests/dserver/Dockerfile must set a non-root USER before
   ENTRYPOINT (the d-server role currently defaults to uid 0 / root)
   test result: FAILED. 0 passed; 1 failed; 0 ignored
   ```
2. **Post-fix (green):** `git stash pop` (restores the `USER` fix) then the
   same command:
   ```
   test runtime_stage_sets_non_root_user_before_entrypoint ... ok
   test result: ok. 1 passed; 0 failed; 0 ignored
   ```
3. `cargo fmt --all -- --check` (from `$PDCA_WORKTREE` root): clean, exit 0 —
   no reformatting needed (the Dockerfile isn't Rust-formatted; the test file
   is already `rustfmt`-clean).
4. `cargo clippy -p wyrd-chunkstore-grpc --all-targets`: clean, exit 0, no
   warnings (workspace lints incl. warnings-as-errors come from root
   `Cargo.toml` `[workspace.lints]` per `xtask/src/main.rs:534-536`).
5. Also ran the full gate, `PDCA_WORKTREE=$PDCA_WORKTREE ./engine/xtask.sh ci`
   (the actual `C4-ci` gate command, `docs/INTEGRATION.md:95`), from the PDCA
   project root — the full fmt+clippy+build+test(incl. DST)+deny+conformance
   sweep. It completed **green, exit 0**: `xtask ci: all checks passed`, with
   `tests/dserver_image.rs`'s `runtime_stage_sets_non_root_user_before_entrypoint
   ... ok` running inside it (confirming the new test is wired into the
   workspace test set the gate actually runs, not just runnable in isolation).
   This corroborates iteration 1's carry-forward note that the one gate
   failure it saw (`cargo test --workspace` exit 101) was transient
   base-drift, not caused by this patch — this run, same base/patch, is clean.
   This goes beyond the brief's minimum fast-pass ask; done because the full
   gate finished well within the runner's timeout budget.

## What's deferred (per the brief's Verification posture)

Building the image and actually running the container (`docker run --rm
--entrypoint id wyrd-dserver:test`, then a real fragment write under `/data`)
needs a Docker host, which this environment/the `cargo xtask ci` gate does not
provide (`brief.md:38-39`: "containers are deliberately outside the DST/gate
substrate, INTEGRATION §3"). Per the brief this is explicitly DEFERRED /
off-Check, to be confirmed by a maintainer per the brief's "Off-Check
verification instructions" (`brief.md:44-63`): build the image, confirm `id`
prints non-zero uid, confirm a write lands under `/data` owned by that uid —
or subsumed by a green `cargo xtask integration` run on a Docker host, which
builds this same image and drives real writes through the compose cluster. I
did not fabricate a Docker-based test to sidestep this; the shipped test only
proves the *encoded* seam (the Dockerfile declares the non-root user
correctly), which is the load-bearing regression guard `cargo xtask ci` can
actually run headlessly.

This DEFERRED verification is the same NEEDS-HUMAN item iteration 1 already
flagged (`iteration-v1/SUMMARY.md`) — unchanged by this iteration, since it is
inherent to the brief's Verification posture (`brief.md:33-43`), not an
artifact of the reviewer crash that caused iteration 1 to be rejected.

## Files

- `patch.diff` — `crates/chunkstore-grpc/tests/dserver/Dockerfile` (13 lines
  added, `Dockerfile:21-26` → `:21-38` post-patch) + new
  `crates/chunkstore-grpc/tests/dserver_image.rs` (106 lines).
- Test file lives at the brief's named path:
  `crates/chunkstore-grpc/tests/dserver_image.rs`.
- Both regenerated fresh this iteration from `$PDCA_WORKTREE` (not copied from
  `iteration-v1/`), content-identical to iteration 1's because the base didn't
  move and the approach was never faulted on its merits — only orphaned by a
  reviewer-tooling crash outside Do's scope to fix.
