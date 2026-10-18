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
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 13.14s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Review #779: advertise the binary's build identity as `Server: wyrd/<version>` on S3 gateway responses; the binding red→green criterion is independently reproduced.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The criterion distinguishes four response categories, exact identity equality, and the explicitly deferred packaging observation; the test asserts those categories against one real child (`brief.md:30`; `target/crates/server/tests/s3_server_version_header.rs:280`). |
| C2 Reproduction (red pre-fix) | PASS | Stashing the two production files while retaining the new test reproduced a successful PUT with no Server header and an assertion failure, exit 101 (`reviewer-red-green.log:276`; `target/crates/server/tests/s3_server_version_header.rs:271`). |
| C3 Change | PASS | The supplied identity reaches the common response boundary, so individual handlers cannot omit it; existing configuration callers retain a valid default (`target/crates/server/src/cli.rs:2660`; `target/crates/gateway-s3/src/lib.rs:181`; `target/crates/gateway-s3/src/lib.rs:1632`). |
| C4 Verification (red→green) | PASS | Restoring the patch made the real-binary test pass all four categories and exact log/header equality; full CI has frozen passing evidence, with independent CI and coverage limitations detailed below (`reviewer-red-green.log:323`; `gate-logs/C4-ci.log:4063`). |
| C5 Causal adequacy | PASS | The shared stamp removes the missing-header cause across dispatch outcomes; construction-time validation accepts arbitrary library configuration and is not a capability probe masking an eager side effect (`target/crates/gateway-s3/src/lib.rs:131`; `target/crates/gateway-s3/src/lib.rs:265`; `target/crates/gateway-s3/src/lib.rs:1632`). |
| T1 Structure | PASS | Build identity remains owned by the composition root, without introducing a gateway-to-server dependency or changing backend seams (`target/crates/server/src/cli.rs:2660`; `target/crates/gateway-s3/src/lib.rs:116`). |
| T2 Shape | PASS | Token validation preserves header grammar, the release comparison rejects duplicate/malformed values, and the living threat model accurately explains disclosure (`target/crates/gateway-s3/src/lib.rs:149`; `target/.github/workflows/release.yml:127`; `target/docs/design/architecture/14-threat-model.md:89`; `reviewer-shell.log:41`). |
| T3 Runtime | PASS | Header rendering occurs once at router construction; the exercised test uses an observed ephemeral address, bounded startup/socket waits, and child kill/reap on drop (`target/crates/gateway-s3/src/lib.rs:265`; `target/crates/server/tests/s3_server_version_header.rs:45`; `target/crates/server/tests/s3_server_version_header.rs:54`; `reviewer-red-green.log:325`). |
| T4 Contribution | N/A | PR/commit artifacts are intentionally absent at Check; the substantive contribution audit must rerun at publish, while affected-path prior art was independently checked (`gate-logs/T4-contribution.log:10`; `reviewer-prior-art.log:1`). |
| T5 Judgment | PASS | The prior quoting and disclosure-text concerns are resolved; updating the existing workflow text fixture is required by its pin and introduces no separate feature scope (`target/.github/workflows/release.yml:61`; `target/.github/workflows/release.yml:127`; `target/docs/design/architecture/14-threat-model.md:89`; `reviewer-shell.log:41`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether the native evidence suffices for client version-keying now and accept release-time observation, or require an out-of-band distribution run — Docker packaging and the installed tarball's VERSION equality were not exercised (`brief.md:192`; `target/.github/workflows/release.yml:112`). |

No builder-fixable defect was found. The remaining human decision concerns fitness and the declared release-observation boundary, not a failed implementation criterion.

- **Production behavior is demonstrated.** `cargo test --offline -p wyrd-server --test s3_server_version_header -- --nocapture` failed for an absent header with production reverted, then passed after `git stash pop` restored it (`reviewer-red-green.log:276`, `reviewer-red-green.log:323`). The 501 request is signed with its `acl` query and explicitly asserts status 501 (`target/crates/server/tests/s3_server_version_header.rs:332`). A genuine backend-generated 500 was not injected; its header follows from the common stamp. Hyper-generated failures before service entry remain outside the invariant (`target/crates/gateway-s3/src/lib.rs:1627`). Final reverse-apply and whitespace checks passed; the target patch was restored and no stash remains.
- **Independent checks support the change, with explicit limits.** The `cargo xtask ci` rerun passed spelling, docs lint/render, repository guards, formatting, clippy, build, workspace tests, and dependency-use scanning; it then stopped because the sandbox could not exclusively lock the read-only advisory database (`reviewer-ci.log:3255`). This is a host caveat. The frozen log actually shows successful advisory checks, conformance, statics, deployment checks, and DST through the final CI pass (`gate-logs/C4-ci.log:3458`, `gate-logs/C4-ci.log:4063`). The feature-specific TiKV/server clippy log records successful compilation, not a live-service integration exercise (`gate-logs/host-tikv.log:2`, `gate-logs/host-tikv.log:211`). The frozen batch-review evidence records zero blocking findings (`gate-logs/T4-batch-review.log:10`).
- **Coverage and mutation results are not overstated.** The frozen coverage checkout lacked prerequisite #778's `crate::version`; its compile error establishes no patch defect (`gate-logs/C4-diff-cov.log:31`). In this complete target, an independent instrumented run compiled and passed, but reported zero CLI production hits, including line 2660; no useful diff-coverage percentage is established (`reviewer-coverage.log:273`, `reviewer-coverage.log:280`). Killing the child before a graceful exit is consistent with missing profile flushing (`target/crates/server/tests/s3_server_version_header.rs:56`). Independent mutation testing found 10 mutants: 3 caught and 7 unviable, with no survivors (`reviewer-mutants.log:5`); denied unused-code warnings account for additional unviable cases. The frozen scanner reports 8 caught and 2 unviable (`gate-logs/C5-mutants.log:13`), which was not identically reproduced.
- **Prior art and the previous implementation concerns were checked directly.** Main-history queries covered all six affected paths, and changed-file lists for all 374 closed PRs included 19 unmerged/rejected PRs; truncated file lists were expanded. The 61 path matches were merged work, with no equivalent rejected header change found (`reviewer-prior-art.log:8`). The release shell's actual extracted comparison passed eight cases, refusing duplicate, missing, mismatched, whitespace-bearing, and empty-version inputs (`reviewer-shell.log:4`). These are comparison checks, not evidence of an installed release. The corrected disclosure text agrees with the existing release/local identity contract (`target/crates/server/src/version.rs:8`; `target/docs/design/architecture/14-threat-model.md:89`).

For the remaining human decision, accept the declared release-time observation or run the branch's `release.yml` via `workflow_dispatch` on a Docker/network-capable runner. Inspect the “smoke the installer in a bookworm container” step: it must receive 403 from the installed binary and report `Server: wyrd/<V> matches VERSION version: <V>`; an unreachable role, missing/duplicate header, or mismatch must fail the step (`target/.github/workflows/release.yml:24`; `target/.github/workflows/release.yml:119`; `target/.github/workflows/release.yml:124`). No distribution build or workflow dispatch was performed in this review.

<!-- pdca:leaf-complete -->

### Advisory — adversary

# Adversarial review — issue 779 (`Server: wyrd/<v>` on the S3 front door)

**Verdict: I could not refute the fix.** I re-ran the red→green myself, wrote four mutants of partial fixes, sent edge-case requests to the real binary, and ran the release-step comparison against crafted inputs. Everything held. The two NEEDS-HUMAN items below are about the gate evidence (diff coverage, batch review), not about the patch.

## Refutation attempts that failed (the fix holds)

- **The red→green proof is real and goes through the production path.** I copied the patched tree to scratch and ran `cargo test -p wyrd-server --test s3_server_version_header`. Green: 1 passed. With only `crates/gateway-s3/src/lib.rs` and `crates/server/src/cli.rs` reverted (test kept), it goes red at `crates/server/tests/s3_server_version_header.rs:271`: `left: []`, `right: ["wyrd/0.0.0+git.dea41c9"]`. The PUT head has `x-amz-request-id` and no `server`. The test drives `env!("CARGO_BIN_EXE_wyrd")` (`s3_server_version_header.rs:37`) over raw TCP, so it runs the real composition root and the real router. It does not re-implement either. It names no symbol the patch adds, so it compiles on the reverted tree. This matches `gate-logs/C4-verify.log`.
- **Each leg can fail on its own, not just leg 1.** The red run stops at leg 1, so on its face it only proves leg 1. I edited the stamp at `crates/gateway-s3/src/lib.rs:1632` and the setter at `crates/server/src/cli.rs:2660` into four partial fixes. Every one fails the test at the expected leg:
  - stamp only on 2xx → fails leg 2 (unsigned 403);
  - stamp only below 500 → fails leg 4 (signed `?acl` 501);
  - skip GET 200 → fails leg 3 (streamed GET);
  - composition root never sets the version → fails leg 5 with `left: ["wyrd/unknown"]`.

  A per-handler fix, a fix that skips errors, and a placeholder value all go red.
- **No response the service produces escapes the stamp.** The router is `Router::new().fallback(handle::<G>).with_state(state)` with no tower layers (`lib.rs:273`), so no layer can answer on its own. The only production caller of `S3Config::new` is `cli.rs:2655`; every other caller is a test. Against the built binary, all of these returned exactly one `server: wyrd/0.0.0+git.dea41c9`: HEAD, PATCH (unknown verb), `GET /`, HTTP/1.0, `Expect: 100-continue`, a 200 KB header, and a `%ff%fe` path. Only hyper's own answers lacked it: 400 for `content-length: abc`, 400 for a garbage request line, and 431 for a 1 MB header or 200 headers. That is exactly the limit stated at `lib.rs:1627-1631` and `docs/design/architecture/14-threat-model.md:89`, so the doc prose is honest.
- **The `wyrd/unknown` fallback cannot fire silently in production.** `is_token` (`lib.rs:149`) accepts every byte that `validate_identity` allows, `[A-Za-z0-9_.+-]` (`crates/server/src/version/derivation.rs:93-95`), and those are all RFC 9110 `tchar`. So `BUILD_IDENTITY` always renders as given. Only a library caller that never sets the field gets `wyrd/unknown`, which is intended.
- **The release smoke step (`.github/workflows/release.yml:105-131`) now fails closed.** I expanded the step's double-quoted word the way the runner's bash does. The container gets the intended script: `"$wire"` and `"wyrd/$shipped"` are quoted, `'\r'` is intact, and the idempotence and uninstall lines after it are still present. So no stray quote ends the word early. I then ran the comparison under `dash -eu` against 10 crafted head/`VERSION` pairs. Only the two exact single matches passed: one with `server: ` and one with `server:` and no space. A mismatch, a duplicate `Server`, a missing header, an empty `version:`, a trailing space, a 200 status, CRLF in `VERSION`, and two `version:` lines all exit 1. The unquoted-operand false pass from the last round is gone. `--data-dir /tmp/vsmoke` is created on start (`cli.rs:1915`). The xtask pin rebuilds the whole step, including the new block (`xtask/tests/dist_two_binary_layout.rs:111`), and it passed in `C4-ci`. The step has still never run end to end, because that needs Docker and a `v*` tag. That is the brief's pre-declared deferral, already held for the human, so I am not raising it again.

## Findings

- NEEDS-HUMAN [human] — **The `C4-diff-cov` row gives the wrong reason.** `check-gates.json` says the target "executed 0 tests" and the log guesses at a compiled-out cfg. The real cause is in `gate-logs/C4-diff-cov.log`: `error[E0433]: cannot find 'version' in 'crate'` at `crates/server/src/cli.rs:2657`, compiled in `/home/eddie/wyrd/wyrd-cov`. That coverage checkout is on a base without #778. The module exists on the real base (`crates/server/src/version.rs:48`), and the patch builds there (C4-ci, C4-verify, and my own run). So this is a stale harness worktree, not a patch defect. Diff coverage has not been measured for this round. Refresh the coverage checkout onto the bundle base if you want a coverage figure.
- NEEDS-HUMAN [human] — **The `T4-batch-review` pass looks thin. Check it before counting it as the rubric's "one deep, multi-pass review".** `gate-logs/T4-batch-review.log` shows "3x codex" finishing in 26.8 s with `0 blocking, 0 recorded-rejected, 0 noise-dropped`. Zero findings of any kind, not even dropped noise, from three passes over a ~650-line diff fits an empty or cached result just as well as a clean one. I can't open `results/issue_779/review-batch.md` from here. Confirm it holds three real passes over this diff.

## Minor note (not NEEDS-HUMAN; no rebuild warranted)

- `docs/design/architecture/14-threat-model.md:89` says "a hand-built binary names the commit it was built on". Rung 3 breaks that: a build that cannot reach git bakes `0.0.0+git.unknown` (`crates/server/src/version.rs:20-21`). A hand build with `WYRD_VERSION` set also carries that value as given. The release examples also leave out the `0.0.0+git.<sha>` shape, which is what every release carries until the first `v*` tag. The row overstates what leaks, never understates it, so there is no security impact. Fix it in passing if the row is touched again.

<!-- pdca:leaf-complete -->

### Advisory — code-review

No findings on either advisory lens: no introduced correctness bugs or actionable reuse, simplification, or efficiency issues identified in this diff.

Reviewed the cached header construction and common response stamp (`crates/gateway-s3/src/lib.rs:265`, `crates/gateway-s3/src/lib.rs:1632`), composition wiring (`crates/server/src/cli.rs:2660`), child cleanup and four response-category assertions (`crates/server/tests/s3_server_version_header.rs:53`, `crates/server/tests/s3_server_version_header.rs:280`), and release comparison (`.github/workflows/release.yml:127`).

Frozen evidence records passing CI, a real binary-test red→green, and eight caught mutants with two unviable. Independently checked the smoke script's shell syntax and comparison through both shell layers: matching values pass; absent, duplicate, mismatched, and whitespace-bearing values fail. Diff coverage remains unmeasured because its build failed resolving `crate::version`; the target source does expose that module. The Docker release smoke remains unobserved, as already declared in the brief.

<!-- pdca:leaf-complete -->

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [x] Validation — fitness-to-purpose — Decide whether the native evidence suffices for client version-keying now and accept release-time observation, or require an out-of-band distribution run — Docker packaging and the installed tarball's VERSION equality were not exercised (`brief.md:192`; `target/.github/workflows/release.yml:112`).
- [x] **The `C4-diff-cov` row gives the wrong reason.** `check-gates.json` says the target "executed 0 tests" and the log guesses at a compiled-out cfg. The real cause is in `gate-logs/C4-diff-cov.log`: `error[E0433]: cannot find 'version' in 'crate'` at `crates/server/src/cli.rs:2657`, compiled in `/home/eddie/wyrd/wyrd-cov`. That coverage checkout is on a base without #778. The module exists on the real base (`crates/server/src/version.rs:48`), and the patch builds there (C4-ci, C4-verify, and my own run). So this is a stale harness worktree, not a patch defect. Diff coverage has not been measured for this round. Refresh the coverage checkout onto the bundle base if you want a coverage figure.
- [x] **The `T4-batch-review` pass looks thin. Check it before counting it as the rubric's "one deep, multi-pass review".** `gate-logs/T4-batch-review.log` shows "3x codex" finishing in 26.8 s with `0 blocking, 0 recorded-rejected, 0 noise-dropped`. Zero findings of any kind, not even dropped noise, from three passes over a ~650-line diff fits an empty or cached result just as well as a clean one. I can't open `results/issue_779/review-batch.md` from here. Confirm it holds three real passes over this diff.
- [x] C4 diff coverage: changed lines executed by the patch's tests unverifiable — the -p wyrd-server --test s3_server_version_header target executed 0 tests, so diff coverage was not measured
- [x] The `C4-diff-cov` row says the test target "executed 0 tests … compiled out". Its own log (`gate-logs/C4-diff-cov.log`) shows otherwise: `error[E0433]: cannot find 'version' in 'crate'` at `crates/server/src/cli.rs:2657` in `/home/eddie/wyrd/wyrd-cov`. The coverage worktree sits on a base without #778's `crates/server/src/version.rs` (present in the target at `version.rs:46`; the cov tree's line 2657 vs the target's `cli.rs:2660` also shows a different base). So diff coverage was never measured, and the gate's stated reason is wrong. This is a harness/base problem, not a patch defect: the same code compiles in `C4-ci` and in my local re-run. The coverage verdict is provisional until the cov worktree is refreshed onto the bundle base.

## 7. Proven / not proven
- Proven by which oracle: gates overall = pass (stub oracles).
- Unproven / needs manual run: anything flagged in §6.

## 8. Ready-to-ship attachments
- patch.diff
- tracker-comment.md     (ALWAYS, every tracker item)
- build-notes.md         (builder rationale — for the human, not the reviewer)

## 9. Check sign-off                     ← human completes Check here
- Disposition confirmed / overridden:
- Outcome: merged-wider
- Iteration delta (if iterating):
- By / date: Eduard Ralph / 2026-10-10

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
