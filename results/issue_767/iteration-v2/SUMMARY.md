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
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): pass — 7 mutants tested in 24s: 7 caught

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): pass — review-branch: 0 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_767/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 13.31s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Review #767: make S3 listing modification times agree with HEAD/GET at whole-second precision while preserving stored milliseconds.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The precision contract is explicit and falsifiable, and independent live sampling confirms the selected S3 behavior: 3,000 whole-second listings and an agreeing HEAD; `brief.md:40`, `reviewer-live-s3.log:1`, `docs/design/architecture/05-building-block-view.md:194`. |
| C2 Reproduction (red pre-fix) | PASS | With tracked changes stashed and the new test retained, the base compiled and failed specifically on `.750Z` versus `.000Z`; the other three tests passed; `reviewer-red.log:276`, `crates/server/tests/s3_last_modified_agreement.rs:842`. |
| C3 Change | PASS | The change stays within the specified rendering, regression-test, and documentation scope; stored precision and conditional-request semantics are unaffected; `crates/gateway-s3/src/lib.rs:637`, `crates/gateway-s3/src/lib.rs:646`, `docs/design/architecture/05-building-block-view.md:194`. |
| C4 Verification (red→green) | NEEDS-HUMAN | Resolve the baseline Rustls advisory-gate disposition and obtain complete CI evidence — independent red→green passed, but frozen CI fails on unchanged dependencies and local CI stops on a read-only advisory lock before later checks; `reviewer-green.log:12`, `gate-logs/C4-ci.log:6473`, `reviewer-ci.log:3263`, `Cargo.lock:3294`. |
| C5 Causal adequacy | PASS | The mismatch's cause is removed at the shared listing formatter, with no capability probe or symptom guard; off-second wire tests and seven independently caught mutants discriminate the fix; `crates/gateway-s3/src/lib.rs:646`, `crates/server/tests/s3_last_modified_agreement.rs:840`, `reviewer-mutants.log:7`, `reviewer-mutants-cap.log:6`. |
| T1 Structure | PASS | Existing gateway seams, storage serialization, and clock ownership remain intact; the test uses real writes and identifies the separate signing-clock lifecycle; `crates/server/tests/s3_last_modified_agreement.rs:104`, `crates/server/tests/s3_last_modified_agreement.rs:200`, `crates/gateway-s3/src/lib.rs:634`. |
| T2 Shape | PASS | Formatting, spelling, and docs lint/render passed independently; the architecture sentence now follows the complete optional-field/CAS explanation without changing its referent; `reviewer-ci.log:2`, `reviewer-ci.log:10`, `reviewer-ci.log:21`, `docs/design/architecture/05-building-block-view.md:190`. |
| T3 Runtime | PASS | Rendering retains constant work and existing out-of-range omission; test socket/store operations have deadlines and the serve task is aborted on drop; `crates/gateway-s3/src/lib.rs:642`, `crates/server/tests/s3_last_modified_agreement.rs:85`, `crates/server/tests/s3_last_modified_agreement.rs:145`, `crates/server/tests/s3_last_modified_agreement.rs:423`. |
| T4 Contribution | PASS | Independent searches covered merged history for all three affected paths and changed-file lists for every closed-unmerged PR, with no rejected overlap; contribution-artifact audit is separately N/A until mandatory publish-time execution; `reviewer-prior-art.log:1`, `reviewer-prior-art.log:68`, `gate-logs/T4-contribution.log:10`. |
| T5 Judgment | PASS | Both listing versions and HEAD/GET are checked with exact timestamps, independently parsed instants, and an unchanged whole-second control; prior framing and timeout concerns are resolved with executable checks; `crates/server/tests/s3_last_modified_agreement.rs:592`, `crates/server/tests/s3_last_modified_agreement.rs:795`, `crates/server/tests/s3_last_modified_agreement.rs:840`, `reviewer-green.log:7`. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Accept whole-second wire timestamps as the intended contract for sync/revalidation workflows — live S3 and the shipping gateway agree in the exercised cases, while product fitness remains the maintainer's sign-off decision; `reviewer-live-s3.log:5`, `crates/server/tests/s3_last_modified_agreement.rs:840`, `docs/design/architecture/05-building-block-view.md:194`. |

No remaining patch defect was found; full CI and product sign-off remain outstanding. The formatter fixes the reproduced cause, the revised wire oracle rejects the previously identified framing failures, and prior-art coverage now includes every affected path. The outstanding CI result belongs to the unchanged dependency baseline and the review host, rather than to this timestamp patch.

