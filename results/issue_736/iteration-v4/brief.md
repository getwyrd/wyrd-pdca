# Brief — issue 736 / s3-server-version-header

> Plan artifact (docs 02 §PLAN). Do reads ONLY this file (plus the peer callsites cited
> under **Citations expected**). The `- **Label:** value` lines are parsed by the driver.
>
> Plan of record: `docs/design/proposals/draft/0017-blackbox-validation-tool.md`
> §Dependencies ("The gateway advertises no version") and §3 ("The matrix must be keyed to
> the server version"). Read in place in the target checkout — never copied here.

- **Slug:** s3-server-version-header
- **Kind:** enhancement
- **Defect:** The S3 gateway advertises no version. Verified on `main` at `65ca4fd`:
  `grep -rn "header::SERVER\|\"server\"" crates/gateway-s3/src/` returns nothing — no
  `Server:` header, and no build identifier anywhere in the response path. The only header
  the gateway stamps on every response is `x-amz-request-id`
  (`crates/gateway-s3/src/lib.rs:1552-1557`). Nothing on the wire says what a client is
  talking to, so a client cannot tell last month's deployment from today's, a captured HTTP
  exchange does not identify the build that produced it, and `wyrd-validate`'s
  version-keyed capability matrix (proposal 0017 §3) has nothing to key on — its interim
  `--server-version` is an operator-supplied parameter that can silently be wrong.
- **Goal:** Every S3 response, success and error alike, carries `Server: wyrd/<version>`,
  where `<version>` is the build identity baked in at compile time and is the SAME string
  `cargo xtask dist` stamps into the tarball's `VERSION` file for that checkout.
- **Success criterion:** BINDING (demonstrable by C4-verify at Check, no container, no
  cluster): a new integration test spawns the **built `wyrd` binary** as an `s3` role over
  a loopback listener (`--s3-listen 127.0.0.1:0 --log-format json`, killed on every exit
  path) and asserts, against that one child process:
  1. **both planes carry the header** — a signed request that succeeds AND an unsigned
     request refused with 403 come back carrying a `Server` header whose value starts
     `wyrd/`. The error leg is the load-bearing half: an error response is the one most
     likely to end up in a bug report, and it is the leg a per-handler implementation
     would miss;
  2. **the value is the BAKED build identity, not a default** — the remainder after
     `wyrd/` is non-empty, is **not** the caller-agnostic `S3Config::new` default
     (`unknown`, see Design), and is not EQUAL to the bare workspace placeholder `0.0.0`
     (equality, not a prefix test: the legitimate derived value on an untagged checkout is
     `0.0.0+git.<sha>`, which starts with `0.0.0` and is correct). This is
     why the criterion drives the BINARY rather than composing `S3Config` in-process: only
     the composition root (`serve_s3`, `cli.rs:2377-2384`) sets the field from the build
     script's constant, so an in-process fixture would go green on `wyrd/unknown` with the
     whole derivation-and-plumbing half absent;
  3. **the startup log records the same string** — the child's `role started` JSON event
     (`cli.rs:2199-2206`, emitted to stderr; `--log-format json` per `cli.rs:496`) carries
     a `version` field whose value is byte-equal to the wire header's remainder. This is a
     tracker definition-of-done item in its own right and is asserted here, not assumed;
  4. **the value came from git, not from a constant** — when a repository is visible,
     `WYRD_VERSION` is unset in the test's environment, and `git describe --tags --always`
     does not resolve exactly onto a tag, the advertised remainder CONTAINS the short commit
     sha that `git rev-parse --short HEAD` prints. Deliberately a containment check on the
     sha rather than string equality with a re-normalized `describe`: it binds the value to
     the real derivation while staying immune to the `-dirty` suffix (a gate applies a patch,
     so the tree may or may not be dirty when the build script runs) and to tag shape. The
     test must not call the production normalizer — see Falsifiability. When the
     preconditions do not hold, this leg is skipped with a printed reason and legs 1-3 still
     bind.
  SUPPLEMENTARY (deferred, see Verification posture): the same string equals what
  `cargo xtask dist` writes to the tarball's `VERSION` for the same checkout.
- **Falsifiability:** RED is producible on the ordinary developer harness Do is pointed at
  — `cargo test -p wyrd-server --test s3_server_version_header`, no Docker, no cluster, no
  feature flag. On the pre-fix tree leg 1 fails because no `Server` header exists at all
  (verified above by grep) and leg 3 fails because the `role started` event has no
  `version` field (verified: `cli.rs:2199-2206` records `role`, `listen`, `region`,
  `dservers` and nothing else). The test must be written so it **still compiles against
  the reverted tree**: drive `env!("CARGO_BIN_EXE_wyrd")` (the `cli_roundtrip.rs:11-18`
  idiom), sign with the existing `wyrd_gateway_s3::sigv4::sign`
  (`s3_http_wire.rs:95-110`), and observe only response headers, exit status and stderr.
  Do MUST NOT reference any symbol this patch introduces — no new `S3Config` field, no new
  `wyrd_server` const, no new `version` module — from the test file. That is not style:
  this instance's `C4-verify` reverts the production change and keeps the test
  (`engine/scripts/run-verify.sh:499-517`), and a test that calls net-new API fails to
  COMPILE on the red leg, which the gate correctly scores `UNVERIFIABLE` (exit 77 →
  SUMMARY §6) rather than as a proven red (`run-verify.sh:201-215`).
  The child process MUST be killed on every exit path — the `s3` role blocks forever.
  Read its stderr until the `wyrd s3: serving S3-compatible HTTP on <addr>` line
  (`cli.rs:2183-2186`, which reports `listener.local_addr()`), so the ephemeral port is
  parsed rather than guessed and two tests can run concurrently. A test that lets the
  pre-fix binary run unbounded turns a red into a hang, which is worse than either verdict.
  (`s3_http_wire.rs`'s `send` helper returns `(status, body)` only; this test needs the
  response HEADERS, so it writes its own reader — the signing and request shape are still
  copied from there.)
  One environment fact checked so the "not the bare `0.0.0`" half cannot false-fail: every
  tree the gates build in is a git worktree, and on `main` today
  `git describe --tags --always --dirty` prints `65ca4fd` (there is one tag,
  `archive/backup-premerge-signoff`, and it is not an ancestor), which
  `normalize_describe` turns into `0.0.0+git.65ca4fd` — non-empty, and not equal to the
  placeholder. If Do finds a gate environment where no version can be derived at all, say
  so in `build-notes.md` rather than quietly weakening the assertion.
- **Invariant to restore:** *Every response the S3 front door emits identifies the build
  that produced it.* Stated over the response CATEGORY — success, client error, server
  error, and the streaming-GET head alike — not over one handler and not over one status
  code. SELF-TEST: this cannot be satisfied by touching a single handler, which is the
  point; the gateway already has exactly one place where the property is expressible for
  all of them (`handle`'s single stamp point, `crates/gateway-s3/src/lib.rs:1550-1557`,
  reached by every request because the router is
  `Router::new().fallback(handle::<G>)`, `:205`). Source: the peer invariant #529
  established for `x-amz-request-id` — "mints one, returns it on every response, records it
  on every log line" (`crates/gateway-s3/src/request_id.rs:1-10`).
- **Repo + branch target:** getwyrd/wyrd @ main
- **Depends on:** 773
- **Conflicts with:** 738
- **Ordering note:** **`Depends on: 773` ADDED 2026-08-18 (maintainer-approved), and it is a
  GATE dependency, not a code one.** `cargo deny check` fails on `main` @ `65ca4fd` —
  RUSTSEC-2026-0258, `h2 0.4.15`, patched `>= 0.4.16`, advisory published 2026-08-17 — and
  `cargo_deny_check()` runs inside `run_ci` (`xtask/src/main.rs:1563`), which is this
  instance's one **gating** row. #773 is the one-line lockfile bump that clears it. Without
  this edge #736 sat in wave 0 *alongside* #773 rather than after it, so it would have built
  and gated on a base that still carried the advisory and failed `C4-ci` for a reason with
  nothing to do with its own patch — which is exactly what happened to #740's third
  iteration. This bundle is already ITERATE_DO, so that round was about to be spent. Nothing
  about the slice's own content changes; only the base it is handed.
  (Historical note: this brief previously said it shared wave 0 with #740. #740 has since
  been SPLIT into #773 → #774 → #775 and builds nothing itself.)
  #736 and #738 both edit `crates/server/src/cli.rs`, and both edit
  `cmd_s3` — this adds `version` to the `role started` event (`cli.rs:2199-2206`) and the
  `serve_s3` config; #738 adds `--chunk-size` parsing to the same function and threads a
  parameter through `serve_s3_role` / `serve_s3_dispatch` / `serve_s3` beneath it. Same
  function, overlapping hunks, so they must not be built blind on the same base. Declaring
  the conflict puts #736 in the earlier wave (the driver orients a conflict pair by id, so
  the lower id builds first — `src/pdca_harness/waves.py:165-177`) and #738 rebuilds on the
  folded result. There is no *dependency* between them; either order would do, and the id
  order is arbitrary but deterministic. Re-verified with the driver's own scheduler after the
  #740 split and this bundle's re-point (2026-08-18): waves are
  `[773] → [736, 774] → [738, 775] → [741] → [742]`, and no conflicting pair shares one.
  This bundle now shares its wave with **#774**, whose file set (`crates/validate/**`, root
  `Cargo.toml`) is disjoint from this one's. Scope (e) adds
  `.github/workflows/release.yml` to this slice's file set; #742 edits the same smoke step
  and already declares `Conflicts with: 736`, and it is last in the batch, so the overlap is
  scheduled apart and nothing further is needed.
