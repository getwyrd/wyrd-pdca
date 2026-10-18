# Result — issue 767 / listing-last-modified-second-granularity

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect: For one and the same object, a listing and a HEAD report different modification
  times, up to 999 ms apart, for every object whose stored stamp is not on a whole second
  (about 999 in 1000). `render_contents` (`crates/gateway-s3/src/lib.rs:1020-1028`) renders
  `<LastModified>` through `iso8601` (`:624-638`), which keeps `epoch_millis % 1_000` as a
  `.mmm` fraction. GET/HEAD render the `Last-Modified` header through `http_date`
  (`:3186-3212`), which drops the milliseconds, as IMF-fixdate requires (RFC 7231 §7.1.1.1).
  Both read the same stored `modified`: the listing at `crates/server/src/lib.rs:739`, and
  GET/ranged GET/HEAD at `:445`, `:531` and `:626`. Example: a stored `1_700_000_000_750` lists
  as `2023-11-14T22:13:20.750Z` and HEADs as `Tue, 14 Nov 2023 22:13:20 GMT`. A sync tool that
  caches a listing's `LastModified` and then revalidates with HEAD sees a mismatch on every
  object, every pass, and cannot tell a real change from a formatting artefact. The header is
  correct; the listing is exposing precision the header cannot represent.
  **What real S3 emits: whole seconds, checked against the live service on 2026-10-14.** The
  issue asks for this to be verified, not assumed, because it decides between a rendering fix
  and a documentation-only answer. Anonymous listings of two public AWS Open Data buckets in two
  regions returned **3,000 of 3,000** `<LastModified>` values ending `.000Z`, under both
  ListObjectsV2 and v1 ListObjects:
  `noaa-goes16` (us-east-1), `?list-type=2&max-keys=1000&prefix=ABI-L1b-RadC/2024/` → 1,000/1,000;
  `noaa-goes16`, v1 `?max-keys=1000&prefix=GLM-L2-LCFA/2024/` → 1,000/1,000;
  `sentinel-cogs` (us-west-2), `?list-type=2&max-keys=1000&prefix=sentinel-s2-l2a-cogs/10/T/ES/2024/`
  → 1,000/1,000. A HEAD of a listed object agreed to the second: the listing said
  `2024-01-08T00:33:48.000Z` for `sentinel-s2-l2a-cogs/10/T/ES/2024/1/S2A_10TES_20240107_0_L2A/AOT.tif`,
  and HEAD said `Last-Modified: Mon, 08 Jan 2024 00:33:48 GMT`. If S3 kept milliseconds, about
  999 in 1,000 of those values would end in something other than `.000`. So S3 is
  second-granular and its listing and header agree, which picks the issue's first branch: render
  `.000Z`. Anyone can re-run the check with
  `curl -s "https://noaa-goes16.s3.amazonaws.com/?list-type=2&max-keys=1000&prefix=ABI-L1b-RadC/2024/"`.
  (The gateway's comment calling `.mmmZ` "the shape S3 uses", `crates/gateway-s3/src/lib.rs:615-617`,
  is about the format; it never said the fraction is non-zero.)
- Success criterion: In a NEW wire-level test over the in-process gateway, take an object
  whose stored `modified` is `1_700_000_000_750` (not on a second boundary):
  (1) ListObjectsV2 **and** v1 ListObjects render its `<LastModified>` as exactly
  `2023-11-14T22:13:20.000Z`;
  (2) HEAD **and** GET carry `Last-Modified: Tue, 14 Nov 2023 22:13:20 GMT`;
  (3) the listing value and the header value, each parsed, name the same instant.
  A second object stored exactly on a second (`1_700_000_000_000`) renders the same on both
  surfaces as it does today. The chosen rule (listings report whole seconds, matching the
  header and S3) is stated in the doc comments of both `iso8601` and `http_date`, and in one
  sentence of the living architecture doc's object-metadata paragraph
  (`docs/design/architecture/05-building-block-view.md:186-200`, where `modified` is described):
  the store keeps `modified` in milliseconds, and every S3 surface reports it at whole seconds.
  That doc line is a merge requirement, because this change alters an API operation's response
  (`AGENTS.md:154-157`, docs currency).