Source citations above are relative to `$PDCA_TARGET`; brief, gate-log, and reviewer-log citations are relative to this review directory. The disposable target matches base `7bf7eb906c634d57d28f2ae26fd0b412732948d5`. All three patched file hashes match the bundle, the complete patch reverses cleanly, and no review stash remains (`reviewer-target.log:2`). No implementation edits were retained.

| Gate evidence | Verdict | Basis |
|---------------|---------|-------|
| C4-ci | NEEDS-HUMAN | Frozen attempts stop at all-features advisories RUSTSEC-2026-0346 and RUSTSEC-2026-0349 in Rustls 0.21.12 through TiKV; local CI passes through cargo-machete, then cannot lock the advisory database. Neither run reaches the subsequent conformance/statics/deployment-guard/DST sequence; `gate-logs/C4-ci.log:6480`, `gate-logs/C4-ci.log:6510`, `reviewer-ci.log:3257`, `reviewer-ci.log:3263`, `xtask/src/main.rs:1683`. |
| C4-verify | PASS | Independently ran `cargo test --locked -p wyrd-server --test s3_last_modified_agreement` with production changes stashed, then restored: exit 101 with the intended timestamp assertion, then exit 0 with 4/4 passing; `reviewer-red.log:276`, `reviewer-green.log:12`; agrees with `gate-logs/C4-verify.log:25`. |
| C4-diff-cov | PASS | Frozen captured execution shows both test suites passing and 29/29 instrumentable changed lines covered; 25 additional changed lines are explicitly unscored. This is log-backed evidence, not an independent coverage rerun; `gate-logs/C4-diff-cov.log:129`, `gate-logs/C4-diff-cov.log:130`, `gate-logs/C4-diff-cov.log:143`. |
| C5-mutants | PASS | Independently caught four arithmetic mutants; three body-replacement mutants first failed compilation on unused parameters, then all three were caught after retrying with the mutation tool's `--cap-lints true`. No survivor remains; `reviewer-mutants.log:3`, `reviewer-mutants-cap.log:3`; frozen evidence also records seven caught at `gate-logs/C5-mutants.log:13`. |
| T4-batch-review | PASS | Frozen log records zero blocking, rejected, or noise-dropped findings; it supplies the summary, not the underlying individual reviews. The judgment above comes from this independent source/evidence review; `gate-logs/T4-batch-review.log:10`. |
| T4-contribution | N/A | The PR description is absent by design at Check; the substantive contribution-artifact audit must rerun at publish, so this deferred row is neither green nor a human escalation; `gate-logs/T4-contribution.log:10`. |
| host-tikv | PASS | Frozen log shows both requested clippy invocations finishing with real TiKV dependencies compiled; this establishes feature compilation, not service/topology runtime coverage; `gate-logs/host-tikv.log:2`, `gate-logs/host-tikv.log:112`, `gate-logs/host-tikv.log:211`. |

The brief's external dependencies were exercised: local CI ran `typos`, docs lint, and the real renderer, rendering 99 pages and passing the link audit (`brief.md:100`, `reviewer-ci.log:2`, `reviewer-ci.log:4`, `reviewer-ci.log:7`). The advisory-lock failure is a host limitation; the separate frozen all-features advisory failure is substantive but predates this patch, whose `Cargo.lock` is byte-identical to the base (`reviewer-target.log:7`). No waiver or dependency change was made during review.

The prior-art investigation closes the earlier coverage gap. GitHub path history returned 33 gateway commits, zero commits for the new test path, and 30 architecture commits. Pagination covered all 381 closed PRs; changed-file lists for all 19 closed-unmerged PRs matched none of the three affected paths, including previous filenames (`reviewer-prior-art.log:1`, `reviewer-prior-art.log:68`). The raw API responses are retained in `reviewer-prior-art-*.json`.

The selected compatibility behavior also has direct service evidence: fresh anonymous v1/v2 listings from two regions returned 3,000/3,000 timestamps ending `.000Z`, and the sampled object's HEAD matched the listed instant (`reviewer-live-s3.log:1`). The [AWS ListObjectsV2 reference](https://docs.aws.amazon.com/AmazonS3/latest/API/API_ListObjectsV2.html) corroborates the timestamp shape in its examples; the live sample establishes the observed precision rather than inferring it solely from documentation. The gateway regression used real filesystem fragments, redb metadata, signed HTTP, and the intentionally off-second stored timestamp, so its environment demonstrably exhibited the forbidden failure before the fix.