- **Surfaces:** data
- **Difficulty:** high
- **Sizing note:** the driver's structural sizer bands this `oversized` (score 9:
  `difficulty=high`, brief length, one declared conflict, "structurally predicts a large
  patch"). Looked at, and deliberately NOT split. Two of the four inputs are artefacts of
  this brief rather than of the slice — its length is prose, and the conflict with #738 is
  a scheduling fact, not size. What remains is the honest `high` blast radius, which the
  field is there to report. The only coherent split line is "header + derivation" | "the
  packaging plumbing", and the second half is one `--build-arg`, one `ARG`, one `ENV`,
  two container-free assertions and ~6 lines of release-workflow shell — a full cycle spent
  to save a dozen lines, while the first
  half would knowingly ship a binary whose tarball advertises `0.0.0`. Splitting at "emit a
  header" | "give it a real version" is worse: the first child ships an inert
  `wyrd/unknown` and its test is rewritten by the second. One slice.
- **Scope:** (a) `S3Config` gains a server-identity field, defaulted in `S3Config::new` so
  every existing caller keeps compiling, and `handle` stamps it on the response beside the
  request id; (b) the composition root supplies the real value — `crates/server` grows a
  build script that bakes the build identity, and `serve_s3` sets the field from it; (c)
  the `role started` event gains the same string, so the running build is readable from the
  logs as well as the wire (a tracker DoD item, asserted by criterion leg 3); (d) the
  version derivation is SINGLE-SOURCED with `cargo xtask dist`'s (see Design), and the
  packaging path is plumbed so the binary an operator actually runs carries a real version
  rather than the fallback; (e) `.github/workflows/release.yml`'s existing installer smoke
  step gains the ONE end-to-end check that closes the tracker's "matches `VERSION`" clause
  — see Verification posture. It is ~6 lines of shell inside the block that already
  installs the tarball; it cannot run at Check and is not part of the binding criterion.
  **/ out of scope:** any capability-negotiation protocol or capability list on the wire
  (the issue says so explicitly — this is one header carrying one string); a `wyrd
  --version` subcommand or a version on any other role's wire surface (worth doing,
  separate issue); changing the workspace `version = "0.0.0"` placeholder
  (root `Cargo.toml:35`); the OCI image's `org.opencontainers.image.version` label, which
  `dist` already sets (`xtask/src/dist.rs:458`).
