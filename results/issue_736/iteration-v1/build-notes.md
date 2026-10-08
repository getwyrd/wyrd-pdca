# Build notes — issue 736 / s3-server-version-header

Target branch: `getwyrd/wyrd @ main`. Built in `$PDCA_WORKTREE`
(`/home/eddie/wyrd/wyrd.pdca-wt-l0`), base `a801997` ("Merge pull request #770 …"). All
`path:line` citations below are against that tree.

## What was built, and where each piece lives

| Scope | File | What |
|---|---|---|
| (a) | `crates/gateway-s3/src/lib.rs:112-171` | `SERVER_PRODUCT` / `UNKNOWN_SERVER_VERSION`, the pure `server_header_value`, and `S3Config.server_version` (defaulted by `S3Config::new`) |
| (a) | `crates/gateway-s3/src/lib.rs:249-256, 300-311, 1614-1623` | `AppState.server` built once in `router()`, cloned before `state` moves into `dispatch`, stamped at `handle`'s single stamp point beside `x-amz-request-id` |
| (b) | `crates/server/build.rs` (new), `crates/server/src/version.rs` (new), `crates/server/src/lib.rs:16-38` | the build script, the shared derivation, and `BUILD_VERSION` |
| (b) | `crates/server/src/cli.rs:2455-2460` | `serve_s3` sets `config.server_version` from `BUILD_VERSION` |
| (c) | `crates/server/src/cli.rs:2199-2213` | the `role started` event gains `version` |
| (d) | `xtask/src/dist.rs:119-129, 139-225, 484` and `deploy/docker/wyrd/Dockerfile:29-39, 46, 66-72` | the normalizer is single-sourced from the shipped crate; `--build-arg WYRD_VERSION` + `ARG`/`ENV` |
| (d) | `xtask/tests/dist_templates.rs:297-360` | the two container-free coupling assertions |
| (e) | `.github/workflows/release.yml:79-96` | the `Server:`-vs-`VERSION` check inside the existing smoke step |

## Refuting my own test (forced, recorded)

**(a) Genuine red?** Yes, actually reverted and re-run. `git stash push -u` of every
production file (leaving only the new test), then
`cargo test -p wyrd-server --test s3_server_version_header`:

* it **compiles** against the reverted tree (no net-new symbol is named from the test — this
  is what keeps `C4-verify` from scoring `UNVERIFIABLE`), and
* both tests fail on the absent header, on both planes:

  ```
  the signed PUT response carries no `Server` header … Head:
  HTTP/1.1 200 OK … x-amz-request-id: a6a4e1653a030c3a0000000000000000 …
  the unsigned 403 response carries no `Server` header … Head:
  HTTP/1.1 403 Forbidden … x-amz-request-id: a3ecc19180d66b4a0000000000000000 …
  test result: FAILED. 0 passed; 2 failed
  ```
  Post-fix, the same command on the same tree: `2 passed`, printing
  `advertised build identity: wyrd/0.0.0+git.a801997.dirty (HEAD a801997)`. (Both runs were
  captured under `$PDCA_SCRATCH/issue_736/{red,green}.log` — scratch the harness reclaims,
  so the excerpts above are quoted rather than referenced.)

Two further **mutation checks**, because leg 1 failing first would otherwise hide whether
legs 2-3 bind anything:

* deleting only `version = crate::BUILD_VERSION` from the `role started` event (leg 3):
  `the role started event carries no version field: {"dservers":0,"listen":…,"message":"role
  started","region":"us-east-1","role":"s3"}` → FAILED.
* deleting only `config.server_version = …` in `serve_s3` (leg 2 — the header ships but
  inert): `assertion left != right failed … left: "unknown"` → FAILED.

So each of the three legs that can be exercised at Check is independently load-bearing, not
carried by leg 1.

