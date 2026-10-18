## Summary
**User impact:** the S3 gateway never says which build it is. A client talking to it
cannot tell last month's deployment from today's, an HTTP exchange pasted into a bug
report does not identify the build that produced it, and a client that needs to adjust
to what a given server version supports has to take the version on trust from whoever
configured it.

This PR adds a `Server: wyrd/<build identity>` header to every response the gateway
produces, errors included. The identity is the same one the `s3` role already writes to
its startup log.

**Depends on #885** (#778), which bakes the build identity into the binary. This branch
is stacked on it, so the diff shows #885's commits until that PR merges.

## What to look at
The header is added in one place that every request passes through, so it lands on
successes, client errors, server errors and the start of a streamed download alike.
The server binary tells the gateway which version to advertise. The threat model now
records that an unauthenticated caller can see the exact build, and that this is
intended. The release smoke test checks the header against the tarball's `VERSION`
file.

To try it:

```
cargo build --bin wyrd
target/debug/wyrd s3 --s3-listen 127.0.0.1:9000 --data-dir "$(mktemp -d)" \
  --access-key k --secret-key s --log-format json &
curl -sS -D- -o /dev/null http://127.0.0.1:9000/
```

The unsigned request is refused `403` and the response still carries
`server: wyrd/<v>`, where `<v>` matches the `version` field of the `role started` line
on stderr.

## Root cause
The only header the gateway added to every response was `x-amz-request-id`, at the end
of `handle`; nothing on the wire carried a version, and axum/hyper add no `Server`
header of their own. After #778 the binary knows its identity and logs it, but nothing
passed it to the gateway.

## Fix
- **The stamp.** `handle` inserts `Server` right after `x-amz-request-id`
  (`crates/gateway-s3/src/lib.rs:1632`). Every request reaches `handle` through
  `Router::new().fallback(handle::<G>)` (`:273`), so no handler can miss it. The value
  is rendered once when the router is built (`:265`) and cloned per request.
- **The config field.** `S3Config::server_version` (`:116`), defaulted in
  `S3Config::new` to `"unknown"` (`UNKNOWN_SERVER_VERSION`, `:121`). Only `new`
  constructs the struct, so existing callers keep compiling. `wyrd/` is spelled in one
  place (`SERVER_PRODUCT`, `:124`; `server_header_value`, `:131`). A version that is not
  an RFC 9110 `token` (`is_token`, `:149`) becomes `wyrd/unknown` with a `warn!`, so the
  header stays present and valid. Every byte #778's validator admits is a token
  character, so a real build never hits the fallback.
- **The value.** `serve_s3` sets the field from `crate::version::BUILD_IDENTITY`
  (`crates/server/src/cli.rs:2660`), the same constant the `role started` event
  records. No parameter is threaded through `serve_s3_role` / `serve_s3_dispatch`.
- **The disclosure decision.** The Information disclosure row of
  `docs/design/architecture/14-threat-model.md:89` now says the exact build is visible
  to unauthenticated callers, what that identity contains (for a release, `dist`'s
  version, with a `dirty` marker if the release was cut from a modified tree; for a hand
  build, the commit it was built on), why it is accepted, and that an operator who must
  hide it strips or rewrites `Server` at a fronting proxy. There is deliberately no knob
  in the gateway.
- **Release smoke check.** Inside the existing bookworm `docker run` in
  `.github/workflows/release.yml` (step at `:76`, new block from `:105`), the installed
  `wyrd s3` is started, one unsigned request is sent, and the step fails unless the
  response is `403` and the captured `Server` value(s) equal exactly `wyrd/` plus the
  `version:` line of `VERSION` (`:125-131`). Every `Server` line is captured and the
  operands are quoted, so a duplicate header, a foreign second `Server` line or a value
  with spaces fails the step instead of slipping past `[`. The exact text is pinned in
  `xtask/tests/dist_two_binary_layout.rs` (`server_header_block`, `:359`, used at
  `:416`).

## Verification
Line numbers are on `main` with #885 and this patch applied.