- **Repro instruction:** On `main` at `65ca4fd`, in the target checkout:
  `cargo test -p wyrd-server --test s3_http_wire` passes today and asserts nothing about a
  `Server` header. Add an assertion to any of its round-trips that the response carries
  `Server:` and it fails — `grep -rn "header::SERVER" crates/gateway-s3/src/` shows the
  gateway never sets one, and axum/hyper do not add it. Equivalently, run
  `cargo xtask dist --check` and note that `VERSION`'s value exists only inside the
  packaging pipeline: nothing compiled into the binary can report it.
- **External dependencies:** none. Beyond the base toolchain the build script shells out to
  git, which is already a hard requirement of the packaging pipeline it single-sources from
  (`xtask/src/dist.rs:356-364`) and of the checkout Do is handed, so it needs no
  registration; the build script must nonetheless fall back cleanly when no repository is
  visible, which is the `.dockerignore` case Design covers. The deferred half of the
  criterion would need a container runtime, but it is NOT part of the binding criterion and
  Do must not attempt an image build. Scope (e)'s release-workflow check needs `curl` and a
  container — both already provisioned inside that step (`release.yml:65-66` installs
  `systemd curl` in the bookworm container), so it adds no new dependency anywhere, and it
  runs on GitHub's runner at tag time, never at Check.
- **Test file:** `crates/server/tests/s3_server_version_header.rs` — a NEW file (the
  discriminator classification this instance's C4-verify uses keys on an ADDED
  `*/tests/*.rs`, `engine/scripts/run-verify.sh:141-144`; appending to the existing
  `s3_http_wire.rs` would silently degrade the gate to green-only).
- **Verification posture:** The BINDING criterion above is the default posture — a flippable
  test, red pre-fix and green post-fix at Check. One half IS deferred and is declared here
  so it lands as a pre-declared sign-off item rather than a surprise NEEDS-HUMAN: the
  claim "the wire string equals the tarball's `VERSION`" cannot be observed inside
  `cargo xtask ci`, because `cargo xtask dist` needs Docker and a network and is
  deliberately not part of `ci` (`xtask/src/dist.rs:26-28`), and
  `xtask/tests/dist_templates.rs` is "container-free by design … with the real build
  deferred to the release workflow" (`:5-9`).
  **And no existing mechanism checks it either — corrected here.** Re-read on `main` at
  `65ca4fd`: the release workflow's smoke step (`.github/workflows/release.yml:59-88`)
  builds the artifacts, untars, runs `./install.sh`, `systemd-analyze verify`s the units,
  runs `/usr/local/bin/wyrd` with NO arguments and greps `usage:`, proves idempotence, then
  uninstalls and asserts absence. It never starts a role, never reads a response header,
  and never reads `VERSION`. So "confirmed by the release workflow" was false as written;
  the promise is narrowed and given a mechanism instead:
  * BUILT AND EXERCISED AT CHECK: the header emission on both response planes AND the
    startup-log field, both against the real composition root (the child-process test
    above); the version-derivation decision as a **pure, unit-tested** function (the
    fallback ORDERING — explicit override wins, then `git describe` normalized, then the
    `0.0.0` fallback — driven over injected inputs, never by mutating process env); and
    container-free coupling assertions that the packaging path actually passes the version
    in (see Design).
  * WRITTEN HERE, OBSERVED AT RELEASE: scope (e) adds to that smoke step, after
    `./install.sh` and the `libfdb_c` install it already does — start
    `/usr/local/bin/wyrd s3 --s3-listen 127.0.0.1:18080 --data-dir /tmp/vsmoke
    --access-key k --secret-key s` in the background, `curl -sS -D-` the endpoint (an
    UNSIGNED request: it is refused 403 and still carries the header, so no signing is
    needed in shell), extract the `Server:` value, and compare its `wyrd/` remainder with
    the `version:` line of the untarred `VERSION` file; kill the role. That is the equality
    clause, stated as an executable check rather than as a claim.
  * WHEN IT IS ACTUALLY OBSERVED, plainly: on the next `v*` tag — and **no `v*` tag has
    ever been cut** (the repo's only tag is `archive/backup-premerge-signoff`;
    `release.yml:20-22` triggers on `push: tags: ["v*"]`). So at this sign-off the equality
    is written and reviewable but UNOBSERVED. Do must say exactly that in `build-notes.md`
    and must not claim it as demonstrated.
  * The deferred half is a *verification* gap, not an unbuilt deliverable: the packaging
    plumbing IS written in this slice, IS exercised by the container-free assertions, and
    its end-to-end check IS written into the release workflow — what is missing is a tag to
    run it.
- **Production reach:** Not applicable in the seam sense — the production path traverses the
  change at Check (the test drives the real `handle` through the real router). One honest
  limit to record in `build-notes.md`: on a checkout with no tag and no override, the
  advertised string is the `0.0.0+git.<sha>` shape, which is a real build identity but not
  a release version; the release-shaped value only appears once a tag exists.