<!-- pdca:leaf-complete -->

### Advisory — adversary

# Adversarial review — issue 767 (listing `<LastModified>` at whole seconds)

**Verdict: I tried to refute the fix and could not.** The red→green is real, the test goes through the production path, and I found no input where the listing and the header disagree. The only gating red (C4-ci) comes from a dependency advisory that this diff does not touch.

## Evidence re-run (independent, on a scratch copy of `$PDCA_TARGET`)

- `crates/server/tests/s3_last_modified_agreement.rs:842` — **red→green reproduced.** With the patch: 4/4 pass. With only `crates/gateway-s3/src/lib.rs` reverted to the base: `off_second_modified_lists_and_heads_as_the_same_instant` fails at `:842` with `left: "2023-11-14T22:13:20.750Z" right: "2023-11-14T22:13:20.000Z"`. That is the defect named in the brief, and it fails on an assertion, not a build error. Same line and message as `gate-logs/C4-verify.log`.
- `crates/server/tests/s3_last_modified_agreement.rs:253-299` — **production path, not a copy.** The test seeds a real redb store through `wyrd_core::write`, runs the real `S3Gateway::serve` on a loopback port, and sends raw signed HTTP. Nothing in the timestamp path is mocked. The only formatter call site is `render_contents` → `iso8601` at `crates/gateway-s3/src/lib.rs:1034`, which serves both v1 and v2, and the test asserts both.
- `crates/server/tests/s3_last_modified_agreement.rs:842-852` — **no wrong-reason pass.** The checks are exact strings, not "the two agree". Plausible wrong fixes would all go red here: rounding to nearest or up (`:21`), dropping the fraction (`…:20Z`), or changing `http_date` instead of `iso8601`. The instant check at `:855-868` keeps the ISO fraction (`parse_iso_millis`, `:636`), so it would also catch `.750` vs `:20`.
- Not a defect, but don't over-read it: `gate-logs/C4-verify.log` ends "4 test(s) ran red", yet only **1 of 4** failed on the reverted base. The parser test, the reader test and the on-second test pass on both sides by design. The red→green claim rests on that one test, and it is enough.

## Attempts to break the fix (all failed)

