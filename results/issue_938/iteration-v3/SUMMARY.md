# Result — issue 938 / validate-s3-client-smoke-verbs

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect: the client can only PUT, GET and DELETE one object
  (`crates/validate/src/s3.rs:229`, `:281`, `:325` on the integration line). Smoke (743.3) also
  needs HEAD; a ranged GET; a GET with a given `If-None-Match`; ListObjectsV2 and v1 under a
  prefix; bulk DeleteObjects; and one request for each operation the matrix declares
  unsupported. Each must report what the server actually sent (status, the headers checks
  read, the S3 error code, the body) with #853's bounded, typed reading. And a 2xx head must
  never be lost: when a body fails after a 2xx head, the report still carries the 2xx, or a
  half-working unsupported operation reads as "no answer" instead of `unexpected-support` (the
  Check finding at the first attempt's `s3.rs:610`).
- Success criterion: the NEW file `crates/validate/tests/s3_client_smoke_verbs.rs` passes,
  against an in-process Wyrd gateway and, for the fault legs, a scripted endpoint or relay
  (peers under *Citations*):
  - **(A) The verbs against the real gateway.** HEAD of a written key → 200 with its size and
    `ETag`; of a missing key → 404 with no body (`ErrorCode::NoBody`). Ranged GET
    `bytes=a-b` → 206, the `Content-Range` header as sent, exactly those bytes. GET with
    `If-None-Match: <its ETag>` → 304, no body; with a different valid tag → 200 and the whole
    body. ListObjectsV2 and v1 under a prefix → every key with its size, across more than one
    page (the client follows the continuation). DeleteObjects of N existing keys → every key
    reported under `<Deleted>`, no `<Error>` row; the keys are then gone (HEAD 404). This is
    the success case only; error rows are (F)'s, because the gateway cannot be made to fail one
    key on demand.
  - **(B) Every unsupported operation reaches the gateway** and is reported with status and
    S3 code: `CopyObject` 501 `NotImplemented`; `CreateMultipartUpload`, `UploadPart`,
    `CompleteMultipartUpload`, `AbortMultipartUpload`, `ListParts`, `ListMultipartUploads`
    501; `GetBucketLocation` 501; `CreateBucket`, `DeleteBucket`, `ListBuckets` 400
    `InvalidRequest`; `HeadBucket` 400 with no body. `CreateBucket` and `DeleteBucket` naming
    a bucket S3's naming rules forbid (for example one ending in `-`) are **sent and reach the
    gateway**, never refused by the client: 743.3's bucket-safety rule depends on it.
  - **(C) A 2xx head survives its body.** A scripted endpoint answers an unsupported operation
    with a `200` head and then (i) stalls the body past a short body deadline
    (`S3Client::with_deadlines`), (ii) cuts the connection mid-body, (iii) sends a body that is
    not a readable document. In each case the report carries status 200 and the body failure,
    never only a timeout or transport error.
  - **(D) Unreadable answers are refused for the new verbs as #853 refuses them for the old.**
    A listing or `<DeleteResult>` document cut short, followed by junk, or past the buffered
    budget (`BUFFERED_BODY_BUDGET`) is a typed refusal, never a partial listing or a short
    delete result.
  - **(E)** `s3_client_roundtrip.rs`, `s3_client_upload_peers.rs` and
    `s3_client_nonconforming_responses.rs` pass unmodified (the third is #853's; it exists on
    the base this slice must build on, see the ordering note).
  - **(F) A mixed `<DeleteResult>` is reported row by row** (added 2026-10-22). The gateway
    answers `200` even when some keys failed (`crates/gateway-s3/src/lib.rs:2293-2300`), so
    the error rows are the only evidence of a failed key. A scripted endpoint answers
    `POST ?delete` with `200` and a `<DeleteResult>` holding two `<Deleted>` rows and one
    `<Error>` row (`<Key>`, `<Code>AccessDenied</Code>`, `<Message>`). The result lists
    exactly the two deleted keys and exactly one error row carrying that key, that code and
    that message as sent, in document order. A client that drops or empties the error list
    fails this leg.
- Repo + branch target: getwyrd/wyrd @ main
- Scope: the new verbs and their typed results in `crates/validate/src/s3.rs` and
  `src/s3/*`; `wyrd-traits` (or whatever the bucket-seeding peer needs) under
  `[dev-dependencies]` in `crates/validate/Cargo.toml`; the new test file. / out of scope:
  - judging answers (743.1); the runner, ledger, version source and arguments (743.3);
    `lib.rs`;
  - the multipart success path (#508, #750); changing the existing verbs' behaviour;
  - any `wyrd-*` crate in `[dependencies]`; any new production dependency (`roxmltree` is
    already one); `deny*.toml`.

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: new-feature
- Confidence: medium
- Recommendation: (set by Do)

## 3. Correctness (Check — chain)
- C1 Spec: none — brief.md
- C2 Reproduction (red pre-fix): none — (no gate configured)
- C3 Change: none — patch.diff
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): fail —   - cargo deny --all-features --config deny-all-features.toml check advisories: exited with exit status: 1
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: unverifiable —                why this slice has no isolable red (the cargo output is above).
- C4 diff coverage: changed lines executed by the patch's tests: fail — diff coverage not measured — patch.diff does not apply on origin/main
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): fail — 130 mutants tested in 23m: 2 missed, 83 caught, 42 unviable, 3 timeouts

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): pass — review-branch: 0 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_938/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 13.43s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Review #938: add the S3 verbs needed by smoke, preserving server responses and keeping bucket probes on the configured endpoint.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | Acceptance legs A–F and the approved routing correction define observable results and preserve the separate runner/judging scope (`brief.md:17`, `brief.md:169`; `crates/validate/tests/s3_client_smoke_verbs.rs:18`). |
| C2 Reproduction (red pre-fix) | N/A | The declared net-new exception applies: stashing production changes while retaining the test produced missing-API compile errors, not an executed red test (`brief.md:98`; `reviewer-red.log:3`; `gate-logs/C4-verify.log:420`). |
| C3 Change | PASS | The diff stays within the authorized client, tests, dev-dependency and living-doc surfaces; the shared `lib.rs` and production dependency boundary remain intact (`crates/validate/Cargo.toml:55`; `docs/design/architecture/05-building-block-view.md:299`; `patch.diff:1`). |
| C4 Verification (red→green) | NEEDS-HUMAN | Assign separate remediation of the existing dependency advisory before claiming full CI green — both advisory checks reproduce RUSTSEC-2026-0385 on unchanged `ctutils 0.4.2`, despite 78 passing client tests (`Cargo.lock:1000`; `reviewer-deny-copied-cache.log:13`; `reviewer-state.log:9`). |
| C5 Causal adequacy | PASS | The observable failures are addressed at response capture and endpoint resolution; live tests preserve 200 alongside body failures and prove exact probe routing, without a capability-presence fallback (`crates/validate/src/s3/answer.rs:346`; `crates/validate/src/s3.rs:504`; `crates/validate/tests/s3_client_smoke_verbs.rs:893`, `:1472`). |
| T1 Structure | PASS | Production remains a blackbox client with shared framing/XML validation; Wyrd dependencies used to construct the real test gateway stay under dev-dependencies (`crates/validate/Cargo.toml:55`; `crates/validate/src/s3/document.rs:16`; `crates/validate/src/s3/response.rs:459`). |
| T2 Shape | PASS | Explicit answer, body-failure and ordered delete-row types preserve uncertainty and partial per-key failure for the caller to judge; no additional scope split is warranted under the recorded decision (`crates/validate/src/s3/answer.rs:133`; `crates/validate/src/s3/document.rs:76`; `brief.md:169`). |
| T3 Runtime | PASS | Executed fault and pagination tests support bounded waits, bounded buffering and finite continuation; request deadlines use the documented Tokio clock (`crates/validate/src/s3/answer.rs:519`; `crates/validate/src/s3/document.rs:22`; `crates/validate/tests/s3_client_smoke_verbs.rs:2131`; `reviewer-restored-green.log:28`). |
| T4 Contribution | N/A | Contribution-artifact adjudication belongs to the mandatory publish rerun: the artifacts are absent by design, so the deferred row is neither green nor a human blocker (`gate-logs/T4-contribution.log:10`); the other T4 evidence is assessed below. |
| T5 Judgment | NEEDS-HUMAN | Confirm the completeness of the recorded path-based prior-art search — the brief names merged/open/closed and rejected work, but the supplied one-commit target has no remote or historical search evidence with which to independently settle completeness (`brief.md:124`; `reviewer-state.log:3`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Accept this client contract for #939's smoke consumer, including caller-owned bucket safety and finite listing/body limits — the client tests exercise the promised behavior, while end-to-end runner fitness is outside this slice (`brief.md:106`; `crates/validate/src/s3/verbs.rs:31`; `crates/validate/src/s3/document.rs:22`). |

No patch-specific implementation defect was found. The client behavior passed independent execution; the full CI result remains red because of an existing dependency advisory. Prior-art completeness and fitness for the subsequent smoke runner remain human decisions.

Source citations above and below refer to the applied tree at `$PDCA_TARGET`; brief, patch, gate-log and reviewer-log citations refer to this bundle directory. The disposable target contains prerequisite #853 and matches the patch: reverse-application checking passed. Its synthetic base records `93a6f3715e8785a22fd1d1ab40206bcf51b3eef4`. Production was restored after the stash experiment, with no outstanding stash or reviewer source changes (`reviewer-state.log:2`, `:5`, `:7`).

- **Independent execution supports the promised client behavior.** Ran `cargo test -p wyrd-validate --test s3_client_smoke_verbs --test s3_client_roundtrip --test s3_client_upload_peers --test s3_client_nonconforming_responses`: 23 + 15 + 12 + 28 tests passed (`reviewer-green.log:292`, `:313`, `:342`, `:360`). With production stashed and the new test retained, compilation failed with missing new symbols; after `git stash pop`, all 23 smoke tests passed again (`reviewer-red.log:3`; `reviewer-restored-green.log:31`). This establishes green behavior and reproduces the declared absence of an executable pre-fix discriminator, not a runtime red→green. The supplied logs do not expose the five individually named handcrafted mutation traces required by `brief.md:101`; the aggregate mutation result is not represented here as those traces.

- **The requested carry-forward cases are exercised, including real routing.** The gateway/relay test checks one request, exact path, configured host, S3 signing and gateway status for both IP and `localhost` endpoints, including host-safe/non-host-safe S3 Express names, Outposts-shaped aliases and ARN-shaped names (`crates/validate/tests/s3_client_smoke_verbs.rs:893`). Tests also cover status retention after stalled/cut/unreadable bodies, typed errors inside a 200, mixed delete rows, non-last-key continuation markers, repeated headers, strict decimal sizes and the page cap (`crates/validate/tests/s3_client_smoke_verbs.rs:1472`, `:1944`, `:2004`, `:2083`, `:2131`, `:2358`, `:2433`). The resolver removes the unwanted SDK routing cause; disabling session authentication is configuration, not the symptom-guard capability-probe pattern.

- **The remaining gate failures have bounded interpretations.** Independent `cargo fmt --all -- --check`, `cargo clippy -p wyrd-validate --all-targets -- -D warnings`, `typos`, docs lint and docs rendering/link audit passed. Both declared external tools were actually exercised; rendering produced 99 pages (`reviewer-docs-lint.log:1`; `reviewer-docs-render.log:2`). Initial offline cargo-deny attempts encountered a read-only cache lock. Copying that cache into this sandbox and changing only the scratch configs' `db-path` allowed the real scanner to run: default and all-feature advisory checks both exited 1 with the same RUSTSEC advisory as frozen CI; default bans, licenses and sources passed (`reviewer-deny-copied-cache.log:13`; `reviewer-deny-all-features-copied-cache.log:7`; `gate-logs/C4-ci.log:7932`). The `ctutils` package block is identical before and after this patch (`Cargo.lock:1000`; `reviewer-state.log:9`), so this is an existing dependency-gate problem, not a new client defect. The frozen coverage gate measured no coverage because its `origin/main` base rejected the stacked patch; that result is a base-state caveat, not a compile/applicability finding against the supplied working target (`gate-logs/C4-diff-cov.log:10`). The mutation log records 83 caught, 42 unviable, three timed-out endpoint mutations and two missed diagnostic interceptor-name substitutions; those two change only the diagnostic label, not client behavior (`gate-logs/C5-mutants.log:13`; `crates/validate/src/s3/answer.rs:454`). Mutation/coverage and full workspace wrappers were not independently rerun; their frozen evidence was assessed without upgrading missing measurements to passes.

- **Contribution evidence and prior art have different limits.** The frozen batched-review log reports zero blocking findings, but contains only its summary; the TiKV log shows actual compilation of both required feature selections, ending successfully (`gate-logs/T4-batch-review.log:10`; `gate-logs/host-tikv.log:111`, `:209`). Neither was independently rerun. The contribution audit remains N/A until publish. The brief's prior-art record explicitly searches the affected S3 paths and names merged #859/#865, open prerequisite #934, no closed-unmerged PRs, and the rejected attempt's status-retention/bucket-safety decisions (`brief.md:124`). Those named failure modes are covered here; search completeness cannot be reconstructed from a target containing one synthetic commit and no remote (`reviewer-state.log:3`). No `INTEGRATION.md` was present in the supplied target, so no additional project-specific human-only list was available (`reviewer-state.log:10`).

<!-- pdca:leaf-complete -->

### Advisory — adversary

# Adversarial review — #938 (743.2: the S3 client's smoke verbs)

**Verdict: I tried to refute the fix and could not.** I rebuilt the patched tree in a scratch
copy and ran the new test (23 of 23 pass). Then I took the fix apart five ways; each time the
test went red. Every attack input I tried on the production code came back reported as the
server sent it. The only gating red is environmental (see the one NEEDS-HUMAN item).

## The evidence (red→green, re-run by me)

- C4-verify is UNVERIFIABLE (exit 77). The brief declared that in advance: the test calls verbs
  the base does not have, so it cannot compile without the fix. I reproduced the five reds the
  brief demands by removing pieces of the fix in a scratch copy:
  1. Return only the timeout when a 2xx body stalls (`crates/validate/src/s3/verbs.rs:381`): the
     (C) test fails for every probe, e.g. `CopyObject Stall: the 200 head was lost: Timeout { phase: BodyIdle, limit: 500ms }`.
  2. Accept a listing body that is not a whole document as an empty page
     (`crates/validate/src/s3/document.rs:144-145`): (D) fails, e.g. `ListV2 cut-short: not refused as Unreadable: Listed([])`.
  3. Put the SDK's own endpoint rules (`DefaultResolver`) back in place of `PathStyle`
     (`crates/validate/src/s3.rs:186`): 4 tests fail, including
     `bucket_probes_send_every_name_as_one_path_segment_on_an_ip_or_a_host_name_endpoint`.
  4. Stop after the first page (`crates/validate/src/s3/verbs.rs:227`, `:261`): the real-gateway
     listing test, the v1 marker test and the page-cap test fail.
  5. Drop the `<Error>` rows (`crates/validate/src/s3/document.rs:211`):
     `a_mixed_delete_result_is_reported_row_by_row_in_document_order` fails.
- The test runs the production code, not a copy of it. It builds `S3Client` through the
  production argument parser (`crates/validate/tests/s3_client_smoke_verbs.rs:226-251`) and
  drives it against the real in-process gateway and a scripted loopback endpoint. Its
  assertions are exact (whole key/size lists, row-by-row equality, status plus request id), not
  counts that could pass for the wrong reason.

## Attacks on the fix that held

- HEAD and HeadBucket answered `200` with stray body bytes (after `Content-Length: 5`, after
  `Content-Length: 0`, or a chunked body): reported as `BodyFailed(Overrun { after: 5 / 15 })`
  beside the 200, never as a clean success (`crates/validate/src/s3/answer.rs:541-545`).
- A listing page that holds a whole document but declares 50 more bytes, then the connection is
  cut: page 1 is `BodyFailed(Transport { received: 440 })` beside the 200, never a listing.
- Odd bucket names: `a/b`, `a?b`, `a#b`, `%41`, ` x `, `é`, a tab, NUL, `../validate`. Each
  reached the endpoint as exactly one percent-encoded segment (`DELETE /..%2Fvalidate/`,
  `/%2541/`, `/%20x%20/`): nothing trimmed, nothing rewritten, and no name escapes into the
  client's own bucket (`crates/validate/src/s3.rs:544-556`).
- Losing the head between SDK hooks: `read_after_transmit` and `modify_before_deserialization`
  run in the same poll, with no await between them, and the body is read after `send()`
  returns (`crates/validate/src/s3/answer.rs:11-18`, `:503`). So the SDK's operation deadline
  cannot fire mid-body and erase the 2xx.
- An endpoint that answers every page with another continuation: bounded at
  `MAX_LISTING_PAGES` (`crates/validate/src/s3/document.rs:22`), and tested.

## Gates and claims

- NEEDS-HUMAN [human] — The gating C4-ci red is environmental, not this patch. `cargo deny`
  fails on RUSTSEC-2026-0385 in `ctutils` 0.4.2, which is already on the base
  (`Cargo.lock:1000-1002` at the base commit). It comes in through `hmac`/`sha2` from
  `aws-sdk-s3`, `wyrd-core` and `wyrd-gateway-s3`. The patch's only `Cargo.lock` change adds
  `wyrd-traits` to `wyrd-validate`'s list. The fix (`cargo update -p ctutils` to >= 0.4.4) is
  outside this slice's scope. Decide: bump the lockfile separately on `main`, or accept this red
  as not caused by the patch. This is not a refutation of the fix.
- C4-diff-cov "patch.diff does not apply on origin/main" is expected. The brief builds this
  slice on #853's integration line, not `main`. Not a refutation.
- C5: the 2 missed mutants are both `Capture::name` (`crates/validate/src/s3/answer.rs:455`),
  the interceptor's label string, which nothing reads. The 3 timeouts
  (`crates/validate/src/s3.rs:505`, `:510`) make every endpoint refused. The scripted-endpoint
  tests then wait out `Endpoint::heads()`'s 20 s bound
  (`crates/validate/tests/s3_client_smoke_verbs.rs:1244-1251`) for requests that never come, so
  the run passes 61 s instead of failing fast. Those mutants did not survive. Not raised.
- I found no unwarranted claim in `check-gates.json`. The T4 batch review's "0 blocking" matches
  what I found.

## Notes (not routed; no action needed in this slice)

- A 2xx body with no `Content-Length` and no `Transfer-Encoding` ends where the connection
  closes, and the client accepts it for documents and empty successes
  (`crates/validate/src/s3/answer.rs:497-502`). A cut document is still refused by the strict
  read. But a headers-only success (CreateBucket, DeleteBucket, UploadPart) cut before its first
  body byte cannot be told from an empty success. This is documented and built into HTTP/1.1
  (the GET object path refuses such framing instead). Not a defect.
- For 743.1: a `<DeleteResult>` that leaves a requested key out of both lists comes back as
  `Ok`. I sent `<DeleteResult/>` for two keys and got `Ok(DeleteResult { rows: [] })`. The brief
  makes judging 743.1's job (`crates/validate/src/s3/document.rs:9-10`). The rubric's
  "absent entries" class applies there: compare the requested keys with the rows, never just
  `errors().count() == 0`.
- Doc wording: `docs/design/architecture/05-building-block-view.md:299` says a probe "can name a
  bucket those rules forbid, and so cannot create or delete one it does not own". The client
  does not enforce that. `Probe::DeleteBucket { bucket }` sends any name
  (`crates/validate/src/s3/verbs.rs:71`, `:377`), so `"validate"` sends `DELETE /validate/`. The
  safety comes from 743.3 picking a forbidden name and the server refusing it, as
  `crates/validate/src/s3/verbs.rs:36-37` puts it. Worth rewording if the doc is touched again;
  not worth a round on its own.

Attempted to refute: (C) keeping the 2xx head, (D) strict listing and delete-result reading, (B)
bucket-name routing on IP and host-name endpoints, (A) following pages, (F) row order, and
RequestNotBuilt vs NoResponse. Could not.

<!-- pdca:leaf-complete -->

### Advisory — code-review

No findings on either advisory lens: no correctness bugs introduced by this diff and no actionable reuse, simplification, or efficiency issues identified.

Reviewed endpoint routing (`crates/validate/src/s3.rs:504`), response capture and classification (`crates/validate/src/s3/verbs.rs:386`), bounded body reading (`crates/validate/src/s3/answer.rs:503`), and listing/delete parsing with shared document validation (`crates/validate/src/s3/document.rs:143`, `crates/validate/src/s3/response.rs:463`).

Validation evidence was read from the frozen gate logs; no builds were rerun. Those logs record all 23 new smoke-verb tests and the three existing S3 client suites passing. The two surviving mutations change only the interceptor's diagnostic name (`crates/validate/src/s3/answer.rs:455`). This advisory does not override the gate results.

<!-- pdca:leaf-complete -->

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C4 Verification (red→green) — Assign separate remediation of the existing dependency advisory before claiming full CI green — both advisory checks reproduce RUSTSEC-2026-0385 on unchanged `ctutils 0.4.2`, despite 78 passing client tests (`Cargo.lock:1000`; `reviewer-deny-copied-cache.log:13`; `reviewer-state.log:9`).
- [ ] T5 Judgment — Confirm the completeness of the recorded path-based prior-art search — the brief names merged/open/closed and rejected work, but the supplied one-commit target has no remote or historical search evidence with which to independently settle completeness (`brief.md:124`; `reviewer-state.log:3`).
- [ ] Validation — fitness-to-purpose — Accept this client contract for #939's smoke consumer, including caller-owned bucket safety and finite listing/body limits — the client tests exercise the promised behavior, while end-to-end runner fitness is outside this slice (`brief.md:106`; `crates/validate/src/s3/verbs.rs:31`; `crates/validate/src/s3/document.rs:22`).
- [ ] The gating C4-ci red is environmental, not this patch. `cargo deny` fails on RUSTSEC-2026-0385 in `ctutils` 0.4.2, which is already on the base (`Cargo.lock:1000-1002` at the base commit). It comes in through `hmac`/`sha2` from `aws-sdk-s3`, `wyrd-core` and `wyrd-gateway-s3`. The patch's only `Cargo.lock` change adds `wyrd-traits` to `wyrd-validate`'s list. The fix (`cargo update -p ctutils` to >= 0.4.4) is outside this slice's scope. Decide: bump the lockfile separately on `main`, or accept this red as not caused by the patch. This is not a refutation of the fix.
- [ ] C4 per-fix red->green: this patch's test red pre-fix, green post-fix unverifiable —                why this slice has no isolable red (the cargo output is above).
- [ ] C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance) FAILED (gating) —   - cargo deny --all-features --config deny-all-features.toml check advisories: exited with exit status: 1
- [ ] **The bulk-delete criterion can pass without exercising error rows.** `brief.md:27-28` requires `<Deleted>` and `<Error>` rows “exactly as the document carried them” followed by “the keys are then gone,” but specifies no mixed-result fixture. On the target, `crates/gateway-s3/src/lib.rs:2237-2240,2260-2272` distinguishes successful deletes from per-key failures while still returning HTTP 200; the existing happy-path test explicitly expects zero errors (`crates/server/tests/s3_delete_objects.rs:448-452`). A client that always returns an empty error list could satisfy the proposed happy-path assertions. Require a scripted mixed `<DeleteResult>` with exact key/code/message assertions and a demonstrated red when an error row is dropped; restrict the disappearance assertion to the successful real-gateway case.
- [ ] **The prerequisite exists in batch metadata but is absent from the supplied source base.** `brief.md:55-59` targets `getwyrd/wyrd @ main` and asserts #853 is “on the integration line”; `dependency-state.json:2-5` confirms an existing COMPLETE bundle. The target remote resolves to that repository, and HEAD/main/origin/main resolve to `4bac53878edfecfea34ad849edbe010f2d79d1db`, but target `crates/validate/src/s3.rs:264-268,313-317` still explicitly defers response hardening to #853. The cited `s3/response.rs`, `s3/wire.rs`, and `s3_client_nonconforming_responses.rs` are absent. Also, `roxmltree` is only a dev-dependency (`crates/validate/Cargo.toml:42-43`), contradicting `brief.md:68-69`'s production-dependency assumption. Reconcile the target/stack instructions with an actual #853-containing base and refresh citations there. Otherwise Do must recreate prerequisite work outside the declared scope, and criterion (E)'s “pass unmodified” names a nonexistent test.
- [ ] **The claimed tracker history cannot be checked from the supplied inputs.** Neither `notes.json` nor `sources/` exists in this sandbox. In particular, `brief.md:99-103` says the rejected attempt “had one finding,” and `brief.md:33-35` attributes forbidden-bucket requests to 743.3's safety rule, without supplying either underlying record. This is an evidence gap, not proof those claims are false. Include the relevant tracker/review excerpts and source identifiers in the revised brief so the scope split, prior failure, and safety constraint can be independently checked.
- [ ] size backstop — this slice is behaving oversized: patch is 203 KB (threshold 100 KB). Recommend answering `iterate-plan` at sign-off and authoring the split in the re-plan (`pdca split`), rather than `iterate-do`: a slice that is too big yields implementation-shaped findings every round, and splitting authors briefs, which is Plan's beat.

