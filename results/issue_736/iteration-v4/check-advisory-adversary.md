# Adversarial review — issue 736 (s3-server-version-header)

Attacked the evidence, the fix and the verdict. The red→green itself survived (see
"could not refute" at the end); four attacks landed, one of them on a claim the
patch's own module doc makes.

- **NEEDS-HUMAN [impl] — `crates/server/build.rs:163` watches the git *index*, which
  reintroduces the exact rebuild cost the doc two paragraphs above it (`:129-137`)
  rejects.** Both halves measured in this sandbox: (a) a plain `git status` after any
  working-tree edit rewrites `.git/index` (mtime bumped, verified on a throwaway repo);
  (b) cargo 1.96.1 recompiles a crate whose build script re-ran **even when the script's
  output is byte-identical** (verified with a two-file probe crate: `touch watched` →
  `Compiling probe2`). Concrete case: edit any file anywhere in the workspace, run
  `git status` (or `git diff`, or let an editor's git integration poll), then
  `cargo test -p wyrd-server` → `wyrd-server` recompiles and its **41** integration-test
  binaries (`crates/server/tests/*.rs`) relink, with nothing in that crate changed. The
  doc at `:132-134` declines to watch source files precisely because that "recompiles and
  relinks `wyrd-server` — the `wyrd` bin plus the ~40 integration-test binaries — every
  time"; the `index` entry buys most of that cost back for the ordinary edit→status→test
  loop while still not closing the `.dirty` lag. Note this pulls *against* the batch
  review's four blocking `build.rs` findings (which ask for a *wider* watch set): the two
  cannot both be satisfied by adding paths, so the builder should pick a side explicitly
  — either accept a lagging `.dirty` and drop `index`, or accept the churn and say so —
  rather than widening the set again.

- **NEEDS-HUMAN [impl] — `crates/server/src/version.rs:104-117`: `is_identity` does not
  enforce the docker-tag rule its own doc invokes to justify its narrowness, so a
  well-formed-looking identity fails the packaging build minutes later.** Compiled
  `version.rs` standalone and ran it; measured outputs:
  `resolve_version(Some("-1.2.3"), …)` → `Ok(version="-1.2.3", origin=Explicit)`,
  `Some(".1.2.3")` → `".1.2.3"`, `Some("+build")` → `"+build"`, and without any override
  `resolve_version(None, Some("v-1.0"), Some("abc1234"), …)` → `"-1.0"` (origin `Git`).
  A docker tag is `[A-Za-z0-9_][A-Za-z0-9_.-]{0,127}` — the **first** character may not be
  `-`, `.` or `+`. So `WYRD_VERSION=-1.2.3 cargo xtask dist` passes rung 1's validation,
  reaches `xtask/src/dist.rs:image_build_args` → `-t wyrd:-1.2.3-<flavor>`, and
  `docker buildx build` dies with `invalid reference format` after the whole build — the
  late, confusing failure the validator at `:104-115` claims to prevent ("a `tchar` such
  as `|` … would be a valid header and an invalid tag"). One added leading-byte rule in
  `is_identity` (rejecting `-`/`.`/`+` first, and a 128-byte ceiling) closes it and would
  turn the failure into the build-time error message `resolve_version` already writes.

- **NEEDS-HUMAN [impl] — `docs/design/architecture/08-crosscutting-concepts.md:85` states
  the header is stamped on "**every** response"; responses hyper generates before the
  service is entered carry none.** `handle`'s stamp (`crates/gateway-s3/src/lib.rs:1660`)
  is only reached once axum dispatches. A malformed request line, an invalid
  `Content-Length`, or a header section over hyper's buffer limit is answered by hyper
  itself with `400`/`431` and no `Server:` — concretely,
  `printf 'GET /a HTTP/1.1\r\nHost: x\r\ncontent-length: abc\r\n\r\n' | nc 127.0.0.1 <port>`
  against a `wyrd s3` role returns a bare 400. This is the same reasoning the pre-fix RED
  leg relies on (hyper adds no `Server` of its own), so it cuts both ways. The code is
  right; the living-doc sentence an operator reads should be scoped to responses the
  gateway service produces, or the gap named.

- **NEEDS-HUMAN [human] — the header is deliberately stamped on the pre-auth 403
  (`crates/gateway-s3/src/lib.rs:1660`, exercised by
  `crates/server/tests/s3_server_version_header.rs`'s unsigned leg), so any anonymous
  client learns the deployment's exact commit and whether it was built from an edited tree
  (`…+git.<sha>.dirty`), and `docs/design/architecture/14-threat-model.md:89`
  ("Information disclosure") was not revisited.** Concrete: `curl -sSD- http://<gw>/`
  with no credentials → `403` + `Server: wyrd/0.0.0+git.abc12de.dirty`. Real S3 answers
  the coarse `Server: AmazonS3` for this reason. The brief decided granularity (decision
  1) and `.dirty` (decision 2), but it decided them for the *client* use case; nothing in
  the slice weighs unauthenticated reach, and there is no configuration knob to coarsen or
  suppress the value for an internet-facing gateway. A human should confirm this is the
  intended posture (and, if so, that the threat model records it) rather than a decision
  taken sideways.

- **NEEDS-HUMAN [human] — the C5 mutation gate never ran, so ~560 net-new lines of pure
  logic have no mutation evidence, and the verdict on them is provisional (#236).**
  `gate-logs/C5-mutants.log:1954` shows the failure is the unmutated *baseline*:
  `xtask/tests/repo_hygiene_guards.rs:137` — `git ls-files -s -z must succeed` — because
  cargo-mutants' scratch copy carries no `.git`. Environmental and untouched by this
  patch, so **not** a refutation, but it means the one gate that would have probed
  `version.rs`'s branch logic (the `-dirty` carry on the fail-closed path, the empty-
  override rung, `describe_is_dirty`) produced nothing. Recorded here so the "gates green
  except a known base failure" framing is not read as "the new logic was mutation-tested".
  Same class: C4-ci's red is exclusively RUSTSEC-2026-0258 (`h2`) at
  `gate-logs/C4-ci.log:2859-2866`, with `bans ok, licenses ok, sources ok` — base-carried
  and matching the brief's `Depends on: 773`, not a defect of this patch.

## Attempted and could not refute

- **The red→green is real and on the production path.** Re-adjudicated from
  `gate-logs/C4-verify.log`: the reverted tree fails at
  `crates/server/tests/s3_server_version_header.rs:279` with the full response head
  printed and no `Server:` line — the test drives the built `wyrd` binary
  (`env!("CARGO_BIN_EXE_wyrd")`) through the real router and the real composition root,
  not an in-process fixture, and it names no symbol the patch introduces, so it compiles
  on both legs. It cannot pass on `wyrd/unknown` (leg 2), on a constant (leg 4's sha
  containment against an independent `git` probe), or with the log field deleted (leg 3
  fails on a missing field rather than skipping).
- **No second production path emits an unconfigured identity.** `S3Config::new` has
  exactly one non-test caller, `crates/server/src/cli.rs:2386`, and `serve_s3` sets
  `server_version` there; `router()` is the only `AppState` constructor
  (`crates/gateway-s3/src/lib.rs:287`) and `serve` adds no layers, so there is no route
  that bypasses the stamp inside the service.
- **The release smoke's kill→reinstall race does not exist.** Hypothesised `ETXTBSY` when
  `./install.sh` (`deploy/dist/install.sh:136`, `install -m 0755 … /usr/local/bin/wyrd`)
  re-runs immediately after `kill $s3_pid` in `.github/workflows/release.yml:104` → `:120`;
  measured on this host — `install`/`cp` over a *still-running* executable both return 0.
- **`is_tchar` (`crates/gateway-s3/src/lib.rs:161`) is exactly RFC 9110 §5.6.2** — all 15
  punctuation `tchar`s, no extras — and `server_header_value`'s `expect` is unreachable
  because every byte is checked first.
- **`resolve_version`'s postcondition holds** on every probed input (explicit, tagged,
  untagged, dirty, unrepresentable-tag, empty, no-input): the returned version always
  satisfies `is_identity`, and the two distinct tags `v1/2` / `v1.2` stay distinct.
- **The packaging coupling is sound**: `ARG WYRD_VERSION=""` is global and re-declared in
  the `build` stage before `ENV WYRD_VERSION=${WYRD_VERSION}`
  (`deploy/docker/wyrd/Dockerfile:39,47,76`), the empty default falls through to the next
  rung rather than baking `wyrd/`, and `release.yml:36-38` checks out with
  `fetch-depth: 0` + `fetch-tags: true`, so `git describe` sees the tag it releases.
