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
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass — run-verify.sh: PASS — red without the fix, green with it (4 test(s) ran red).
- C4 diff coverage: changed lines executed by the patch's tests: pass — diff coverage 100.0% — 29 of 29 instrumentable changed lines executed (floor 80%); 29 of 54 changed lines were instrumen
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): pass — 7 mutants tested in 25s: 7 caught

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): pass — review-branch: 0 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_767/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 13.66s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Issue #767 aligns S3 listing modification times with HEAD/GET at whole-second precision; independent red→green testing supports the fix, while baseline CI disposition and fitness approval remain with the human.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The required v1/v2 listing and HEAD/GET agreement is explicit and falsifiable; the live S3 rerun corroborates whole-second precision without changing stored milliseconds (`brief.md:36`, `reviewer-s3-live.log:1`). |
| C2 Reproduction (red pre-fix) | PASS | With production changes stashed and the new test retained, compilation succeeded and the off-second case failed on `.750Z` versus `.000Z`; the other three tests passed (`reviewer-red.log:263`, `reviewer-red.log:276`). |
| C3 Change | PASS | The change stays within rendering, regression coverage and the required architecture sentence; persisted metadata and conditional-request comparisons retain their existing semantics (`crates/gateway-s3/src/lib.rs:634`, `docs/design/architecture/05-building-block-view.md:194`). |
| C4 Verification (red→green) | NEEDS-HUMAN | Resolve the existing TiKV/Rustls advisory-gate disposition and obtain complete CI evidence — independent red→green passes, but frozen CI fails on unchanged dependencies and local CI stops on a host lock restriction (`reviewer-green.log:12`, `gate-logs/C4-ci.log:6473`, `reviewer-ci.log:3262`). |
| C5 Causal adequacy | PASS | Removing fractional output eliminates the observed mismatch at its rendering source; both formatters use the same truncation, with no new capability probe or symptom guard (`crates/gateway-s3/src/lib.rs:637`, `crates/gateway-s3/src/lib.rs:646`, `crates/gateway-s3/src/lib.rs:3204`). |
| T1 Structure | PASS | Existing gateway/store boundaries and dependency direction remain intact; the test uses the ordinary write path and an existing SDK date dependency (`crates/server/tests/s3_last_modified_agreement.rs:122`, `crates/server/tests/s3_last_modified_agreement.rs:183`, `crates/server/Cargo.toml:134`). |
| T2 Shape | PASS | The wire contract is documented at both formatters and in the living architecture doc; independent SDK parsing avoids duplicating calendar logic in the regression oracle (`crates/gateway-s3/src/lib.rs:620`, `crates/gateway-s3/src/lib.rs:3193`, `crates/server/tests/s3_last_modified_agreement.rs:480`, `docs/design/architecture/05-building-block-view.md:194`). |
| T3 Runtime | PASS | Real in-process gateway exchanges pass for both stored timestamps; socket waits are bounded and the server task is aborted on drop, with no additional production I/O or clock source (`reviewer-green.log:9`, `crates/server/tests/s3_last_modified_agreement.rs:148`, `crates/server/tests/s3_last_modified_agreement.rs:426`). |
| T4 Contribution | PASS | Independent merged-history and closed/rejected-work checks cover all three affected paths and found no prior matching fix; the contribution-artifact audit is separately N/A until publish (`reviewer-prior-art.log:2`, `reviewer-prior-art.log:5`, `gate-logs/T4-contribution.log:10`). |
| T5 Judgment | PASS | Exact wire assertions and fraction-preserving SDK parsing detect the original defect; framing refusals, deadlines and the architecture wording address the carried-forward implementation concerns (`crates/server/tests/s3_last_modified_agreement.rs:501`, `crates/server/tests/s3_last_modified_agreement.rs:517`, `crates/server/tests/s3_last_modified_agreement.rs:765`, `docs/design/architecture/05-building-block-view.md:191`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Accept whole-second wire precision as the intended client synchronization contract — gateway tests and live S3 samples support it, while suitability for the intended sync workflow remains a sign-off decision (`brief.md:36`, `crates/server/tests/s3_last_modified_agreement.rs:765`, `reviewer-s3-live.log:4`). |

No builder-fixable defect was found. The remaining CI concern belongs to the existing dependency gate and runner environment; it is not evidence that this timestamp patch is incorrect.

Source citations resolve inside `$PDCA_TARGET`; evidence-log citations resolve in this review directory. The target is the declared base with the supplied patch applied, not a stale dependent checkout. After the stash/pop experiment, all three affected file blobs match `patch.diff` and the stash is empty (`reviewer-target.log:1`, `reviewer-target.log:5`).

- **Independent verification passes.** Ran `cargo test --offline --locked -p wyrd-server --test s3_last_modified_agreement` before and after restoring the production patch: one assertion failure before, four passes after. The local `cargo xtask ci` rerun also passed typos, docs lint/render, repository guards, formatting, workspace clippy/build/tests and cargo-machete before the advisory-database lock failure (`reviewer-ci.log:2`, `reviewer-ci.log:20`, `reviewer-ci.log:3256`). Both host TiKV clippy commands passed independently (`reviewer-host-tikv.log`). The brief's external tools, `typos` and the docs renderer, were actually exercised; the separate render produced 99 pages and passed its link audit (`reviewer-doc-render.log:2`).
- **Complete CI remains unproven.** Both frozen attempts fail the all-features advisory scan on Rustls 0.21.12 through `tikv-client`, naming RUSTSEC-2026-0346 and RUSTSEC-2026-0349 (`gate-logs/C4-ci.log:6480`, `gate-logs/C4-ci.log:6510`). `Cargo.lock` and advisory policy are outside this patch; the dependency is already present at `Cargo.lock:3294`, and the optional TiKV exposure boundary is documented in `deny-all-features.toml:50` under #543. The independent run instead stops because the advisory database lock is read-only (`reviewer-ci.log:3262`), a host caveat. Neither CI run reaches the subsequent dedicated conformance, statics, deployment guard and DST sequence (`xtask/src/main.rs:1683`). This review neither waives the deterministic gate nor asks the builder to expand #767 into dependency remediation.
- **Frozen wrapper evidence supports its stated results.** The instance-scoped wrappers are not available in this target copy, so their logs were read rather than treating wrapper absence as a defect. C4-verify shows exactly one failing test among four before the fix, then a passing fixed suite (`gate-logs/C4-verify.log:12`, `gate-logs/C4-verify.log:24`), independently reproduced above. C4-diff-cov reports 29/29 instrumentable changed lines covered, with 25 unscored lines and a stated limitation for co-located tests; this is not coverage of every added test-helper line (`gate-logs/C4-diff-cov.log:10`, `gate-logs/C4-diff-cov.log:143`). C5-mutants reports seven tested and caught mutants (`gate-logs/C5-mutants.log:10`). T4-batch-review reports zero blocking findings; its individual review reports are not in this bundle (`gate-logs/T4-batch-review.log:10`). The host-tikv log records successful feature builds, corroborated by the independent clippy rerun (`gate-logs/host-tikv.log:211`). **T4-contribution: N/A** — the artifacts are absent by design, and its substantive audit must rerun at publish (`gate-logs/T4-contribution.log:10`).
- **Compatibility and prior art were investigated directly.** Anonymous requests to the three S3 listing URLs in the brief returned 3,000/3,000 `.000Z` timestamps, and the sampled object's HEAD parsed to the same instant (`reviewer-s3-live.log:1`). GitHub path-history queries retrieved 33 gateway commits and 30 architecture commits through the declared base; the new test path has no history. All 381 closed PRs were enumerated, and all 19 closed-unmerged PRs had their complete file inventories checked: none touches the three affected paths (`reviewer-prior-art.log:2`). Relevant merged work introduced millisecond listing precision and invalid-date omission, without this agreement fix (`reviewer-prior-art.log:6`).

<!-- pdca:leaf-complete -->

### Advisory — adversary

# Adversarial review — issue 767 (listing `<LastModified>` at whole seconds)

**Bottom line:** I tried to refute the fix and its red→green evidence and could not. The two
open gate problems are a baseline dependency advisory and a question about how the review
gate was run. Neither is a defect in this diff, and the builder can't fix either by
rebuilding.

## Evidence (re-run independently)

- `crates/server/tests/s3_last_modified_agreement.rs:767` — I re-ran red→green in a scratch
  copy of `$PDCA_TARGET`. **Green:** 4/4 integration tests, and the 3 related unit tests in
  `crates/gateway-s3/src/lib.rs` pass. **Red** (only `crates/gateway-s3/src/lib.rs` reverted,
  test kept): `off_second_modified_lists_and_heads_as_the_same_instant` fails at `:767` with
  `"2023-11-14T22:13:20.750Z"` vs `"…20.000Z"`. This matches `gate-logs/C4-verify.log:25-29`.
  Could not refute.
- `crates/server/tests/s3_last_modified_agreement.rs:782` — the red run stops at check (1),
  so check (3) never runs before the fix. I turned check (1) off in the scratch copy and ran
  the pre-fix base again. Check (3) fails on its own (`20.75Z` vs `20Z`, parsed by
  `aws-smithy-types`), so the parsed-instant check really tells the two apart. It is not a
  tautology. Could not refute.
- `crates/gateway-s3/src/lib.rs:1130`, `:1187` — the test runs the production path: it seeds
  through `wyrd_core::write`, then sends real signed requests over TCP to the real `Gateway` +
  `S3Gateway`. The v1 and v2 listings both render through `render_contents` (`:1030`), and the
  test asserts both. No mirrored copy of the production code is involved.
- `gate-logs/C4-verify.log:38` (wording only, no action) — the gate's summary says "4 test(s)
  ran red", but only 1 of the 4 failed before the fix (`:35`, "3 passed; 1 failed"). The
  parser and response-reader tests check the test harness itself, and the on-second test is a
  regression guard. All three are green on both sides, by design. Read this row as one red
  proof, not four.

## The fix (tried to break it)

- `crates/gateway-s3/src/lib.rs:637`, `:3204` — both formatters derive the instant from
  `epoch_millis / 1_000`, so they agree to the second for every `u64`. They also share the
  year-9999 cut-off (`:642`, `:3211`): `253_402_300_800_000` and `u64::MAX` give `None` from
  both, so no value can render on one surface and not the other. Rounding instead of
  truncating would be caught: 750 ms rounds up to `:21`, the test expects `:20`, and the unit
  test has a 999 ms row (`:4640`). Could not refute.
- `crates/gateway-s3/src/lib.rs:2499`, `:2607`, `:2787`, `:2802` — every header surface
  (200, 206, ranged HEAD, 304) goes through `http_date`, and the conditional-request
  comparisons already truncate to seconds. The only readers of `modified`
  (`crates/server/src/lib.rs:445`, `:531`, `:626`, `:739`) feed these paths. I found no other
  surface that renders the stored milliseconds. Could not refute.
- `crates/server/tests/s3_last_modified_agreement.rs:247`, `:344` — I looked for a
  false-success case in the strict response reader. What leniency is left (a status line with
  no space before an empty reason phrase; header names checked as `ascii_graphic` instead of
  `tchar`, the RFC's allowed token characters) only accepts slightly malformed responses, and
  the timestamp checks are exact string matches. A framing leak can't make the property pass
  falsely. Not a finding.
- A trade-off, not a refutation: truncating means two same-size overwrites within one second
  now list with the same `LastModified`, so a sync tool that compares only size and time
  (`aws s3 sync` by default) skips the second one. Real S3 behaves the same way, the HEAD
  header already did, and draft 0017 already notes it
  (`docs/design/proposals/draft/0017-blackbox-validation-tool.md:369-370`). No action.

## The verdict (claims in check-gates.json)

- NEEDS-HUMAN [human] — `gate-logs/C4-ci.log:3472-3550` (the second attempt repeats at
  `:6474-6551`): the failing C4-ci gate is the baseline advisory RUSTSEC-2026-0346/0349 in
  `rustls 0.21.12`, pulled in through `tikv-client` (`Cargo.lock:3294-3295`). It is not caused by
  this diff: the patch doesn't touch `Cargo.lock`, the default `cargo deny check` passes
  (`:3469-3470`), and typos, docs lint and render, fmt, clippy, build, the full test run and
  cargo-machete all passed before the stop. The gate never reached the steps after deny, so I
  ran `cargo xtask statics` and `cargo xtask conformance` on the patched tree in scratch, and
  both pass. I did not run DST, but `crates/dst/Cargo.toml` depends on neither `gateway-s3`
  nor `server`. This failure has already triggered two auto-iterate rebuilds
  (`brief.md:133-141`) that the builder cannot fix, because bumping the dependency or
  ignoring the advisory is a human-only call (ADR-0003 / `deny.toml`). Decide the advisory
  disposition; don't send this back to Do again.
- NEEDS-HUMAN [human] — `gate-logs/T4-batch-review.log:6`, `:10`: the gating rubric review
  reports "0 blocking, 0 recorded-rejected, 0 noise-dropped" after 22.28 s for "3x codex".
  Last round the same gate reported 6 blocking. Three model passes over an ~800-line diff in
  22 s is fast enough that a cached or empty result is possible. If so, the zero count passes
  while no review happened (the rubric's "count-based assertion that can pass while the
  property fails" class). I can't see `results/issue_767/review-batch.md`. Before counting
  this row as green, confirm it holds three real passes over this patch.
- NEEDS-HUMAN [human] — `docs/design/proposals/draft/0017-blackbox-validation-tool.md:461-463`:
  this draft proposal still says a listing "keeps milliseconds" and that the two surfaces
  differ by up to 999 ms. After this patch that is false. The brief scopes out any other docs
  edit (`brief.md:91-93`), and the rubric says an out-of-scope finding gets a decline with an
  issue reference. Decide whether to amend the draft here (drafts aren't frozen) or file a
  follow-up against #769. The living architecture doc is correct
  (`docs/design/architecture/05-building-block-view.md:194-196`), and the iteration-1
  placement problem is fixed.

<!-- pdca:leaf-complete -->

### Advisory — code-review

- No introduced correctness bugs found. The listing formatter preserves whole-second truncation and the existing year bound (`crates/gateway-s3/src/lib.rs:634`); the new tests assert exact timestamps and parsed agreement for both listing versions, HEAD, and GET (`crates/server/tests/s3_last_modified_agreement.rs:765`). Socket exchanges are bounded and framing failures are rejected (`crates/server/tests/s3_last_modified_agreement.rs:247`, `crates/server/tests/s3_last_modified_agreement.rs:426`).
- No actionable reuse, simplification, or efficiency findings. The formatter retains the shared calendar conversion (`crates/gateway-s3/src/lib.rs:641`), and the independent test oracle reuses the existing SDK date parser (`crates/server/tests/s3_last_modified_agreement.rs:483`).

Validation used the frozen gate evidence: the regression fails on the pre-fix fractional timestamp and passes with the patch; changed-line coverage and all seven mutation checks pass. Full CI still fails on unchanged Rustls dependencies, outside this diff. No builds were rerun or target files modified.

<!-- pdca:leaf-complete -->

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C4 Verification (red→green) — Resolve the existing TiKV/Rustls advisory-gate disposition and obtain complete CI evidence — independent red→green passes, but frozen CI fails on unchanged dependencies and local CI stops on a host lock restriction (`reviewer-green.log:12`, `gate-logs/C4-ci.log:6473`, `reviewer-ci.log:3262`).
- [ ] Validation — fitness-to-purpose — Accept whole-second wire precision as the intended client synchronization contract — gateway tests and live S3 samples support it, while suitability for the intended sync workflow remains a sign-off decision (`brief.md:36`, `crates/server/tests/s3_last_modified_agreement.rs:765`, `reviewer-s3-live.log:4`).
- [ ] `gate-logs/C4-ci.log:3472-3550` (the second attempt repeats at `:6474-6551`): the failing C4-ci gate is the baseline advisory RUSTSEC-2026-0346/0349 in `rustls 0.21.12`, pulled in through `tikv-client` (`Cargo.lock:3294-3295`). It is not caused by this diff: the patch doesn't touch `Cargo.lock`, the default `cargo deny check` passes (`:3469-3470`), and typos, docs lint and render, fmt, clippy, build, the full test run and cargo-machete all passed before the stop. The gate never reached the steps after deny, so I ran `cargo xtask statics` and `cargo xtask conformance` on the patched tree in scratch, and both pass. I did not run DST, but `crates/dst/Cargo.toml` depends on neither `gateway-s3` nor `server`. This failure has already triggered two auto-iterate rebuilds (`brief.md:133-141`) that the builder cannot fix, because bumping the dependency or ignoring the advisory is a human-only call (ADR-0003 / `deny.toml`). Decide the advisory disposition; don't send this back to Do again.
- [ ] `gate-logs/T4-batch-review.log:6`, `:10`: the gating rubric review reports "0 blocking, 0 recorded-rejected, 0 noise-dropped" after 22.28 s for "3x codex". Last round the same gate reported 6 blocking. Three model passes over an ~800-line diff in 22 s is fast enough that a cached or empty result is possible. If so, the zero count passes while no review happened (the rubric's "count-based assertion that can pass while the property fails" class). I can't see `results/issue_767/review-batch.md`. Before counting this row as green, confirm it holds three real passes over this patch.
- [ ] `docs/design/proposals/draft/0017-blackbox-validation-tool.md:461-463`: this draft proposal still says a listing "keeps milliseconds" and that the two surfaces differ by up to 999 ms. After this patch that is false. The brief scopes out any other docs edit (`brief.md:91-93`), and the rubric says an out-of-scope finding gets a decline with an issue reference. Decide whether to amend the draft here (drafts aren't frozen) or file a follow-up against #769. The living architecture doc is correct (`docs/design/architecture/05-building-block-view.md:194-196`), and the iteration-1 placement problem is fixed.
- [ ] C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance) FAILED (gating) — xtask: `cargo deny --all-features --config deny-all-features.toml check advisories` failed with exit status: 1
- [ ] The fix is selected before resolving the tracker’s explicit decision point. `brief.md:22-28` concludes S3 listings and headers “agree by construction” from a `.000Z` example and makes a live check optional; that conclusion also determines the expected output in `brief.md:29-37`. But `notes.json` → `body`, “To confirm before choosing a fix”, says “That should be verified against the real service rather than assumed, because it decides the fix” and explicitly allows a documentation-only resolution if S3 emits sub-second values. The target’s cited “shape S3 uses” comment (`crates/gateway-s3/src/lib.rs:615-617`) accompanies code that retains the millisecond remainder (`:627`, `:636`); it establishes a format, not an always-zero precision contract. The proposed test can prove Wyrd truncates, but cannot establish that truncation is the right compatibility behavior. Revise the brief to supply the requested service evidence or leave the behavioral choice explicitly for human adjudication, without presenting S3 agreement as established.
- [ ] The scope excludes documentation that the target repository requires for this behavior change. `brief.md:65-70` rules out “any ADR or `docs/` edit”, while the proposed ListObjects response changes `.750Z` to `.000Z` (`brief.md:29-37`). Target `AGENTS.md:154-157` requires a change that “adds or alters … an API operation” to update the living architecture doc in the same PR. Formatter comments alone do not meet that instruction. Revise the scope to allow the minimal living architecture update for the chosen timestamp semantics; accepted ADRs can remain untouched.
- [ ] `gate-logs/C4-ci.log:3471-3549`: the only C4-ci red is `cargo deny … check advisories`, on RUSTSEC-2026-0346 and RUSTSEC-2026-0349 against `rustls 0.21.12` (via `tokio-rustls 0.24.1`). That dependency comes from the base. This diff does not touch `Cargo.toml` or `Cargo.lock`, and every test step passed in both attempts. The red is real and gating, but it is not evidence against this fix. It needs its own dependency bump issue, or a decision on whether a base-wide advisory blocks unrelated PRs.
- [ ] `Cargo.lock:3294-3295` (`rustls 0.21.12`, reached via `tikv-client 0.4.0` → `tonic 0.10.2`): the only gating red, C4-ci, is `cargo deny --all-features … check advisories` failing on RUSTSEC-2026-0346 and RUSTSEC-2026-0349 (`gate-logs/C4-ci.log:6473-6551`). The default-feature `cargo deny check` passes (`:6470-6471`), and fmt, clippy, build, the full test run (including this file, `:2698-2706`), typos and the docs checks are all green. The diff does not touch `Cargo.lock`, `Cargo.toml` or either `deny*.toml`, so this red is pre-existing on the base and not evidence against the fix. Fixing it means bumping the TiKV client stack or adding an advisory exception, which is outside this slice. A human has to decide whether to accept with the baseline red or wait for a separate dependency fix.

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
- Iteration delta (if iterating): Auto-iterate (round 3): rebuilding for the implementation-level findings — C4 Verification (red→green) — Resolve the existing TiKV/Rustls advisory-gate disposition and obtain complete CI evidence — independent red→green passes, but frozen CI fails on unchanged dependencies and local CI stops on a host lock restriction (`reviewer-green.log:12`, `gate-logs/C4-ci.log:6473`, `reviewer-ci.log:3262`).; C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance) FAILED (gating) — xtask: `cargo deny --all-features --config deny-all-features.toml check advisories` failed with exit status: 1. Deferred, not addressed here: 5 finding(s) that need human judgment, held in deferred-findings.json for the human at handover — not build work
- By / date: auto-iterate / 2026-10-14

## 10. Act candidates (hints for the next Act review)
- Plan advisory: 2 finding(s); brief revised: yes (plan-advisory-*.md)
- (empty is the common case)