- Repo + branch target: getwyrd/wyrd @ main
- Scope: how the S3 listing renders an object's `<LastModified>`, and the stated semantics
  at both timestamp formatters / out of scope: the stored precision (`modified` stays epoch
  milliseconds per ADR-0047, and #768's ordering rule relies on it); the conditional-request
  comparison, which already truncates to seconds (`crates/gateway-s3/src/lib.rs:2777`, `:2792`);
  ordering across overwrites (#768); the blackbox validation tool (`crates/validate`); any ADR
  edit (ADR-0047 already says `modified` is stored in milliseconds, and that stays true); any
  `docs/` edit beyond the one architecture-doc sentence in the success criterion.

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: likely-fix
- Confidence: medium
- Recommendation: (set by Do)

## 3. Correctness (Check — chain)
- C1 Spec: none — brief.md
- C2 Reproduction (red pre-fix): none — (no gate configured)
- C3 Change: none — patch.diff
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): fail — xtask: `cargo deny --all-features --config deny-all-features.toml check advisories` failed with exit status: 1
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass — run-verify.sh: PASS — red without the fix, green with it (3 test(s) ran red).
- C4 diff coverage: changed lines executed by the patch's tests: pass — diff coverage 100.0% — 29 of 29 instrumentable changed lines executed (floor 80%); 29 of 52 changed lines were instrumen
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): pass — 7 mutants tested in 24s: 7 caught

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 6 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_767/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 13.20s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Issue #767 aligns S3 listing modification times with HEAD/GET at whole-second precision; the production fix reproduces red→green, with two test-harness corrections remaining.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The required client-visible agreement, on-second compatibility, and preservation of stored milliseconds are explicit and falsifiable; `brief.md:39`, `crates/server/tests/s3_last_modified_agreement.rs:62`, `docs/design/architecture/05-building-block-view.md:191`. |
| C2 Reproduction (red pre-fix) | PASS | Independently stashing the production fix while retaining the new test produces the intended assertion failure, `.750Z` versus `.000Z`, with the other two tests passing; `reviewer-red.log:275`, `crates/server/tests/s3_last_modified_agreement.rs:384`. |
| C3 Change | PASS | The response-precision change stays within the approved scope and includes the required living architecture update; stored metadata and conditional-request semantics are untouched; `crates/gateway-s3/src/lib.rs:636`, `docs/design/architecture/05-building-block-view.md:191`. |
| C4 Verification (red→green) | NEEDS-HUMAN | Resolve the outstanding baseline advisory-gate disposition and obtain a complete CI result on a writable runner — independent red→green passed, but frozen CI rejects unchanged Rustls dependencies and local CI stops on a read-only advisory lock; `reviewer-green.log:13`, `gate-logs/C4-ci.log:6472`, `reviewer-ci.log:3264`, `Cargo.lock:3294`. |
| C5 Causal adequacy | PASS | Removing fractional precision addresses the formatter mismatch directly for both listing forms; there is no added capability probe or symptom guard, and both formatters retain matching representability bounds; `crates/gateway-s3/src/lib.rs:633`, `crates/gateway-s3/src/lib.rs:1033`, `crates/gateway-s3/src/lib.rs:3200`. |
| T1 Structure | PASS | Formatting remains at the S3 boundary, with concrete storage wiring confined to the server integration test and no new dependency or production seam; `crates/gateway-s3/src/lib.rs:633`, `crates/server/tests/s3_last_modified_agreement.rs:40`, `crates/server/tests/s3_last_modified_agreement.rs:143`. |
| T2 Shape | PASS | Whole-second wire formatting preserves absent/unrenderable metadata behavior and documents the API contract; formatting, spelling, and docs rendering passed locally; `crates/gateway-s3/src/lib.rs:641`, `crates/gateway-s3/src/lib.rs:1033`, `reviewer-ci.log:4`, `reviewer-ci.log:23`. |
| T3 Runtime | FAIL | The new test helper can wait indefinitely for socket EOF, so a stalled gateway can hang CI despite a complete response; an independent peer remained pending until an external watchdog cancelled it; `crates/server/tests/s3_last_modified_agreement.rs:202`, `crates/server/tests/s3_last_modified_agreement.rs:209`, `reviewer-socket.log:2`, `AGENTS.md:181`. |
| T4 Contribution | NEEDS-HUMAN | Confirm merged and closed/rejected prior art by all affected file paths — the brief records the gateway path and issue-specific PR search, while the supplied one-commit snapshot cannot establish coverage for the new test and architecture path; `brief.md:112`, `reviewer-target.log:2`. |
| T5 Judgment | NEEDS-HUMAN [impl] | Correct the response reader's false-success cases and bound its socket operations — the new wire oracle accepts incomplete/malformed responses and can hang, contrary to the standing rubric; `crates/server/tests/s3_last_modified_agreement.rs:229`, `crates/server/tests/s3_last_modified_agreement.rs:250`, `crates/server/tests/s3_last_modified_agreement.rs:209`, `reviewer-framing.log:1`, `reviewer-socket.log:1`. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Accept whole-second listing semantics for the intended sync-client workflow — the four requested wire surfaces agree in executable tests, while product fitness remains the maintainer's sign-off decision; `crates/server/tests/s3_last_modified_agreement.rs:382`, `docs/design/architecture/05-building-block-view.md:191`. |