- **Citations expected:** Do must cite `path:line` on `main` for every change. Peer
  callsites Do MAY open and should mirror:
  * **The one place a header reaches every response** — `crates/gateway-s3/src/lib.rs:1550-1557`
    (the `x-amz-request-id` stamp at the end of `handle`, after `finish_response`). Stamp
    the `Server` header there, the same way, for the same reason. Every request reaches it:
    `Router::new().fallback(handle::<G>)`, `crates/gateway-s3/src/lib.rs:205`.
  * **The config seam and its default** — `crates/gateway-s3/src/lib.rs:101-122`
    (`S3Config` and `S3Config::new`). Only `new` constructs it (verified: no struct literal
    exists anywhere in the tree), so adding a field is source-compatible.
  * **The composition root that sets config on the way in** — `crates/server/src/cli.rs:2377-2384`
    (`serve_s3` building `S3Config`, setting `config.region`, then `S3Gateway::new`). Set
    the new field there, from the baked constant — ADR-0010: the composition root is what
    knows the build identity, the wire crate does not.
  * **The role-started event to extend** — `crates/server/src/cli.rs:2199-2206` (today it
    records `role`, `listen`, `region`, `dservers`; add `version` beside them). It reaches
    stderr through the process-global subscriber (`logging::init_global`,
    `crates/server/src/logging.rs:301-329` — "writing to **stderr**"), and
    `--log-format json` (`cli.rs:496`, `logging.rs:164-175`) makes it machine-readable,
    which is how criterion leg 3 asserts on it.
  * **Driving the built binary from an integration test** —
    `crates/server/tests/cli_roundtrip.rs:11-18` (`const WYRD: &str =
    env!("CARGO_BIN_EXE_wyrd");`). That is the idiom; this test differs only in that its
    child is a long-running server, so it is spawned, read from, and killed rather than
    `output()`-ed.
  * **The release smoke step scope (e) extends** — `.github/workflows/release.yml:59-88`
    (the bookworm container that untars, `./install.sh`s, installs `libfdb_c` and the
    `curl` it already uses, then uninstalls). Add the role-start + `Server`-vs-`VERSION`
    comparison inside that same `docker run`, and leave the existing assertions untouched.
  * **The version derivation to single-source** — `xtask/src/dist.rs:119-127`
    (`normalize_describe`, the three `git describe` shapes) and `:356-364`
    (`derive_version`). Its behaviour is already pinned by
    `xtask/tests/dist_templates.rs:300-321`; that test must keep passing unchanged.
  * **The build-args the image build already takes** — `xtask/src/dist.rs:450-462`
    (`--build-arg FEATURES=…`, `--label org.opencontainers.image.version={version}`) and
    `deploy/docker/wyrd/Dockerfile:29-31,36-39` (`ARG FEATURES` declared in the build
    stage). Add the version the same way.
  * **The wire-test fixture** — `crates/server/tests/s3_http_wire.rs:56-64`
    (`build_gateway`) and `:95-110` (signing a request by hand with the production
    `sigv4::sign`). Copy that shape; it is the only in-process fixture that exercises both
    a signed success and an unsigned 403 over a real listener.
  * **A build script must carry the crate-root attribute** — `xtask/tests/repo_hygiene_guards.rs:256-294`
    (`scan_crate_roots_covers_build_scripts_benches_and_examples`): `build.rs` is a
    `custom-build` crate root and the #616 guard scans it, so it needs
    `#![forbid(unsafe_code)]` or `cargo xtask ci` goes red.
- **Prior-art check (triage cycles):** searched by affected path on `main` at `65ca4fd`.
  `git log --oneline -- crates/gateway-s3/src/lib.rs` → the recent history is #504/#506/#509/#510
  wire-surface work and the #616/#619 lint sweeps; none adds a response header beyond
  #529's request id. `git log --oneline -- xtask/src/dist.rs` → one commit, `f5d4575`
  ("dist(570): one pipeline ships the operator tarball and the OCI image"), which
  introduced `normalize_describe`. `gh pr list --state all --search "Server header"` and
  `--search "chunk-size"` → no PR, open or closed, has attempted a `Server:` header. No
  superseded or rejected attempt exists.
- **Disposition hint:** new-feature

## Motivation

Three reasons, in the order they bite.

**An external tool has to be told what it is validating.** `wyrd-validate`'s capability
matrix declares, per operation, `required` or `unsupported(#NNN)` — and those expectations
change as slices land (#504's CopyObject rejection, #508's multipart, #511's bucket
operations). Without a version on the wire, a tool run against an older release fails on
rows naming issues fixed after that build, and the operator reads a spurious failure. The
proposal's interim answer is an operator-supplied `--server-version`, i.e. a parameter that
can silently be wrong — which for a *validation* tool is the worst kind of input.

**Support.** "Which version were you running" currently has no answer a client can capture.
A wire-visible version means a captured HTTP exchange identifies the build.

**It is the conventional thing.** Real S3 sends `Server: AmazonS3`. S3 clients, proxies and
caches expect the header; sending nothing is the unusual choice.

## Design

### Where the header is stamped

At `handle`'s single stamp point (`crates/gateway-s3/src/lib.rs:1550-1557`), beside the
request id, after `finish_response`. Every request reaches `handle` — the router is a bare
`fallback` (`:205`) — so success, 403, 404, 405, 501 and the streaming-GET head are all
covered by one insertion. Stamping in the handlers instead would satisfy the success path
and miss exactly the responses that end up in bug reports.

`S3Config` gains the identity string; `S3Config::new` defaults it (a caller-agnostic
placeholder such as `wyrd/unknown`, so a library caller that never sets it still emits a
well-formed header). The gateway crate owns the header NAME and the `wyrd/` shape; it does
not own the version, because a wire crate has no business shelling out to git.

