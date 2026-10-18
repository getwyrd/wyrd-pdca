# Result — issue 779 / s3-server-version-header-stamp

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect: The S3 gateway advertises no version on the wire. Verified on `origin/main`
  at `a801997`: `grep -rn "header::SERVER" crates/gateway-s3/src/` returns nothing, and the
  only header `handle` stamps on every response is `x-amz-request-id`
  (`crates/gateway-s3/src/lib.rs:1552-1556`). Real S3 sends `Server: AmazonS3`; sending
  nothing is the unusual choice. A client cannot tell last month's deployment from today's,
  a captured HTTP exchange does not identify the build that produced it, and
  `wyrd-validate`'s version-keyed capability matrix (proposal 0017 §3) has nothing to key
  on — its interim `--server-version` is an operator-supplied parameter that can silently
  be wrong, which for a *validation* tool is the worst kind of input. After #778 the binary
  knows its identity and logs it; nothing puts it on the wire.
- Success criterion: BINDING (demonstrable by C4-verify at Check — no container, no
  cluster, no feature flag): a new integration test spawns the **built `wyrd` binary** as
  an `s3` role (`--s3-listen 127.0.0.1:0 --data-dir <tmp> --access-key … --secret-key …
  --log-format json`, killed on every exit path) and asserts, against that one child
  process:
  1. **the success plane carries it** — a signed `PUT` that succeeds returns a `Server`
     header;
  2. **the error plane carries it** — an UNSIGNED request refused `403` returns a `Server`
     header. This is the load-bearing leg: an error response is the one most likely to end
     up in a bug report, and it is the leg a per-handler implementation would miss;
  3. **the streaming-GET head carries it** — a signed `GET` of the object just written
     returns a `Server` header. Named in the invariant, so asserted rather than assumed;
  4. **the SERVER-ERROR class carries it** — a **signed** `GET /wyrd-bucket?acl` is refused
     `501 NotImplemented` by the subresource denylist (`acl` is listed at
     `crates/gateway-s3/src/lib.rs:342-360`; the refusal is `error_response(...
     NOT_IMPLEMENTED ...)` at `:1655-1661`), and that response returns a `Server` header.
     This leg exists because the invariant names four response CATEGORIES and legs 1-3 only
     exercised three: without it a check can go green while the 5xx category is untested.
     It is deterministic, needs no fault injection, runs post-auth (a different return path
     from leg 2's pre-auth refusal) and does not require the bucket to exist — the denylist
     is consulted before any handler. **Signing gotcha, do not trip on it:** the request
     carries a query, so it must be signed WITH that query — `sigv4::sign`'s third argument
     is the raw query string (`crates/gateway-s3/src/sigv4.rs:619-629`; `sign` applies
     `canonical_query` itself, and `canonical_query("acl") == "acl="`, `:868`). The existing
     `signed_headers` helper hardcodes `""` there (`s3_http_wire.rs:94-105`), so copy its
     shape with the query threaded through rather than reusing it as-is; a request signed
     over an empty query is refused `403` and would silently turn this leg into a second
     copy of leg 2;
  5. **the value is the real build identity, not a placeholder** — each of those four
     header values is EXACTLY `wyrd/` + V, where V is the `version` field of the same child
     process's `role started` JSON event (#778's observable, read from the same stderr
     stream). Byte equality against an independently-observed value, not a shape test: this
     is what makes the criterion unfakeable, and it is why this slice depends on #778.
     A `wyrd/unknown`, a hardcoded constant, or a per-handler stamp that misses the 403 all
     fail it.

  **What leg 4 does NOT claim.** A genuine `500 InternalError` (`gateway_error_response`,
  `crates/gateway-s3/src/lib.rs:3324`, through `classify`'s `InternalError` arm at
  `:3380-3395`) is not deterministically
  producible from a black-box test without injecting a backend fault, and inventing one is
  out of scope here. `501` is the deterministic representative of the server-error class;
  what carries the rest of the category is structural and is the point of the invariant —
  every response `dispatch` returns, `500` included, flows through `handle`'s ONE stamp
  point (`:1541-1557`). Say exactly that in `build-notes.md`; do not write prose claiming
  the `500` path was exercised.