The two implementation concerns are confined to the newly added test harness. The six frozen batch-review entries reduce to these two distinct concerns, both independently confirmed; they do not establish six separate defects (`gate-logs/T4-batch-review.log:10`).

1. **Reject incomplete or malformed HTTP responses before asserting timestamps.** At `crates/server/tests/s3_last_modified_agreement.rs:229`, non-chunked bodies bypass `Content-Length` validation. At `:250`, an invalid chunk size becomes zero; `:252` silently stops on insufficient data, and `:256` skips two bytes without checking their CRLF. A standalone Rust probe using the exact target helpers accepted a missing terminal chunk, an invalid terminal size, a missing terminal CRLF, and an invalid data terminator, preserving the expected timestamp in every case (`reviewer-framing.log:1`). A loopback peer declaring 1,132 bytes but returning only 132 also passed the timestamp assertion (`reviewer-socket.log:1`). Validate response framing, including bodyless HEAD semantics, and fail on premature EOF or malformed chunk termination. This is the protocol-input obligation at `AGENTS.md:161`, applied to newly introduced code.

2. **Bound the complete socket exchange.** `crates/server/tests/s3_last_modified_agreement.rs:202` through `:209` await connect, write, flush, and EOF without a deadline. Using the exact `send` helper, a peer supplied a complete length-delimited response and kept the connection open; the operation remained pending until the probe's external 300-ms watchdog cancelled it (`reviewer-socket.log:2`). Add a test-owned timeout with a useful failure message so a server regression terminates the test. This is the await obligation at `AGENTS.md:181`.

Verification supports the production correction, with explicit limits:

- **Independent red→green: PASS.** Ran `cargo test --locked --offline -p wyrd-server --test s3_last_modified_agreement` with the tracked fix stashed and the new test retained, then restored the fix and reran. Red: one intended assertion failure and two passes (`reviewer-red.log:275`). Green: all three pass (`reviewer-green.log:8`). The frozen discriminator agrees (`gate-logs/C4-verify.log:10`). The target files were restored byte-for-byte; their blob IDs match all three patch postimages (`reviewer-target.log:21`). There is no stale-target caveat.
- **Local CI: incomplete due to a host restriction.** `cargo xtask ci` passed typos, docs lint/render and link audit, repository guards, formatting, workspace clippy/build/tests, and dependency-use scanning, then failed to acquire an exclusive advisory-database lock on the read-only home path (`reviewer-ci.log:4`, `reviewer-ci.log:3258`, `reviewer-ci.log:3264`). This is a host caveat, not a patch defect. Separately reran the later conformance and statics checks: all 5 valid and 6 invalid vectors passed, and no prohibited global state was found (`reviewer-extra-checks.log:1`).
- **Frozen CI: real baseline advisory failure.** Both captured attempts reach the all-features advisory scan and reject `RUSTSEC-2026-0346` and `RUSTSEC-2026-0349` for Rustls 0.21.12 through TiKV (`gate-logs/C4-ci.log:3471`, `gate-logs/C4-ci.log:6471`, `gate-logs/C4-ci.log:6502`). The grounded dependency is `Cargo.lock:3294`; the lockfile and both deny configurations are unchanged. Keep that work outside #767 and route it through the existing backend/advisory tracking context, #435/#543 (`deny-all-features.toml:50`, `deny-all-features.toml:66`). Existing recorded exceptions are not reopened here; the current gate is nevertheless red. Later steps absent from the frozen log are not claimed as passed.
- **Coverage and mutation evidence: PASS from frozen logs.** The instance-scoped wrappers are not supplied in this leaf. Their captured outputs report 29/29 instrumented changed lines covered, with 23 unscored lines and a stated allowance for colocated tests (`gate-logs/C4-diff-cov.log:10`, `gate-logs/C4-diff-cov.log:141`), and seven of seven mutants caught (`gate-logs/C5-mutants.log:10`). These are logged gate results, not independent coverage or mutation reruns.
- **TiKV feature compilation: PASS independently.** Re-ran both captured clippy commands with `WYRD_TIKV_TOOLCHAIN=1`: the TiKV crate and server `tikv,etcd` test targets compiled successfully (`reviewer-tikv.log:1`, `reviewer-tikv.log:105`, `reviewer-tikv.log:106`, `reviewer-tikv.log:206`), agreeing with `gate-logs/host-tikv.log:211`. This proves compilation, not operation against a live TiKV service.
- **Contribution-artifact audit: N/A.** The row is deferred because `pr-description.md` is not drafted until after Check; its substantive audit runs at publish (`gate-logs/T4-contribution.log:10`). This creates no human-clearance item.
- **Declared external dependencies: discharged.** Both `typos` and the actual docs renderer ran successfully, including 99 rendered pages and a link audit, in the frozen CI and in this review (`brief.md:100`, `gate-logs/C4-ci.log:17`, `reviewer-ci.log:4`). No alias, code-read substitute, or missing-tool waiver supports the timestamp result.