**That default is exactly why the criterion drives the binary.** A fixture that builds
`S3Config::new` in-process (the `s3_http_wire.rs:78-89` shape) would go green on
`wyrd/unknown` with the build script, the derivation and the composition-root assignment
all missing — the header would ship inert. Criterion leg 2 therefore rejects `unknown`
outright, and the only way to satisfy it is through `serve_s3`, which is reachable only by
running the real binary. Keep the default anyway: it is the library-caller contract, and it
is worth one supplementary in-process assertion (a `S3Config::new` gateway emits a
well-formed `wyrd/unknown`) — supplementary, never the discriminator.

### Where the version comes from

The composition root. `crates/server` gains a build script that resolves, in order:

1. `WYRD_VERSION` from the build environment, used verbatim — this is how the packaging
   pipeline injects the value it has already derived;
2. otherwise `git describe --tags --always --dirty` in the workspace, put through the SAME
   `normalize_describe` `cargo xtask dist` uses;
3. otherwise the `0.0.0` fallback `derive_version` already passes
   (`xtask/src/dist.rs:363`).

It emits `cargo:rustc-env=WYRD_VERSION=<v>` plus the `rerun-if-env-changed` /
`rerun-if-changed` directives needed so a rebuild after a commit does not serve a stale
string. `crates/server` reads it with `env!` and hands it to `serve_s3`.

`build.rs` is a crate root for the #616 unsafe guard, so it carries
`#![forbid(unsafe_code)]`.

### Why rung 1 exists at all — the finding that shapes this slice

**`.dockerignore` excludes `.git/`** (verified: `.dockerignore:6`). `cargo xtask dist`'s
default path builds the production image and extracts the binary out of it — "so the
tarball's `bin/wyrd` is bit-identical to the image's `/usr/local/bin/wyrd`"
(`xtask/src/dist.rs:5-7`). A build script running `git describe` inside that build stage
therefore sees no repository at all and falls to `0.0.0`, while the tarball's `VERSION`
file next to it says `0.1.0+git.3.abc12de`. The binary an operator actually runs would
advertise the least informative value available, and the issue's own definition of done —
"the version matches what `cargo xtask dist` stamps into the tarball's `VERSION` file for
the same checkout" — would be violated by construction on the shipping path.

So the packaging path must pass the version in: `dist` adds
`--build-arg WYRD_VERSION={version}` beside the `FEATURES` arg it already passes
(`xtask/src/dist.rs:450-455`) using the same `version` it writes to `VERSION`, and the
Dockerfile declares `ARG WYRD_VERSION` in the build stage and exports it to the `cargo
build` (`deploy/docker/wyrd/Dockerfile:36-39,66`). One `--build-arg`, one `ARG`, one `ENV`.

Two container-free assertions pin that coupling inside `cargo xtask ci`, in the idiom
`xtask/tests/dist_templates.rs` already uses for the FDB pin (`:279-295` — read the
Dockerfile, parse the declaration, assert): the Dockerfile declares `ARG WYRD_VERSION` in
the build stage, and the image build's argument list carries `WYRD_VERSION` set to the same
value the staging plan substitutes. Prefer making the build-arg list a **pure function**
(`fn image_build_args(version, cfg) -> Vec<String>`, extracted from `obtain_binary`) and
asserting on its output — that is a real assertion rather than a grep of our own source,
and it matches the module's own "the pure decisions … are unit-tested inside `ci`" stance
(`xtask/src/dist.rs:26-28`). If extracting it turns out to reach further than expected, a
file-read + substring assertion in the `:279-295` style is an acceptable fallback; say
which you chose and why in `build-notes.md`.

### Single-sourcing `normalize_describe`

Two implementations of a version-normalizer is exactly the drift `xtask/tests/fdb_image.rs`
exists to prevent for the FDB pin, and the failure is silent: the tarball says
`0.1.0+git.3.abc12de` and the wire says `v0.1.0-3-gabc12de`, and the capability matrix keyed
on one cannot match the other.

Move the pure function into its own small module in the **shipped** crate —
`crates/server/src/version.rs`, holding `normalize_describe` verbatim — and have both
consumers compile that one file:

* `crates/server/build.rs` includes it with `#[path = "src/version.rs"] mod version;` (the
  standard build-script pattern), and `crates/server`'s lib exposes it too;
* `xtask/src/dist.rs` includes it with `#[path = "../../crates/server/src/version.rs"] mod
  version;` and re-exports it (`pub use version::normalize_describe;`), so
  `dist::normalize_describe` keeps its existing call site (`dist.rs:363`) and its existing
  test (`xtask/tests/dist_templates.rs:300-321`) unchanged and green.

The direction matters: tooling reaches into the product, never the other way round. A
`build-dependency` from `wyrd-server` onto `xtask`, or a normal dependency from `xtask`
onto `wyrd-server`, would invert the ship boundary and pull the whole server graph into
every `cargo xtask` invocation.

### The three decisions the issue asks for

1. **Granularity** — the full normalized `dist` string, not a coarser `wyrd/0.1.0`. The
   definition of done requires it to match `VERSION`; and past a tag, a coarse string
   cannot distinguish the builds the capability matrix needs to distinguish.
2. **Does a `-dirty` build advertise as dirty?** Yes. `normalize_describe` already carries
   `.dirty` into the artifact version, so suppressing it on the wire would mean *two*
   shapes again. An untagged, dirty binary in a deployment is precisely the thing worth
   catching, and it is noise only in development, where nobody is reading the header.
3. **Anything beyond the version?** No — explicitly out of scope per the issue.

## Alternatives considered

