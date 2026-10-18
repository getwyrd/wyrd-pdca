# Build notes — issue 779 / s3-server-version-header-stamp (iteration 2)

Base: `getwyrd/wyrd@main` as integrated in the cycle worktree, `6032ff5` ("pdca-integrate:
issue_778"). #778 is on it: `wyrd_server::version::BUILD_IDENTITY`
(`crates/server/src/version.rs:48`) and the `version` field of the `role started` event, so
leg 5 can go green here. No STOP needed.

Line numbers are on the PATCHED tree unless marked "base".

## What this iteration changed, per carry-forward item

The v1 production change (gateway stamp, config field, composition-root wiring) and the v1
test were not faulted by any reviewer and are carried over unchanged. Three items were
raised; this is what I did with each.

### 1. C4 "choose release-time observation or an out-of-band dist smoke" — not build work

The brief settles this one: Do must not attempt an image build, `cargo xtask dist`, or a
`workflow_dispatch` run (`brief.md:187-201`). I attempted none. It stays a human choice at
sign-off; the two routes are restated under "Deferred" below.

### 2. Threat model claimed the identity is "a commit, never a claim about the working tree"

The reviewer was right. `cargo xtask dist` runs `git describe --tags --always --dirty`
(`xtask/src/dist.rs:542`), normalizes it (`crates/server/src/version/derivation.rs:41-69`) and
hands the result to the image build as `WYRD_VERSION`, which is used verbatim, `.dirty`
included (`crates/server/src/version.rs:8-13`; pinned by `derivation.rs:329-343`). So a
release cut from a modified tree advertises that fact.

Rewritten at `docs/design/architecture/14-threat-model.md:89` (base `:89`, the Information
disclosure row, gateway column). It now says: for a release the identity is the version
`dist` wrote to the tarball's `VERSION` (`0.1.0` on a tag, `0.1.0+git.3.abc12de` past one),
carrying a `dirty` marker (`0.1.0+git.dirty`, `0.1.0+git.3.abc12de.dirty`) when cut from a
modified tree, so that fact is disclosed too; a hand-built binary names the commit it was
built on and says nothing about edits on top of it (`version.rs:26-30`). I spelled out both
dirty shapes because `normalize_describe` produces `+git.dirty` on an exact tag
(`derivation.rs:59-60`), not an appended `.dirty`, so "`.dirty` appended" would have been
wrong again in a smaller way.

**For the human:** the brief's Decision 2 rests on the same wrong premise ("#778's Decision 1
already removed the dirty-tree half of the disclosure", `brief.md:282-284`). That is true only
for hand builds (rung 2 runs without `--dirty`), not for `dist` releases. The decision to
accept the disclosure is binding and I did not reopen it; the doc now records what is
actually disclosed. Whether a dirty release should ever be cut is a release-process question
outside this slice.

### 3. Unquoted `[ x$wire != x$shipped ]` could pass without comparing

Also right, and reproduced. Changed at `.github/workflows/release.yml:125-131` and its pinned
copy `xtask/tests/dist_two_binary_layout.rs:379-385`:

- `wire` now captures the value of EVERY `Server` line, whatever its product, after a
  case-insensitive name and optional blanks (RFC 9110 OWS):
  `sed -n 's/^[Ss][Ee][Rr][Vv][Ee][Rr]:[[:blank:]]*//p'`. A second `Server` line makes `wire`
  multi-line and so unequal. v1's pattern only matched `server: wyrd/` lines, so a foreign
  second `Server` line was invisible to it.
- Operands are quoted, written `\"` so the runner's shell passes them into the container's
  `sh -c` word: `[ -n \"\$shipped\" ] && [ \"\$wire\" = \"wyrd/\$shipped\" ]`.
- The test is in positive form: `if <proven equal>; then echo match; else echo both values;
  dump head; exit 1; fi`. An error inside `[` (status 2) now lands in the failing branch.
  That is the real cause of the false pass: `set -e` does not apply to an `if` condition, so
  v1's `if <mismatch>; then fail` turned a `[` error into "no mismatch".
- Failure output brackets both values, so an empty or multi-line value is visible.
- Dropped the unused loop variable (`for _ in …`, shellcheck SC2034).
- `release.yml:68-75`, the YAML comment above the step (outside the pin, which starts at
  `- name:`, `dist_two_binary_layout.rs:315`), said to keep `"` out of the script. Added one
  sentence: where the script needs a quote, write it escaped as `\"`. Otherwise the next
  editor would read the new `\"` as a violation.

**Evidence (shell logic only, not a release run).** I extracted the block from
`release.yml`, expanded it through bash's double-quote handling the way the runner does
(`printf "%s\n" "<block>"`), moved `/tmp/` paths into scratch, and ran it under `dash -eu`
(bookworm's `/bin/sh`).

Comparison alone, 18 crafted response heads (CRLF, as `curl -D` writes them), v1 condition
vs the new one:

| case | v1 | new | want |
|---|---|---|---|
| single matching header | PASS | PASS | PASS |
| title-case header name | PASS | PASS | PASS |
| tab after the colon (OWS) | FAIL | PASS | PASS |
| two Server lines, `1.2.3` then `2.0.0` (the reviewer's repro) | **PASS** | FAIL | FAIL |
| two identical Server lines | **PASS** | FAIL | FAIL |
| `wyrd/1.2.3` plus a foreign `Server: nginx` | **PASS** | FAIL | FAIL |
| whitespace-bearing mismatch `wyrd/9.9.9 1.2.3` | **PASS** | FAIL | FAIL |
| trailing word `wyrd/1.2.3 x` | **PASS** | FAIL | FAIL |
| no Server header | FAIL | FAIL | FAIL |
| foreign product `nginx/1.2.3` | FAIL | FAIL | FAIL |
| glob value `wyrd/*` | FAIL | FAIL | FAIL |
| `wyrd/!` | FAIL | FAIL | FAIL |
| whole value `!` / `(` / `-n` / `=` (4 cases) | FAIL | FAIL | FAIL |
| version mismatch `2.0.0` | FAIL | FAIL | FAIL |
| VERSION with no `version:` line | FAIL | FAIL | FAIL |

v1 on the reviewer's case printed `[: x1.2.3: unexpected operator` and exited 0, as reported.
The new condition fails all five false-pass cases and the four operator-valued cases fail
through the mismatch branch (their output is the `Server value(s): [!] …` line, not a `[`
error).

Whole block against the real built binary (`target/debug/wyrd`):
- binary built from the REVERTED tree, matching `VERSION`: exit 1,
  `Server value(s): [], VERSION version: [0.0.0+git.6032ff5]` plus the head;
- binary built from the fixed tree, matching `VERSION`: exit 0,
  `Server: wyrd/0.0.0+git.6032ff5 matches VERSION version: 0.0.0+git.6032ff5`;
- fixed binary, `VERSION` says `0.0.0+git.6032ff6`: exit 1, both values printed;
- fixed binary, `VERSION` with no `version:` line: exit 1.

`shellcheck -s sh` and `-s bash` on the expanded block: clean. The pin agrees:
`cargo test -p xtask --test dist_two_binary_layout --test dist_templates` → 31 + 2 passed.

Not added as a shipped test: the brief allows exactly ONE added `*/tests/*.rs`
(`brief.md:156-167`), and a durable shell test would have to extract and un-escape the
in-container script from YAML inside `dist_two_binary_layout.rs` (roughly 60-80 lines of
extraction plus a dependency on `dash` being on the CI runner). The exact-text pin already
forces any later edit to the comparison to be made in two places on purpose.

## Unchanged from v1 (for the reader's convenience)

1. `S3Config.server_version` — `crates/gateway-s3/src/lib.rs:116`, defaulted in
   `S3Config::new` to `UNKNOWN_SERVER_VERSION` (`:121`, `:181`; base struct `:101`, `new`
   body `:115-120`). Only `new` constructs the struct, so every caller keeps compiling.
2. One spelling of `wyrd/` — `SERVER_PRODUCT` (`:124`) and `server_header_value` (`:131`).
   A version that is not an RFC 9110 `token` (`is_token`, `:149`) is replaced by `unknown`
   with a `warn!`, so the header stays present and grammatical. Every byte #778's
   `validate_identity` admits (`derivation.rs:93-95`) is a `tchar`, so a real build never
   hits the fallback.
3. Rendered once in `router()` (`:265`), held in `AppState.server` (`:202`), cloned in
   `handle` (`:1543`), inserted at the single stamp point beside `x-amz-request-id`
   (`:1632`; base `:1552-1556`). Every request reaches `handle` through
   `Router::new().fallback(handle::<G>)` (`:273`; base `:205`).
4. Composition root — `crates/server/src/cli.rs:2660` (base `serve_s3` at `:2655-2656`),
   from `crate::version::BUILD_IDENTITY`. No threading through `serve_s3_role` /
   `serve_s3_dispatch`.
5. Unit test of the rendering/fallback — `crates/gateway-s3/src/lib.rs:3527` (a `#[cfg(test)]`
   unit test in `src/`, not a `*/tests/*.rs` file).

## The binding test — `crates/server/tests/s3_server_version_header.rs` (unchanged)

Spawns `env!("CARGO_BIN_EXE_wyrd") s3 --s3-listen 127.0.0.1:0 --data-dir <tmp>
--access-key … --secret-key … --log-format json` under a `RoleGuard` that kills and reaps on
drop (`:52`). `start_role` (`:72`) reads stderr on a thread, bounded by a 60 s deadline,
until it has both the `serving S3-compatible HTTP on <addr>` line (port observed, not
guessed) and the `role started` JSON event's `fields.version` (V). Requests use blocking
sockets with 30 s timeouts. Against that one child:

1. signed `PUT /wyrd-bucket/server-version-object` → `200`, `Server` == `wyrd/`+V (`:301`);
2. unsigned `GET` of the same key → `403`, `Server` == `wyrd/`+V (`:310`);
3. signed `GET` of the same key (streamed body) → `200`, `Server` == `wyrd/`+V (`:322`);
4. signed `GET /wyrd-bucket?acl`, signed WITH query `acl` → `501`, `Server` == `wyrd/`+V
   (`:336`).

Leg 5 is applied inside each: exactly one `Server` header, byte-equal to `wyrd/` + the V the
same process logged (`assert_server`, `:269`). The test names no symbol this patch adds; it
uses only `sigv4::{sign, format_amz_date, Credentials}`, which exist on base.

**What leg 4 does NOT claim.** A genuine `500 InternalError` (`gateway_error_response`
through `classify`'s `InternalError` arm) is not driven. It is not deterministically
producible from a black-box test without injecting a backend fault, and that is out of
scope. `501` is the deterministic representative of the server-error class. What carries the
rest of the category is structural: every response `dispatch` returns, `500` included, flows
through `handle`'s one stamp point (`crates/gateway-s3/src/lib.rs:1632`). The `500` path was
not exercised.

## Red → green, this iteration

Command: `cargo test -p wyrd-server --test s3_server_version_header` (the brief's named
command), wrapped in `timeout`.

- Fixed tree: `1 passed`.
- `crates/gateway-s3/src/lib.rs` and `crates/server/src/cli.rs` checked out to base, test
  kept: compiled, ran 1 test, FAILED at leg 1 (`:271` inside `assert_server`) with
  `left: []` vs `right: ["wyrd/0.0.0+git.6032ff5"]`; the printed head carries
  `content-length`, `etag`, `x-amz-request-id`, `connection`, `date` and no `server`. rc 101.
- Restored, re-built: green again (the rebuilt binary is the one the shell checks above ran).

## Self-refutation

- **(a) Genuine red?** Yes. Reverted both production files, kept the test, re-ran: it
  compiled, ran one test, and failed on an absent header (above). In v1 a partial revert
  (stamp kept, `cli.rs` line removed) also went red on `wyrd/unknown` vs the logged value,
  so leg 5 binds the composition-root value, not just header presence. For the shell fix,
  the v1 condition passes five malformed inputs that the new one fails (table above).
- **(b) Production path?** Yes. The test spawns the real built `wyrd`; requests go through
  the real `serve_s3` composition root, the real axum router, `handle`, and the real SigV4
  verifier, signed with the production `sigv4::sign`. The shell checks ran the exact text
  extracted from `release.yml` (not a retyped copy) against the real binary.
- **(c) Fixture includes the fault?** Yes. The fault is "a response category lacks the
  header"; the test covers all four categories the invariant names, including the two error
  planes (pre-auth 403, post-auth 501) a per-handler stamp would miss, and compares with a
  value read from the same process's log. For the shell, the fixtures include the duplicate
  and whitespace values the reviewer named. Stated limit: the real `500` is not driven.

## Gates run

- `cargo xtask ci` via `./engine/xtask.sh ci` (the project's runner), run 1: typos, docs
  lint/render, repo guards, fmt and clippy passed; the workspace test step failed one
  unrelated unit test, `tests::a_bodyless_response_is_recorded_complete_not_aborted`
  (`crates/gateway-s3/src/lib.rs:4313`, empty log capture). v1 hit the same test and called
  it a flake without proof, so I measured it (next section).
- Run 2, same tree: `xtask ci: all checks passed`, exit 0. It ran typos, docs, the repo
  guards, `cargo fmt --all -- --check`, workspace clippy/build/test (including
  `every_response_category_names_the_logged_build_identity`,
  `server_header_value_renders_…`, `dist_two_binary_layout`, and this time
  `a_bodyless_response…` passed), cargo-machete, the three cargo-deny runs, conformance,
  statics, deploy-guard and the madsim DST tier.
- The target has no commit hooks (no `core.hooksPath`, no pre-commit config, no active hooks
  under `.git/hooks`), so its formatter and linters are the ones `ci` runs above.
- Classifier dry run (`./engine/scripts/run-verify.sh --classify patch.diff`):
  `ADDED_TEST crates/server/tests/s3_server_version_header.rs`, `CRATE crates/gateway-s3`,
  `CRATE crates/server`, `CRATE xtask`. One discriminator; `xtask` appears because its
  existing test file is modified, not added.
- `git apply --check patch.diff` on a clean export of base `6032ff5`: applies.

### The `wyrd-gateway-s3` log-capture flake predates this patch

Same test binary CI builds (`cargo test --workspace --exclude wyrd-dst`), run directly:

| tree | alone, 60 runs | one core (`taskset -c 0`), 40 runs | 8 copies at once, 200 runs |
|---|---|---|---|
| patched | 0 failed | 0 failed | 8 failed |
| base (`lib.rs`, `cli.rs` reverted, rebuilt; my unit test absent from `--list`) | — | — | 8 failed |

Failing tests under contention — patched: `a_bodyless_response_is_recorded_complete_not_aborted`
×2, `the_access_line_carries_the_request_id_under_a_target_scoped_filter` ×4,
`the_access_row_reports_the_observed_transfer_not_the_status_line` ×1,
`the_id_handed_to_the_client_selects_a_server_side_row_even_when_refused` ×1. Base:
`a_bodyless_response…` ×3, `the_access_line_carries…` ×5. All install a thread-local
`tracing` subscriber and assert on captured output; under contention one of them sees an
empty capture. That is the same class as the closed #272 ("scrub durability-metric tests race
tracing's global callsite cache"). Same rate on both trees (8/200), and the failing test
drives `finish_response` directly, which this patch does not touch.

**Suggested for the human (out of scope here, not filed):** open a "Flaky test:" issue for
the `wyrd-gateway-s3` log-capture tests, citing the numbers above. No issue tracks it today
(searched `getwyrd/wyrd` for the test names and for "flaky … tracing": only the closed #214,
#272, #414 come up, all in other crates).

## Deferred, declared in the brief: equality with the tarball's `VERSION`

The smoke-step comparison is **written and reviewable but UNOBSERVED** against a real
release. It runs only in `.github/workflows/release.yml`, which triggers on `v*` tags (no
`v*` tag has ever been cut) and on `workflow_dispatch`. I did not attempt an image build,
`cargo xtask dist`, or a dispatch run. Two ways to observe it without a tag, for the human to
choose at sign-off, both needing Docker and a network:
- `gh workflow run release.yml --repo getwyrd/wyrd --ref <branch>` and read the "smoke the
  installer in a bookworm container" step: it should print
  `Server: wyrd/<v> matches VERSION version: <v>`;
- locally `cargo xtask dist`, then compare the `version:` line of
  `target/dist/wyrd-*/VERSION` with the `Server:` value of a spawned role.

The shell checks above prove the comparison's logic on the real binary; they are not that
release run.

## Considered and ruled out

- **`case "$wire" in "wyrd/$shipped") …`** instead of `[`: needs quotes anyway (an unquoted
  pattern is a glob, so `$shipped` with `*` or `?` would match too much), so it is no
  simpler than quoted `[`, and readers know `[` better.
- **Counting lines first (`grep -ci '^server:'` must be 1), then an unquoted compare**: two
  checks where one does it, and the unquoted compare still splits a spaced value. Capturing
  every `Server` line into one quoted operand covers duplicates and spacing in one test.
- **Keeping the `x` prefix** (`"x$wire" = "xwyrd/$shipped"`): with quoted operands POSIX `[`
  decides by argument count (3 args, `=` in the middle is a binary test), confirmed by the
  `!`, `(`, `-n`, `=` cases on dash. shellcheck flags it (SC2268), so dropped.
- **Avoiding `"` entirely** to honour the old "keep `"` out" comment: every quote-free form
  either splits (`[ $x … ]`) or globs (`case`). Escaped `\"` is the standard way to put a
  quote inside a double-quoted word, and the comment now says so.
- From v1, still ruled out: per-handler stamping (misses the pre-auth 403 by construction);
  a tower `SetResponseHeader` layer (same coverage, one more layer for a one-line insert
  beside an existing stamp); threading the version through `serve_s3_role` /
  `serve_s3_dispatch` (unneeded, and it would collide with #738); defaulting to
  `CARGO_PKG_VERSION` (claims a version the caller never set).
