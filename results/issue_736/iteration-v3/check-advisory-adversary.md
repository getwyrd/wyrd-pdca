# Adversarial review — issue #736 (s3-server-version-header)

Attacked: the red→green evidence, the stamp point, the derivation's inputs, the skip
paths in the binding test, and the gate verdicts. The evidence itself survived (see the
last section); five attacks landed.

- **NEEDS-HUMAN [impl] — leg 4's skip predicate is minted by the artifact under test, so the
  only provenance assertion can be silently switched off.** `crates/server/tests/s3_server_version_header.rs:423-437`
  reads `version_origin` out of the child's own `role started` event and `return`s before the
  sha assertion whenever it is not `git`. Concrete failing case: change `crates/server/build.rs:65-68`
  to emit `WYRD_VERSION_ORIGIN=explicit` unconditionally (a one-token slip in the one line
  no test pins) and the entire bundle stays green — `crates/server/src/lib.rs:827` only
  checks membership in `{explicit,git,unknown}`, `crates/server/src/cli.rs:2213` merely
  prints it, and the test prints `leg 4 skipped` and passes. `cargo mutants` would not have
  caught it either: build scripts are not mutated. This is not hypothetical — in the C5
  baseline the copied tree has no `.git` (which is exactly why `scan_gitlinks_is_green_over_the_real_index`
  failed, `gate-logs/C5-mutants.log:1942`), so the test ran with origin `unknown` and leg 4
  was skipped while the gate recorded the test green (`gate-logs/C5-mutants.log:1620`). Cheap
  strengthening that keeps every supported configuration passing: `origin == "unknown"` is a
  *contradiction* when `git rev-parse --short HEAD` succeeds from `CARGO_MANIFEST_DIR` (the
  same directory `build.rs:45` asked from) — that case should fail, not skip.

- **NEEDS-HUMAN [impl] — under git's `reftable` ref backend the build script's watch set is
  inert, so a commit bakes a stale identity and nothing says so.** `crates/server/build.rs:98-127`
  opts the crate out of cargo's default package-file watch (`:100-103`) and claims the
  replacement set is complete. Measured here on git 2.53 with `git init --ref-format=reftable`:
  `.git/refs/heads` is a 41-byte stub *file*, `.git/refs/tags` and `.git/packed-refs` do not
  exist (both dropped by the `resolved.exists()` guard at `:123`), `symbolic-ref -q HEAD`
  answers `refs/heads/master` whose file does not exist (dropped too), and `.git/HEAD` is the
  fixed `ref: refs/heads/.invalid`. Two commits later `git describe` had moved `56f2791` →
  `44611c8` while the mtime of every watched path was unchanged; only `.git/reftable/` moved.
  So `cargo build` after a commit does not re-run the script: the binary advertises the
  *previous* commit's sha and the `role started` event still reports `version_origin: git` —
  precisely the staleness `emit_rerun_directives`' doc comment claims completeness against.
  One-line fix: also watch `git rev-parse --git-path reftable` (or the whole common dir when
  `git rev-parse --show-ref-format` reports `reftable`).

- **NEEDS-HUMAN [impl] — `git describe` escapes the source tree, so a build can confidently
  advertise a completely unrelated repository's commit.** `crates/server/build.rs:45` runs git
  with `current_dir(CARGO_MANIFEST_DIR)` and accepts whatever repository git's upward
  discovery finds; nothing checks that the repository owns this checkout. Measured: a source
  directory nested inside (and even `.gitignore`d by) an unrelated repo yields that outer
  repo's `describe`. Concrete case: `cd ~/anything-under-git && tar xf wyrd-src.tar.gz && cargo build --release`
  ships a binary that answers `Server: wyrd/0.0.0+git.<foreign sha>` and logs
  `version_origin: git` — a version that "can silently be wrong", which is the exact failure
  the issue exists to remove, now stated with more authority than the operator-supplied
  `--server-version` it replaces. The binding test cannot catch it: its own oracle
  (`crates/server/tests/s3_server_version_header.rs:289`) probes git from the same directory
  with the same escaping discovery, so leg 4 goes *green* on the foreign sha. Guard cheaply
  with `git ls-files --error-unmatch Cargo.toml` (or compare `--show-toplevel` against the
  workspace root) and fall to `0.0.0+git.unknown` / origin `unknown` when it fails.