- Repo + branch target: getwyrd/wyrd @ main
- Scope: (a) `S3Config` gains a server-identity field, defaulted in `S3Config::new` so
  every existing caller keeps compiling and a library caller that never sets it still emits
  a well-formed header; (b) `handle` stamps `Server: wyrd/<v>` at its single stamp point,
  beside the request id; (c) the composition root supplies the real value — `serve_s3` sets
  the field from `wyrd_server::version::BUILD_IDENTITY`, which #778 put in this same crate,
  so **no parameter threading through `serve_s3_role`/`serve_s3_dispatch` is needed or
  wanted**; (d) `docs/design/architecture/14-threat-model.md` records the disclosure
  decision (see Decision 2); (e) `.github/workflows/release.yml`'s existing installer smoke
  step gains the ONE end-to-end check that closes the tracker's "matches `VERSION`" clause —
  see Verification posture. It is a few lines of shell inside the `docker run` block that
  already installs the tarball; it cannot run at Check and is not part of the binding
  criterion. **/ out of scope:** any derivation, build script, normalizer or packaging
  change — #778 owns the identity and it is already on your base (a patch touching
  `crates/server/build.rs`, `crates/server/src/version.rs`, `xtask/src/dist.rs` or the
  Dockerfile will be rejected); any capability-negotiation protocol or capability list on
  the wire (the issue says so explicitly — one header carrying one string); a configuration
  knob to suppress or coarsen the header (Decision 2 — a follow-up issue if an operator ever
  asks); coarsening the value; the version on any other role's wire surface; a `wyrd
  --version` subcommand.

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: new-feature
- Confidence: medium
- Recommendation: (set by Do)

## 3. Correctness (Check — chain)
- C1 Spec: none — brief.md
- C2 Reproduction (red pre-fix): none — (no gate configured)
- C3 Change: none — patch.diff
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): pass — xtask ci: all checks passed
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass — run-verify.sh: PASS — red without the fix, green with it (1 test(s) ran red).
- C4 diff coverage: changed lines executed by the patch's tests: unverifiable — the -p wyrd-server --test s3_server_version_header target executed 0 tests, so diff coverage was not measured
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): pass — 10 mutants tested in 2m: 8 caught, 2 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): pass — review-branch: 0 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_779/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 13.15s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Issue #779’s S3 build-identity header satisfies the native response contract; packaged-version equality and fitness-to-purpose still require human sign-off.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The contract is falsifiable across success, pre-auth refusal, streaming GET and post-auth 5xx, with transport-generated errors explicitly outside its boundary (`brief.md:29`; `target/crates/server/tests/s3_server_version_header.rs:279`; `target/docs/design/architecture/14-threat-model.md:89`). |
| C2 Reproduction (red pre-fix) | PASS | Independently stashing the tracked fix while retaining the test produced a compiled, successful PUT with no Server header and assertion failure, establishing the missing observable rather than a build failure (`reviewer-red-green.log:19`; `target/crates/server/tests/s3_server_version_header.rs:271`). |
| C3 Change | PASS | Scope and existing release safeguards are preserved; the additional edit to the existing layout test is required by the workflow’s exact-text contract, not a new deliverable (`target/.github/workflows/release.yml:61`; `target/xtask/tests/dist_two_binary_layout.rs:411`; `target/crates/server/src/cli.rs:2660`). |
| C4 Verification (red→green) | NEEDS-HUMAN | Choose release-time observation or an out-of-band distribution smoke before sign-off — Docker packaging was not exercised, so tarball VERSION equality remains unobserved despite the independently confirmed native red→green (`brief.md:192`; `target/.github/workflows/release.yml:107`; `reviewer-red-green.log:65`). |
| C5 Causal adequacy | PASS | The common response exit covers the missing property across handlers, including 500 structurally; no capability probe masks an eager side effect, and the four-category test checks the same child’s independently logged identity (`target/crates/gateway-s3/src/lib.rs:1550`; `target/crates/gateway-s3/src/lib.rs:1632`; `target/crates/server/tests/s3_server_version_header.rs:284`). |
| T1 Structure | PASS | Dependency direction remains intact: build identity belongs to the composition root, while the gateway accepts configuration without importing the server crate (`target/crates/server/src/cli.rs:2660`; `target/crates/gateway-s3/src/lib.rs:116`). |
| T2 Shape | PASS | The new wire value respects product-token grammar, retains a well-formed default for library callers and replaces rather than appends the header; the living threat model records the accepted boundary (`target/crates/gateway-s3/src/lib.rs:131`; `target/crates/gateway-s3/src/lib.rs:1632`; `target/docs/design/architecture/14-threat-model.md:89`). |
| T3 Runtime | PASS | Header work is prepared once and does not consume or buffer the streaming body; the real-process test bounds startup/socket waits and kills and reaps the role on failure (`target/crates/gateway-s3/src/lib.rs:265`; `target/crates/server/tests/s3_server_version_header.rs:54`; `target/crates/server/tests/s3_server_version_header.rs:227`). |
| T4 Contribution | N/A | Contribution artifacts are intentionally drafted after Check and must be substantively audited at publish; this deferred row needs no human clearance, and affected-path prior art was independently checked (`gate-logs/T4-contribution.log:10`; `reviewer-prior-art.md:3`). |
| T5 Judgment | PASS | No builder-fixable defect or scope re-entry was found; 501 is the stated server-error representative, a genuine backend 500 remains a structural argument, and the already accepted disclosure decision is not reopened (`target/crates/server/tests/s3_server_version_header.rs:20`; `target/crates/server/tests/s3_server_version_header.rs:335`; `brief.md:280`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Accept the demonstrated service-response identity as sufficient for version-keyed client expectations and diagnosis — delivery of the identity is established, but its fitness for the intended consumer remains a human judgment (`target/docs/design/proposals/draft/0017-blackbox-validation-tool.md:257`; `target/crates/server/tests/s3_server_version_header.rs:286`). |

No implementation correction is requested. The remaining decisions concern the declared packaging observation and fitness-to-purpose, not a missing native reproduction.

- **Independent behavior evidence:** `cargo test --offline --locked -p wyrd-server --test s3_server_version_header` passed, failed with the tracked patch stashed (exit 101, absent header on HTTP 200), and passed after `git stash pop` (exit 0). The tracked diff was restored byte-for-byte and no stash remains (`reviewer-red-green.log:6`, `reviewer-red-green.log:37`, `reviewer-red-green.log:57`, `reviewer-red-green.log:69`). Green reaches all four status/header assertions, including query-signed `501`, against the same built process; red stops at the first missing-header assertion and does not independently exercise the later red assertions.
- **Repository checks and host caveat:** The independent `cargo xtask ci` run passed typos, docs lint/render/link audit, repository guards, fmt, workspace clippy/build/tests and cargo-machete, including both the new integration test and release-layout pin (`reviewer-ci.log:4`, `reviewer-ci.log:2558`, `reviewer-ci.log:2985`, `reviewer-ci.log:3250`). It then failed solely because `/home/eddie/.cargo/advisory-dbs/db.lock` is read-only in this sandbox (`reviewer-ci.log:3256`). That is a host limitation, not a patch defect or a fully green reviewer CI run. The frozen evidence shows the three dependency audits passing, conformance and statics/deployment guards passing, and DST finishing successfully (`gate-logs/C4-ci.log:3458`, `gate-logs/C4-ci.log:3466`, `gate-logs/C4-ci.log:4063`).
- **Other frozen gate evidence:** The coverage row measured nothing: its compile failed because that checkout lacked `crate::version`, the #778 prerequisite (`gate-logs/C4-diff-cov.log:54`). The supplied target includes it (`target/crates/server/src/lib.rs:22`) and independently compiles and runs; this is a stale coverage-checkout caveat, not a C4 patch failure, and no changed-line coverage percentage is claimed. The instance-scoped mutation wrapper’s log reports an unmutated baseline and 10 mutants: 8 caught, 2 unviable (`gate-logs/C5-mutants.log:11`). The batch-review log reports zero blocking findings; it does not expose the individual reviews (`gate-logs/T4-batch-review.log:10`). The host-feature log shows both TiKV and server TiKV/etcd clippy invocations finishing (`gate-logs/host-tikv.log:112`, `gate-logs/host-tikv.log:211`). These are read-log adjudications, not claimed independent reruns of those wrappers.
- **Prior art and release boundary:** Queried merged commit history separately for all six affected paths and compared those paths against all 374 returned closed PRs, expanding the two capped file lists; no closed-unmerged PR touched an affected path (`reviewer-prior-art.md:3`). The workflow still runs on `v*` tags and manual dispatch without a path filter or new feature guard (`target/.github/workflows/release.yml:21`). Its extracted smoke script passes `bash -n` and `shellcheck -s bash`; that proves shell validity, not packaged runtime behavior.

To discharge the packaging decision before a tag, the human can run `gh workflow run release.yml --repo getwyrd/wyrd --ref <reviewed-branch>` and inspect the “smoke the installer in a bookworm container” step. It must observe unsigned HTTP 403 and equality between the identity after the Server header’s `wyrd/` prefix and the unpacked VERSION file’s `version:` value (`target/.github/workflows/release.yml:119`). Otherwise explicitly accept the brief’s release-time observation. No distribution build or workflow dispatch was attempted in this review.

All source citations above refer to files under the supplied disposable `target/`; evidence citations refer to this review directory. The target patch remains restored and unmodified.
<!-- pdca:leaf-complete -->

### Advisory — adversary

# Adversarial review — issue 779 (`Server: wyrd/<v>` on the S3 front door)

Bottom line: I could not break the fix. The red→green reproduces, the test runs the real
binary through the real composition root and router, and the error-plane and 5xx legs each
catch a weakened fix on their own. One real defect is in the new threat-model prose. One
gate result has the wrong stated cause. One shell check has a latent hole that cannot fire today.

## Findings

- NEEDS-HUMAN [impl] — `docs/design/architecture/14-threat-model.md:89` says the identity an anonymous caller learns is "(a commit, never a claim about the working tree)". That is false for the binaries operators actually run. `cargo xtask dist` derives the release version with `git describe --tags --always --dirty` (`xtask/src/dist.rs:542`) and hands it to the build as `WYRD_VERSION`, which is used verbatim, "`.dirty` suffix included when `dist` ran on a dirty tree" (`crates/server/src/version.rs:11`; pinned by `crates/server/src/version/derivation.rs:329-343`, e.g. `0.0.0+git.abc12de.dirty`). Concrete case: a release cut from a dirty tree answers `curl -sSD- http://<gw>/` with `server: wyrd/0.1.0+git.3.abc12de.dirty`, which is a claim about the working tree. A tagged release answers `wyrd/0.1.0`, which names a version, not a commit. Fix: reword the parenthetical to match `crates/server/src/version.rs:26-30` (a hand-built binary names its base commit; a released one carries `dist`'s version, `.dirty` included if cut from a dirty tree). The brief's Decision 2 rests on the same wrong premise ("#778's Decision 1 already removed the dirty-tree half of the disclosure"). I doubt it changes the decision to accept the disclosure, but the recorded rationale should be accurate.