## 7. Proven / not proven
- Proven by which oracle: gates overall = fail (stub oracles).
- Unproven / needs manual run: anything flagged in §6.

## 8. Ready-to-ship attachments
- patch.diff
- tracker-comment.md     (ALWAYS, every tracker item)
- build-notes.md         (builder rationale — for the human, not the reviewer)

## 9. Check sign-off                     ← human completes Check here
- Disposition confirmed / overridden:
- Outcome: iterated-to-Plan
- Iteration delta (if iterating): The slice is too big, not wrong: the patch grew 131 KB -> 192 KB -> 208 KB over three rounds against a 100 KB limit (about 1,650 production lines across answer.rs, verbs.rs, document.rs, s3.rs plus about 2,500 test lines). The reviewer found no defect in the patch, and the code is worth keeping as the starting point for the children. Split 743.2 in Plan (pdca split) into smaller verb slices, for example reads and listing vs. deletes and bucket probes; the exact split is decided in Plan. Each child keeps the brief's existing constraints: it runs in parallel with #937 (743.1) and does not depend on it, it does not touch crates/validate/src/lib.rs, and #939 (743.3) consumes all of them. The C4-ci red was only the base advisory wall, now cleared on main by #950 (ctutils) and #954 (tokio); ../wyrd is at b552ca2 and both advisory checks pass. Base the children on that.
- By / date: Eduard Ralph / 2026-10-19

## 10. Act candidates (hints for the next Act review)
- Plan advisory: 3 finding(s); brief revised: yes (plan-advisory-*.md)
- (empty is the common case)