- **Claim:** every response category the gateway produces carries
  `Server: wyrd/<v>`, and `<v>` is byte-equal to the identity the same process logged.
  - **Checked:** `crates/gateway-s3/src/lib.rs:1525-1633` (`handle`, stamp at `:1632`
    after the request-id stamp at `:1625`); `:273` (the single fallback route);
    `crates/server/src/cli.rs:2660` (the value comes from `BUILD_IDENTITY`,
    `crates/server/src/version.rs:48`).
  - **Test:** `crates/server/tests/s3_server_version_header.rs`
    (`every_response_category_names_the_logged_build_identity`, `:280`). It spawns the
    built `wyrd` binary as an `s3` role on an ephemeral port, reads the port and the
    logged `version` from stderr, kills the child on every exit path, and checks four
    responses from that one process:
    1. signed `PUT` → `200` (`:291`);
    2. unsigned `GET` → `403`, refused before auth (`:303`);
    3. signed `GET` of the object just written, streamed → `200` (`:312`);
    4. signed `GET /wyrd-bucket?acl` → `501 NotImplemented`, refused after auth by the
       unsupported-subresource list (`:324`; list at `lib.rs:410`).

    Each must carry exactly one `Server` header equal to `wyrd/` + the logged version
    (`assert_server`, `:269`). **Fails without the fix:** with `lib.rs` and `cli.rs`
    reverted and the test kept, it compiles and fails at leg 1 with no `server` header in
    the response. **Passes with it:**
    `cargo test -p wyrd-server --test s3_server_version_header`. With only the `cli.rs`
    line reverted it also fails, on `wyrd/unknown` vs the logged value, so the test
    checks the real value and not just that a header exists.
- **Claim:** the header value is always well-formed, and an unset or bad version
  degrades to `wyrd/unknown` rather than disappearing.
  - **Test:** `server_header_value_renders_the_version_and_falls_back_when_unusable`
    (`crates/gateway-s3/src/lib.rs:3527`): normal, `+git`, `.dirty`, newline, non-ASCII,
    space, `/`, empty, and the `S3Config::new` default.
- **Claim:** the release smoke comparison only passes on proven equality.
  - **Checked:** `.github/workflows/release.yml:105-131`, pinned at
    `xtask/tests/dist_two_binary_layout.rs:359`.
  - **Test:** the pin (`cargo test -p xtask --test dist_two_binary_layout`) keeps the
    workflow and its expected text in step. The comparison logic itself was run outside
    CI, extracted from the workflow and run under `dash -eu` (Debian's `/bin/sh`) against
    18 crafted response heads and against the real binary: it passes on a matching
    header and fails on a missing header, a version mismatch, two `Server` lines, a
    foreign `Server` line, and values with spaces or shell operators.
- `cargo xtask ci` passes on the patched tree.

## Known limits
- **A real `500` is not exercised.** Producing one from outside the process needs an
  injected backend fault. The `501` is the repeatable stand-in for the server-error
  class; a `500` leaves `dispatch` through the same stamp point in `handle`, but no
  test sends one.
- **The release smoke check has not run yet.** It runs only on a `v*` tag (none has
  been cut) or a manual `workflow_dispatch` of `release.yml`. Until then, equality with
  the tarball's `VERSION` is written and reviewed but not observed end to end. A manual
  run on this branch, or a local `cargo xtask dist` followed by comparing
  `target/dist/wyrd-*/VERSION` with a spawned role's `Server` value, would show it.
  Both need Docker and network access.
- **Responses hyper sends on its own** (a malformed request line, an unparseable
  `Content-Length`, an oversized header section) never reach the gateway and carry no
  `Server` header. The code comment and the threat-model row say so.
- **Unrelated flaky tests.** Some `wyrd-gateway-s3` unit tests that capture `tracing`
  output (e.g. `a_bodyless_response_is_recorded_complete_not_aborted`) can see an empty
  capture under heavy parallel load. Measured at the same rate with and without this
  patch (8 failures in 200 parallel runs on each), so it predates this change.

Fixes #779