**Use the workspace `version = "0.0.0"`.** Rejected in the issue: it carries no
information, and it is the same string for every build ever made.

**`option_env!("WYRD_VERSION")` with no build script.** Simplest possible change — rustc
tracks env deps, so it even rebuilds correctly — but it drops the `git describe` fallback
entirely. Wyrd has no releases yet, so *every* build today would advertise `unknown` and
the feature would be inert until the first tag, including for the tool that motivates it.

**Let the build script emit the raw `git describe` output and skip the shared module.**
Removes three files of change, and produces two shapes on the wire (`v0.1.0-3-gabc12de`
from a dev build, `0.1.0+git.3.abc12de` from a dist build). A version-keyed matrix that has
to parse both is the bug this issue exists to prevent.

**Duplicate `normalize_describe` in the build script and pin the two with a test.** The
repo's precedent for cross-file coupling (`xtask/tests/fdb_image.rs`) pins a *constant*
across three files, which a substring assertion can genuinely do. It cannot meaningfully
pin two copies of an *algorithm*.

**Emit the version only on error responses** (where it is most useful for bug reports).
Rejected: S3 clients and proxies expect `Server` on every response, and a header that
appears only on failures is a protocol oddity of its own.

## Impact & compatibility

Additive on the wire: `Server` is a standard response header and no S3 client rejects one.
No request behaviour, status code, body or existing header changes; `S3Config` gains a
field but only `S3Config::new` constructs it, so no caller breaks.

The build gains a build script for `crates/server`, which costs one `git describe` per
cold build and makes the compiled binary depend on the checkout's git state — deliberate,
and the same dependency `cargo xtask dist` already has.

The image build gains one `ARG`. An image built WITHOUT the new build-arg (a bare
`docker build` by hand) still succeeds and advertises the fallback — the arg is additive
and unset-safe, which the Dockerfile's existing `ARG FEATURES=""` establishes as the
convention.

One thing that deliberately does NOT change: the "bit-identical binary" guarantee between
the tarball and the image (`xtask/src/dist.rs:5-7`) still holds, because the version enters
the *build* rather than being patched into an artifact afterwards.

## Plan-review response (#301 revision pass)

Three findings, all accepted; the brief is revised rather than defended.

* **"The wire test can go green on `wyrd/unknown`."** Correct, and it was the load-bearing
  hole: the old criterion asserted only "non-empty and not `0.0.0`", while Design
  deliberately defaults `S3Config::new` to `wyrd/unknown` and the cited fixture composes
  `S3Config` directly, bypassing `serve_s3`. The criterion now drives the **built binary**
  as an `s3` role (so the composition root runs), explicitly rejects the `unknown` default,
  and adds a fourth leg pinning the value to `dist`'s normalized `git describe` whenever the
  test can derive one. Falsifiability, Design and Citations follow it.