- `crates/gateway-s3/src/lib.rs:634-648` — Boundaries: `.000`, `.750`, `.999`, and the last renderable millisecond `253_402_300_799_999` all render `.000Z`. The unit test at `:4623-4656` pins them next to `http_date`. The year-9999 cutoff for omitting the value is unchanged. I found no input where `iso8601` and `http_date` name different seconds, since both use the same `epoch_millis / 1_000`.
- `crates/gateway-s3/src/lib.rs:2499`, `:2607` — Other surfaces: the 200/206 header and the 304 header both use `http_date`. No other source formats `modified`: grep finds no CopyObject result, no ListParts or ListObjectVersions renderer, and no second formatter (`civil_from_days` callers are only `iso8601`, `http_date`, the conditional parser, and SigV4). The brief's "every S3 surface" invariant holds for the code that exists today.
- `crates/gateway-s3/src/lib.rs:2787`, `:2802` — Revalidation round trip: a client that copies the listing's `22:13:20.000Z` into `If-Modified-Since: …22:13:20 GMT` gets 304 for a stored `…750`, because the comparison truncates the same way. Before the fix, a client keeping the `.750` had no way to send it in an HTTP date anyway.
- `docs/design/architecture/05-building-block-view.md:194-196` — Docs currency: the sentence now sits after the serde/CAS sentence. "Each is optional…" still refers to the three fields, which fixes last round's wording problem. Lint, render and typos passed in `gate-logs/C4-ci.log:16-25`.
- Rubric, await discipline and protocol input (`crates/server/tests/s3_last_modified_agreement.rs:186-190`, `:345-485`, `:524-535`): every socket and store await is under a 30 s timeout. The serve task is aborted on drop (`:246-250`). The reader refuses short or long `Content-Length`, a missing chunked terminal CRLF, and a body on HEAD, and `:692-878` proves each case. Last round's three blocking test-quality findings are addressed.
- Design trade-off, considered and **not raised**: two overwrites within the same second are now indistinguishable by listing `LastModified` alone (before, `.250` vs `.750` showed a change). That matches the header, which already had this property, and live S3 (the brief's 3,000/3,000 sample). The SHA-256 ETag still changes. The brief chose this on evidence, so it is not a defect in this diff.

## For the human

- NEEDS-HUMAN [human] — `Cargo.lock:3294-3295` (`rustls 0.21.12`, reached via `tikv-client 0.4.0` → `tonic 0.10.2`): the only gating red, C4-ci, is `cargo deny --all-features … check advisories` failing on RUSTSEC-2026-0346 and RUSTSEC-2026-0349 (`gate-logs/C4-ci.log:6473-6551`). The default-feature `cargo deny check` passes (`:6470-6471`), and fmt, clippy, build, the full test run (including this file, `:2698-2706`), typos and the docs checks are all green. The diff does not touch `Cargo.lock`, `Cargo.toml` or either `deny*.toml`, so this red is pre-existing on the base and not evidence against the fix. Fixing it means bumping the TiKV client stack or adding an advisory exception, which is outside this slice. A human has to decide whether to accept with the baseline red or wait for a separate dependency fix.
- Observation, not a finding: the gating T4 review row reports "0 blocking" in 9.25 s (`gate-logs/T4-batch-review.log`), and its log is a single summary line. I could not tell from the frozen evidence whether three review passes ran fresh against this diff. Its result does match what I found: last round's six blocking items are addressed in the patch.

<!-- pdca:leaf-complete -->

### Advisory — code-review

No introduced production correctness bugs found. The frozen red→green evidence confirms that the new wire test detects the original fractional-second mismatch.

- NEEDS-HUMAN [impl] — Optional reuse/simplification at `crates/server/tests/s3_last_modified_agreement.rs:477`: the new calendar conversion and two date parsers duplicate functionality available in `aws-smithy-types`, already a direct dev-dependency (`crates/server/Cargo.toml:134`). Use its `DateTime` parsing for the ISO-8601 and HTTP-date strings while retaining the exact wire-string assertions at `crates/server/tests/s3_last_modified_agreement.rs:842`. This preserves an oracle independent of the production formatter and removes roughly 100 lines of custom date-parsing logic without adding a dependency.

<!-- pdca:leaf-complete -->

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C4 Verification (red→green) — Resolve the baseline Rustls advisory-gate disposition and obtain complete CI evidence — independent red→green passed, but frozen CI fails on unchanged dependencies and local CI stops on a read-only advisory lock before later checks; `reviewer-green.log:12`, `gate-logs/C4-ci.log:6473`, `reviewer-ci.log:3263`, `Cargo.lock:3294`.
- [ ] Validation — fitness-to-purpose — Accept whole-second wire timestamps as the intended contract for sync/revalidation workflows — live S3 and the shipping gateway agree in the exercised cases, while product fitness remains the maintainer's sign-off decision; `reviewer-live-s3.log:5`, `crates/server/tests/s3_last_modified_agreement.rs:840`, `docs/design/architecture/05-building-block-view.md:194`.
- [ ] C4-ci — Frozen attempts stop at all-features advisories RUSTSEC-2026-0346 and RUSTSEC-2026-0349 in Rustls 0.21.12 through TiKV; local CI passes through cargo-machete, then cannot lock the advisory database. Neither run reaches the subsequent conformance/statics/deployment-guard/DST sequence; `gate-logs/C4-ci.log:6480`, `gate-logs/C4-ci.log:6510`, `reviewer-ci.log:3257`, `reviewer-ci.log:3263`, `xtask/src/main.rs:1683`.
- [ ] `Cargo.lock:3294-3295` (`rustls 0.21.12`, reached via `tikv-client 0.4.0` → `tonic 0.10.2`): the only gating red, C4-ci, is `cargo deny --all-features … check advisories` failing on RUSTSEC-2026-0346 and RUSTSEC-2026-0349 (`gate-logs/C4-ci.log:6473-6551`). The default-feature `cargo deny check` passes (`:6470-6471`), and fmt, clippy, build, the full test run (including this file, `:2698-2706`), typos and the docs checks are all green. The diff does not touch `Cargo.lock`, `Cargo.toml` or either `deny*.toml`, so this red is pre-existing on the base and not evidence against the fix. Fixing it means bumping the TiKV client stack or adding an advisory exception, which is outside this slice. A human has to decide whether to accept with the baseline red or wait for a separate dependency fix.
- [ ] Optional reuse/simplification at `crates/server/tests/s3_last_modified_agreement.rs:477`: the new calendar conversion and two date parsers duplicate functionality available in `aws-smithy-types`, already a direct dev-dependency (`crates/server/Cargo.toml:134`). Use its `DateTime` parsing for the ISO-8601 and HTTP-date strings while retaining the exact wire-string assertions at `crates/server/tests/s3_last_modified_agreement.rs:842`. This preserves an oracle independent of the production formatter and removes roughly 100 lines of custom date-parsing logic without adding a dependency.
- [ ] C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance) FAILED (gating) — xtask: `cargo deny --all-features --config deny-all-features.toml check advisories` failed with exit status: 1
- [ ] The fix is selected before resolving the tracker’s explicit decision point. `brief.md:22-28` concludes S3 listings and headers “agree by construction” from a `.000Z` example and makes a live check optional; that conclusion also determines the expected output in `brief.md:29-37`. But `notes.json` → `body`, “To confirm before choosing a fix”, says “That should be verified against the real service rather than assumed, because it decides the fix” and explicitly allows a documentation-only resolution if S3 emits sub-second values. The target’s cited “shape S3 uses” comment (`crates/gateway-s3/src/lib.rs:615-617`) accompanies code that retains the millisecond remainder (`:627`, `:636`); it establishes a format, not an always-zero precision contract. The proposed test can prove Wyrd truncates, but cannot establish that truncation is the right compatibility behavior. Revise the brief to supply the requested service evidence or leave the behavioral choice explicitly for human adjudication, without presenting S3 agreement as established.
- [ ] The scope excludes documentation that the target repository requires for this behavior change. `brief.md:65-70` rules out “any ADR or `docs/` edit”, while the proposed ListObjects response changes `.750Z` to `.000Z` (`brief.md:29-37`). Target `AGENTS.md:154-157` requires a change that “adds or alters … an API operation” to update the living architecture doc in the same PR. Formatter comments alone do not meet that instruction. Revise the scope to allow the minimal living architecture update for the chosen timestamp semantics; accepted ADRs can remain untouched.
- [ ] `gate-logs/C4-ci.log:3471-3549`: the only C4-ci red is `cargo deny … check advisories`, on RUSTSEC-2026-0346 and RUSTSEC-2026-0349 against `rustls 0.21.12` (via `tokio-rustls 0.24.1`). That dependency comes from the base. This diff does not touch `Cargo.toml` or `Cargo.lock`, and every test step passed in both attempts. The red is real and gating, but it is not evidence against this fix. It needs its own dependency bump issue, or a decision on whether a base-wide advisory blocks unrelated PRs.

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
- Iteration delta (if iterating): Auto-iterate (round 2): rebuilding for the implementation-level findings — C4 Verification (red→green) — Resolve the baseline Rustls advisory-gate disposition and obtain complete CI evidence — independent red→green passed, but frozen CI fails on unchanged dependencies and local CI stops on a read-only advisory lock before later checks; `reviewer-green.log:12`, `gate-logs/C4-ci.log:6473`, `reviewer-ci.log:3263`, `Cargo.lock:3294`.; C4-ci — Frozen attempts stop at all-features advisories RUSTSEC-2026-0346 and RUSTSEC-2026-0349 in Rustls 0.21.12 through TiKV; local CI passes through cargo-machete, then cannot lock the advisory database. Neither run reaches the subsequent conformance/statics/deployment-guard/DST sequence; `gate-logs/C4-ci.log:6480`, `gate-logs/C4-ci.log:6510`, `reviewer-ci.log:3257`, `reviewer-ci.log:3263`, `xtask/src/main.rs:1683`.; Optional reuse/simplification at `crates/server/tests/s3_last_modified_agreement.rs:477`: the new calendar conversion and two date parsers duplicate functionality available in `aws-smithy-types`, already a direct dev-dependency (`crates/server/Cargo.toml:134`). Use its `DateTime` parsing for the ISO-8601 and HTTP-date strings while retaining the exact wire-string assertions at `crates/server/tests/s3_last_modified_agreement.rs:842`. This preserves an oracle independent of the production formatter and removes roughly 100 lines of custom date-parsing logic without adding a dependency.; C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance) FAILED (gating) — xtask: `cargo deny --all-features --config deny-all-features.toml check advisories` failed with exit status: 1. Deferred, not addressed here: 3 finding(s) that need human judgment, held in deferred-findings.json for the human at handover — not build work
- By / date: auto-iterate / 2026-10-14

## 10. Act candidates (hints for the next Act review)
- Plan advisory: 2 finding(s); brief revised: yes (plan-advisory-*.md)
- (empty is the common case)