Prior-art coverage remains unconfirmed beyond the brief's recorded scope. The path-filtered history investigation returned only the synthetic base commit, and the target has no remote (`reviewer-target.log:2`). The brief identifies merged formatter work and no closed/rejected PR on #767, but supplies no equivalent affected-path evidence for `crates/server/tests/s3_last_modified_agreement.rs` or `docs/design/architecture/05-building-block-view.md` (`brief.md:112`). A maintainer must confirm those searches; this is an evidence gap, not a claim that conflicting work exists.

All source citations above are grounded in the supplied `$PDCA_TARGET`; bundle and reviewer log citations are relative to this report's directory. No production fix was edited or committed. The independent probes and build artifacts remain inside the harness-owned review directory.

<!-- pdca:leaf-complete -->

### Advisory — adversary

# Adversarial review — issue #767 (listing `<LastModified>` at whole seconds)

Verdict: **could not refute the fix.** I re-ran the red→green proof, tried to break it with a
mutant, and looked for a timestamp surface the patch misses. Nothing broke. What follows is
one small doc wording slip, one gate red that this diff did not cause, and notes.

## Evidence (attempted to refute; could not)

- `crates/server/tests/s3_last_modified_agreement.rs:384` — re-ran it myself in a scratch copy
  of `$PDCA_TARGET`. With the fix: 3/3 pass (plus the in-file unit test
  `crates/gateway-s3/src/lib.rs:4621`). With the pre-fix `lib.rs` from `HEAD`: fails at `:384`,
  `left: "2023-11-14T22:13:20.750Z"`, `right: "…20.000Z"`. This is the same result as
  `gate-logs/C4-verify.log`. The test goes through the real path: seeded store →
  `Gateway::new` → `S3Gateway::serve` → raw signed HTTP. No mock or copy of the formatter is
  involved.
- Mutant probe: I changed `iso8601` to round (`(epoch_millis + 500) / 1_000`) instead of
  truncating. The test catches it at `:384` (`…22:13:21.000Z` vs `…20.000Z`). A cross-check
  that only compared shapes, or allowed ±1 s, would have missed this. This one doesn't.
- False-pass paths I checked and ruled out: a missing `<Contents>` row, a missing or unclosed
  `<LastModified>`, or a missing `Last-Modified` header all panic (`:362-376`, `:470-477`).
  An omitted element cannot pass as "agreement". The v1 leg really is v1: it checks that
  `<KeyCount>` is absent (`:455`), and dispatch is at `crates/gateway-s3/src/lib.rs:832-844`.
- Minor: `check-gates.json` C4-verify says "3 test(s) ran red". Only 1 test went red. The
  other 2 (parser self-check, on-second object) pass on base by design. This wording comes
  from the harness, not from the patch.

## The fix (attempted to break; could not)

