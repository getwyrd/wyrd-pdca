# Result — issue 736 / s3-server-version-header

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect: The S3 gateway advertises no version. Verified on `main` at `65ca4fd`:
  `grep -rn "header::SERVER\|\"server\"" crates/gateway-s3/src/` returns nothing — no
  `Server:` header, and no build identifier anywhere in the response path. The only header
  the gateway stamps on every response is `x-amz-request-id`
  (`crates/gateway-s3/src/lib.rs:1552-1557`). Nothing on the wire says what a client is
  talking to, so a client cannot tell last month's deployment from today's, a captured HTTP
  exchange does not identify the build that produced it, and `wyrd-validate`'s
  version-keyed capability matrix (proposal 0017 §3) has nothing to key on — its interim
  `--server-version` is an operator-supplied parameter that can silently be wrong.
- Success criterion: BINDING (demonstrable by C4-verify at Check, no container, no
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
- Repo + branch target: getwyrd/wyrd @ main
- Scope: (a) `S3Config` gains a server-identity field, defaulted in `S3Config::new` so
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

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: new-feature
- Confidence: medium
- Recommendation: (set by Do)

## 3. Correctness (Check — chain)
- C1 Spec: none — brief.md
- C2 Reproduction (red pre-fix): none — (no gate configured)
- C3 Change: none — patch.diff
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): unverifiable — gate exceeded its 7200s timeout
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass — run-verify.sh: PASS — red without the fix, green with it (1 test(s) ran red).
- C4 diff coverage: changed lines executed by the patch's tests: pass — diff coverage 87.6% — 261 of 298 instrumentable changed lines executed (floor 80%); 298 of 991 changed lines were instru
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): fail — ERROR cargo test failed in an unmutated tree, so no mutants were tested

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 5 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_736/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 13.04s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Reviewing issue #736: make every S3 response and the startup log expose the baked build identity, aligned with the distribution artifact version.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The decision is bounded to success/error response headers, baked/logged identity equality, git provenance, and a separately declared release-only packaging check, with no external dependency for the binding criterion (`brief.md:21`, `brief.md:24`, `brief.md:56`, `brief.md:152`). |
| C2 Reproduction (red pre-fix) | PASS | Independent stash/reapply execution reproduced a pre-fix signed 200 without `Server` and a post-fix green test; the frozen rerun records the same behavioral red rather than a compile failure (`gate-logs/C4-verify.log:15`, `gate-logs/C4-verify.log:24`, `gate-logs/C4-verify.log:41`). |
| C3 Change | NEEDS-HUMAN | Decide whether `version_origin` belongs in this slice — the brief scopes the startup-event change to the version string, while the patch adds a new public constant and observable log field, expanding the maintained contract (`brief.md:127`, `crates/server/src/lib.rs:38`, `crates/server/src/cli.rs:2205`). |
| C4 Verification (red→green) | NEEDS-HUMAN | Decide whether focused proof suffices pending a clean full-suite rerun — red→green, 87.6% diff coverage, focused tests and clippy pass, but the frozen `cargo xtask ci` was killed at 7200s while unrelated `custodian_gc` tests stalled (`gate-logs/C4-ci.log:7`, `gate-logs/C4-ci.log:1963`, `gate-logs/C4-diff-cov.log:632`). |
| C5 Causal adequacy | NEEDS-HUMAN [impl] | A rebuild must make tag normalization injective — replacing every forbidden byte with `.` lets distinct exact tags collapse to one advertised/artifact identity, defeating the invariant that the value identifies the build (`crates/server/src/version.rs:119`, `crates/server/src/version.rs:130`, `brief.md:88`). |
| T1 Structure | PASS | The architecture choice is coherent: one shared pure resolver feeds the composition root, and one post-dispatch stamp covers the fallback router’s entire response category (`crates/server/src/version.rs:14`, `crates/server/src/cli.rs:2386`, `crates/gateway-s3/src/lib.rs:274`, `crates/gateway-s3/src/lib.rs:1646`). |
| T2 Shape | FAIL | Leading/trailing whitespace is silently repaired before validation in both the gateway and explicit override path, so the wire/build may advertise a value different from the configured value despite the documented verbatim-or-reject/degrade contract (`crates/gateway-s3/src/lib.rs:135`, `crates/server/src/version.rs:214`, `crates/server/src/version.rs:227`). |
| T3 Runtime | PASS | The built-binary test exercised one child through signed success, unsigned 403, header/log equality, and git-SHA binding; the independent green rerun and focused 80/50/16-test suites completed successfully (`crates/server/tests/s3_server_version_header.rs:321`, `crates/server/tests/s3_server_version_header.rs:381`, `crates/server/tests/s3_server_version_header.rs:440`). |
| T4 Contribution | N/A | Contribution artifacts are absent by design and their substantive audit reruns at publish; the affected-path merged/closed-PR scan found no competing version-header work (the only closed-unmerged overlap was unrelated PR #647) (`gate-logs/T4-contribution.log:10`). |
| T5 Judgment | NEEDS-HUMAN [impl] | A rebuild must add regressions for whitespace and normalization collisions — current tests cover interior malformed bytes and advertisability, but omit the cases that expose both surviving defects (`crates/gateway-s3/src/lib.rs:5547`, `crates/server/src/version.rs:280`, `crates/server/src/version.rs:344`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether release fitness is acceptable before any `v*` tag has executed the installed tarball-versus-wire smoke — pure coupling checks cannot demonstrate that end-to-end artifact equality (`brief.md:196`, `brief.md:201`, `.github/workflows/release.yml:79`). |

### Advisory — adversary

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

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C3 Change — Decide whether `version_origin` belongs in this slice — the brief scopes the startup-event change to the version string, while the patch adds a new public constant and observable log field, expanding the maintained contract (`brief.md:127`, `crates/server/src/lib.rs:38`, `crates/server/src/cli.rs:2205`).
- [ ] C4 Verification (red→green) — Decide whether focused proof suffices pending a clean full-suite rerun — red→green, 87.6% diff coverage, focused tests and clippy pass, but the frozen `cargo xtask ci` was killed at 7200s while unrelated `custodian_gc` tests stalled (`gate-logs/C4-ci.log:7`, `gate-logs/C4-ci.log:1963`, `gate-logs/C4-diff-cov.log:632`).
- [ ] C5 Causal adequacy — A rebuild must make tag normalization injective — replacing every forbidden byte with `.` lets distinct exact tags collapse to one advertised/artifact identity, defeating the invariant that the value identifies the build (`crates/server/src/version.rs:119`, `crates/server/src/version.rs:130`, `brief.md:88`).
- [ ] T5 Judgment — A rebuild must add regressions for whitespace and normalization collisions — current tests cover interior malformed bytes and advertisability, but omit the cases that expose both surviving defects (`crates/gateway-s3/src/lib.rs:5547`, `crates/server/src/version.rs:280`, `crates/server/src/version.rs:344`).
- [ ] Validation — fitness-to-purpose — Decide whether release fitness is acceptable before any `v*` tag has executed the installed tarball-versus-wire smoke — pure coupling checks cannot demonstrate that end-to-end artifact equality (`brief.md:196`, `brief.md:201`, `.github/workflows/release.yml:79`).
- [ ] C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance) unverifiable — gate exceeded its 7200s timeout
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 5 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_736/review-b
- [ ] The binding wire test can go green without proving the stated build identity. It only requires a non-empty value other than `0.0.0` (`brief.md:24-32`), while the design deliberately makes `S3Config::new` default to `wyrd/unknown` (`brief.md:215-217`). The cited fixture constructs `S3Config::new` and `S3Gateway::new` directly (`crates/server/tests/s3_http_wire.rs:78-89`), bypassing the production assignment in `serve_s3` (`crates/server/src/cli.rs:2377-2386`). Therefore `Server: wyrd/unknown` satisfies both test legs even if the build script and composition-root plumbing are absent; revise the criterion/fixture so the red→green gate exercises or independently proves the baked identity and rejects `unknown`.
- [ ] The brief defers a tracker definition-of-done claim without naming any mechanism that can verify it. The tracker requires that the wire version “matches what `cargo xtask dist` stamps into the tarball's `VERSION` file,” but the brief says this equality is “confirmed by `.github/workflows/release.yml`” (`brief.md:122-141`) and assumes that workflow needs no change (`brief.md:355-357`). The target workflow only builds the artifacts, installs the tarball, invokes `wyrd` with no arguments, and checks usage/uninstall behavior; it never starts S3, reads `Server`, or compares it with `VERSION` (`.github/workflows/release.yml:53-88`). Container-free build-argument coupling is not end-to-end equality, so the claimed outcome remains unfalsifiable unless the brief adds a concrete release check or narrows the promise.
- [ ] The tracker also makes “Startup log records the same string” part of its definition of done (`notes.json`, `Definition of done`), and the brief adds that log change to scope (`brief.md:90-97`), but its binding success criterion observes only two HTTP responses (`brief.md:24-32`) and its verification posture names no startup-log assertion (`brief.md:130-141`). The existing production event is the `role started` record at `crates/server/src/cli.rs:2199-2206`; require a deterministic assertion that this event carries the same baked value, or remove that independently observable deliverable from this slice.
- [ ] size backstop — this slice is behaving oversized: 2 round(s) already spent (threshold 2). Recommend answering `iterate-plan` at sign-off and authoring the split in the re-plan (`pdca split`), rather than `iterate-do`: a slice that is too big yields implementation-shaped findings every round, and splitting authors briefs, which is Plan's beat.
- [ ] leaf produced no usable verdict (needs a human) — advisory leaf 'adversary' did not produce findings (produced no artifact); re-run it or adjudicate by hand.
- [ ] **C5 cannot go green for this bundle, and auto-iterating on it will
- [ ] **The brief's Design decision #2 is reversed without the brief's

## 7. Proven / not proven
- Proven by which oracle: gates overall = fail (stub oracles).
- Unproven / needs manual run: anything flagged in §6.

## 8. Ready-to-ship attachments
- patch.diff
- tracker-comment.md     (ALWAYS, every tracker item)
- build-notes.md         (builder rationale — for the human, not the reviewer)

## 9. Check sign-off                     ← human completes Check here
- Disposition confirmed / overridden:
- Outcome: iterated-to-Do
- Iteration delta (if iterating): Rebuild targeting the adversarial review's confirmed implementation defects, in priority order: 1. Most severe: `git describe` is not scoped to the source tree (`crates/server/build.rs:45`, `current_dir(CARGO_MANIFEST_DIR)`), so a checkout nested inside an unrelated git repo bakes and confidently advertises a foreign commit's SHA as the build's own identity — exactly the "can silently be wrong" failure mode this issue exists to remove. Guard with `git ls-files --error-unmatch Cargo.toml` (or compare `--show-toplevel` against the workspace root) and fall back to `0.0.0+git.unknown` / origin `unknown` on failure. 2. Leg 4's provenance skip predicate is self-minted from the artifact under test (`crates/server/tests/s3_server_version_header.rs:423-437` reads `version_origin` from the child's own event and skips the sha assertion whenever it isn't `git`), so a one-token slip in `build.rs` silently disables the only provenance check while staying green. Make `origin == "unknown"` a hard failure (not a skip) when `git rev-parse --short HEAD` independently succeeds from the same directory. 3. Under git's `reftable` ref backend, the build script's rerun-if-changed watch set is incomplete (`crates/server/build.rs:98-127`), so a new commit does not trigger a rebuild and the binary can silently ship a stale baked identity while still claiming `version_origin: git`. Also watch `git rev-parse --git-path reftable` (or the whole common dir when `--show-ref-format` reports `reftable`). 4. The brief's explicit design decision ("a dirty build must advertise as dirty") is reversed in the implementation (`crates/server/build.rs:39-45,87-96` omits `--dirty` from `git describe`) without being ratified at sign-off — either honor the brief's decision or get it explicitly reversed in the brief, not silently in code. 5. The streaming-GET response path, named in the invariant text itself ("success, client error, server error, and the streaming-GET head alike"), is never asserted on the wire — add a signed GET leg alongside the existing PUT/403 legs. 6. C5 causal adequacy / T5: tag normalization is not injective (`crates/server/src/version.rs:119,130` — replacing every forbidden byte with `.` lets distinct exact tags collapse to one identity) and whitespace is silently trimmed before validation in both the gateway and override path (T2 Shape FAIL) — add regressions for both and fix the normalization to fail closed rather than collapse. 7. C3 scope question: decide (and record) whether `version_origin` as a new public constant/observable log field belongs in this slice, or trim it back to the version string only. Explicitly NOT a cause for concern and out of scope for this iteration: the C4-ci `unverifiable` timeout — the same `custodian_gc` binary with this same patch ran clean (10/10 passed in 0.17s) in the same-day C5 baseline run, so the stall reads as host contention/scheduling on the earlier run, not a regression from this patch. A rerun of C4-ci is cheap confirmation but not itself a code change to make.
- By / date: Eduard Ralph / 2026-08-17

## 10. Act candidates (hints for the next Act review)
- Plan advisory: 3 finding(s); brief revised: yes (plan-advisory-*.md)
- (empty is the common case)
