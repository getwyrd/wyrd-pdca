# Build notes — issue 779 / s3-server-version-header-stamp

Base: `getwyrd/wyrd@main` as integrated in the cycle worktree, `6032ff5` ("pdca-integrate:
issue_778"), so #778's `wyrd_server::version::BUILD_IDENTITY` and the `version` field of the
`role started` event are present (`crates/server/src/version.rs:45`,
`crates/server/src/cli.rs:2398`). Leg 5 therefore could go green on this base; no STOP needed.

Line numbers below are on the patched tree unless marked "base".

## What changed and why

1. **`S3Config` gains `server_version`** — `crates/gateway-s3/src/lib.rs:116`, defaulted in
   `S3Config::new` to `UNKNOWN_SERVER_VERSION` (`:121`, `:181`). Only `new` builds the struct
   (no struct literal in the tree; every caller in `crates/**` goes through `S3Config::new`),
   so existing callers keep compiling. The default is the literal `unknown`, not the crate's
   `CARGO_PKG_VERSION`: a library caller was never told a build, and the rubric's
   "never fabricate" rule (serialization identity) argues against inventing one.
2. **One spelling of `wyrd/`** — `SERVER_PRODUCT` (`:124`) and `server_header_value` (`:131`)
   are the only place the header value is formed. The version is checked against the
   RFC 9110 `token` grammar (`is_token`, `:149`); a non-token version is replaced by
   `unknown` with a `warn!`, so the header stays present and grammatical. My first draft
   relied on `HeaderValue::from_str` failing for bad input; the unit test showed it accepts
   non-ASCII (obs-text), so I switched to the explicit token check (rubric: grammar
   strictness). Every character #778's `validate_identity` allows
   (`crates/server/src/version/derivation.rs:93-95`: `A-Z a-z 0-9 _ . + -`) is a `tchar`, so
   the fallback never fires for a real build.
3. **Rendered once, stamped once** — the `HeaderValue` is built in `router()` (`:265`), kept
   in `AppState.server` (`:202`), cloned at the top of `handle` (`:1543`) and inserted at the
   single stamp point next to `x-amz-request-id` (`:1632`; base `:1552-1556`). Every request
   reaches `handle` through `Router::new().fallback(handle::<G>)` (base `:205`), and every
   response `dispatch` returns passes this point, so the invariant holds for the whole
   category rather than per handler.
4. **Composition root sets it** — `crates/server/src/cli.rs:2660`, in `serve_s3`, from
   `crate::version::BUILD_IDENTITY` (ADR-0010: the composition root knows the build, the
   wire crate does not). No parameter threading through `serve_s3_role`/`serve_s3_dispatch`.
5. **Threat model** — `docs/design/architecture/14-threat-model.md:89`, the Information
   disclosure row, gateway column: records that every response the gateway *service*
   produces (pre-auth 403 included) discloses the exact build, that this is intended
   (proposal 0017 §3), that an operator strips/rewrites `Server` at a fronting proxy, that
   there is no gateway knob, and that hyper-generated responses carry no `Server`.
6. **Release smoke step** — `.github/workflows/release.yml:103-124`, inside the existing
   `docker run`, after the `libfdb_c` install and the `wyrd` usage check: start `wyrd s3` on
   `127.0.0.1:18080`, poll with an unsigned `curl` (20 tries, `--max-time 10` each), fail
   loudly with the role's log if it never answers, kill the role, require `403`, extract the
   `Server: wyrd/` remainder and compare it with the `version:` line of `VERSION`; a mismatch
   or an empty `VERSION` line prints both values and fails. No `"` or backtick in the
   in-container script; container-side `$` written as `\$` (the script is one double-quoted
   word of the runner's shell, per the step's own comment).
7. **Smoke-step pin (not foreseen by the brief)** — the smoke step is pinned as exact text by
   `xtask/tests/dist_two_binary_layout.rs` (the comment at `release.yml:62-67` says so), so
   editing the workflow without the pin would fail `cargo xtask ci`. Added
   `server_header_block(prefix, roles)` (`xtask/tests/dist_two_binary_layout.rs:359`) and
   pushed it after the roles binary's usage block (`:411`). This is a MODIFIED existing
   `*/tests/*.rs`, not an added one, so the C4-verify classifier's discriminator stays the
   single added file `crates/server/tests/s3_server_version_header.rs`.

## The test — `crates/server/tests/s3_server_version_header.rs`

Spawns `env!("CARGO_BIN_EXE_wyrd") s3 --s3-listen 127.0.0.1:0 --data-dir <tmp> --access-key …
--secret-key … --log-format json`, held in a `RoleGuard` that kills and reaps on drop (every
exit path). A reader thread forwards stderr; startup waits on a 60 s deadline for both the
`serving S3-compatible HTTP on <addr>` line (the port is read, not guessed) and the
`role started` JSON event, whose `fields.version` is V. Requests go over blocking
`std::net::TcpStream` with 30 s read/write timeouts. Then, against that one child:

1. signed `PUT /wyrd-bucket/server-version-object` → `200`, `Server` == `wyrd/`+V;
2. unsigned `GET` of the same key → `403`, `Server` == `wyrd/`+V;
3. signed `GET` of the same key (streamed body) → `200`, `Server` == `wyrd/`+V;
4. signed `GET /wyrd-bucket?acl`, signed WITH query `acl` (the local `signed_headers`
   threads the query into `sigv4::sign`'s third argument) → `501`, `Server` == `wyrd/`+V.

Leg 5 is applied in each of 1-4: exactly one `Server` header, byte-equal to `wyrd/` + the
logged V. The test names no symbol this patch introduces (only `sigv4::{sign,
format_amz_date, Credentials}`, which exist on base), so it compiles on the reverted tree.

**What leg 4 does NOT claim.** A genuine `500 InternalError` (`gateway_error_response`
through `classify`'s `InternalError` arm) is not driven: it is not deterministically
producible from a black-box test without injecting a backend fault, and that is out of scope.
`501` is the deterministic representative of the server-error class. What carries the rest
of the category is structural: every response `dispatch` returns, `500` included, flows
through `handle`'s one stamp point (`crates/gateway-s3/src/lib.rs:1632`). The `500` path was
not exercised.

A small unit test (`crates/gateway-s3/src/lib.rs:3527`) pins `server_header_value`'s
rendering and fallback (token, `.dirty`, newline, non-ASCII, space, `/`, empty, `new()`
default). It is a `#[cfg(test)]` unit test in `src/`, not a `*/tests/*.rs` file.

## Red → green (run with `cargo test -p wyrd-server --test s3_server_version_header`, the
brief's named command, wrapped in `timeout`)

- Fixed tree: `test every_response_category_names_the_logged_build_identity ... ok`.
- Production reverted (`crates/gateway-s3/src/lib.rs` + `crates/server/src/cli.rs` checked
  out to base, test kept): FAILED at leg 1 — `left: []` (no `Server` header) vs
  `right: ["wyrd/0.0.0+git.6032ff5"]`; the response head printed shows only `content-length`,
  `etag`, `x-amz-request-id`, `connection`, `date`.
- Partial revert (gateway stamp kept, `cli.rs` line removed): FAILED — the header is
  `wyrd/unknown`, not `wyrd/0.0.0+git.6032ff5`. So leg 5 binds the composition-root value,
  not just the presence of a header.

## Self-refutation

- **(a) Genuine red?** Yes. Reverted both production files and re-ran: red on an absent
  header (above). A second partial revert proved leg 5 also catches a placeholder.
- **(b) Production path?** Yes. The test spawns the real built `wyrd` binary; requests go
  through the real `serve_s3` composition root, the real axum router, `handle`, and the
  real SigV4 verifier. Nothing is mocked; signing uses the production `sigv4::sign`.
- **(c) Fixture includes the fault?** Yes. The fault is "some response category lacks the
  header"; the fixture includes all four categories the invariant names, including the two
  error planes (pre-auth 403, post-auth 501) a per-handler stamp would miss. The expected
  value is read from the same process's log, not a constant. The 500 limit is stated above.

## Gates run

- `cargo fmt --all --check`: clean.
- `cargo clippy -p wyrd-gateway-s3 -p wyrd-server -p xtask --all-targets -- -D warnings`:
  clean.
- `cargo test -p xtask --test dist_templates --test dist_two_binary_layout`: 33 passed (the
  smoke pin agrees with the edited workflow).
- `cargo xtask ci` through the PDCA runner (`engine/xtask.sh ci`): first run failed on
  `typos` (`unparseable` in my test; fixed to `unparsable`). Second run failed one unrelated
  unit test, `tests::a_bodyless_response_is_recorded_complete_not_aborted`
  (`crates/gateway-s3/src/lib.rs`, empty log capture). It drives `finish_response` directly,
  which this patch does not touch, and it passed 3 of 3 re-runs of
  `cargo test -p wyrd-gateway-s3 --lib`; I read it as a pre-existing flake (an empty
  capture is the shape of `tracing` callsite-interest caching across parallel tests), not a
  regression. Third run (after adding `--max-time 10` to the smoke curl): `xtask ci: all checks
  passed`, exit 0.

## Smoke step, checked locally without Docker

I extracted the new in-container block from the workflow, un-escaped `\$`, pointed it at
the locally built `target/debug/wyrd` with a hand-written `VERSION`, and ran it under
`sh -eu`:
- with a stale binary that advertised `wyrd/unknown` it failed loudly, printing
  `wire Server: wyrd/unknown, VERSION version: 0.0.0+git.6032ff5` and the response head;
- with the current binary it passed (`wire=0.0.0+git.6032ff5`), after one refused connect
  that the retry loop absorbed.
This checks the shell logic only. It is not the release run.

## Deferred, declared in the brief: equality with the tarball's `VERSION`

The smoke-step comparison is **written and reviewable but UNOBSERVED**. It runs only in
`.github/workflows/release.yml`, which triggers on `v*` tags, and no `v*` tag has ever been
cut. I did not attempt an image build, `cargo xtask dist`, or a `workflow_dispatch` run. The
two ways to observe it without a tag, for the human to choose at sign-off: a
`workflow_dispatch` run of the release workflow on the branch, or locally `cargo xtask dist`
and comparing `target/dist/wyrd-*/VERSION`'s `version:` line with the `Server:` value of a
spawned role. Both need Docker and a network.

## Considered and ruled out

- **Per-handler stamping**: fails the invariant by construction (misses the pre-auth 403);
  the single stamp point is one `insert` line.
- **An axum `SetResponseHeader` layer** (tower-http): equivalent coverage, but adds a layer
  and possibly a dependency for what is one line beside an existing stamp; the brief points
  at the stamp point.
- **Threading the version through `serve_s3_role`/`serve_s3_dispatch`**: unnecessary,
  `serve_s3` is in the same crate as `BUILD_IDENTITY`; it would also collide with #738.
- **Defaulting to `CARGO_PKG_VERSION`**: claims a version the caller never set.