- Missed surfaces: `iso8601` has exactly one production caller, `render_contents`
  (`crates/gateway-s3/src/lib.rs:1033`), and v1 and v2 share it. The GET/HEAD header goes
  through `http_date` (`:2499`, `:2607`). If-(Un)Modified-Since already compares `m / 1_000`
  (`:2786`, `:2801`), so a client that echoes a listed time back as a conditional gets a
  consistent answer. There is no CopyObject result (`:1809` refuses it), no ListBuckets
  `CreationDate`, and no ListParts yet. `gateway-core` only carries the raw `u64`.
  `crates/dst`, `crates/validate` and `crates/testkit` have no timestamp assertions that the
  change could silently break. (The C4-ci gate excludes `wyrd-dst`, so I grepped it.)
- Boundaries: both formatters compute `secs = epoch_millis / 1_000`, call the same
  `civil_from_days`, and return `None` on the same `year > 9_999` check (`:641`, `:3209`). So
  for every `u64` they either both render the same second or both omit. The last renderable
  millisecond `253_402_300_799_999` is pinned (`:4642`). I found no input where the two
  surfaces disagree.

## Findings

- NEEDS-HUMAN [impl] — `docs/design/architecture/05-building-block-view.md:190-194`: the new
  sentence went in between the list of the three fields and "Each is optional with a serde
  default *and* skipped when absent". "Each" now follows a sentence whose subjects are "the
  `Last-Modified` header and a listing's `<LastModified>`", so it reads as if the two wire
  surfaces are "optional with a serde default". Fix: move the new sentence after the
  serde/CAS sentence (after `:197`), or write "Each of the three fields is optional…". Also
  re-wrap the stub line `:194`.
- NEEDS-HUMAN [human] — `gate-logs/C4-ci.log:3471-3549`: the only C4-ci red is
  `cargo deny … check advisories`, on RUSTSEC-2026-0346 and RUSTSEC-2026-0349 against
  `rustls 0.21.12` (via `tokio-rustls 0.24.1`). That dependency comes from the base. This diff
  does not touch `Cargo.toml` or `Cargo.lock`, and every test step passed in both attempts. The
  red is real and gating, but it is not evidence against this fix. It needs its own dependency
  bump issue, or a decision on whether a base-wide advisory blocks unrelated PRs.

## Notes (no action needed for this diff)

- T4-batch-review's 6 blockers (`gate-logs/T4-batch-review.log`) are all about the test
  harness: lenient `dechunk`, `Content-Length` ignored, and no socket timeout. None of them
  shows a way for this test to pass falsely. A truncated body loses the asserted element and
  panics. The no-timeout point fits the rubric's await-discipline rule, but the peer harness
  the brief told the builder to copy has the same unbounded read
  (`crates/server/tests/s3_list_objects.rs:206`). Wrapping connect and read in
  `tokio::time::timeout` is cheap. The gate already sends this back, so I have not tagged it.
- Visible one-time change on upgrade: a client that cached `…20.750Z` from a pre-fix gateway
  will see `…20.000Z` on its first post-upgrade listing. A tool that compares listing to
  cache (rather than listing to HEAD) will then treat each such object as changed once. This
  is inherent to the fix, and the fix is still correct (it matches S3 and the header). It is
  worth one sentence in the PR description when it is drafted (T4-contribution is deferred).
- Out of scope per the brief: the draft proposal
  `docs/design/proposals/draft/0017-blackbox-validation-tool.md:461-468` still says a listing
  "keep[s] milliseconds" and that the oracle "never compares one [surface] to the other". After
  this fix the oracle could compare them. Per the rubric's out-of-scope rule, this calls for a
  follow-up issue, not an in-PR edit.
<!-- pdca:leaf-complete -->

### Advisory — code-review

The formatter change has no identified production correctness, reuse/simplification, or efficiency issue. Two findings concern the new regression harness:

- NEEDS-HUMAN [impl] — `crates/server/tests/s3_last_modified_agreement.rs:209`: The new socket exchange has no deadline around connect, write, or `read_to_end`. If the gateway stalls or leaves the connection open, either integration test can hang indefinitely instead of reporting a failure. Bound the complete exchange with `tokio::time::timeout`, as required by the await-discipline rubric.
- NEEDS-HUMAN [impl] — `crates/server/tests/s3_last_modified_agreement.rs:250`: The new response decoder treats invalid chunk sizes as termination, returns accumulated bytes when the terminal chunk is missing, and skips chunk terminators without validating them. For example, one complete XML chunk followed by EOF passes the timestamp assertions without the required `0\r\n\r\n`. The non-chunked branch at `crates/server/tests/s3_last_modified_agreement.rs:236` also ignores declared `Content-Length`. Reject incomplete or malformed framing before exposing the body, including length checks with HEAD's bodyless semantics. Otherwise these wire tests can pass responses a conforming client rejects, contrary to the protocol-input rubric.