**(b) Production path?** Yes. The test spawns `env!("CARGO_BIN_EXE_wyrd")` as an `s3` role
(`--s3-listen 127.0.0.1:0`), i.e. the real `cmd_s3` → `serve_s3_role` → `serve_s3` →
`S3Gateway::router` → `handle` path, over a real TCP socket with a signature produced by the
production `sigv4::sign`. Nothing is mocked and nothing is re-implemented: the assertions read
response headers, the child's stderr, and `git`. This is why the criterion drives the binary —
an in-process `S3Config::new` fixture would have gone green on `wyrd/unknown` with the whole
derivation-and-plumbing half missing (proved above by mutation check 2).

**(c) Fixture includes the fault?** Yes. The failing element here is the *error* plane and the
*default* value, and both are in the fixture rather than curated out: the unsigned 403 is
asserted to carry the same header as the 200 (a per-handler implementation passes the first
and fails the second), and leg 2 rejects the `unknown` default by equality. Leg 4 asserts the
advertised string contains `git rev-parse --short HEAD` — the real derivation, not a constant.
Nothing about the fixture excludes the pre-fix state: the same file, unchanged, is what went
red above.

## Decisions worth the reviewer's time

**The bare version in the config, not the finished header** (brief Open question 1). `S3Config`
carries `server_version: String` (default `"unknown"`), and `gateway-s3` owns the `wyrd/`
token — `SERVER_PRODUCT`, spelled exactly once
(`crates/gateway-s3/src/lib.rs:116`). Had the field held the whole header value, the
composition root would have had to spell `wyrd/` a second time, which the brief explicitly
rules out ("exactly one place that spells `wyrd/`"). The default still yields a well-formed
`wyrd/unknown`, which was the other half of that constraint.

**Built once, inserted infallibly.** The request-id stamp is best-effort
(`if let Ok(value) = HeaderValue::from_str(…)`, `:1608-1610`) — acceptable for an id it mints
itself, but a *silent skip* on a caller-supplied string would defeat the very invariant this
slice restores ("every response … "). So the value is validated and converted **once** in
`router()` and stored in `AppState`; `handle` does an infallible `insert`. Validation is the
RFC 9110 §5.6.2 `tchar` set, every byte of it (the rubric's grammar-strictness class); a
non-token identity degrades to `wyrd/unknown` rather than vanishing, and a unit test drives
`""`, `"   "`, a space, an embedded `\n` (header injection) and a non-ASCII byte.

**Pure function, not a grep, for the packaging coupling** (Design offered either). I extracted
`dist::image_build_args(cfg, version, revision) -> Vec<String>` and `dist::version_file(…)`
out of `obtain_binary` / `assemble`, so `xtask/tests/dist_templates.rs` asserts on *what the
pipeline actually passes* and on the fact that the two consumers of one `version` string
agree, rather than on our own source text. The extraction cost is 2 helper fns + ~55 moved
lines with `obtain_binary` shrinking by 45; the alternative file-read assertion would have
been ~10 lines but could only have proved that a string appears in `dist.rs`. The Dockerfile
half genuinely is a file read (`the_dockerfile_declares_the_version_arg_in_the_build_stage`),
because a Dockerfile has no pure function to call — it parses the build stage out (between
`AS build` and the next `FROM`) rather than grepping the whole file, so an `ARG` in the
*runtime* stage could not satisfy it.

**Single-sourcing direction.** `crates/server/src/version.rs` is the one copy;
`crates/server/build.rs` compiles it with `#[path = "src/version.rs"]` and `xtask/src/dist.rs`
with `#[path = "../../crates/server/src/version.rs"] pub mod version;` + `pub use
version::normalize_describe;`. `dist::normalize_describe`'s call site (`dist.rs:484`) and its
existing test (`dist_templates.rs:352-375`, unchanged) stay green. The module is `pub` in
`dist.rs` deliberately: with a private `mod`, `resolve_version` and `FALLBACK_VERSION` would
be unreachable from the crate root and `dead_code` (a `warnings = "deny"` workspace) would
fail the gate. `FALLBACK_VERSION` is now shared too, so `derive_version`'s `"0.0.0"` and the
build script's cannot drift.

**Rebuild freshness.** The build script emits `rerun-if-env-changed=WYRD_VERSION` plus
`rerun-if-changed` for `build.rs`, `src/version.rs`, and the git files that decide
`describe`'s answer — resolved with `git rev-parse --git-path`, not a `.git/…` join, because
every gate here runs in a **linked worktree** where `.git` is a file and refs live in the
common dir. Non-existent paths are skipped: cargo treats a missing `rerun-if-changed` target
as always-changed, which would re-run the script and relink on every build.

**The child is told its log level.** `start_role` passes `--log-level info` as well as
`--log-format json`: `--log-level` overrides both the default and `RUST_LOG`
(`crates/server/src/logging.rs:184-206`), so an ambient `RUST_LOG=error` in a runner's
environment cannot filter the `role started` event out from under leg 3 and turn it into a
60-second timeout. Verified by running the suite with `RUST_LOG=error` set — still green.

## One brief deviation, measured rather than assumed

Criterion leg 4 says to skip when "`WYRD_VERSION` is unset in the test's environment" is
false. **Cargo re-exports a build script's own `cargo:rustc-env` into the environment of the
test binary it runs** — measured here: the first green run printed
`skipped: WYRD_VERSION is set …`, i.e. the leg would have been *vacuous in every run*, which
is exactly the hollow-evidence shape the brief's own plan-review pass was written against. And
from inside the process an inherited override is indistinguishable from cargo's re-export (the
value is identical either way, since rung 1 wins). So the precondition is narrowed to the two
states in which a git-derived version genuinely carries no sha — no repository visible, or
HEAD exactly on a tag — and the leg now runs and binds. The cost, stated in the assertion's
own message: a deliberate `WYRD_VERSION=1.2.3 cargo test …` fails this one leg. That is a
strictly stronger test than the brief specified, not a weaker one.

