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
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): pass — xtask ci: all checks passed
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: unverifiable —                why this slice has no isolable red (the cargo output is above).
- C4 diff coverage: changed lines executed by the patch's tests: fail — diff coverage not measured — patch.diff does not apply on origin/main
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): fail — 124 mutants tested in 21m: 2 missed, 80 caught, 42 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): pass — review-branch: 0 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_938/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 17.89s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

No actionable defect found in #938’s S3 smoke-client extension: HEAD, ranged/conditional GET, paginated listings, bulk deletion, and unsupported-operation probes preserve response evidence; fitness-to-purpose remains a human decision.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | Criteria A–F distinguish successful requests, malformed answers, retained 2xx heads, and mixed delete results, with explicit scope and prerequisite constraints (`brief.md:23`, `brief.md:86`). |
| C2 Reproduction (red pre-fix) | N/A | These APIs are new: stashing production while retaining the test reproduces missing-symbol compilation errors, not an executed behavioral RED, as predeclared (`brief.md:98`, `reviewer-red-green.log:6`). |
| C3 Change | PASS | The change stays within the authorized client/test/documentation surface and adds only a development dependency; the downstream runner and judgment layer remain separate (`crates/validate/Cargo.toml:52`, `crates/validate/src/s3.rs:80`, `brief.md:157`). |
| C4 Verification (red→green) | PASS | Post-fix behavior is independently confirmed by 76 client tests; CI’s two advisory-cache failures were discharged by successful policy-equivalent scans; behavioral pre-fix RED remains N/A (`reviewer-tests-green.log:292`, `reviewer-ci.log:3303`, `reviewer-deny.log:9`). |
| C5 Causal adequacy | PASS | Capturing the head before body consumption removes the status-loss cause; fault tests preserve 200 beside failures, and six selected behavioral mutants were caught (`crates/validate/src/s3/answer.rs:469`, `crates/validate/tests/s3_client_smoke_verbs.rs:1229`, `reviewer-mutants-capped.log:6`). |
| T1 Structure | PASS | The production dependency boundary remains intact, shared document/framing readers govern the new operations, and no shared mutable global is introduced (`crates/validate/src/s3/response.rs:463`, `crates/validate/src/s3/answer.rs:408`, `reviewer-ci.log:19`). |
| T2 Shape | PASS | Typed results distinguish an absent answer, a received head with a failed body, and per-key delete errors without inventing server evidence (`crates/validate/src/s3/answer.rs:126`, `crates/validate/src/s3/document.rs:69`). |
| T3 Runtime | PASS | Body waits use one Tokio clock and explicit deadlines, buffering and pagination are bounded, and dropped bodies release their connection tasks; adversarial cases execute successfully (`crates/validate/src/s3/answer.rs:514`, `crates/validate/src/s3/document.rs:22`, `crates/validate/src/s3/wire.rs:233`). |
| T4 Contribution | PASS | Living architecture describes the expanded contract and limits; the frozen batch review reports zero blockers; contribution-artifact checking is N/A until its mandatory publish rerun (`docs/design/architecture/05-building-block-view.md:299`, `gate-logs/T4-batch-review.log:10`, `gate-logs/T4-contribution.log:10`). |
| T5 Judgment | PASS | No unresolved implementation or scope concern remains; path-based history confirms the existing client lineage, and all 19 closed-unmerged PR file lists contain no client-path predecessor (`reviewer-prior-art-modules.log:1`, `reviewer-prior-art-files.log:1`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Confirm this bounded client contract is sufficient for #939’s smoke scenario — the downstream runner and classification are outside this slice, so their end-to-end fitness is not established here (`brief.md:89`, `brief.md:106`, `docs/design/architecture/05-building-block-view.md:299`). |

Source citations refer to the supplied `$PDCA_TARGET` tree; evidence citations refer to this review directory. The target contains the #853 prerequisite, and `git apply --reverse --check patch.diff` succeeds. The production stash was restored, no stash remains, and the supplied source changes are intact.

The independent evidence supports the implementation, with these limits:

- **Behavior and causal tests:** `cargo test -p wyrd-validate` with the four named integration-test targets passed 21 new tests plus 28 nonconforming-response, 15 roundtrip, and 12 upload-peer tests (`reviewer-tests-green.log:292`, `:313`, `:340`, `:358`). Stashing production produced 33 missing-API/dependency compilation errors; restoring it yielded 21 passing tests (`reviewer-red-green.log:338`, `:392`). These focused runs used Rust 1.96.1; the full CI rerun also exercised the suites with the repository’s pinned Rust 1.96.0. Selected mutations to S3 Express routing, the body-budget boundary, and decimal grammar were caught (`reviewer-mutants.log:3`). Three additional mutations initially triggered unused-code warnings; rerunning with cargo-mutants’ lint cap caught removal of head capture, dropped delete-error rows, and an empty v2 listing (`reviewer-mutants-capped.log:3`). This is targeted discrimination evidence, not a claim to have reproduced every bespoke experiment mentioned in the brief.
- **Repository checks and dependencies:** the pinned `cargo xtask ci` rerun passed formatting, Clippy, builds, workspace/DST tests, dependency isolation, unused-dependency checks, conformance and repository guards. Its exit was nonzero solely because two advisory scans could not lock the read-only home database (`reviewer-ci.log:3817`, `:3822`, `:4454`). Both scans passed using scratch copies of the configurations differing only in `advisories.db-path`; parsed policy equality was checked (`reviewer-deny.log:1`). These were real scans, with no tool substitution. Both declared external dependencies were exercised: `typos` and the renderer, including its 99-page link audit (`reviewer-ci.log:2`, `:7`).
- **Frozen gate adjudication:** C4-verify remains UNVERIFIABLE for behavioral RED, as declared (`gate-logs/C4-verify.log:347`). C4-diff-cov leaves coverage unmeasured because its patch application against `origin/main` failed; the prerequisite-containing supplied target applies and builds, so this is a base-state caveat, not a compilation defect (`gate-logs/C4-diff-cov.log:10`, `brief.md:71`). C5’s full frozen run reports 80 caught, 42 unviable, and two survivors changing only the interceptor’s diagnostic name; neither survivor changes the client contract (`gate-logs/C5-mutants.log:13`, `crates/validate/src/s3/answer.rs:450`). The TiKV log records successful feature compilation, and the batch-review log records zero blockers (`gate-logs/host-tikv.log:209`, `gate-logs/T4-batch-review.log:10`). **T4-contribution: N/A** — its artifacts are intentionally drafted after Check and its substantive audit must rerun at publish (`gate-logs/T4-contribution.log:10`).
- **Prior art and standing rubric:** GitHub history was queried by `crates/validate/src/s3.rs` and `crates/validate/src/s3/`; changed-file lists were checked for every closed-unmerged PR, with no client-path matches. PR #934 remains open and supplies the prerequisite already present in this target (`reviewer-prior-art-path.log:1`, `reviewer-prior-art-files.log:1`, `reviewer-prior-art-934.log:1`). The rejected-attempt concerns recorded in the brief are covered by retained-head and forbidden-bucket tests (`brief.md:124`, `crates/validate/tests/s3_client_smoke_verbs.rs:817`, `:1229`). The C5 symptom-guard trigger does not fire: early response capture removes the loss mechanism, and endpoint rewriting transforms SDK routing rather than probing for an assumed capability.

<!-- pdca:leaf-complete -->

### Advisory — adversary

# Adversarial review — #938 (743.2), validate S3 client smoke verbs

Bottom line: the core of the fix held up. A 2xx head survives a body that stalls, is cut or can't be read. Listings and delete results are read strictly. Mixed `<DeleteResult>` rows come back in order. I could not break any of that. What I did break is the carried-forward bucket-name item: the "a forbidden name is sent as given, path-style" claim holds only when `--endpoint` is an IP literal. The test can't see this, because every endpoint in it is `http://127.0.0.1:…`. I reproduced both breaks against the production `S3Client` in a scratch build (`aws-sdk-s3` 1.152.0, the version in the target's lock file).

## Refutations

- NEEDS-HUMAN [impl] — **An `--x-s3` / `--xa-s3` bucket name on a hostname endpoint is not sent path-style. A name that can't be a host name is refused before sending.** `WithoutS3Express` (`crates/validate/src/s3.rs:453-494`) keeps whatever URL the SDK's rules pick for an S3 Express name (`s3.rs:461`). For an IP endpoint the rules pick path-style. For a hostname endpoint they pick virtual-host style, even with `force_path_style(true)`. Here is what the real client sent with `--endpoint http://localhost:PORT`. For `Probe::CreateBucket { bucket: "wyrd-validate--x-s3" }`, the request line was `PUT / HTTP/1.1` with `host: wyrd-validate--x-s3.localhost:PORT`: the bucket was not in the path, and the request was addressed to a different host. A path-style gateway sees `PUT /`, which is not CreateBucket, and the probe reports that answer as CreateBucket's. If the host name doesn't resolve, the probe reads as `NoResponse` even though the gateway never saw it. For `Probe::CreateBucket { bucket: "Wyrd_Validate--x-s3" }` the result was `Err(RequestNotBuilt { "S3Express bucket name is not a valid virtual hostable name." })`. That is a forbidden name refused by the client, which brief (B) rules out ("sent and reach the gateway, never refused by the client"). With `http://127.0.0.1:PORT` both went out as `PUT /<name>/`, which is the only case the test checks (`crates/validate/tests/s3_client_smoke_verbs.rs:206-207`, `:816-871`, `:95-109`). The false claims: `crates/validate/src/s3/verbs.rs:35-43` ("reaches the endpoint as given, as the one path segment…", "sent as any other is", "One exception: … `arn:`"), `s3.rs:4-6`, and `docs/design/architecture/05-building-block-view.md:299` ("path-style", "sent as one ordinary request"). The reviewer accepted the sign-off carry-forward ("disable S3 Express session auth … or narrow the claim and name those cases") as closed on the strength of that IP-only test. That acceptance is not warranted. Fix: the client always has `--endpoint` and always forces path-style, so the SDK's rules add nothing here except these reroutes. A resolver that always builds `{endpoint}{path}/{uri-encoded bucket}` (or none, for `ListBuckets`) closes it. Then add a leg on `http://localhost:{port}` with one host-safe and one non-host-safe `--x-s3` name. The other option is to narrow every claim to IP endpoints and name these cases. The name 743.3 actually probes with, `wyrd-validate-probe-`, resolves path-style on both IP and hostname endpoints (checked), so its bucket-safety rule is not affected.

- NEEDS-HUMAN [impl] — **An Outposts-alias-shaped name goes to AWS's URL, even on an IP endpoint. Nothing is sent, but the probe reports `NoResponse` ("sent and got no answer").** Same root cause; the fix above removes it. With `--endpoint http://127.0.0.1:PORT`, the SDK's rules resolve `Probe::CreateBucket { bucket: "eyaccesspoint-b47ue8fiqesrvmvwyrdtdoexample--op-s3" }` (50 characters, ending `--op-s3`) to `https://…--op-s3.ec2.s3-outposts.us-east-1.amazonaws.com/`. The plain-HTTP connector refuses it (`crates/validate/src/s3/wire.rs:103-106`). That happens after `read_before_transmit` has set `transmitted` (`crates/validate/src/s3/answer.rs:453-460`). So `captured()` keeps it as `NoResponse` (`verbs.rs:400-411`), and the real client returned `Err(NoResponse { "…is not an http URL" })`. The gateway saw nothing, and nothing left the host. "Handed to the connector" (`answer.rs:426`) is being read as "sent". That holds for a refused `connect`, but not for the connector's own refusal of the URL. The same shape with an `m` where the SDK expects `e`/`o` gives `RequestNotBuilt { "Unrecognized hardware type…" }`. Classifying that as unsent is correct, but it is a second refusal beyond the only exception the docs name (`arn:`, `verbs.rs:41-43`). In practice nobody probes with such a name. The finding is that the documented split between unsent and unanswered can be wrong.

## Attempted and could not refute

- **(C) a 2xx head survives its body.** The head and framing are taken at `read_after_transmit`, and the body at `modify_before_deserialization`. The orchestrator runs both hook groups in that order, before any deserialization (`aws-smithy-runtime` 1.16.0 `client/orchestrator.rs:507-511`). After that the SDK only ever sees an empty body. `read_whole` (`answer.rs:498-559`) is bounded by both the idle and the whole-body deadline. The test checks the exact phase, limit and `received` count for stall, trickle, cut and unreadable bodies, across all 11 probes that carry a body (`s3_client_smoke_verbs.rs:1229-1318`).
- **(D) strict documents.** Cut documents, trailing junk, a second document, bytes past `Content-Length`, the budget (with a length and chunked), a document of exactly the budget, `<Size>` grammar, a missing or empty V2 token, and the 100-page cap are all refused or read as specified (`document.rs:143-191`, `:251-256`). I found no input that gets a partial listing through.
- **(F) row order, and an `<Error>` row with no `<Key>`.** That row is refused whole, not skipped (`document.rs:211-219`).
- **The evidence.** C4-verify returned UNVERIFIABLE because the test doesn't compile without the new API (`gate-logs/C4-verify.log`). The brief declared this up front for net-new code. C5 is the independent proof that the tests catch real breaks: 80 mutants caught, and the only 2 survivors are `Capture::name` (`answer.rs:449-451`), a display string, which no test could catch. C4-diff-cov never ran because the patch is stacked on #853's line, not `origin/main` (as the brief's ordering note expects). That is not a defect of this diff.
- **Scope.** `lib.rs` is untouched. `wyrd-traits` is only under `[dev-dependencies]` (`crates/validate/Cargo.toml:55-63`). The three existing client tests are unmodified and pass (`gate-logs/C4-ci.log:3232-3330`).
- Two notes I'm not raising as findings. First, ListObjects v1 treats an empty `<NextMarker/>` as absent and continues from the last key (`document.rs:180-183`), while v2 refuses an empty token (`:177-179`). That choice is deliberate and tested (`s3_client_smoke_verbs.rs:1846-1847`). Second, one listing call is bounded only per page: at the default deadlines, 100 pages × (operation + body) could take about 50 h (`s3.rs:125-133`, `document.rs:18-22`). Every wait is bounded, so this meets the rubric, but 743.3 should put its own limit on the whole run.

<!-- pdca:leaf-complete -->

### Advisory — code-review

- No introduced correctness bugs found in the reviewed diff: response-head preservation and bounded body reading (`crates/validate/src/s3/answer.rs:463`, `crates/validate/src/s3/answer.rs:498`), pagination (`crates/validate/src/s3/verbs.rs:201`), and listing/delete-result parsing (`crates/validate/src/s3/document.rs:143`, `crates/validate/src/s3/document.rs:196`).
- No actionable reuse, simplification, or efficiency findings. The new paths share XML validation (`crates/validate/src/s3/response.rs:463`) and reuse the existing streaming object reader (`crates/validate/src/s3/verbs.rs:462`).

Validation evidence: frozen CI passed, including all 21 new smoke tests. The two surviving mutants only change the interceptor's diagnostic name (`crates/validate/src/s3/answer.rs:450`); neither warrants a finding. Diff coverage was not measured because the patch did not apply to origin/main. No local builds or tests were run; the target remained read-only.
<!-- pdca:leaf-complete -->

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] Validation — fitness-to-purpose — Confirm this bounded client contract is sufficient for #939’s smoke scenario — the downstream runner and classification are outside this slice, so their end-to-end fitness is not established here (`brief.md:89`, `brief.md:106`, `docs/design/architecture/05-building-block-view.md:299`).
- [ ] **An `--x-s3` / `--xa-s3` bucket name on a hostname endpoint is not sent path-style. A name that can't be a host name is refused before sending.** `WithoutS3Express` (`crates/validate/src/s3.rs:453-494`) keeps whatever URL the SDK's rules pick for an S3 Express name (`s3.rs:461`). For an IP endpoint the rules pick path-style. For a hostname endpoint they pick virtual-host style, even with `force_path_style(true)`. Here is what the real client sent with `--endpoint http://localhost:PORT`. For `Probe::CreateBucket { bucket: "wyrd-validate--x-s3" }`, the request line was `PUT / HTTP/1.1` with `host: wyrd-validate--x-s3.localhost:PORT`: the bucket was not in the path, and the request was addressed to a different host. A path-style gateway sees `PUT /`, which is not CreateBucket, and the probe reports that answer as CreateBucket's. If the host name doesn't resolve, the probe reads as `NoResponse` even though the gateway never saw it. For `Probe::CreateBucket { bucket: "Wyrd_Validate--x-s3" }` the result was `Err(RequestNotBuilt { "S3Express bucket name is not a valid virtual hostable name." })`. That is a forbidden name refused by the client, which brief (B) rules out ("sent and reach the gateway, never refused by the client"). With `http://127.0.0.1:PORT` both went out as `PUT /<name>/`, which is the only case the test checks (`crates/validate/tests/s3_client_smoke_verbs.rs:206-207`, `:816-871`, `:95-109`). The false claims: `crates/validate/src/s3/verbs.rs:35-43` ("reaches the endpoint as given, as the one path segment…", "sent as any other is", "One exception: … `arn:`"), `s3.rs:4-6`, and `docs/design/architecture/05-building-block-view.md:299` ("path-style", "sent as one ordinary request"). The reviewer accepted the sign-off carry-forward ("disable S3 Express session auth … or narrow the claim and name those cases") as closed on the strength of that IP-only test. That acceptance is not warranted. Fix: the client always has `--endpoint` and always forces path-style, so the SDK's rules add nothing here except these reroutes. A resolver that always builds `{endpoint}{path}/{uri-encoded bucket}` (or none, for `ListBuckets`) closes it. Then add a leg on `http://localhost:{port}` with one host-safe and one non-host-safe `--x-s3` name. The other option is to narrow every claim to IP endpoints and name these cases. The name 743.3 actually probes with, `wyrd-validate-probe-`, resolves path-style on both IP and hostname endpoints (checked), so its bucket-safety rule is not affected.
- [ ] **An Outposts-alias-shaped name goes to AWS's URL, even on an IP endpoint. Nothing is sent, but the probe reports `NoResponse` ("sent and got no answer").** Same root cause; the fix above removes it. With `--endpoint http://127.0.0.1:PORT`, the SDK's rules resolve `Probe::CreateBucket { bucket: "eyaccesspoint-b47ue8fiqesrvmvwyrdtdoexample--op-s3" }` (50 characters, ending `--op-s3`) to `https://…--op-s3.ec2.s3-outposts.us-east-1.amazonaws.com/`. The plain-HTTP connector refuses it (`crates/validate/src/s3/wire.rs:103-106`). That happens after `read_before_transmit` has set `transmitted` (`crates/validate/src/s3/answer.rs:453-460`). So `captured()` keeps it as `NoResponse` (`verbs.rs:400-411`), and the real client returned `Err(NoResponse { "…is not an http URL" })`. The gateway saw nothing, and nothing left the host. "Handed to the connector" (`answer.rs:426`) is being read as "sent". That holds for a refused `connect`, but not for the connector's own refusal of the URL. The same shape with an `m` where the SDK expects `e`/`o` gives `RequestNotBuilt { "Unrecognized hardware type…" }`. Classifying that as unsent is correct, but it is a second refusal beyond the only exception the docs name (`arn:`, `verbs.rs:41-43`). In practice nobody probes with such a name. The finding is that the documented split between unsent and unanswered can be wrong.
- [ ] C4 per-fix red->green: this patch's test red pre-fix, green post-fix unverifiable —                why this slice has no isolable red (the cargo output is above).
- [ ] **The bulk-delete criterion can pass without exercising error rows.** `brief.md:27-28` requires `<Deleted>` and `<Error>` rows “exactly as the document carried them” followed by “the keys are then gone,” but specifies no mixed-result fixture. On the target, `crates/gateway-s3/src/lib.rs:2237-2240,2260-2272` distinguishes successful deletes from per-key failures while still returning HTTP 200; the existing happy-path test explicitly expects zero errors (`crates/server/tests/s3_delete_objects.rs:448-452`). A client that always returns an empty error list could satisfy the proposed happy-path assertions. Require a scripted mixed `<DeleteResult>` with exact key/code/message assertions and a demonstrated red when an error row is dropped; restrict the disappearance assertion to the successful real-gateway case.
- [ ] **The prerequisite exists in batch metadata but is absent from the supplied source base.** `brief.md:55-59` targets `getwyrd/wyrd @ main` and asserts #853 is “on the integration line”; `dependency-state.json:2-5` confirms an existing COMPLETE bundle. The target remote resolves to that repository, and HEAD/main/origin/main resolve to `4bac53878edfecfea34ad849edbe010f2d79d1db`, but target `crates/validate/src/s3.rs:264-268,313-317` still explicitly defers response hardening to #853. The cited `s3/response.rs`, `s3/wire.rs`, and `s3_client_nonconforming_responses.rs` are absent. Also, `roxmltree` is only a dev-dependency (`crates/validate/Cargo.toml:42-43`), contradicting `brief.md:68-69`'s production-dependency assumption. Reconcile the target/stack instructions with an actual #853-containing base and refresh citations there. Otherwise Do must recreate prerequisite work outside the declared scope, and criterion (E)'s “pass unmodified” names a nonexistent test.
- [ ] **The claimed tracker history cannot be checked from the supplied inputs.** Neither `notes.json` nor `sources/` exists in this sandbox. In particular, `brief.md:99-103` says the rejected attempt “had one finding,” and `brief.md:33-35` attributes forbidden-bucket requests to 743.3's safety rule, without supplying either underlying record. This is an evidence gap, not proof those claims are false. Include the relevant tracker/review excerpts and source identifiers in the revised brief so the scope split, prior failure, and safety constraint can be independently checked.
- [ ] size backstop — this slice is behaving oversized: patch is 187 KB (threshold 100 KB). Recommend answering `iterate-plan` at sign-off and authoring the split in the re-plan (`pdca split`), rather than `iterate-do`: a slice that is too big yields implementation-shaped findings every round, and splitting authors briefs, which is Plan's beat.

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
- Iteration delta (if iterating): Keep the attempt-2 design unchanged: retained 2xx heads, strict listing/DeleteResult reading, mixed DeleteResult rows, and the tests that already pass all held under adversarial review. Fix only the bucket-name routing bug and the claims that overstate it: - Bug: WithoutS3Express (crates/validate/src/s3.rs:453-494) keeps the SDK endpoint rules' URL. On a hostname endpoint (http://localhost:PORT), `--x-s3`/`--xa-s3` names go virtual-host style (`PUT /` with host `<bucket>.localhost`), and non-host-safe ones (e.g. `Wyrd_Validate--x-s3`) are refused as RequestNotBuilt. Outposts-alias names (`...--op-s3`) resolve to an amazonaws.com https URL even on an IP endpoint and are reported as NoResponse, though nothing was sent. Fix: the client always has `--endpoint` and always forces path-style, so use a resolver that always builds `{endpoint}{path}/{uri-encoded bucket}` (no bucket segment for ListBuckets). Any name refused before sending must report RequestNotBuilt, never NoResponse. - Tests: add legs on `http://localhost:{port}` with one host-safe and one non-host-safe `--x-s3` name, plus an Outposts-alias-shaped name. Each must reach the endpoint as `PUT /<name>/`. - Code comments and docs: make crates/validate/src/s3/verbs.rs:35-43, crates/validate/src/s3.rs:4-6, crates/validate/src/s3/answer.rs:426 ("handed to the connector" = sent) and docs/design/architecture/05-building-block-view.md:299 match the fixed behaviour exactly. Name any remaining exception (e.g. `arn:`) explicitly. Do not split: the size backstop was weighed and declined. This is already a split child of #743, and the findings are narrow.
- By / date: Eduard Ralph / 2026-10-19

## 10. Act candidates (hints for the next Act review)
- Plan advisory: 3 finding(s); brief revised: yes (plan-advisory-*.md)
- (empty is the common case)