- **NEEDS-HUMAN [human] — the `-dirty` decision the brief made explicitly is reversed in the
  build script, so two different binaries can carry one identity.** Brief §Design "The three
  decisions", item 2 answers "Does a `-dirty` build advertise as dirty? **Yes**"; `crates/server/build.rs:39-45`
  and `:87-96` deliberately ask `git describe --tags --always` *without* `--dirty` for
  rebuild-cost reasons. Consequence: a `cargo build` from a modified tree advertises
  byte-identically to a clean build of the same commit — "an untagged, dirty binary in a
  deployment is precisely the thing worth catching" (brief) is no longer caught — and one
  checkout now yields two identities depending on the build path, since `xtask/src/dist.rs:486-493`
  passes the `--dirty` derivation in verbatim (`cargo build` → `0.0.0+git.abc`,
  `cargo xtask dist --host` → `0.0.0+git.abc.dirty`). The measured rebuild cost behind the
  reversal is real, so this is a scope/fitness call rather than an obvious defect — but it is
  a silent reversal of a stated brief decision and should be ratified, not inherited.

- **NEEDS-HUMAN [impl] — the streaming-GET head is named in the invariant and asserted
  nowhere.** The invariant is stated over the response CATEGORY — "success, client error,
  server error, and the streaming-GET head alike". The bundle asserts a signed PUT 200
  (`crates/server/tests/s3_server_version_header.rs:322-330`), an unsigned 403 (`:334`) and an
  in-process 403 (`crates/gateway-s3/src/lib.rs:5573`). The one response whose body is produced
  *after* `handle` returns — a successful GET's streamed body, wrapped by `finish_response` —
  is never checked on the wire. Risk today is low (the stamp at `crates/gateway-s3/src/lib.rs:1654`
  is shared), but the object is already stored by the PUT three lines earlier, so a signed GET
  plus `advertised_version(&head, "signed GET")` is a 4-line addition that makes the asserted
  set match the invariant as written.

- **NEEDS-HUMAN [human] — the gating workspace gate never completed, so "nothing else
  regressed" is unproven (toolchain/environment, verdict provisional).** `check-gates.json`
  records C4-ci `unverifiable` at the 7200s timeout; the log ends with seven `custodian_gc`
  tests "running for over 60 seconds", so `cargo deny`, the conformance/statics gates and the
  rest of `cargo test --workspace` never ran. Explicitly **not** scored as a refutation and
  not attributable to this diff: `typos`, the docs lint/render, clippy and
  `cargo build --workspace --all-targets` all completed green (`gate-logs/C4-ci.log:11-21,325,583`),
  and the very same `custodian_gc` binary ran green on the same host with the same patch in
  the C5 baseline (`gate-logs/C5-mutants.log:1277-1284`). Recording it so sign-off treats the
  workspace-wide clean bill as assumed rather than demonstrated.

## Attacked and could not refute

- **The red→green is real and on the production path.** `gate-logs/C4-verify.log` shows the
  reverted tree failing at `crates/server/tests/s3_server_version_header.rs:273` on a real
  `wyrd s3` child's `HTTP/1.1 200 OK` head that carries `x-amz-request-id` and no `Server`,
  then green with the fix. No net-new symbol is named from the test file, so the red leg
  compiles; the child is the built binary, so the composition root (`crates/server/src/cli.rs:2392`)
  is genuinely traversed rather than mocked.
- **The stamp point cannot be bypassed.** The router is a bare fallback with no layers
  (`crates/gateway-s3/src/lib.rs:289`) and `handle` has no early return between entry and the
  insert at `:1654`, so 200/403/404/405/501 and the wrapped streaming head all pass it. I found
  no second production composition of an `S3Config` — `crates/server/src/cli.rs:2386` is the
  only non-test caller of `S3Config::new`.
- **The `tchar` set is right.** `crates/gateway-s3/src/lib.rs:155` lists exactly RFC 9110
  §5.6.2's fifteen punctuation characters plus alphanumerics; the derivation's narrower
  `is_identity` is a strict subset, and `crates/server/src/lib.rs:806-847` pins the two
  contracts against each other over real tag shapes, including the `/`-bearing one.
- **Leg 2 can no longer false-fail on a supported build.** The repository-less rung yields
  `0.0.0+git.unknown`, not the bare `0.0.0` the assertion rejects — confirmed by the `.git`-less
  C5 baseline running the test green.
- **Docs currency holds**: the only file in `docs/`+`specs/` naming `x-amz-request-id` is the
  one this patch updates.
- Not re-raised: the five trim/sanitize findings already blocking at T4 (`gate-logs/T4-batch-review.log`).