## Deferred half — written, reviewable, UNOBSERVED

The claim "the wire string equals the tarball's `VERSION`" is **not demonstrated by this
bundle**, and I am not claiming it is. `cargo xtask dist` needs Docker and a network and is
deliberately outside `ci` (`xtask/src/dist.rs:26-28`); I did not attempt an image build. What
this bundle does instead:

* the coupling is *written* — `--build-arg WYRD_VERSION={version}` from the same `version`
  that goes into `VERSION`, `ARG`/`ENV` in the Dockerfile's build stage;
* it is *pinned container-free* by the two new `dist_templates.rs` tests, which run inside
  `cargo xtask ci`;
* the end-to-end equality is *written into* `.github/workflows/release.yml`'s existing smoke
  step (start the role, unsigned `curl` → 403 → compare the `Server:` remainder with
  `VERSION`'s `version:` line, kill the role).

**When it is actually observed: on the next `v*` tag — and no `v*` tag has ever been cut**
(the repo's only tag is `archive/backup-premerge-signoff`; `release.yml:21-22` triggers on
`push: tags: ["v*"]`). At this sign-off that check is written and reviewable but has never
run. The shell inside that step lives in a double-quoted `docker run … sh -c "…"`, so every
container-side `$` is escaped `\$` and inner quotes `\"` — that escaping is the part a human
should eyeball, since no gate can.

Confirmed while there (brief Open question 3): `release.yml:36-39` already checks out full
history and tags, so **no change was needed** for `git describe`.

## Honest limits

* On a checkout with no tag and no override — every checkout today — the advertised string is
  the `0.0.0+git.<sha>[.dirty]` shape (this run: `wyrd/0.0.0+git.a801997.dirty`). That is a
  real build identity, but not a release version; the release-shaped value only appears once a
  tag exists. The `.dirty` suffix is deliberate and carried from `normalize_describe` (brief
  Design decision 2).
* `C4-diff-cov` (advisory) will likely read low on `crates/server/build.rs`: a build script
  runs during the *build*, not under the instrumented test binary, so its 40-odd executable
  lines are changed-and-uncovered by construction. Its decision function `resolve_version` is
  the part that is unit-tested, deliberately — that is the whole reason the impure half is
  this thin.
* The version is baked at compile time, so a binary built before a commit and run after it
  advertises the older sha until it is rebuilt; the `rerun-if-changed` directives make cargo
  do that rebuild, but a *copied* binary is not re-derived. This is the same property
  `cargo xtask dist` already has.
* **Docs currency (rubric):** no living architecture doc was touched. The rubric's enumerated
  surfaces are a port, an API operation, an RPC, a CLI flag or a persisted field — this adds a
  response header and a docker build-arg, none of them. Checked the precedent it mirrors:
  #529's `x-amz-request-id` is named nowhere under `docs/` either
  (`grep -rn "x-amz-request-id" docs/` → no matches), and `05-building-block-view.md:132`
  describes the S3 gateway at one table row per component, a level at which a header does not
  belong. Flagging it here so the reviewer can disagree cheaply.

## Alternatives ruled out

* **`option_env!("WYRD_VERSION")`, no build script** (~5 lines instead of ~115): drops the
  `git describe` rung, so every build today — no tags — would advertise the fallback and the
  feature would be inert for the tool that motivates it.
* **Raw `git describe` from the build script, no shared module** (removes `version.rs` and the
  `#[path]` line in `dist.rs`, ~8 lines of diff): two spellings on two surfaces
  (`v0.1.0-3-gabc12de` vs `0.1.0+git.3.abc12de`), which is the exact bug the version-keyed
  matrix exists to avoid.
* **Duplicating `normalize_describe` and pinning the copies with a test**: the repo's
  cross-file precedent (`xtask/tests/fdb_image.rs`) pins a *constant*, which a substring
  assertion can do; it cannot pin two copies of an algorithm.
* **Stamping the header per handler**: satisfies the success path and misses the 403/404/501
  responses — the ones that end up in bug reports. The router is a bare
  `Router::new().fallback(handle::<G>)` (`crates/gateway-s3/src/lib.rs:216`), so one insertion
  in `handle` covers the whole response category, which is what the invariant is stated over.
* **Passing the version as a parameter through `serve_s3_role` / `serve_s3_dispatch`**: three
  more signatures (all already `#[allow(clippy::too_many_arguments)]`) and a guaranteed
  conflict with #738, which threads `--chunk-size` through those same three. `serve_s3` reads
  the crate constant directly instead: the constant *is* the composition root's knowledge.

## External dependencies

None missing. The build script shells out to `git`, which is present in every gate
environment here (and the packaging pipeline it single-sources from already requires it,
`xtask/src/dist.rs:415-420`); when it is absent or there is no repository the script falls
through to the `0.0.0` rung, which is the `.dockerignore` case the `--build-arg` rung exists
for. No container was needed and none was used.

## Gates run here

* `cargo test -p wyrd-server --test s3_server_version_header` — red pre-fix (compiles,
  2 failed), green post-fix (2 passed), plus the two mutation checks above.
* `cargo fmt --all` over every touched file (the target's commit hook runs `rustfmt`).
* `./engine/xtask.sh ci` — the project's whole gate (typos, docs lint + render, gitlink and
  unsafe-forbid guards, `fmt --check`, clippy `--all-targets`, build, `cargo test
  --workspace`, machete, deny × 3, conformance, statics, deploy-guard, clippy + test under
  `--cfg madsim`): **`xtask ci: all checks passed`, exit 0**. The new `build.rs` passes the
  #616 crate-root scan (`xtask unsafe-guard: every crate root forbids unsafe code`), and
  every new test ran inside it — `s3_server_version_header` (2), the four `gateway-s3` unit
  tests, `version::tests` (compiled and run in **both** `wyrd-server` and `xtask`, which is
  the single-sourcing working), and the two `dist_templates` coupling tests.
* Re-checked after the last edit: `cargo fmt --all -- --check` clean, and `git apply --check`
  of `patch.diff` against a throwaway worktree cut from `origin/main` (`a801997`) applies all
  ten files cleanly. The workflow edit was rendered through the outer shell and syntax-checked
  (`sh -n`) rather than eyeballed, since no gate parses it: the container-side script comes out
  as intended and the readiness loop is `set -e`-safe (`false && break` does not exit).