* **"The deferred `VERSION` equality names no mechanism."** Correct — re-read of
  `.github/workflows/release.yml:59-88` confirms it installs the tarball, runs `wyrd` with
  no arguments and checks usage/uninstall, and never starts a role, reads a header or reads
  `VERSION`. The false "confirmed by release.yml" sentence is gone. Scope (e) now WRITES the
  check (unsigned request → 403 → `Server:` compared with `VERSION`'s `version:` line) into
  that smoke step, and the posture states plainly that it is unobserved until a `v*` tag —
  of which none has ever been cut.
* **"The startup-log DoD is in scope but not in the criterion."** Correct. Criterion leg 3
  now asserts the child's `role started` JSON event carries a `version` field byte-equal to
  the wire header's remainder — verified feasible: the event is `tracing::info!` at
  `cli.rs:2199-2206`, the subscriber writes to stderr (`logging.rs:301-329`), and
  `--log-format json` is an existing flag on every command (`cli.rs:496`).

## Open questions

1. **Where the `wyrd/` prefix lives** — in `gateway-s3` (so the wire crate owns the whole
   header shape and the composition root passes a bare version) or in `crates/server` (so
   the root passes the finished string). The brief assumes the former; either is fine as
   long as the default value is a well-formed header and there is exactly one place that
   spells `wyrd/`.
2. **Should the other roles' startup logs carry the same string?** `cmd_d_server` and
   `cmd_custodian` emit their own `role started` events. Cheap to extend and clearly
   useful, but out of this slice's scope; worth a follow-up issue rather than scope creep.
3. **`.github/workflows/release.yml`** already checks out full history and tags for
   `dist`'s `git describe` (`:35`), so it needs no change. Confirm that during
   implementation; if it does need one, say so rather than editing it silently.

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a draft PR
MAY happen during the cycle. The PR MUST NOT be marked ready before sign-off accepts.

## Iteration 1 — carry-forward (from the previous attempt)
- Sign-off rationale: Auto-iterate (round 1): rebuilding for the implementation-level findings — C5 Causal adequacy — Rebuild must reconcile the discriminator with the supported repository-less fallback: `0.0.0` is produced at `crates/server/src/version.rs:113` but rejected at `crates/server/tests/s3_server_version_header.rs:331`, so the unmutated mutation baseline failed and causal strength was not tested (`gate-logs/C5-mutants.log:1639`).; T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 5 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_736/review-b. 4 finding(s) needing human judgment were deferred to sign-off, not addressed here.
- Failing gate: C5 surviving mutants on the bundle diff (cargo mutants --in-diff) (advisory) — ERROR cargo test failed in an unmutated tree, so no mutants were tested
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 5 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_736/review-b
- Full previous attempt preserved in `iteration-v1/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 2 — carry-forward (from the previous attempt)
- Sign-off rationale: Auto-iterate (round 2): rebuilding for the implementation-level findings — T5 Judgment — Rebuild must add an override-origin signal or equivalent so the SHA leg skips only intentional overrides—the current test rejects the supported distribution input at `crates/server/tests/s3_server_version_header.rs:408-441`.; **A reachable tag containing `/` makes every response advertise; **The release smoke's own failure path cannot report itself.**; T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 3 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_736/review-b. 6 finding(s) needing human judgment were deferred to sign-off, not addressed here.
- Failing gate: C5 surviving mutants on the bundle diff (cargo mutants --in-diff) (advisory) — ERROR cargo test failed in an unmutated tree, so no mutants were tested
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 3 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_736/review-b
- Full previous attempt preserved in `iteration-v2/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 3 — carry-forward (from the previous attempt)
- Sign-off rationale: Rebuild targeting the adversarial review's confirmed implementation defects, in priority order: 1. Most severe: `git describe` is not scoped to the source tree (`crates/server/build.rs:45`, `current_dir(CARGO_MANIFEST_DIR)`), so a checkout nested inside an unrelated git repo bakes and confidently advertises a foreign commit's SHA as the build's own identity — exactly the "can silently be wrong" failure mode this issue exists to remove. Guard with `git ls-files --error-unmatch Cargo.toml` (or compare `--show-toplevel` against the workspace root) and fall back to `0.0.0+git.unknown` / origin `unknown` on failure. 2. Leg 4's provenance skip predicate is self-minted from the artifact under test (`crates/server/tests/s3_server_version_header.rs:423-437` reads `version_origin` from the child's own event and skips the sha assertion whenever it isn't `git`), so a one-token slip in `build.rs` silently disables the only provenance check while staying green. Make `origin == "unknown"` a hard failure (not a skip) when `git rev-parse --short HEAD` independently succeeds from the same directory. 3. Under git's `reftable` ref backend, the build script's rerun-if-changed watch set is incomplete (`crates/server/build.rs:98-127`), so a new commit does not trigger a rebuild and the binary can silently ship a stale baked identity while still claiming `version_origin: git`. Also watch `git rev-parse --git-path reftable` (or the whole common dir when `--show-ref-format` reports `reftable`). 4. The brief's explicit design decision ("a dirty build must advertise as dirty") is reversed in the implementation (`crates/server/build.rs:39-45,87-96` omits `--dirty` from `git describe`) without being ratified at sign-off — either honor the brief's decision or get it explicitly reversed in the brief, not silently in code. 5. The streaming-GET response path, named in the invariant text itself ("success, client error, server error, and the streaming-GET head alike"), is never asserted on the wire — add a signed GET leg alongside the existing PUT/403 legs. 6. C5 causal adequacy / T5: tag normalization is not injective (`crates/server/src/version.rs:119,130` — replacing every forbidden byte with `.` lets distinct exact tags collapse to one identity) and whitespace is silently trimmed before validation in both the gateway and override path (T2 Shape FAIL) — add regressions for both and fix the normalization to fail closed rather than collapse. 7. C3 scope question: decide (and record) whether `version_origin` as a new public constant/observable log field belongs in this slice, or trim it back to the version string only. Explicitly NOT a cause for concern and out of scope for this iteration: the C4-ci `unverifiable` timeout — the same `custodian_gc` binary with this same patch ran clean (10/10 passed in 0.17s) in the same-day C5 baseline run, so the stall reads as host contention/scheduling on the earlier run, not a regression from this patch. A rerun of C4-ci is cheap confirmation but not itself a code change to make.
- Sign-off session carry-forward (captured live, before §9 flattened it):
  Rebuild targeting the adversarial review's confirmed implementation defects, in priority order:
  1. Most severe: `git describe` is not scoped to the source tree (`crates/server/build.rs:45`, `current_dir(CARGO_MANIFEST_DIR)`), so a checkout nested inside an unrelated git repo bakes and confidently advertises a foreign commit's SHA as the build's own identity — exactly the "can silently be wrong" failure mode this issue exists to remove. Guard with `git ls-files --error-unmatch Cargo.toml` (or compare `--show-toplevel` against the workspace root) and fall back to `0.0.0+git.unknown` / origin `unknown` on failure.
  2. Leg 4's provenance skip predicate is self-minted from the artifact under test (`crates/server/tests/s3_server_version_header.rs:423-437` reads `version_origin` from the child's own event and skips the sha assertion whenever it isn't `git`), so a one-token slip in `build.rs` silently disables the only provenance check while staying green. Make `origin == "unknown"` a hard failure (not a skip) when `git rev-parse --short HEAD` independently succeeds from the same directory.
  3. Under git's `reftable` ref backend, the build script's rerun-if-changed watch set is incomplete (`crates/server/build.rs:98-127`), so a new commit does not trigger a rebuild and the binary can silently ship a stale baked identity while still claiming `version_origin: git`. Also watch `git rev-parse --git-path reftable` (or the whole common dir when `--show-ref-format` reports `reftable`).
  4. The brief's explicit design decision ("a dirty build must advertise as dirty") is reversed in the implementation (`crates/server/build.rs:39-45,87-96` omits `--dirty` from `git describe`) without being ratified at sign-off — either honor the brief's decision or get it explicitly reversed in the brief, not silently in code.
  5. The streaming-GET response path, named in the invariant text itself ("success, client error, server error, and the streaming-GET head alike"), is never asserted on the wire — add a signed GET leg alongside the existing PUT/403 legs.
  6. C5 causal adequacy / T5: tag normalization is not injective (`crates/server/src/version.rs:119,130` — replacing every forbidden byte with `.` lets distinct exact tags collapse to one identity) and whitespace is silently trimmed before validation in both the gateway and override path (T2 Shape FAIL) — add regressions for both and fix the normalization to fail closed rather than collapse.
  7. C3 scope question: decide (and record) whether `version_origin` as a new public constant/observable log field belongs in this slice, or trim it back to the version string only.
  Explicitly NOT a cause for concern and out of scope for this iteration: the C4-ci `unverifiable` timeout — the same `custodian_gc` binary with this same patch ran clean (10/10 passed in 0.17s) in the same-day C5 baseline run, so the stall reads as host contention/scheduling on the earlier run, not a regression from this patch. A rerun of C4-ci is cheap confirmation but not itself a code change to make.
- Failing gate: C5 surviving mutants on the bundle diff (cargo mutants --in-diff) (advisory) — ERROR cargo test failed in an unmutated tree, so no mutants were tested
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 5 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_736/review-b
- Full previous attempt preserved in `iteration-v3/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 4 — carry-forward (from the previous attempt)
- Sign-off rationale: Rationale: T4's rubric review failed (4 blocking), C5 mutation testing never ran (environmental, but leaves ~560 new lines unevidenced), and the size backstop has tripped (patch 116KB > 100KB threshold, 3 rounds already spent > 2 threshold) — matches the bundle's own iterate-plan recommendation. Send back to re-plan rather than iterate-do because several open items are brief/criterion defects, not just implementation gaps: - The brief defers the tracker's "wire version matches cargo xtask dist's tarball VERSION" requirement to the release workflow, but that workflow never actually performs the comparison — the equality claim is currently unfalsifiable as scoped. - The tracker's "startup log records the same string" definition-of-done item is not covered by the binding success criterion at all, only by non-binding scope prose — needs to be made binding or dropped from this slice's claimed deliverables. - An unresolved tension between the batch review (wants a wider build.rs watch set) and the adversary review (current git-index watch already reintroduces the rebuild-cost problem the script's own doc says it avoids) — these can't both be satisfied by adding more paths; the re-plan needs to pick a side (accept dirty-state staleness, or accept the rebuild churn) and say so explicitly in the brief. - Version-string validator doesn't enforce Docker's tag-naming rule (leading -/./+ rejected), causing a confusing late failure in the Docker build step rather than an early, clear one. - Unauthenticated 403 responses disclose the exact build commit and dirty-tree status; the brief decided this granularity for the client use case but never weighed it against the threat model or gave an operator a way to suppress/coarsen it for internet-facing gateways. - One §6 item (binding test can go green on the unconfigured `wyrd/unknown` default via s3_http_wire.rs) appears to be carried forward from an earlier round and may already be addressed — the current adversary review confirms the red->green runs against the built binary and cannot pass on `unknown` or a hardcoded constant. Re-plan should confirm/resolve this discrepancy rather than carry it forward again. - Two §6 bullets were truncated by the known SUMMARY assembly bug (same as #771); full text was not retrieved before this decision — re-plan should check the archived iteration rounds' check-advisory-adversary.md for the complete findings before re-authoring. Human directive: iterate-plan for 736 — return to Plan for `pdca split` / brief rework.
- Sign-off session carry-forward (captured live, before §9 flattened it):
  Rationale: T4's rubric review failed (4 blocking), C5 mutation testing never ran (environmental,
  but leaves ~560 new lines unevidenced), and the size backstop has tripped (patch 116KB > 100KB
  threshold, 3 rounds already spent > 2 threshold) — matches the bundle's own iterate-plan
  recommendation.

  Send back to re-plan rather than iterate-do because several open items are brief/criterion
  defects, not just implementation gaps:
  - The brief defers the tracker's "wire version matches cargo xtask dist's tarball VERSION"
    requirement to the release workflow, but that workflow never actually performs the
    comparison — the equality claim is currently unfalsifiable as scoped.
  - The tracker's "startup log records the same string" definition-of-done item is not covered
    by the binding success criterion at all, only by non-binding scope prose — needs to be made
    binding or dropped from this slice's claimed deliverables.
  - An unresolved tension between the batch review (wants a wider build.rs watch set) and the
    adversary review (current git-index watch already reintroduces the rebuild-cost problem the
    script's own doc says it avoids) — these can't both be satisfied by adding more paths; the
    re-plan needs to pick a side (accept dirty-state staleness, or accept the rebuild churn) and
    say so explicitly in the brief.
  - Version-string validator doesn't enforce Docker's tag-naming rule (leading -/./+ rejected),
    causing a confusing late failure in the Docker build step rather than an early, clear one.
  - Unauthenticated 403 responses disclose the exact build commit and dirty-tree status; the
    brief decided this granularity for the client use case but never weighed it against the
    threat model or gave an operator a way to suppress/coarsen it for internet-facing gateways.
  - One §6 item (binding test can go green on the unconfigured `wyrd/unknown` default via
    s3_http_wire.rs) appears to be carried forward from an earlier round and may already be
    addressed — the current adversary review confirms the red->green runs against the built
    binary and cannot pass on `unknown` or a hardcoded constant. Re-plan should confirm/resolve
    this discrepancy rather than carry it forward again.
  - Two §6 bullets were truncated by the known SUMMARY assembly bug (same as #771); full text
    was not retrieved before this decision — re-plan should check the archived iteration rounds'
    check-advisory-adversary.md for the complete findings before re-authoring.

  Human directive: iterate-plan for 736 — return to Plan for `pdca split` / brief rework.
- Failing gate: C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance) — xtask: `cargo deny check` failed with exit status: 1
- Failing gate: C5 surviving mutants on the bundle diff (cargo mutants --in-diff) (advisory) — ERROR cargo test failed in an unmutated tree, so no mutants were tested
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 4 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_736/review-b
- Full previous attempt preserved in `iteration-v4/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).