- NEEDS-HUMAN [human] — The `C4-diff-cov` row says the test target "executed 0 tests … compiled out". Its own log (`gate-logs/C4-diff-cov.log`) shows otherwise: `error[E0433]: cannot find 'version' in 'crate'` at `crates/server/src/cli.rs:2657` in `/home/eddie/wyrd/wyrd-cov`. The coverage worktree sits on a base without #778's `crates/server/src/version.rs` (present in the target at `version.rs:46`; the cov tree's line 2657 vs the target's `cli.rs:2660` also shows a different base). So diff coverage was never measured, and the gate's stated reason is wrong. This is a harness/base problem, not a patch defect: the same code compiles in `C4-ci` and in my local re-run. The coverage verdict is provisional until the cov worktree is refreshed onto the bundle base.

- Note, no action required (hardening only) — `.github/workflows/release.yml:122` and its pinned copy at `xtask/tests/dist_two_binary_layout.rs:380` compare with unquoted `[ x$wire != x$shipped ]`. If a response ever carried two `Server` lines, `wire` would hold two words, `[` would error out ("unexpected operator", status 2), and the `if` would treat that as false, so the step passes without comparing. I reproduced this with dash (bookworm's `/bin/sh`). It cannot happen with this patch: the gateway uses `insert` (`crates/gateway-s3/src/lib.rs:1632`) and hyper adds no `Server` of its own. A `case` match or a `grep -ci '^server:'` count of 1 would close it if the step is touched again.

## Refutation attempts that failed

- **Red→green, re-run independently.** In a scratch copy of `$PDCA_TARGET`: with the fix, `cargo test -p wyrd-server --test s3_server_version_header` → 1 passed. With `crates/gateway-s3/src/lib.rs` and `crates/server/src/cli.rs` reverted and the test kept → fails at `crates/server/tests/s3_server_version_header.rs:271` (leg 1) with `left: []`, matching `gate-logs/C4-verify.log`.
- **"Only leg 1 was ever seen red."** True: the single test stops at its first failing assert. So I checked the other legs directly. Stamping only on success fails at leg 2 (unsigned `GET` → 403). Stamping everything except 5xx fails at leg 4 (signed `GET /wyrd-bucket?acl` → 501). The error-plane and server-error legs do real work.
- **Production path vs. a copy.** The test spawns `CARGO_BIN_EXE_wyrd`. The only production `S3Config` built anywhere is `crates/server/src/cli.rs:2655`, and the field is set at `:2660`. The router is `Router::new().fallback(handle::<G>)` (`crates/gateway-s3/src/lib.rs:273`), served by bare `axum::serve` (`:279`) with no outer tower layers (unlike the D server's load-shed stack), so nothing can answer around `handle`'s stamp at `:1632`.
- **Leg 5 tautology?** Both sides read the same constant (`cli.rs:2398` for the log, `:2660` for the wire). So leg 5 proves "wire equals log", not "the identity is correct". That is #778's job and is what the brief asked for. If the composition root forgot to set the field, the wire would say `wyrd/unknown` and leg 5 would fail.
- **Fallback firing on a real build.** `is_token` (`crates/gateway-s3/src/lib.rs:149`) falls back to `wyrd/unknown` for non-token input. But `validate_identity` only admits `[A-Za-z0-9_.+-]` (`crates/server/src/version/derivation.rs:93-103`), all of which are RFC 9110 `tchar`, so a built binary can never hit the fallback.
- **Undocumented header-less responses.** I probed the built binary. Unsigned `GET /` and unsigned `HEAD` both carry `server: wyrd/<v>`. A garbage request line and `content-length: abc` get hyper's bare `400`. A 500 KB header and 200 headers get hyper's bare `431`. None of the hyper responses carry `Server`, exactly as `lib.rs:1627-1631` and the threat-model row say.
- **Release smoke logic.** I extracted the new step and ran it with dash against the built binary. A matching `VERSION` passes. A mismatched `VERSION` exits 1 and prints both values plus the response head. An unreachable role fails after 20 tries with a message and the role log, so it fails loudly as the brief requires. The step text is pinned by `dist_two_binary_layout`, which passed in `C4-ci`. Equality with a real `dist` tarball is still unobserved until a `v*` tag or a `workflow_dispatch` run, as the brief declares.
- **Rubric surfaces.** Grammar strictness: `is_token` matches the RFC 9110 §5.6.2 `tchar` set exactly. Clock: the test's wall-clock read states its owner (SigV4 freshness). Await discipline: the test bounds startup (60 s) and socket I/O (30 s), and kills the child on every exit path via `RoleGuard` (drop order kills before the temp dir is removed). No issues found.

<!-- pdca:leaf-complete -->

### Advisory — code-review

- NEEDS-HUMAN [impl] — `.github/workflows/release.yml:122` (also pinned at `xtask/tests/dist_two_binary_layout.rs:380`): the release version comparison can falsely pass because `$wire` and `$shipped` are unquoted. Duplicate `Server` headers yield newline-separated values, so `[` receives extra operands and errors; inside an `if` condition, `set -e` does not stop execution, and the mismatch branch is skipped. Running the extracted condition under `sh -eu` with wire values `1.2.3` and `2.0.0` separated by a newline, against shipped version `1.2.3`, reproduced an `unexpected operator` diagnostic followed by exit 0. A whitespace-bearing mismatched value also passes. Quote both operands, escaping the quotes for the outer Docker command, update the pinned fixture, and verify malformed/duplicate values fail.

No additional correctness, reuse, simplification, or efficiency findings in this diff.

<!-- pdca:leaf-complete -->

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C4 Verification (red→green) — Choose release-time observation or an out-of-band distribution smoke before sign-off — Docker packaging was not exercised, so tarball VERSION equality remains unobserved despite the independently confirmed native red→green (`brief.md:192`; `target/.github/workflows/release.yml:107`; `reviewer-red-green.log:65`).
- [ ] Validation — fitness-to-purpose — Accept the demonstrated service-response identity as sufficient for version-keyed client expectations and diagnosis — delivery of the identity is established, but its fitness for the intended consumer remains a human judgment (`target/docs/design/proposals/draft/0017-blackbox-validation-tool.md:257`; `target/crates/server/tests/s3_server_version_header.rs:286`).
- [ ] `docs/design/architecture/14-threat-model.md:89` says the identity an anonymous caller learns is "(a commit, never a claim about the working tree)". That is false for the binaries operators actually run. `cargo xtask dist` derives the release version with `git describe --tags --always --dirty` (`xtask/src/dist.rs:542`) and hands it to the build as `WYRD_VERSION`, which is used verbatim, "`.dirty` suffix included when `dist` ran on a dirty tree" (`crates/server/src/version.rs:11`; pinned by `crates/server/src/version/derivation.rs:329-343`, e.g. `0.0.0+git.abc12de.dirty`). Concrete case: a release cut from a dirty tree answers `curl -sSD- http://<gw>/` with `server: wyrd/0.1.0+git.3.abc12de.dirty`, which is a claim about the working tree. A tagged release answers `wyrd/0.1.0`, which names a version, not a commit. Fix: reword the parenthetical to match `crates/server/src/version.rs:26-30` (a hand-built binary names its base commit; a released one carries `dist`'s version, `.dirty` included if cut from a dirty tree). The brief's Decision 2 rests on the same wrong premise ("#778's Decision 1 already removed the dirty-tree half of the disclosure"). I doubt it changes the decision to accept the disclosure, but the recorded rationale should be accurate.
- [ ] The `C4-diff-cov` row says the test target "executed 0 tests … compiled out". Its own log (`gate-logs/C4-diff-cov.log`) shows otherwise: `error[E0433]: cannot find 'version' in 'crate'` at `crates/server/src/cli.rs:2657` in `/home/eddie/wyrd/wyrd-cov`. The coverage worktree sits on a base without #778's `crates/server/src/version.rs` (present in the target at `version.rs:46`; the cov tree's line 2657 vs the target's `cli.rs:2660` also shows a different base). So diff coverage was never measured, and the gate's stated reason is wrong. This is a harness/base problem, not a patch defect: the same code compiles in `C4-ci` and in my local re-run. The coverage verdict is provisional until the cov worktree is refreshed onto the bundle base.
- [ ] `.github/workflows/release.yml:122` (also pinned at `xtask/tests/dist_two_binary_layout.rs:380`): the release version comparison can falsely pass because `$wire` and `$shipped` are unquoted. Duplicate `Server` headers yield newline-separated values, so `[` receives extra operands and errors; inside an `if` condition, `set -e` does not stop execution, and the mismatch branch is skipped. Running the extracted condition under `sh -eu` with wire values `1.2.3` and `2.0.0` separated by a newline, against shipped version `1.2.3`, reproduced an `unexpected operator` diagnostic followed by exit 0. A whitespace-bearing mismatched value also passes. Quote both operands, escaping the quotes for the outer Docker command, update the pinned fixture, and verify malformed/duplicate values fail.
- [ ] C4 diff coverage: changed lines executed by the patch's tests unverifiable — the -p wyrd-server --test s3_server_version_header target executed 0 tests, so diff coverage was not measured

## 7. Proven / not proven
- Proven by which oracle: gates overall = pass (stub oracles).
- Unproven / needs manual run: anything flagged in §6.

## 8. Ready-to-ship attachments
- patch.diff
- tracker-comment.md     (ALWAYS, every tracker item)
- build-notes.md         (builder rationale — for the human, not the reviewer)

## 9. Check sign-off                     ← human completes Check here
- Disposition confirmed / overridden:
- Outcome: iterated-to-Do
- Iteration delta (if iterating): Auto-iterate (round 1): rebuilding for the implementation-level findings — C4 Verification (red→green) — Choose release-time observation or an out-of-band distribution smoke before sign-off — Docker packaging was not exercised, so tarball VERSION equality remains unobserved despite the independently confirmed native red→green (`brief.md:192`; `target/.github/workflows/release.yml:107`; `reviewer-red-green.log:65`).; `docs/design/architecture/14-threat-model.md:89` says the identity an anonymous caller learns is "(a commit, never a claim about the working tree)". That is false for the binaries operators actually run. `cargo xtask dist` derives the release version with `git describe --tags --always --dirty` (`xtask/src/dist.rs:542`) and hands it to the build as `WYRD_VERSION`, which is used verbatim, "`.dirty` suffix included when `dist` ran on a dirty tree" (`crates/server/src/version.rs:11`; pinned by `crates/server/src/version/derivation.rs:329-343`, e.g. `0.0.0+git.abc12de.dirty`). Concrete case: a release cut from a dirty tree answers `curl -sSD- http://<gw>/` with `server: wyrd/0.1.0+git.3.abc12de.dirty`, which is a claim about the working tree. A tagged release answers `wyrd/0.1.0`, which names a version, not a commit. Fix: reword the parenthetical to match `crates/server/src/version.rs:26-30` (a hand-built binary names its base commit; a released one carries `dist`'s version, `.dirty` included if cut from a dirty tree). The brief's Decision 2 rests on the same wrong premise ("#778's Decision 1 already removed the dirty-tree half of the disclosure"). I doubt it changes the decision to accept the disclosure, but the recorded rationale should be accurate.; `.github/workflows/release.yml:122` (also pinned at `xtask/tests/dist_two_binary_layout.rs:380`): the release version comparison can falsely pass because `$wire` and `$shipped` are unquoted. Duplicate `Server` headers yield newline-separated values, so `[` receives extra operands and errors; inside an `if` condition, `set -e` does not stop execution, and the mismatch branch is skipped. Running the extracted condition under `sh -eu` with wire values `1.2.3` and `2.0.0` separated by a newline, against shipped version `1.2.3`, reproduced an `unexpected operator` diagnostic followed by exit 0. A whitespace-bearing mismatched value also passes. Quote both operands, escaping the quotes for the outer Docker command, update the pinned fixture, and verify malformed/duplicate values fail.. Deferred, not addressed here: 1 finding(s) that need human judgment, held in deferred-findings.json for the human at handover — not build work
- By / date: auto-iterate / 2026-10-10

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