Reviewed against the read-only target and frozen gate logs. The regression is confirmed red before the fix and green afterward; no builds were rerun.

<!-- pdca:leaf-complete -->

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C4 Verification (red→green) — Resolve the outstanding baseline advisory-gate disposition and obtain a complete CI result on a writable runner — independent red→green passed, but frozen CI rejects unchanged Rustls dependencies and local CI stops on a read-only advisory lock; `reviewer-green.log:13`, `gate-logs/C4-ci.log:6472`, `reviewer-ci.log:3264`, `Cargo.lock:3294`.
- [ ] T4 Contribution — Confirm merged and closed/rejected prior art by all affected file paths — the brief records the gateway path and issue-specific PR search, while the supplied one-commit snapshot cannot establish coverage for the new test and architecture path; `brief.md:112`, `reviewer-target.log:2`.
- [ ] T5 Judgment — Correct the response reader's false-success cases and bound its socket operations — the new wire oracle accepts incomplete/malformed responses and can hang, contrary to the standing rubric; `crates/server/tests/s3_last_modified_agreement.rs:229`, `crates/server/tests/s3_last_modified_agreement.rs:250`, `crates/server/tests/s3_last_modified_agreement.rs:209`, `reviewer-framing.log:1`, `reviewer-socket.log:1`.
- [ ] Validation — fitness-to-purpose — Accept whole-second listing semantics for the intended sync-client workflow — the four requested wire surfaces agree in executable tests, while product fitness remains the maintainer's sign-off decision; `crates/server/tests/s3_last_modified_agreement.rs:382`, `docs/design/architecture/05-building-block-view.md:191`.
- [ ] `docs/design/architecture/05-building-block-view.md:190-194`: the new sentence went in between the list of the three fields and "Each is optional with a serde default *and* skipped when absent". "Each" now follows a sentence whose subjects are "the `Last-Modified` header and a listing's `<LastModified>`", so it reads as if the two wire surfaces are "optional with a serde default". Fix: move the new sentence after the serde/CAS sentence (after `:197`), or write "Each of the three fields is optional…". Also re-wrap the stub line `:194`.
- [ ] `gate-logs/C4-ci.log:3471-3549`: the only C4-ci red is `cargo deny … check advisories`, on RUSTSEC-2026-0346 and RUSTSEC-2026-0349 against `rustls 0.21.12` (via `tokio-rustls 0.24.1`). That dependency comes from the base. This diff does not touch `Cargo.toml` or `Cargo.lock`, and every test step passed in both attempts. The red is real and gating, but it is not evidence against this fix. It needs its own dependency bump issue, or a decision on whether a base-wide advisory blocks unrelated PRs.
- [ ] `crates/server/tests/s3_last_modified_agreement.rs:209`: The new socket exchange has no deadline around connect, write, or `read_to_end`. If the gateway stalls or leaves the connection open, either integration test can hang indefinitely instead of reporting a failure. Bound the complete exchange with `tokio::time::timeout`, as required by the await-discipline rubric.
- [ ] `crates/server/tests/s3_last_modified_agreement.rs:250`: The new response decoder treats invalid chunk sizes as termination, returns accumulated bytes when the terminal chunk is missing, and skips chunk terminators without validating them. For example, one complete XML chunk followed by EOF passes the timestamp assertions without the required `0\r\n\r\n`. The non-chunked branch at `crates/server/tests/s3_last_modified_agreement.rs:236` also ignores declared `Content-Length`. Reject incomplete or malformed framing before exposing the body, including length checks with HEAD's bodyless semantics. Otherwise these wire tests can pass responses a conforming client rejects, contrary to the protocol-input rubric.
- [ ] C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance) FAILED (gating) — xtask: `cargo deny --all-features --config deny-all-features.toml check advisories` failed with exit status: 1
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 6 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_767/review-b
- [ ] The fix is selected before resolving the tracker’s explicit decision point. `brief.md:22-28` concludes S3 listings and headers “agree by construction” from a `.000Z` example and makes a live check optional; that conclusion also determines the expected output in `brief.md:29-37`. But `notes.json` → `body`, “To confirm before choosing a fix”, says “That should be verified against the real service rather than assumed, because it decides the fix” and explicitly allows a documentation-only resolution if S3 emits sub-second values. The target’s cited “shape S3 uses” comment (`crates/gateway-s3/src/lib.rs:615-617`) accompanies code that retains the millisecond remainder (`:627`, `:636`); it establishes a format, not an always-zero precision contract. The proposed test can prove Wyrd truncates, but cannot establish that truncation is the right compatibility behavior. Revise the brief to supply the requested service evidence or leave the behavioral choice explicitly for human adjudication, without presenting S3 agreement as established.
- [ ] The scope excludes documentation that the target repository requires for this behavior change. `brief.md:65-70` rules out “any ADR or `docs/` edit”, while the proposed ListObjects response changes `.750Z` to `.000Z` (`brief.md:29-37`). Target `AGENTS.md:154-157` requires a change that “adds or alters … an API operation” to update the living architecture doc in the same PR. Formatter comments alone do not meet that instruction. Revise the scope to allow the minimal living architecture update for the chosen timestamp semantics; accepted ADRs can remain untouched.

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
- Iteration delta (if iterating): Auto-iterate (round 1): rebuilding for the implementation-level findings — C4 Verification (red→green) — Resolve the outstanding baseline advisory-gate disposition and obtain a complete CI result on a writable runner — independent red→green passed, but frozen CI rejects unchanged Rustls dependencies and local CI stops on a read-only advisory lock; `reviewer-green.log:13`, `gate-logs/C4-ci.log:6472`, `reviewer-ci.log:3264`, `Cargo.lock:3294`.; T4 Contribution — Confirm merged and closed/rejected prior art by all affected file paths — the brief records the gateway path and issue-specific PR search, while the supplied one-commit snapshot cannot establish coverage for the new test and architecture path; `brief.md:112`, `reviewer-target.log:2`.; T5 Judgment — Correct the response reader's false-success cases and bound its socket operations — the new wire oracle accepts incomplete/malformed responses and can hang, contrary to the standing rubric; `crates/server/tests/s3_last_modified_agreement.rs:229`, `crates/server/tests/s3_last_modified_agreement.rs:250`, `crates/server/tests/s3_last_modified_agreement.rs:209`, `reviewer-framing.log:1`, `reviewer-socket.log:1`.; `docs/design/architecture/05-building-block-view.md:190-194`: the new sentence went in between the list of the three fields and "Each is optional with a serde default *and* skipped when absent". "Each" now follows a sentence whose subjects are "the `Last-Modified` header and a listing's `<LastModified>`", so it reads as if the two wire surfaces are "optional with a serde default". Fix: move the new sentence after the serde/CAS sentence (after `:197`), or write "Each of the three fields is optional…". Also re-wrap the stub line `:194`.; `crates/server/tests/s3_last_modified_agreement.rs:209`: The new socket exchange has no deadline around connect, write, or `read_to_end`. If the gateway stalls or leaves the connection open, either integration test can hang indefinitely instead of reporting a failure. Bound the complete exchange with `tokio::time::timeout`, as required by the await-discipline rubric.; `crates/server/tests/s3_last_modified_agreement.rs:250`: The new response decoder treats invalid chunk sizes as termination, returns accumulated bytes when the terminal chunk is missing, and skips chunk terminators without validating them. For example, one complete XML chunk followed by EOF passes the timestamp assertions without the required `0\r\n\r\n`. The non-chunked branch at `crates/server/tests/s3_last_modified_agreement.rs:236` also ignores declared `Content-Length`. Reject incomplete or malformed framing before exposing the body, including length checks with HEAD's bodyless semantics. Otherwise these wire tests can pass responses a conforming client rejects, contrary to the protocol-input rubric.; C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance) FAILED (gating) — xtask: `cargo deny --all-features --config deny-all-features.toml check advisories` failed with exit status: 1; T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 6 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_767/review-b. Deferred, not addressed here: 3 finding(s) that need human judgment, held in deferred-findings.json for the human at handover — not build work
- By / date: auto-iterate / 2026-10-14

## 10. Act candidates (hints for the next Act review)
- Plan advisory: 2 finding(s); brief revised: yes (plan-advisory-*.md)
- (empty is the common case)
